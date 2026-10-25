//! `intake-server`: the HTTP face of intake. Library so the integration tests can build the
//! same router `main` serves.

#![forbid(unsafe_code)]

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod flow;
pub mod github;
pub mod model;
pub mod ratelimit;
pub mod routes;
pub mod scrub;

pub use app::{build_app, load_prompts, AppState};
pub use config::Config;
pub use model::{AnthropicModel, FakeModel, Model};

/// Embedded migrations, applied at boot (and by `sqlx::test`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
