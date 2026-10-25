use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use intake_server::{build_app, db, load_prompts, AnthropicModel, AppState, Config};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "intake-server", about = "Feedback intake persona server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (default).
    Serve,
    /// Administrative tasks that need the database but not the server.
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
}

#[derive(Subcommand)]
enum AdminAction {
    /// Register a site: the widget sends `--key`; `--origin` may repeat.
    AddSite {
        #[arg(long)]
        key: String,
        #[arg(long)]
        name: String,
        /// One-liner handed to the persona ("what the product is").
        #[arg(long, default_value = "")]
        blurb: String,
        /// Allowed browser origin, e.g. https://example.com (repeatable; none = no check).
        #[arg(long = "origin")]
        origins: Vec<String>,
        /// GitHub repository for approved tickets, owner/repo.
        #[arg(long)]
        repo: String,
    },
    /// List registered sites.
    ListSites,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();
    let cli = Cli::parse();
    let config = Config::from_env()?;
    let pool = PgPoolOptions::new().max_connections(8).connect(&config.database_url).await?;
    intake_server::MIGRATOR.run(&pool).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Admin { action: AdminAction::AddSite { key, name, blurb, origins, repo } } => {
            let site = db::insert_site(&pool, &key, &name, &blurb, &origins, &repo).await?;
            println!(
                "site {} created: key={} repo={} origins={:?}",
                site.id, site.key, site.github_repo, site.origin_allowlist
            );
            Ok(())
        }
        Command::Admin { action: AdminAction::ListSites } => {
            for s in db::list_sites(&pool).await? {
                println!("{}\t{}\t{}\t{}\t{:?}", s.id, s.key, s.name, s.github_repo, s.origin_allowlist);
            }
            Ok(())
        }
        Command::Serve => serve(config, pool).await,
    }
}

async fn serve(config: Config, pool: sqlx::PgPool) -> anyhow::Result<()> {
    let prompts = load_prompts(Path::new(&config.prompts_dir))?;
    let model = AnthropicModel::new(
        config.anthropic_api_key.clone(),
        config.anthropic_base_url.clone(),
        config.intake_model.clone(),
    )?;
    let port = config.port;
    let state = AppState::new(pool, Arc::new(model), prompts, config)?;
    let n = state.refresh_origins().await?;
    tracing::info!(origins = n, model = state.model.name(), prompts = ?state.prompt_versions, "intake-server starting");
    state.spawn_origin_refresher();

    let app = build_app(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler");
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
    tracing::info!("shutting down");
}
