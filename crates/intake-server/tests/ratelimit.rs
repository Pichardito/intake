mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn eleventh_start_per_minute_from_one_client_is_refused(pool: PgPool) {
    let t = test_app(pool).await;
    let body = json!({"site_key": SITE_KEY});
    for i in 0..10 {
        let r = t.send(t.post("/v1/conversations", &body)).await;
        assert_eq!(r.status, StatusCode::CREATED, "request {i}");
    }
    let r = t.send(t.post("/v1/conversations", &body)).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.json["error"].as_str().unwrap().contains("too many"));

    // Another client address is unaffected.
    let mut other = t.post("/v1/conversations", &body);
    other.headers_mut().insert("x-forwarded-for", "198.51.100.9".parse().unwrap());
    assert_eq!(t.send(other).await.status, StatusCode::CREATED);
}
