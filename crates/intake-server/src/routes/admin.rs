//! Admin routes: GitHub OAuth for exactly one login, then the queue. Every action here is
//! the human gate the architecture promises: nothing below reaches GitHub without it.

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use intake_core::{rank_duplicates, Kind, Severity, TicketStub, Triage};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::{
    self, Session, OAUTH_STATE_COOKIE, OAUTH_STATE_TTL_SECS, SESSION_COOKIE, SESSION_TTL_SECS,
};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::github::{issue_body, issue_number, issue_title};

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// Extractor: a valid session cookie whose login is the configured admin.
pub struct Admin(pub Session);

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let cookie_header =
            parts.headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).ok_or(AppError::Unauthorized)?;
        let raw = auth::cookie_value(cookie_header, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        let session = auth::decode_session(raw, &state.config.session_secret, now_unix())
            .ok_or(AppError::Unauthorized)?;
        if session.login != state.config.admin_github_login {
            return Err(AppError::Forbidden);
        }
        Ok(Admin(session))
    }
}

fn redirect_uri(state: &AppState) -> String {
    format!("{}/admin/callback", state.config.public_origin)
}

fn with_cookie(mut resp: Response, cookie: String) -> Response {
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    resp
}

pub async fn login(State(state): State<AppState>) -> Response {
    let nonce = auth::new_token();
    let signed = auth::sign(nonce.as_bytes(), &state.config.session_secret);
    let url = state.github.authorize_url(&redirect_uri(&state), &nonce);
    with_cookie(
        Redirect::temporary(&url).into_response(),
        auth::cookie(OAUTH_STATE_COOKIE, &signed, OAUTH_STATE_TTL_SECS, state.config.secure_cookies()),
    )
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let cookie_header =
        headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).ok_or(AppError::Unauthorized)?;
    let signed = auth::cookie_value(cookie_header, OAUTH_STATE_COOKIE).ok_or(AppError::Unauthorized)?;
    let expected = auth::verify_signed(signed, &state.config.session_secret).ok_or(AppError::Unauthorized)?;
    if expected != q.state.as_bytes() {
        return Err(AppError::Unauthorized);
    }
    let access = state
        .github
        .exchange_code(&q.code, &redirect_uri(&state))
        .await
        .map_err(|e| AppError::Upstream(e.into()))?;
    let login = state.github.login_for(&access).await.map_err(|e| AppError::Upstream(e.into()))?;
    if login != state.config.admin_github_login {
        db::audit(&state.pool, &login, "admin.login_refused", "admin", json!({})).await?;
        return Err(AppError::Forbidden);
    }
    let session = Session { login: login.clone(), exp: now_unix() + SESSION_TTL_SECS };
    let value = auth::encode_session(&session, &state.config.session_secret);
    db::audit(&state.pool, &login, "admin.login", "admin", json!({})).await?;
    let secure = state.config.secure_cookies();
    let resp = Redirect::to("/console").into_response();
    let resp = with_cookie(resp, auth::cookie(SESSION_COOKIE, &value, SESSION_TTL_SECS, secure));
    Ok(with_cookie(resp, auth::cookie(OAUTH_STATE_COOKIE, "", 0, secure)))
}

pub async fn logout(State(state): State<AppState>) -> Response {
    with_cookie(
        StatusCode::NO_CONTENT.into_response(),
        auth::cookie(SESSION_COOKIE, "", 0, state.config.secure_cookies()),
    )
}

pub async fn me(Admin(session): Admin) -> impl IntoResponse {
    Json(json!({ "login": session.login, "exp": session.exp }))
}

pub async fn sites(Admin(_): Admin, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    Ok(Json(db::list_sites(&state.pool).await?))
}

