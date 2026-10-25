//! One error type for every handler. Visitors get a generic message; details go to the log.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("rate limited")]
    RateLimited,
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("upstream error: {0}")]
    Upstream(anyhow::Error),
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::Db(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
        }
    }

    /// What the client sees. Never includes an upstream or database message.
    fn public_message(&self) -> String {
        match self {
            AppError::BadRequest(m) | AppError::Conflict(m) => m.clone(),
            AppError::RateLimited => "too many requests; try again in a minute".into(),
            AppError::Upstream(_) => "the service is temporarily unavailable".into(),
            AppError::Db(_) | AppError::Internal(_) => "something went wrong".into(),
            other => other.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() || matches!(self, AppError::Upstream(_)) {
            tracing::error!(error = %self, "request failed");
        } else {
            tracing::debug!(error = %self, "request rejected");
        }
        (status, Json(json!({ "error": self.public_message() }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
