mod common;

use axum::http::{header, StatusCode};
use common::*;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn three_turns_then_draft_ticket(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    t.model.push(persona(
        "Sorry about that. What were you doing?",
        "ask_more",
        json!({"kind_hint": "bug", "actual": "export errored"}),
    ));
    t.model.push(persona(
        "What did you expect to happen?",
        "ask_more",
        json!({"doing": "exporting invoices"}),
    ));
    t.model.push(persona("Thanks, we've got it.", "enough", json!({"expected": "a CSV download"})));
    t.model.push(triage(
        "bug",
        "CSV export fails",
        "Export of invoices errors instead of downloading.",
        0.85,
        None,
    ));

    let (id, token) = t.start().await;
    let r1 = t.message(&id, &token, "the export is broken").await;
    assert_eq!(r1.status, StatusCode::OK, "{:?}", r1.json);
    assert_eq!((r1.json["state"].as_str(), r1.json["asked"].as_u64()), (Some("clarifying"), Some(1)));
    assert_eq!(r1.json["done"], false);
    let r2 = t.message(&id, &token, "I was exporting invoices").await;
    assert_eq!((r2.json["state"].as_str(), r2.json["asked"].as_u64()), (Some("clarifying"), Some(2)));
    let r3 = t.message(&id, &token, "I expected a CSV download").await;
    assert_eq!(r3.status, StatusCode::OK, "{:?}", r3.json);
    assert_eq!(r3.json["state"], "triaged");
    assert_eq!(r3.json["done"], true);
    let ticket_id = r3.json["ticket_id"].as_i64().expect("ticket id");

    let row =
        sqlx::query!("select kind, status, title, confidence, site_id from tickets where id = $1", ticket_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((row.kind.as_str(), row.status.as_str()), ("bug", "draft"));
    assert_eq!(row.title, "CSV export fails");
    assert!((row.confidence - 0.85).abs() < 1e-6);
    assert_eq!(row.site_id, t.site_id);

    let msgs = sqlx::query!(
        "select role from messages where conversation_id = $1::uuid order by id",
        uuid::Uuid::parse_str(&id).unwrap()
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        msgs.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
        ["user", "assistant", "user", "assistant", "user", "assistant"]
    );

    let state = sqlx::query_scalar!(
        "select state from conversations where id = $1::uuid",
        uuid::Uuid::parse_str(&id).unwrap()
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "triaged");

    let audits = sqlx::query_scalar!("select action from audit order by id").fetch_all(&pool).await.unwrap();
    assert!(audits.contains(&"conversation.started".to_string()));
    assert!(audits.contains(&"ticket.drafted".to_string()));
    assert!(audits.contains(&"conversation.triaged".to_string()));

    // Persona then triage: exactly four model calls, no retries.
    let reqs = t.model.requests();
    assert_eq!(
        reqs.iter().map(|r| r.purpose).collect::<Vec<_>>(),
        ["persona", "persona", "persona", "triage"]
    );
    assert!(reqs[0].system.contains("You are a product researcher for Example"));
    assert!(reqs[3].system.contains("# Triage"));

    // end is idempotent after triage
    let e = t.end(&id, &token).await;
    assert_eq!(e.status, StatusCode::OK);
    assert_eq!(e.json["ticket_id"].as_i64(), Some(ticket_id));
    // and no more messages are accepted
    let late = t.message(&id, &token, "one more thing").await;
    assert_eq!(late.status, StatusCode::CONFLICT);
    assert_eq!(t.github.received_requests().await.unwrap().len(), 0, "triage must never touch GitHub");
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn persona_is_capped_at_three_questions(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    // The model keeps asking and never fills a field.
    for _ in 0..4 {
        t.model.push(persona("Tell me more?", "ask_more", json!({"kind_hint": "bug"})));
    }
    t.model.push(triage("bug", "Vague report", "Not enough detail.", 0.3, None));
    let (id, token) = t.start().await;
    let states: Vec<_> = {
        let mut v = vec![];
        for i in 0..4 {
            let r = t.message(&id, &token, &format!("message {i}")).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            v.push((r.json["state"].as_str().unwrap().to_string(), r.json["asked"].as_u64().unwrap()));
        }
        v
    };
    assert_eq!(states[0], ("clarifying".to_string(), 1));
    assert_eq!(states[1], ("clarifying".to_string(), 2));
    assert_eq!(states[2], ("clarifying".to_string(), 3));
    assert_eq!(states[3], ("triaged".to_string(), 0));
    let purposes: Vec<_> = t.model.requests().iter().map(|r| r.purpose).collect();
    assert_eq!(purposes, ["persona", "persona", "persona", "persona", "triage"]);
    let n = sqlx::query_scalar!(r#"select count(*) as "n!" from tickets"#).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn end_runs_triage_and_empty_end_makes_no_ticket(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    // explicit end after one turn
    t.model.push(persona("Who would use this?", "ask_more", json!({"kind_hint": "feature"})));
    t.model.push(triage("feature", "Dark mode", "Visitor wants a dark theme.", 0.6, None));
    let (id, token) = t.start().await;
    t.message(&id, &token, "please add dark mode").await;
    let e = t.end(&id, &token).await;
    assert_eq!(e.status, StatusCode::OK, "{:?}", e.json);
    assert!(e.json["ticket_id"].is_i64());
    assert_eq!(e.json["state"], "triaged");

    // end with no messages
    let (id2, token2) = t.start().await;
    let e2 = t.end(&id2, &token2).await;
    assert_eq!(e2.status, StatusCode::OK);
    assert!(e2.json["ticket_id"].is_null());
    let n = sqlx::query_scalar!(r#"select count(*) as "n!" from tickets"#).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn auth_and_input_guards(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    let (id, token) = t.start().await;

    let r = t.message(&id, "wrong-token", "hi").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = t.send(t.post(&format!("/v1/conversations/{id}/messages"), &json!({"text": "hi"}))).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "missing bearer");
    let r = t.message("00000000-0000-0000-0000-000000000000", &token, "hi").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = t.message(&id, &token, "   ").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 4 KB body cap
    let big = "x".repeat(5000);
    let r = t.message(&id, &token, &big).await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);

    // unknown site key
    let r = t.send(t.post("/v1/conversations", &json!({"site_key": "nope"}))).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(t.model.requests().len(), 0, "no model call for rejected requests");
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn origin_allowlist_and_cors(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    let body = json!({"site_key": SITE_KEY});
    let mut bad = t.post("/v1/conversations", &body);
    bad.headers_mut().insert(header::ORIGIN, "https://evil.test".parse().unwrap());
    assert_eq!(t.send(bad).await.status, StatusCode::FORBIDDEN);
    let mut none = t.post("/v1/conversations", &body);
    none.headers_mut().remove(header::ORIGIN);
    assert_eq!(t.send(none).await.status, StatusCode::FORBIDDEN);
    assert_eq!(t.send(t.post("/v1/conversations", &body)).await.status, StatusCode::CREATED);

    // CORS preflight reflects only allowlisted origins
    let preflight = |origin: &str| {
        axum::http::Request::builder()
            .method("OPTIONS")
            .uri("/v1/conversations")
            .header(header::ORIGIN, origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "content-type")
            .body(axum::body::Body::empty())
            .unwrap()
    };
    let ok = t.send(preflight(SITE_ORIGIN)).await;
    assert_eq!(
        ok.headers.get("access-control-allow-origin").and_then(|v| v.to_str().ok()),
        Some(SITE_ORIGIN)
    );
    let no = t.send(preflight("https://evil.test")).await;
    assert!(no.headers.get("access-control-allow-origin").is_none());
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn health_reports_prompt_versions(pool: PgPool) {
    let t = test_app(pool).await;
    let r = t
        .send(axum::http::Request::builder().uri("/v1/health").body(axum::body::Body::empty()).unwrap())
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["ok"], true);
    assert_eq!(r.json["model"], "fake");
    assert!(r.json["prompts"]["persona"].as_str().is_some_and(|v| v != "unversioned"));
}
