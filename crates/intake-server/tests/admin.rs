mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use intake_server::auth::{cookie_value, OAUTH_STATE_COOKIE, SESSION_COOKIE};
use serde_json::json;
use sqlx::PgPool;
use wiremock::matchers::{body_partial_json, header as hdr, method, path};
use wiremock::{Mock, ResponseTemplate};

fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

async fn seed_draft(t: &TestApp, title: &str) -> i64 {
    t.model.push(persona(
        "What were you doing?",
        "ask_more",
        json!({"kind_hint": "bug", "doing": "x", "expected": "y", "actual": "z"}),
    ));
    t.model.push(triage("bug", title, "Export of invoices errors instead of downloading.", 0.8, None));
    let (id, token) = t.start().await;
    t.message(&id, &token, "export broke").await;
    let e = t.end(&id, &token).await;
    e.json["ticket_id"].as_i64().expect("ticket")
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn oauth_login_only_for_the_configured_admin(pool: PgPool) {
    let t = test_app(pool.clone()).await;

    // 1. /admin/login redirects to GitHub and sets a signed state cookie.
    let r = t.send(get("/admin/login")).await;
    assert_eq!(r.status, StatusCode::TEMPORARY_REDIRECT);
    let location = r.headers.get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    assert!(
        location.starts_with(&format!("{}/login/oauth/authorize?client_id=cid", t.github.uri())),
        "{location}"
    );
    let state_param = location.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let set_cookie = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
    let state_cookie = cookie_value(&set_cookie, OAUTH_STATE_COOKIE).unwrap().to_string();

    // 2. Callback without the cookie, or with a mismatched state, is refused.
    let r = t.send(get(&format!("/admin/callback?code=abc&state={state_param}"))).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = t
        .send(
            Request::builder()
                .uri("/admin/callback?code=abc&state=forged")
                .header(header::COOKIE, format!("{OAUTH_STATE_COOKIE}={state_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // 3. Stubbed GitHub: code → token → user.
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_partial_json(json!({"client_id": "cid", "client_secret": "csecret", "code": "good-code"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "gho_x", "token_type": "bearer"})),
        )
        .mount(&t.github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(hdr("authorization", "Bearer gho_x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": ADMIN, "id": 1})))
        .mount(&t.github)
        .await;
    let r = t
        .send(
            Request::builder()
                .uri(format!("/admin/callback?code=good-code&state={state_param}"))
                .header(header::COOKIE, format!("{OAUTH_STATE_COOKIE}={state_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{:?}", r.json);
    assert_eq!(r.headers.get(header::LOCATION).unwrap(), "/console");
    let cookies: Vec<String> =
        r.headers.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    let session = cookies
        .iter()
        .find_map(|c| cookie_value(c, SESSION_COOKIE).map(str::to_string))
        .expect("session cookie");
    assert!(cookies.iter().any(|c| c.contains("HttpOnly")));

    // 4. The session works.
    let me = t
        .send(
            Request::builder()
                .uri("/admin/me")
                .header(header::COOKIE, format!("{SESSION_COOKIE}={session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["login"], ADMIN);

    // 5. Another GitHub user gets 403 and no cookie.
    t.github.reset().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "gho_y"})))
        .mount(&t.github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "mallory"})))
        .mount(&t.github)
        .await;
    let r2 = t.send(get("/admin/login")).await;
    let loc2 = r2.headers.get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    let st2 = loc2.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let ck2 = cookie_value(r2.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap(), OAUTH_STATE_COOKIE)
        .unwrap()
        .to_string();
    let r = t
        .send(
            Request::builder()
                .uri(format!("/admin/callback?code=any&state={st2}"))
                .header(header::COOKIE, format!("{OAUTH_STATE_COOKIE}={ck2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .all(|c| !c.to_str().unwrap().starts_with(SESSION_COOKIE)));
    let refused = sqlx::query_scalar!("select actor from audit where action = 'admin.login_refused'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(refused, vec!["mallory".to_string()]);
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn admin_routes_need_a_session_for_the_right_login(pool: PgPool) {
    let t = test_app(pool).await;
    assert_eq!(t.send(get("/admin/tickets")).await.status, StatusCode::UNAUTHORIZED);
    let other = Request::builder()
        .uri("/admin/tickets")
        .header(header::COOKIE, t.admin_cookie("mallory"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(t.send(other).await.status, StatusCode::FORBIDDEN);
    let ok = t.send(t.admin_req("GET", "/admin/tickets", None)).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.json["tickets"], json!([]));
    assert_eq!(ok.json["sites"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn approve_creates_the_issue_once(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    let id = seed_draft(&t, "CSV export fails").await;

    let list =
        t.send(t.admin_req("GET", &format!("/admin/tickets?site={}&status=draft", t.site_id), None)).await;
    assert_eq!(list.json["tickets"].as_array().unwrap().len(), 1);
    let other = seed_draft(&t, "CSV export fails on invoices").await;
    let detail = t.send(t.admin_req("GET", &format!("/admin/tickets/{id}"), None)).await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.json["ticket"]["kind"], "bug");
    assert_eq!(detail.json["transcript"].as_array().unwrap().len(), 2);
    assert_eq!(detail.json["conversation"]["page_url"], "https://example.test/app");
    let dups = detail.json["duplicates"].as_array().unwrap();
    assert_eq!(dups.len(), 1, "{dups:?}");
    assert_eq!(dups[0]["id"].as_i64(), Some(other));
    assert_eq!(dups[0]["status"], "draft");
    assert!(dups[0]["score"].as_f64().unwrap() > 0.5);
    assert!(dups[0]["github_issue_url"].is_null());

    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/issues")))
        .and(hdr("authorization", "Bearer ghp_test"))
        .and(body_partial_json(
            json!({"title": "[bug] CSV export fails (edited)", "labels": ["intake", "bug"]}),
        ))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"html_url": "https://github.com/owner/repo/issues/42", "number": 42})),
        )
        .expect(1)
        .mount(&t.github)
        .await;

    // Unknown edit keys are rejected before anything happens.
    let bad = t
        .send(t.admin_req(
            "POST",
            &format!("/admin/tickets/{id}/approve"),
            Some(&json!({"fields": {"action": "open_pr"}})),
        ))
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{:?}", bad.json);

    let edits = json!({"fields": {"title": "CSV export fails (edited)", "severity": "high", "steps": ["Open Invoices", "Export"]}});
    let r = t.send(t.admin_req("POST", &format!("/admin/tickets/{id}/approve"), Some(&edits))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["github_issue_url"], "https://github.com/owner/repo/issues/42");
    let row = sqlx::query!(
        "select status, github_issue_url, decided_at, title, severity, fields_json from tickets where id = $1",
        id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.status, "approved");
    assert_eq!((row.title.as_str(), row.severity.as_str()), ("CSV export fails (edited)", "high"));
    assert_eq!(row.fields_json["steps"], json!(["Open Invoices", "Export"]));
    assert_eq!(row.fields_json["extracted"]["kind_hint"], "bug", "untouched keys survive the merge");
    assert_eq!(row.github_issue_url.as_deref(), Some("https://github.com/owner/repo/issues/42"));
    assert!(row.decided_at.is_some());

    // The body carries the untrusted marker and the queue link.
    let sent = t.github.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&sent[0].body).unwrap();
    let text = body["body"].as_str().unwrap();
    assert!(
        text.contains("> Visitor-provided content, untrusted. Queue entry: http://localhost:8788/console/t/"),
        "{text}"
    );
    assert!(
        text.contains("## Steps") && text.contains("## Expected / Actual") && text.contains("TestBrowser/1")
    );

    // A second approve is a conflict and does not call GitHub again (expect(1) above).
    let again = t.send(t.admin_req("POST", &format!("/admin/tickets/{id}/approve"), Some(&json!({})))).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let audit = sqlx::query_scalar!("select actor from audit where action = 'ticket.approved'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(audit, vec![ADMIN.to_string()]);
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn merge_comments_on_the_target_issue(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    let target = seed_draft(&t, "CSV export fails").await;
    sqlx::query!("update tickets set status = 'approved', github_issue_url = 'https://github.com/owner/repo/issues/42' where id = $1", target)
        .execute(&pool)
        .await
        .unwrap();
    let dup = seed_draft(&t, "Export shows an error").await;

    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/issues/42/comments")))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"html_url": "https://github.com/owner/repo/issues/42#issuecomment-1"})),
        )
        .expect(1)
        .mount(&t.github)
        .await;

    let bad = t
        .send(t.admin_req("POST", &format!("/admin/tickets/{dup}/merge"), Some(&json!({"into": dup}))))
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let r = t
        .send(t.admin_req("POST", &format!("/admin/tickets/{dup}/merge"), Some(&json!({"into": target}))))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let row = sqlx::query!("select status, duplicate_of, github_issue_url from tickets where id = $1", dup)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "merged");
    assert_eq!(row.duplicate_of, Some(target));
    assert_eq!(row.github_issue_url.as_deref(), Some("https://github.com/owner/repo/issues/42"));
}

#[sqlx::test(migrator = "intake_server::MIGRATOR")]
async fn dismiss_needs_a_reason_and_takedown_removes_the_conversation(pool: PgPool) {
    let t = test_app(pool.clone()).await;
    let id = seed_draft(&t, "Noise").await;
    let r = t
        .send(t.admin_req("POST", &format!("/admin/tickets/{id}/dismiss"), Some(&json!({"reason": "  "}))))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = t
        .send(t.admin_req("POST", &format!("/admin/tickets/{id}/dismiss"), Some(&json!({"reason": "spam"}))))
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let row = sqlx::query!("select status, decided_reason, conversation_id from tickets where id = $1", id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((row.status.as_str(), row.decided_reason.as_deref()), ("dismissed", Some("spam")));
    let conv = row.conversation_id.unwrap();

    let r = t.send(t.admin_req("DELETE", &format!("/admin/conversations/{conv}"), None)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let msgs =
        sqlx::query_scalar!(r#"select count(*) as "n!" from messages where conversation_id = $1"#, conv)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msgs, 0);
    let link = sqlx::query_scalar!("select conversation_id from tickets where id = $1", id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(link.is_none(), "ticket keeps its text but loses the link");
    let r = t.send(t.admin_req("DELETE", &format!("/admin/conversations/{conv}"), None)).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let who = sqlx::query_scalar!("select actor from audit where action = 'conversation.deleted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(who, ADMIN);
    assert_eq!(t.github.received_requests().await.unwrap().len(), 0);
}
