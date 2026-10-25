//! Every SQL statement lives here, as `sqlx::query!` so the schema is checked at compile time
//! (offline data in `.sqlx/`). Handlers never see SQL.

use std::str::FromStr;

use intake_core::{Kind, Severity, State, TicketStub, Triage};
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Site {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub product_blurb: String,
    pub origin_allowlist: Vec<String>,
    pub github_repo: String,
}

#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: Uuid,
    pub site_id: i64,
    pub token_hash: String,
    pub state: State,
    pub extracted: intake_core::Extracted,
    pub page_url: Option<String>,
    pub ua: Option<String>,
    pub viewport: Option<String>,
    pub started_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageRow {
    pub id: i64,
    pub role: String,
    pub text: String,
    pub redactions: Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TicketRow {
    pub id: i64,
    pub site_id: i64,
    pub conversation_id: Option<Uuid>,
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub steps: Vec<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub severity: Severity,
    pub confidence: f32,
    pub duplicate_of: Option<i64>,
    pub status: String,
    pub github_issue_url: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub decided_at: Option<OffsetDateTime>,
    pub decided_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

fn bad(col: &str, e: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Decode(format!("{col}: {e}").into())
}

// ---- sites ---------------------------------------------------------------------------

pub async fn insert_site(
    pool: &PgPool,
    key: &str,
    name: &str,
    blurb: &str,
    origins: &[String],
    repo: &str,
) -> sqlx::Result<Site> {
    let r = sqlx::query!(
        r#"insert into sites (key, name, product_blurb, origin_allowlist, github_repo)
           values ($1, $2, $3, $4, $5)
           returning id, key, name, product_blurb, origin_allowlist, github_repo"#,
        key,
        name,
        blurb,
        origins,
        repo
    )
    .fetch_one(pool)
    .await?;
    Ok(Site {
        id: r.id,
        key: r.key,
        name: r.name,
        product_blurb: r.product_blurb,
        origin_allowlist: r.origin_allowlist,
        github_repo: r.github_repo,
    })
}

pub async fn find_site_by_key(pool: &PgPool, key: &str) -> sqlx::Result<Option<Site>> {
    let r = sqlx::query!(
        "select id, key, name, product_blurb, origin_allowlist, github_repo from sites where key = $1",
        key
    )
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| Site {
        id: r.id,
        key: r.key,
        name: r.name,
        product_blurb: r.product_blurb,
        origin_allowlist: r.origin_allowlist,
        github_repo: r.github_repo,
    }))
}

pub async fn find_site(pool: &PgPool, id: i64) -> sqlx::Result<Option<Site>> {
    let r = sqlx::query!(
        "select id, key, name, product_blurb, origin_allowlist, github_repo from sites where id = $1",
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| Site {
        id: r.id,
        key: r.key,
        name: r.name,
        product_blurb: r.product_blurb,
        origin_allowlist: r.origin_allowlist,
        github_repo: r.github_repo,
    }))
}

/// Union of every site's allowlist, for the CORS layer.
pub async fn all_origins(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    let rows = sqlx::query!("select origin_allowlist from sites").fetch_all(pool).await?;
    Ok(rows.into_iter().flat_map(|r| r.origin_allowlist).collect())
}

pub async fn list_sites(pool: &PgPool) -> sqlx::Result<Vec<Site>> {
    let rows = sqlx::query!(
        "select id, key, name, product_blurb, origin_allowlist, github_repo from sites order by id"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Site {
            id: r.id,
            key: r.key,
            name: r.name,
            product_blurb: r.product_blurb,
            origin_allowlist: r.origin_allowlist,
            github_repo: r.github_repo,
        })
        .collect())
}

// ---- conversations -------------------------------------------------------------------

pub struct NewConversation<'a> {
    pub id: Uuid,
    pub site_id: i64,
    pub token_hash: &'a str,
    pub page_url: Option<&'a str>,
    pub ua: Option<&'a str>,
    pub viewport: Option<&'a str>,
}

