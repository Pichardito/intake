//! The real client against a stubbed Messages API: request shape and error mapping.

use intake_core::{Message, Role};
use intake_server::model::{AnthropicModel, Model, ModelError, ModelRequest};
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request() -> ModelRequest {
    ModelRequest::from_messages(
        vec![
            Message { role: Role::System, content: "be brief".into() },
            Message { role: Role::User, content: "hello".into() },
        ],
        64,
        "persona",
    )
}

#[tokio::test]
async fn sends_the_documented_shape_and_joins_text_blocks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(body_partial_json(json!({
            "model": "claude-haiku-5-5", "max_tokens": 64, "system": "be brief",
            "messages": [{"role": "user", "content": "hello"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
            "content": [{"type": "text", "text": "Hi "}, {"type": "text", "text": "there"}],
            "stop_reason": "end_turn"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let m = AnthropicModel::new("sk-test".into(), server.uri(), "claude-haiku-5-5".into()).unwrap();
    assert_eq!(m.complete(request()).await.unwrap(), "Hi there");
    assert_eq!(m.name(), "claude-haiku-5-5");
}

#[tokio::test]
async fn maps_status_errors_and_refusals() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_string("overloaded"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let m = AnthropicModel::new("k".into(), server.uri(), "x".into()).unwrap();
    match m.complete(request()).await {
        Err(ModelError::Status { status: 529, body }) => assert_eq!(body, "overloaded"),
        other => panic!("{other:?}"),
    }
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [], "stop_reason": "refusal", "stop_details": {"type": "refusal", "category": "general_harms"}
        })))
        .mount(&server)
        .await;
    assert!(matches!(m.complete(request()).await, Err(ModelError::Refused)));
}
