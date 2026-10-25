//! The conversation service: one visitor turn, and triage. This is the only place where the
//! core's decisions, the scrub, the model and the database meet.

use intake_core::{
    advance, fallback_triage, parse_persona, parse_triage, persona_messages, resolve_duplicate,
    triage_messages, Extracted, PersonaCtx, PersonaReply, Role, State, StateHint, TriageCtx, Turn,
    VisitorInput,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth;
use crate::db::{self, ConversationRow, NewConversation};
use crate::error::{AppError, AppResult};
use crate::model::ModelRequest;
use crate::scrub::scrub;

/// Hard cap on visitor messages per conversation (the state machine normally closes long
/// before this; the cap is for a broken or hostile client).
pub const MAX_VISITOR_MESSAGES: i64 = 12;
const PERSONA_MAX_TOKENS: u32 = 600;
const TRIAGE_MAX_TOKENS: u32 = 1200;
const FTS_CANDIDATES: i64 = 20;
const FALLBACK_REPLY: &str = "Thanks. Could you tell me a little more about what happened?";

#[derive(Debug, Serialize)]
pub struct StartResponse {
    pub id: Uuid,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub reply: String,
    pub state: State,
    pub redactions: std::collections::BTreeMap<String, u32>,
    pub done: bool,
    pub ticket_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct EndResponse {
    pub ticket_id: Option<i64>,
    pub state: State,
}

pub struct StartMeta<'a> {
    pub page_url: Option<&'a str>,
    pub ua: Option<&'a str>,
    pub viewport: Option<&'a str>,
}

/// Create a conversation for `site_key`. `origin` is the request's `Origin` header; when the
/// site has an allowlist it must match.
pub async fn start(
    state: &AppState,
    site_key: &str,
    origin: Option<&str>,
    meta: StartMeta<'_>,
) -> AppResult<StartResponse> {
    let site = db::find_site_by_key(&state.pool, site_key).await?.ok_or(AppError::NotFound)?;
    if !site.origin_allowlist.is_empty() {
        let ok = origin.is_some_and(|o| site.origin_allowlist.iter().any(|a| a.eq_ignore_ascii_case(o)));
        if !ok {
            return Err(AppError::Forbidden);
        }
    }
    let id = Uuid::new_v4();
    let token = auth::new_token();
    let token_hash = auth::hash_token(&token)?;
    let clip =
        |s: Option<&str>| s.map(|s| scrub(&state.engine, s).text.chars().take(512).collect::<String>());
    let (page_url, ua, viewport) = (clip(meta.page_url), clip(meta.ua), clip(meta.viewport));
    db::insert_conversation(
        &state.pool,
        NewConversation {
            id,
            site_id: site.id,
            token_hash: &token_hash,
            page_url: page_url.as_deref(),
            ua: ua.as_deref(),
            viewport: viewport.as_deref(),
        },
    )
    .await?;
    db::audit(&state.pool, "visitor", "conversation.started", &subject(id), json!({ "site": site.key }))
        .await?;
    Ok(StartResponse { id, token })
}

fn subject(id: Uuid) -> String {
    format!("conversation:{id}")
}

async fn authed(state: &AppState, id: Uuid, token: &str) -> AppResult<ConversationRow> {
    let conv = db::load_conversation(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    if !auth::verify_token(token, &conv.token_hash) {
        return Err(AppError::Unauthorized);
    }
    Ok(conv)
}

fn turns(messages: &[db::MessageRow]) -> Vec<Turn> {
    messages
        .iter()
        .map(|m| Turn {
            role: if m.role == "assistant" { Role::Assistant } else { Role::User },
            text: m.text.clone(),
        })
        .collect()
}

/// One visitor turn: scrub → persona call → parse → advance → persist → maybe triage.
pub async fn visitor_message(
    state: &AppState,
    id: Uuid,
    token: &str,
    input: VisitorInput,
) -> AppResult<MessageResponse> {
    let conv = authed(state, id, token).await?;
    if !conv.state.accepts_messages() {
        return Err(AppError::Conflict("this conversation is finished".into()));
    }
    let text = input.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("text is required".into()));
    }
    let sent = db::count_visitor_messages(&state.pool, id).await?;
    if sent >= MAX_VISITOR_MESSAGES {
        let ticket_id = triage(state, &conv).await?;
        return Ok(MessageResponse {
            reply: "Thanks, we've got it. We read every one.".into(),
            state: State::Triaged,
            redactions: Default::default(),
            done: true,
            ticket_id: Some(ticket_id),
        });
    }

    let scrubbed = scrub(&state.engine, text);
    let history = turns(&db::list_messages(&state.pool, id).await?);
    let site = db::find_site(&state.pool, conv.site_id).await?.ok_or(AppError::NotFound)?;
    let asked = match conv.state {
        State::Clarifying { asked } => asked,
        _ => 0,
    };
    let ctx = PersonaCtx {
        site_name: &site.name,
        product_blurb: &site.product_blurb,
        history: &history,
        visitor_text: &scrubbed.text,
        kind_hint: conv.extracted.kind_hint,
        extracted: &conv.extracted,
        asked,
    };
    let req =
        ModelRequest::from_messages(persona_messages(&state.prompts, &ctx), PERSONA_MAX_TOKENS, "persona");
    let reply = call_persona(state, req).await?;

    let (reply_text, merged, hint) = match reply {
        Some(r) => {
            let merged = conv.extracted.merge(&r.extracted);
            (r.text, merged, r.state_hint)
        }
        // The model failed twice to produce the format: keep the visitor's message, do not
        // move the state, answer with a fixed line. Nothing the model said is trusted.
        None => (FALLBACK_REPLY.to_string(), conv.extracted.clone(), StateHint::AskMore),
    };
    let next = if reply_text == FALLBACK_REPLY && merged == conv.extracted {
        conv.state
    } else {
        advance(conv.state, merged.kind_hint, &merged, hint)
    };

    let redactions = serde_json::to_value(&scrubbed.redactions).unwrap_or(json!({}));
    db::insert_message(&state.pool, id, "user", &scrubbed.text, &redactions).await?;
    db::insert_message(&state.pool, id, "assistant", &reply_text, &json!({})).await?;
    db::update_conversation(&state.pool, id, next, &merged, false).await?;
    db::audit(
        &state.pool,
        "system",
        "conversation.turn",
        &subject(id),
        json!({ "from": conv.state.to_column(), "to": next.to_column(), "redactions": redactions }),
    )
    .await?;

    let mut out = MessageResponse {
        reply: reply_text,
        state: next,
        redactions: scrubbed.redactions,
        done: false,
        ticket_id: None,
    };
    if next == State::Closing {
        let refreshed = db::load_conversation(&state.pool, id).await?.ok_or(AppError::NotFound)?;
        out.ticket_id = Some(triage(state, &refreshed).await?);
        out.state = State::Triaged;
        out.done = true;
    }
    Ok(out)
}