#[derive(Deserialize)]
pub struct TicketFilter {
    pub site: Option<i64>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_tickets(
    Admin(_): Admin,
    State(state): State<AppState>,
    Query(f): Query<TicketFilter>,
) -> AppResult<impl IntoResponse> {
    let limit = f.limit.unwrap_or(200).clamp(1, 1000);
    let tickets = db::list_tickets(&state.pool, f.site, f.status.as_deref(), limit).await?;
    let sites = db::list_sites(&state.pool).await?;
    Ok(Json(json!({ "tickets": tickets, "sites": sites })))
}

pub async fn get_ticket(
    Admin(_): Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let ticket = db::get_ticket(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    let (transcript, conversation) = match ticket.conversation_id {
        Some(cid) => {
            let conv = db::load_conversation(&state.pool, cid).await?;
            let transcript = db::list_messages(&state.pool, cid).await?;
            let meta = conv.map(|c| {
                json!({ "id": c.id, "page_url": c.page_url, "ua": c.ua, "viewport": c.viewport,
                        "started_at": c.started_at.unix_timestamp(), "extracted": c.extracted })
            });
            (transcript, meta)
        }
        None => (vec![], None),
    };
    let duplicates = duplicates_for(&state, &ticket).await?;
    let audit = db::audit_for(&state.pool, &format!("ticket:{id}")).await?;
    Ok(Json(json!({
        "ticket": ticket, "transcript": transcript, "conversation": conversation,
        "duplicates": duplicates, "audit": audit,
    })))
}

/// Candidate duplicates for the console: FTS over the ticket's own words, scored by the same
/// Jaccard the triage step uses, with the ticket's recorded `duplicate_of` always included.
async fn duplicates_for(state: &AppState, ticket: &db::TicketRow) -> AppResult<Vec<serde_json::Value>> {
    let mut terms: Vec<String> =
        intake_core::dedup::tokens(&format!("{} {}", ticket.title, ticket.summary)).into_iter().collect();
    terms.sort();
    terms.truncate(24);
    let mut rows = db::fts_candidate_rows(&state.pool, ticket.site_id, ticket.id, &terms, 20).await?;
    if let Some(d) = ticket.duplicate_of {
        if !rows.iter().any(|r| r.id == d) {
            if let Some(t) = db::get_ticket(&state.pool, d).await? {
                rows.push(db::CandidateRow {
                    id: t.id,
                    title: t.title,
                    summary: t.summary,
                    status: t.status,
                    github_issue_url: t.github_issue_url,
                });
            }
        }
    }
    let stubs: Vec<TicketStub> = rows
        .iter()
        .map(|r| TicketStub { id: r.id, title: r.title.clone(), summary: r.summary.clone() })
        .collect();
    let as_triage = Triage {
        kind: ticket.kind,
        title: ticket.title.clone(),
        summary: ticket.summary.clone(),
        steps: vec![],
        expected: None,
        actual: None,
        severity: ticket.severity,
        confidence: ticket.confidence,
        duplicate_of: ticket.duplicate_of,
    };
    let ranked = rank_duplicates(&stubs, &as_triage);
    Ok(ranked
        .into_iter()
        .filter_map(|(id, score)| {
            let r = rows.iter().find(|r| r.id == id)?;
            Some(json!({
                "id": r.id, "title": r.title, "status": r.status,
                "github_issue_url": r.github_issue_url, "score": score,
            }))
        })
        .collect())
}

/// The owner's edited triage fields, all optional; unknown keys are rejected.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TicketEdits {
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub steps: Option<Vec<String>>,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub actual: Option<String>,
    #[serde(default)]
    pub severity: Option<Severity>,
}

#[derive(Deserialize, Default)]
pub struct ApproveBody {
    /// `{ "fields": { … } }` — edits made in the console before approval.
    #[serde(default)]
    pub fields: TicketEdits,
}

pub async fn approve(
    Admin(session): Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ApproveBody>,
) -> AppResult<impl IntoResponse> {
    let edits = body.fields;
    let mut ticket = db::get_ticket(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    if ticket.status != "draft" {
        return Err(AppError::Conflict(format!("ticket is already {}", ticket.status)));
    }
    let non_empty = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
    let mut edited = false;
    if let Some(t) = non_empty(edits.title) {
        ticket.title = t;
        edited = true;
    }
    if let Some(s) = non_empty(edits.summary) {
        ticket.summary = s;
        edited = true;
    }
    if let Some(k) = edits.kind {
        ticket.kind = k;
        edited = true;
    }
    if let Some(sev) = edits.severity {
        ticket.severity = sev;
        edited = true;
    }
    if let Some(steps) = edits.steps {
        ticket.steps = steps;
        edited = true;
    }
    if let Some(e) = edits.expected {
        ticket.expected = non_empty(Some(e));
        edited = true;
    }
    if let Some(a) = edits.actual {
        ticket.actual = non_empty(Some(a));
        edited = true;
    }
    if edited {
        db::update_ticket_fields(&state.pool, &ticket).await?;
    }
    let site = db::find_site(&state.pool, ticket.site_id).await?.ok_or(AppError::NotFound)?;
    let conversation = match ticket.conversation_id {
        Some(cid) => db::load_conversation(&state.pool, cid).await?,
        None => None,
    };
    let admin_url = format!("{}/console/t/{}", state.config.public_origin, ticket.id);
    let title = issue_title(ticket.kind, &ticket.title);
    let body = issue_body(&ticket, conversation.as_ref(), &admin_url);

    // Flip the status first so a double click cannot create two issues; store the URL after.
    if !db::decide_ticket(&state.pool, id, "approved", None, None, None).await? {
        return Err(AppError::Conflict("ticket is no longer a draft".into()));
    }
    let url = state
        .github
        .create_issue(&site.github_repo, &title, &body, &["intake", ticket.kind.as_str()])
        .await
        .map_err(|e| AppError::Upstream(e.into()))?;
    db::set_issue_url(&state.pool, id, &url).await?;
    db::audit(
        &state.pool,
        &session.login,
        "ticket.approved",
        &format!("ticket:{id}"),
        json!({ "issue": url }),
    )
    .await?;
    Ok(Json(json!({ "id": id, "status": "approved", "github_issue_url": url })))
}

#[derive(Deserialize)]
pub struct MergeBody {
    /// Ticket id whose GitHub issue receives the comment.
    pub into: i64,
}

pub async fn merge(
    Admin(session): Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MergeBody>,
) -> AppResult<impl IntoResponse> {
    if body.into == id {
        return Err(AppError::BadRequest("a ticket cannot be merged into itself".into()));
    }
    let ticket = db::get_ticket(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    if ticket.status != "draft" {
        return Err(AppError::Conflict(format!("ticket is already {}", ticket.status)));
    }
    let target = db::get_ticket(&state.pool, body.into)
        .await?
        .ok_or(AppError::BadRequest("target ticket not found".into()))?;
    if target.site_id != ticket.site_id {
        return Err(AppError::BadRequest("target belongs to another site".into()));
    }
    let url = target
        .github_issue_url
        .clone()
        .ok_or(AppError::BadRequest("target ticket has no GitHub issue".into()))?;
    let number = issue_number(&url).ok_or(AppError::BadRequest("target issue URL is malformed".into()))?;
    let site = db::find_site(&state.pool, ticket.site_id).await?.ok_or(AppError::NotFound)?;
    let admin_url = format!("{}/console/t/{}", state.config.public_origin, ticket.id);
    let comment = format!(
        "Another report of this was merged in from the feedback queue.\n\n## Summary\n\n{}\n\n> Visitor-provided content, untrusted. Queue entry: {}\n",
        ticket.summary.trim(),
        admin_url
    );
    if !db::decide_ticket(&state.pool, id, "merged", Some(&url), None, Some(target.id)).await? {
        return Err(AppError::Conflict("ticket is no longer a draft".into()));
    }
    let comment_url = state
        .github
        .comment_issue(&site.github_repo, number, &comment)
        .await
        .map_err(|e| AppError::Upstream(e.into()))?;
    db::audit(
        &state.pool,
        &session.login,
        "ticket.merged",
        &format!("ticket:{id}"),
        json!({ "into": target.id, "comment": comment_url }),
    )
    .await?;
    Ok(Json(json!({ "id": id, "status": "merged", "into": target.id, "comment_url": comment_url })))
}

#[derive(Deserialize)]
pub struct DismissBody {
    pub reason: String,
}

pub async fn dismiss(
    Admin(session): Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<DismissBody>,
) -> AppResult<impl IntoResponse> {
    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }
    db::get_ticket(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    if !db::decide_ticket(&state.pool, id, "dismissed", None, Some(reason), None).await? {
        return Err(AppError::Conflict("ticket is not a draft".into()));
    }
    db::audit(
        &state.pool,
        &session.login,
        "ticket.dismissed",
        &format!("ticket:{id}"),
        json!({ "reason": reason }),
    )
    .await?;
    Ok(Json(json!({ "id": id, "status": "dismissed" })))
}

/// Takedown: remove a conversation and its messages. Tickets keep their (scrubbed) text and
/// lose the link; the audit trail records who did it.
pub async fn delete_conversation(
    Admin(session): Admin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    if !db::delete_conversation(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    db::audit(&state.pool, &session.login, "conversation.deleted", &format!("conversation:{id}"), json!({}))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
