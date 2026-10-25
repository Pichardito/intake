#![allow(dead_code)]
//! Shared harness: a router over a per-test database, a scripted model and a stubbed GitHub.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use intake_server::auth::{encode_session, Session, SESSION_COOKIE};
use intake_server::{build_app, db, load_prompts, AppState, Config, FakeModel};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use wiremock::MockServer;

pub const SITE_KEY: &str = "site-1";
pub const SITE_ORIGIN: &str = "https://example.test";
pub const REPO: &str = "owner/repo";
pub const ADMIN: &str = "octocat";
pub const SECRET: &str = "test-session-secret-at-least-32-chars!!";

pub struct TestApp {
    pub app: Router,
    pub state: AppState,
    pub model: Arc<FakeModel>,
    pub github: MockServer,
    pub site_id: i64,
}

pub async fn test_app(pool: PgPool) -> TestApp {
    let github = MockServer::start().await;
    let config = Config {
        database_url: String::new(),
        anthropic_api_key: "test".into(),
        anthropic_base_url: "http://127.0.0.1:9".into(),
        intake_model: "fake".into(),
        github_client_id: "cid".into(),
        github_client_secret: "csecret".into(),
        github_token: "ghp_test".into(),
        github_api_base: github.uri(),
        github_oauth_base: github.uri(),
        admin_github_login: ADMIN.into(),
        session_secret: SECRET.into(),
        public_origin: "http://localhost:8788".into(),
        port: 0,
        prompts_dir: String::new(),
        console_dist: "/nonexistent/console".into(),
        widget_dist: "/nonexistent/widget".into(),
        trust_proxy: true,
    };
    let prompts =
        load_prompts(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../prompts")).expect("prompts");
    let model = Arc::new(FakeModel::default());
    let state = AppState::new(pool.clone(), model.clone(), prompts, config).expect("state");
    let site =
        db::insert_site(&pool, SITE_KEY, "Example", "an example product", &[SITE_ORIGIN.to_string()], REPO)
            .await
            .expect("site");
    state.refresh_origins().await.expect("origins");
    TestApp { app: build_app(state.clone()), state, model, github, site_id: site.id }
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub json: Value,
}

impl TestApp {
    pub async fn send(&self, req: Request<Body>) -> Resp {
        let resp = self.app.clone().oneshot(req).await.expect("infallible");
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Resp { status, headers, json }
    }

    pub fn post(&self, path: &str, body: &Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, SITE_ORIGIN)
            .header("x-forwarded-for", "203.0.113.7")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    pub fn post_auth(&self, path: &str, token: &str, body: &Value) -> Request<Body> {
        let mut req = self.post(path, body);
        req.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        req
    }

    /// Start a conversation; returns `(id, token)`.
    pub async fn start(&self) -> (String, String) {
        self.start_from("203.0.113.7").await
    }

    /// Start from a given client address (the start bucket is 10/min per address + site).
    pub async fn start_from(&self, ip: &str) -> (String, String) {
        let mut req = self.post(
            "/v1/conversations",
            &json!({ "site_key": SITE_KEY, "page_url": "https://example.test/app", "ua": "TestBrowser/1", "viewport": "1280x800" }),
        );
        req.headers_mut().insert("x-forwarded-for", ip.parse().unwrap());
        let r = self.send(req).await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
        (r.json["id"].as_str().unwrap().to_string(), r.json["token"].as_str().unwrap().to_string())
    }

    pub async fn message(&self, id: &str, token: &str, text: &str) -> Resp {
        self.send(self.post_auth(
            &format!("/v1/conversations/{id}/messages"),
            token,
            &json!({ "text": text }),
        ))
        .await
    }

    pub async fn end(&self, id: &str, token: &str) -> Resp {
        self.send(self.post_auth(&format!("/v1/conversations/{id}/end"), token, &json!({}))).await
    }

    pub fn admin_cookie(&self, login: &str) -> String {
        let exp = time::OffsetDateTime::now_utc().unix_timestamp() + 3600;
        format!("{SESSION_COOKIE}={}", encode_session(&Session { login: login.into(), exp }, SECRET))
    }

    pub fn admin_req(&self, method: &str, path: &str, body: Option<&Value>) -> Request<Body> {
        let b = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, self.admin_cookie(ADMIN))
            .header(header::CONTENT_TYPE, "application/json");
        b.body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap()
    }
}

/// A well-formed persona reply.
pub fn persona(text: &str, hint: &str, extracted: Value) -> String {
    format!("{text}\n\n```json\n{}\n```", json!({ "state_hint": hint, "extracted": extracted }))
}

/// A well-formed triage result.
pub fn triage(kind: &str, title: &str, summary: &str, confidence: f64, duplicate_of: Option<i64>) -> String {
    json!({
        "kind": kind, "title": title, "summary": summary,
        "steps": if kind == "bug" { json!(["Open the page", "Click export"]) } else { json!([]) },
        "expected": if kind == "bug" { json!("a download") } else { Value::Null },
        "actual": if kind == "bug" { json!("an error") } else { Value::Null },
        "severity": "medium", "confidence": confidence, "duplicate_of": duplicate_of,
    })
    .to_string()
}