/// Call the model, parse strictly, retry once on a parse error. `None` = gave up.
async fn call_persona(state: &AppState, req: ModelRequest) -> AppResult<Option<PersonaReply>> {
    for attempt in 0..2 {
        let raw = state.model.complete(req.clone()).await.map_err(|e| AppError::Upstream(e.into()))?;
        match parse_persona(&raw) {
            Ok(r) => return Ok(Some(r)),
            Err(e) => {
                tracing::warn!(attempt, error = %e, prompt_version = %state.prompt_versions.persona, "persona output rejected")
            }
        }
    }
    Ok(None)
}

/// Visitor ends the conversation. Idempotent: an already-triaged conversation returns its ticket.
pub async fn end(state: &AppState, id: Uuid, token: &str) -> AppResult<EndResponse> {
    let conv = authed(state, id, token).await?;
    if conv.state == State::Triaged {
        let ticket_id = db::ticket_for_conversation(&state.pool, id).await?;
        return Ok(EndResponse { ticket_id, state: State::Triaged });
    }
    if db::count_visitor_messages(&state.pool, id).await? == 0 {
        db::update_conversation(&state.pool, id, State::Triaged, &conv.extracted, true).await?;
        db::audit(&state.pool, "visitor", "conversation.ended_empty", &subject(id), json!({})).await?;
        return Ok(EndResponse { ticket_id: None, state: State::Triaged });
    }
    let ticket_id = triage(state, &conv).await?;
    Ok(EndResponse { ticket_id: Some(ticket_id), state: State::Triaged })
}

/// Triage a conversation into a draft ticket: FTS candidates → prompt → strict parse (retry
/// once, then the Noise fallback) → dedup resolution → `tickets` row → state `Triaged`.
async fn triage(state: &AppState, conv: &ConversationRow) -> AppResult<i64> {
    let id = conv.id;
    let site = db::find_site(&state.pool, conv.site_id).await?.ok_or(AppError::NotFound)?;
    let transcript = turns(&db::list_messages(&state.pool, id).await?);
    let visitor_text: String = transcript
        .iter()
        .filter(|t| t.role == Role::User)
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let mut terms: Vec<String> = intake_core::dedup::tokens(&visitor_text).into_iter().collect();
    terms.sort();
    terms.truncate(24);
    let candidates = db::fts_candidates(&state.pool, site.id, &terms, FTS_CANDIDATES).await?;

    let ctx = TriageCtx {
        site_name: &site.name,
        product_blurb: &site.product_blurb,
        transcript: &transcript,
        extracted: &conv.extracted,
        candidates: &candidates,
    };
    let req = ModelRequest::from_messages(triage_messages(&state.prompts, &ctx), TRIAGE_MAX_TOKENS, "triage");
    let mut parsed = None;
    let mut last_raw = String::new();
    for attempt in 0..2 {
        last_raw = state.model.complete(req.clone()).await.map_err(|e| AppError::Upstream(e.into()))?;
        match parse_triage(&last_raw) {
            Ok(t) => {
                parsed = Some(t);
                break;
            }
            Err(e) => {
                tracing::warn!(attempt, error = %e, prompt_version = %state.prompt_versions.triage, "triage output rejected")
            }
        }
    }
    let fell_back = parsed.is_none();
    // A fallback is raw model output, not a triage: it never gets a duplicate link, otherwise
    // every unparseable reply would be "a duplicate" of the previous unparseable reply.
    let triage = match parsed {
        Some(t) => resolve_duplicate(t, &candidates),
        None => fallback_triage(&last_raw),
    };

    let ticket_id = db::insert_draft_ticket(&state.pool, site.id, id, &triage, &conv.extracted).await?;
    db::update_conversation(&state.pool, id, State::Triaged, &conv.extracted, true).await?;
    db::audit(
        &state.pool,
        "system",
        "ticket.drafted",
        &format!("ticket:{ticket_id}"),
        json!({
            "conversation": id, "kind": triage.kind, "confidence": triage.confidence,
            "duplicate_of": triage.duplicate_of, "fallback": fell_back,
            "candidates": candidates.len(), "prompt_version": state.prompt_versions.triage,
        }),
    )
    .await?;
    db::audit(&state.pool, "system", "conversation.triaged", &subject(id), json!({ "ticket": ticket_id }))
        .await?;
    Ok(ticket_id)
}

/// Exposed for the admin takedown path.
pub fn extracted_of(conv: &ConversationRow) -> &Extracted {
    &conv.extracted
}
