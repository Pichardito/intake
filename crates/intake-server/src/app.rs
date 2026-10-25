//! Shared state and the router. `build_app` is what `main` serves and what the integration
//! tests drive directly.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, post};
use axum::Router;
use intake_core::PromptSet;
use serde::Serialize;
use sqlx::PgPool;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use veilkit_core::Engine;

use crate::config::Config;
use crate::github::GitHub;
use crate::model::Model;
use crate::ratelimit::RateLimiters;
use crate::routes::{admin, public};

/// Public request bodies are capped here (spec §3.3).
pub const PUBLIC_BODY_LIMIT: usize = 4 * 1024;
const ORIGIN_REFRESH: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct PromptVersions {
    pub persona: String,
    pub triage: String,
    pub guard: String,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub model: Arc<dyn Model>,
    pub engine: Arc<Engine>,
    pub prompts: Arc<PromptSet>,
    pub prompt_versions: Arc<PromptVersions>,
    pub config: Arc<Config>,
    pub github: Arc<GitHub>,
    pub limiters: Arc<RateLimiters>,
    /// Union of every site's origin allowlist; refreshed in the background.
    pub origins: Arc<RwLock<HashSet<String>>>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        model: Arc<dyn Model>,
        prompts: PromptSet,
        config: Config,
    ) -> anyhow::Result<AppState> {
        let engine = Engine::all_packs().map_err(|e| anyhow::anyhow!("veilkit engine: {e}"))?;
        let github = GitHub::new(
            config.github_api_base.clone(),
            config.github_oauth_base.clone(),
            config.github_token.clone(),
            config.github_client_id.clone(),
            config.github_client_secret.clone(),
        )?;
        let prompt_versions = PromptVersions {
            persona: PromptSet::version_of(&prompts.persona).to_string(),
            triage: PromptSet::version_of(&prompts.triage).to_string(),
            guard: PromptSet::version_of(&prompts.guard).to_string(),
        };
        Ok(AppState {
            pool,
            model,
            engine: Arc::new(engine),
            prompts: Arc::new(prompts),
            prompt_versions: Arc::new(prompt_versions),
            config: Arc::new(config),
            github: Arc::new(github),
            limiters: Arc::new(RateLimiters::new()),
            origins: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Load the CORS origin set from the sites table (call once at boot, then periodically).
    pub async fn refresh_origins(&self) -> sqlx::Result<usize> {
        let list = crate::db::all_origins(&self.pool).await?;
        let n = list.len();
        *self.origins.write().expect("origins lock") = list.into_iter().collect();
        Ok(n)
    }

    /// Periodic refresh so `admin add-site` takes effect without a restart.
    pub fn spawn_origin_refresher(&self) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(ORIGIN_REFRESH);
            loop {
                tick.tick().await;
                if let Err(e) = me.refresh_origins().await {
                    tracing::warn!(error = %e, "origin refresh failed");
                }
            }
        });
    }
}

/// Read the three prompt files from `dir`.
pub fn load_prompts(dir: &Path) -> anyhow::Result<PromptSet> {
    let read = |name: &str| -> anyhow::Result<String> {
        let p = dir.join(name);
        std::fs::read_to_string(&p).map_err(|e| anyhow::anyhow!("reading {}: {e}", p.display()))
    };
    Ok(PromptSet {
        persona: read("persona.md")?,
        triage: read("triage.md")?,
        guard: read("shared/guard.md")?,
    })
}

pub fn build_app(state: AppState) -> Router {
    let origins = state.origins.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            origin
                .to_str()
                .map(|o| origins.read().map(|set| set.contains(o)).unwrap_or(false))
                .unwrap_or(false)
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .max_age(Duration::from_secs(600));

    let public_api = Router::new()
        .route("/v1/health", get(public::health))
        .route("/v1/conversations", post(public::start))
        .route("/v1/conversations/{id}/messages", post(public::message))
        .route("/v1/conversations/{id}/end", post(public::end))
        .layer(DefaultBodyLimit::max(PUBLIC_BODY_LIMIT))
        .layer(cors);

    let admin_api = Router::new()
        .route("/admin/login", get(admin::login))
        .route("/admin/callback", get(admin::callback))
        .route("/admin/logout", post(admin::logout))
        .route("/admin/me", get(admin::me))
        .route("/admin/sites", get(admin::sites))
        .route("/admin/tickets", get(admin::list_tickets))
        .route("/admin/tickets/{id}", get(admin::get_ticket))
        .route("/admin/tickets/{id}/approve", post(admin::approve))
        .route("/admin/tickets/{id}/merge", post(admin::merge))
        .route("/admin/tickets/{id}/dismiss", post(admin::dismiss))
        .route("/admin/conversations/{id}", delete(admin::delete_conversation))
        .layer(DefaultBodyLimit::max(64 * 1024));

    let mut app = Router::new().merge(public_api).merge(admin_api);

    // Static: the widget bundle (immutable) and the console SPA, only when built.
    let widget = Path::new(&state.config.widget_dist).join("intake.js");
    if widget.is_file() {
        let immutable = SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
        let svc = tower::ServiceBuilder::new().layer(immutable).service(ServeFile::new(widget));
        app = app.nest_service("/widget/intake.js", svc);
    } else {
        tracing::info!(path = %widget.display(), "widget bundle not found; /widget/intake.js not served");
    }
    let console = Path::new(&state.config.console_dist);
    if console.join("index.html").is_file() {
        let spa = ServeDir::new(console).not_found_service(ServeFile::new(console.join("index.html")));
        app = app
            .nest_service("/console", spa)
            .route("/", get(|| async { axum::response::Redirect::temporary("/console") }));
    } else {
        tracing::info!(path = %console.display(), "console build not found; /console not served");
    }

    app.layer(TraceLayer::new_for_http()).with_state(state)
}
