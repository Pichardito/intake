//! Visitor-facing routes. JSON only, 4 KB bodies, token bucket per client, generic errors.

use axum::extract::{ConnectInfo, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use intake_core::VisitorInput;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use uuid::Uuid;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::flow::{self, StartMeta};
use crate::ratelimit::client_ip;

#[derive(Deserialize)]
pub struct StartBody {
    pub site_key: String,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub ua: Option<String>,
    #[serde(default)]
    pub viewport: Option<String>,
}

/// The peer address when the server was started with connect-info, else `None` (tests).
pub struct Peer(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for Peer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Peer(parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0)))
    }
}

fn peer(headers: &HeaderMap, conn: Peer, state: &AppState) -> String {
    client_ip(headers, conn.0, state.config.trust_proxy)
}

fn bearer(headers: &HeaderMap) -> AppResult<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or(AppError::Unauthorized)
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query_scalar::<_, i32>("select 1").fetch_one(&state.pool).await.is_ok();
    let status = if db_ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "ok": db_ok, "model": state.model.name(), "prompts": state.prompt_versions })))
}

pub async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    conn: Peer,
    Json(body): Json<StartBody>,
) -> AppResult<impl IntoResponse> {
    let key = format!("{}|{}", peer(&headers, conn, &state), body.site_key);
    if state.limiters.start.check_key(&key).is_err() {
        return Err(AppError::RateLimited);
    }
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let meta = StartMeta {
        page_url: body.page_url.as_deref(),
        ua: body.ua.as_deref(),
        viewport: body.viewport.as_deref(),
    };
    let out = flow::start(&state, &body.site_key, origin, meta).await?;
    Ok((axum::http::StatusCode::CREATED, Json(out)))
}

pub async fn message(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    conn: Peer,
    Json(input): Json<VisitorInput>,
) -> AppResult<impl IntoResponse> {
    let key = format!("{}|{}", peer(&headers, conn, &state), id);
    if state.limiters.message.check_key(&key).is_err() {
        return Err(AppError::RateLimited);
    }
    let token = bearer(&headers)?;
    Ok(Json(flow::visitor_message(&state, id, token, input).await?))
}

pub async fn end(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let token = bearer(&headers)?;
    Ok(Json(flow::end(&state, id, token).await?))
}