pub async fn insert_conversation(pool: &PgPool, c: NewConversation<'_>) -> sqlx::Result<()> {
    sqlx::query!(
        r#"insert into conversations (id, site_id, token_hash, page_url, ua, viewport)
           values ($1, $2, $3, $4, $5, $6)"#,
        c.id,
        c.site_id,
        c.token_hash,
        c.page_url,
        c.ua,
        c.viewport
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_conversation(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<ConversationRow>> {
    let r = sqlx::query!(
        r#"select id, site_id, token_hash, state, extracted, page_url, ua, viewport, started_at, ended_at
           from conversations where id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    r.map(|r| {
        Ok(ConversationRow {
            id: r.id,
            site_id: r.site_id,
            token_hash: r.token_hash,
            state: State::parse_column(&r.state).map_err(|e| bad("state", e))?,
            extracted: serde_json::from_value(r.extracted).map_err(|e| bad("extracted", e))?,
            page_url: r.page_url,
            ua: r.ua,
            viewport: r.viewport,
            started_at: r.started_at,
            ended_at: r.ended_at,
        })
    })
    .transpose()
}

pub async fn update_conversation(
    pool: &PgPool,
    id: Uuid,
    state: State,
    extracted: &intake_core::Extracted,
    ended: bool,
) -> sqlx::Result<()> {
    let extracted = serde_json::to_value(extracted).map_err(|e| bad("extracted", e))?;
    sqlx::query!(
        r#"update conversations
           set state = $2, extracted = $3,
               ended_at = case when $4 then coalesce(ended_at, now()) else ended_at end
           where id = $1"#,
        id,
        state.to_column(),
        extracted,
        ended
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Takedown. Messages cascade; tickets keep their text but lose the link. Returns whether a
/// row existed.
pub async fn delete_conversation(pool: &PgPool, id: Uuid) -> sqlx::Result<bool> {
    let r = sqlx::query!("delete from conversations where id = $1", id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

// ---- messages -------------------------------------------------------------------------

pub async fn insert_message(
    pool: &PgPool,
    conversation_id: Uuid,
    role: &str,
    text: &str,
    redactions: &Value,
) -> sqlx::Result<i64> {
    let r = sqlx::query!(
        r#"insert into messages (conversation_id, role, text_scrubbed, redactions_json)
           values ($1, $2, $3, $4) returning id"#,
        conversation_id,
        role,
        text,
        redactions
    )
    .fetch_one(pool)
    .await?;
    Ok(r.id)
}

pub async fn list_messages(pool: &PgPool, conversation_id: Uuid) -> sqlx::Result<Vec<MessageRow>> {
    let rows = sqlx::query!(
        r#"select id, role, text_scrubbed, redactions_json, created_at
           from messages where conversation_id = $1 order by id"#,
        conversation_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MessageRow {
            id: r.id,
            role: r.role,
            text: r.text_scrubbed,
            redactions: r.redactions_json,
            created_at: r.created_at,
        })
        .collect())
}

pub async fn count_visitor_messages(pool: &PgPool, conversation_id: Uuid) -> sqlx::Result<i64> {
    let r = sqlx::query!(
        r#"select count(*) as "n!" from messages where conversation_id = $1 and role = 'user'"#,
        conversation_id
    )
    .fetch_one(pool)
    .await?;
    Ok(r.n)
}

// ---- tickets --------------------------------------------------------------------------

/// Top `limit` open (draft or approved) tickets of the site whose title+summary match any of
/// the `terms` (plain alphanumeric tokens), best `ts_rank` first.
pub async fn fts_candidates(
    pool: &PgPool,
    site_id: i64,
    terms: &[String],
    limit: i64,
) -> sqlx::Result<Vec<TicketStub>> {
    let safe: Vec<&str> = terms
        .iter()
        .map(String::as_str)
        .filter(|t| t.chars().all(char::is_alphanumeric) && !t.is_empty())
        .collect();
    if safe.is_empty() {
        return Ok(vec![]);
    }
    let query = safe.join(" | ");
    let rows = sqlx::query!(
        r#"select id, title, summary
           from tickets
           where site_id = $1 and status in ('draft', 'approved')
             and fts @@ to_tsquery('english', $2)
           order by ts_rank(fts, to_tsquery('english', $2)) desc, id desc
           limit $3"#,
        site_id,
        query,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| TicketStub { id: r.id, title: r.title, summary: r.summary }).collect())
}

pub async fn insert_draft_ticket(
    pool: &PgPool,
    site_id: i64,
    conversation_id: Uuid,
    t: &Triage,
    extracted: &intake_core::Extracted,
) -> sqlx::Result<i64> {
    let fields = serde_json::json!({
        "steps": t.steps,
        "expected": t.expected,
        "actual": t.actual,
        "extracted": extracted,
    });
    let r = sqlx::query!(
        r#"insert into tickets
             (site_id, conversation_id, kind, title, summary, fields_json, severity, confidence, duplicate_of)
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           returning id"#,
        site_id,
        conversation_id,
        t.kind.as_str(),
        t.title,
        t.summary,
        fields,
        t.severity.as_str(),
        t.confidence,
        t.duplicate_of
    )
    .fetch_one(pool)
    .await?;
    Ok(r.id)
}

/// The raw `tickets` row as `query_as!` reads it; converted to the typed [`TicketRow`].
struct RawTicket {
    id: i64,
    site_id: i64,
    conversation_id: Option<Uuid>,
    kind: String,
    title: String,
    summary: String,
    fields_json: Value,
    severity: String,
    confidence: f32,
    duplicate_of: Option<i64>,
    status: String,
    github_issue_url: Option<String>,
    decided_at: Option<OffsetDateTime>,
    decided_reason: Option<String>,
    created_at: OffsetDateTime,
}

impl TryFrom<RawTicket> for TicketRow {
    type Error = sqlx::Error;

    fn try_from(r: RawTicket) -> sqlx::Result<TicketRow> {
        let steps: Vec<String> = r
            .fields_json
            .get("steps")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let text_field = |k: &str| r.fields_json.get(k).and_then(Value::as_str).map(str::to_string);
        Ok(TicketRow {
            id: r.id,
            site_id: r.site_id,
            conversation_id: r.conversation_id,
            kind: Kind::from_str(&r.kind).map_err(|e| bad("kind", e))?,
            title: r.title,
            summary: r.summary,
            steps,
            expected: text_field("expected"),
            actual: text_field("actual"),
            severity: Severity::from_str(&r.severity).map_err(|e| bad("severity", e))?,
            confidence: r.confidence,
            duplicate_of: r.duplicate_of,
            status: r.status,
            github_issue_url: r.github_issue_url,
            decided_at: r.decided_at,
            decided_reason: r.decided_reason,
            created_at: r.created_at,
        })
    }
}

pub async fn get_ticket(pool: &PgPool, id: i64) -> sqlx::Result<Option<TicketRow>> {
    let r = sqlx::query_as!(
        RawTicket,
        r#"select id, site_id, conversation_id, kind, title, summary, fields_json, severity, confidence,
                  duplicate_of, status, github_issue_url, decided_at, decided_reason, created_at
           from tickets where id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    r.map(TicketRow::try_from).transpose()
}

pub async fn list_tickets(
    pool: &PgPool,
    site_id: Option<i64>,
    status: Option<&str>,
    limit: i64,
) -> sqlx::Result<Vec<TicketRow>> {
    let rows = sqlx::query_as!(
        RawTicket,
        r#"select id, site_id, conversation_id, kind, title, summary, fields_json, severity, confidence,
                  duplicate_of, status, github_issue_url, decided_at, decided_reason, created_at
           from tickets
           where ($1::bigint is null or site_id = $1)
             and ($2::text is null or status = $2)
           order by created_at desc, id desc
           limit $3"#,
        site_id,
        status,
        limit
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(TicketRow::try_from).collect()
}

/// Move a draft to a decided status. Returns false when the ticket was not a draft (so a
/// double click cannot create two issues).
pub async fn decide_ticket(
    pool: &PgPool,
    id: i64,
    status: &str,
    github_issue_url: Option<&str>,
    reason: Option<&str>,
    duplicate_of: Option<i64>,
) -> sqlx::Result<bool> {
    let r = sqlx::query!(
        r#"update tickets
           set status = $2, github_issue_url = coalesce($3, github_issue_url), decided_reason = $4,
               duplicate_of = coalesce($5, duplicate_of), decided_at = now()
           where id = $1 and status = 'draft'"#,
        id,
        status,
        github_issue_url,
        reason,
        duplicate_of
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Fill in the issue URL after a successful create (used when the status flip happened first).
pub async fn set_issue_url(pool: &PgPool, id: i64, url: &str) -> sqlx::Result<()> {
    sqlx::query!("update tickets set github_issue_url = $2 where id = $1", id, url).execute(pool).await?;
    Ok(())
}

pub async fn ticket_for_conversation(pool: &PgPool, conversation_id: Uuid) -> sqlx::Result<Option<i64>> {
    let r = sqlx::query!(
        "select id from tickets where conversation_id = $1 order by id desc limit 1",
        conversation_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| r.id))
}

// ---- audit ----------------------------------------------------------------------------

pub async fn audit(
    pool: &PgPool,
    actor: &str,
    action: &str,
    subject: &str,
    detail: Value,
) -> sqlx::Result<()> {
    sqlx::query!(
        "insert into audit (actor, action, subject, detail) values ($1, $2, $3, $4)",
        actor,
        action,
        subject,
        detail
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRow {
    pub actor: String,
    pub action: String,
    pub detail: Value,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

pub async fn audit_for(pool: &PgPool, subject: &str) -> sqlx::Result<Vec<AuditRow>> {
    let rows = sqlx::query!(
        "select actor, action, detail, at from audit where subject = $1 order by at, id",
        subject
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AuditRow { actor: r.actor, action: r.action, detail: r.detail, at: r.at })
        .collect())
}
