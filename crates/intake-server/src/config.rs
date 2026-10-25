//! Environment configuration. Every required variable is checked up front so a misconfigured
//! deploy fails at boot with the full list of what is missing, not on the first request.

use std::env;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variables: {}", .0.join(", "))]
    Missing(Vec<String>),
    #[error("{name} is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub anthropic_api_key: String,
    pub anthropic_base_url: String,
    pub intake_model: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_token: String,
    pub github_api_base: String,
    pub github_oauth_base: String,
    pub admin_github_login: String,
    pub session_secret: String,
    pub public_origin: String,
    pub port: u16,
    pub prompts_dir: String,
    pub console_dist: String,
    pub widget_dist: String,
    /// Trust `fly-client-ip` / `x-forwarded-for` for the client address (set behind a proxy).
    pub trust_proxy: bool,
}

const REQUIRED: &[&str] = &[
    "DATABASE_URL",
    "ANTHROPIC_API_KEY",
    "GITHUB_CLIENT_ID",
    "GITHUB_CLIENT_SECRET",
    "GITHUB_TOKEN",
    "ADMIN_GITHUB_LOGIN",
    "SESSION_SECRET",
    "PUBLIC_ORIGIN",
];

impl Config {
    pub fn from_env() -> Result<Config, ConfigError> {
        let missing: Vec<String> = REQUIRED
            .iter()
            .filter(|k| env::var(k).map(|v| v.trim().is_empty()).unwrap_or(true))
            .map(|k| k.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(ConfigError::Missing(missing));
        }
        let get = |k: &str| env::var(k).unwrap_or_default();
        let or = |k: &str, d: &str| {
            env::var(k).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string())
        };

        let session_secret = get("SESSION_SECRET");
        if session_secret.len() < 32 {
            return Err(ConfigError::Invalid {
                name: "SESSION_SECRET",
                reason: "must be at least 32 characters".into(),
            });
        }
        let port = or("PORT", "8788")
            .parse::<u16>()
            .map_err(|e| ConfigError::Invalid { name: "PORT", reason: e.to_string() })?;
        let public_origin = get("PUBLIC_ORIGIN").trim_end_matches('/').to_string();
        if !(public_origin.starts_with("http://") || public_origin.starts_with("https://")) {
            return Err(ConfigError::Invalid {
                name: "PUBLIC_ORIGIN",
                reason: "must start with http:// or https://".into(),
            });
        }

        Ok(Config {
            database_url: get("DATABASE_URL"),
            anthropic_api_key: get("ANTHROPIC_API_KEY"),
            anthropic_base_url: or("ANTHROPIC_BASE_URL", "https://api.anthropic.com"),
            intake_model: or("INTAKE_MODEL", "claude-haiku-5-5"),
            github_client_id: get("GITHUB_CLIENT_ID"),
            github_client_secret: get("GITHUB_CLIENT_SECRET"),
            github_token: get("GITHUB_TOKEN"),
            github_api_base: or("GITHUB_API_BASE", "https://api.github.com"),
            github_oauth_base: or("GITHUB_OAUTH_BASE", "https://github.com"),
            admin_github_login: get("ADMIN_GITHUB_LOGIN"),
            session_secret,
            public_origin,
            port,
            prompts_dir: or("PROMPTS_DIR", "prompts"),
            console_dist: or("CONSOLE_DIST", "console/dist"),
            widget_dist: or("WIDGET_DIST", "widget/dist"),
            trust_proxy: matches!(or("TRUST_PROXY", "false").as_str(), "1" | "true" | "yes"),
        })
    }

    pub fn secure_cookies(&self) -> bool {
        self.public_origin.starts_with("https://")
    }
}
