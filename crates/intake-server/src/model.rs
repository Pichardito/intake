//! The `Model` seam. The server talks to "a model"; production uses the Messages API over
//! plain HTTPS, tests use a scripted fake. Nothing else in the server knows which.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use intake_core::{Message, Role};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("model returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("model refused the request")]
    Refused,
    #[error("model returned no text")]
    Empty,
    #[error("fake model has no scripted reply left")]
    NoScript,
}

/// One call. `system` is lifted out of the core's message list; `messages` alternate
/// user/assistant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    /// `"persona"` or `"triage"`; for logs and the fake's bookkeeping.
    pub purpose: &'static str,
}

impl ModelRequest {
    /// Split the core's messages into the API's `system` field and the chat turns.
    pub fn from_messages(messages: Vec<Message>, max_tokens: u32, purpose: &'static str) -> ModelRequest {
        let mut system = String::new();
        let mut rest = Vec::with_capacity(messages.len());
        for m in messages {
            match m.role {
                Role::System => {
                    if !system.is_empty() {
                        system.push_str("\n\n");
                    }
                    system.push_str(&m.content);
                }
                _ => rest.push(m),
            }
        }
        ModelRequest { system, messages: rest, max_tokens, purpose }
    }
}

#[async_trait]
pub trait Model: Send + Sync {
    async fn complete(&self, req: ModelRequest) -> Result<String, ModelError>;
    fn name(&self) -> &str;
}

/// Messages API client: `POST {base}/v1/messages`. No SDK; the request is small JSON.
pub struct AnthropicModel {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<ApiMessage<'a>>,
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    content: Vec<ApiContent>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

impl AnthropicModel {
    pub const TIMEOUT: Duration = Duration::from_secs(20);

    pub fn new(api_key: String, base_url: String, model: String) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(Self::TIMEOUT).build()?;
        Ok(AnthropicModel { http, api_key, base_url: base_url.trim_end_matches('/').to_string(), model })
    }
}

#[async_trait]
impl Model for AnthropicModel {
    async fn complete(&self, req: ModelRequest) -> Result<String, ModelError> {
        let body = ApiRequest {
            model: &self.model,
            max_tokens: req.max_tokens,
            system: &req.system,
            messages: req
                .messages
                .iter()
                .map(|m| ApiMessage {
                    role: match m.role {
                        Role::Assistant => "assistant",
                        _ => "user",
                    },
                    content: &m.content,
                })
                .collect(),
        };
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ModelError::Status {
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }
        let parsed: ApiResponse = resp.json().await?;
        if parsed.stop_reason.as_deref() == Some("refusal") {
            return Err(ModelError::Refused);
        }
        let text: String = parsed
            .content
            .iter()
            .filter(|c| c.kind == "text")
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            return Err(ModelError::Empty);
        }
        Ok(text)
    }

    fn name(&self) -> &str {
        &self.model
    }
}

/// Scripted model for tests: returns replies in order and records every request it saw.
#[derive(Default)]
pub struct FakeModel {
    replies: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl FakeModel {
    pub fn scripted<I, S>(replies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        FakeModel {
            replies: Mutex::new(replies.into_iter().map(Into::into).collect()),
            requests: Mutex::default(),
        }
    }

    pub fn push(&self, reply: impl Into<String>) {
        self.replies.lock().expect("fake model lock").push_back(reply.into());
    }

    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().expect("fake model lock").clone()
    }

    pub fn remaining(&self) -> usize {
        self.replies.lock().expect("fake model lock").len()
    }
}

#[async_trait]
impl Model for FakeModel {
    async fn complete(&self, req: ModelRequest) -> Result<String, ModelError> {
        self.requests.lock().expect("fake model lock").push(req);
        self.replies.lock().expect("fake model lock").pop_front().ok_or(ModelError::NoScript)
    }

    fn name(&self) -> &str {
        "fake"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_is_lifted_out_of_the_message_list() {
        let msgs = vec![
            Message { role: Role::System, content: "rules".into() },
            Message { role: Role::User, content: "hi".into() },
            Message { role: Role::Assistant, content: "hello".into() },
        ];
        let r = ModelRequest::from_messages(msgs, 100, "persona");
        assert_eq!(r.system, "rules");
        assert_eq!(r.messages.len(), 2);
        assert_eq!(r.messages[0].role, Role::User);
    }

    #[tokio::test]
    async fn fake_model_is_in_order_and_records() {
        let fake = FakeModel::scripted(["a", "b"]);
        let req = ModelRequest { system: "s".into(), messages: vec![], max_tokens: 1, purpose: "persona" };
        assert_eq!(fake.complete(req.clone()).await.unwrap(), "a");
        assert_eq!(fake.complete(req.clone()).await.unwrap(), "b");
        assert!(matches!(fake.complete(req).await, Err(ModelError::NoScript)));
        assert_eq!(fake.requests().len(), 3);
    }
}
