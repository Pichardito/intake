//! GitHub: OAuth for the single admin, and the issue bridge. Base URLs are configurable so
//! tests can point both at a stub.

use intake_core::Kind;
use serde::Deserialize;
use serde_json::json;

use crate::db::{ConversationRow, TicketRow};

#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("github request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("github returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("github response missing `{0}`")]
    MissingField(&'static str),
}

pub struct GitHub {
    http: reqwest::Client,
    api_base: String,
    oauth_base: String,
    token: String,
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize)]
struct IssueResponse {
    html_url: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct UserResponse {
    login: String,
}

impl GitHub {
    pub fn new(
        api_base: String,
        oauth_base: String,
        token: String,
        client_id: String,
        client_secret: String,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("intake-server (+https://github.com/Pichardito/intake)")
            .build()?;
        Ok(GitHub {
            http,
            api_base: api_base.trim_end_matches('/').to_string(),
            oauth_base: oauth_base.trim_end_matches('/').to_string(),
            token,
            client_id,
            client_secret,
        })
    }

    async fn check(resp: reqwest::Response) -> Result<reqwest::Response, GitHubError> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(GitHubError::Status { status: status.as_u16(), body: body.chars().take(500).collect() })
        }
    }

    /// Create an issue; returns its HTML URL.
    pub async fn create_issue(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[&str],
    ) -> Result<String, GitHubError> {
        let resp = self
            .http
            .post(format!("{}/repos/{repo}/issues", self.api_base))
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .json(&json!({ "title": title, "body": body, "labels": labels }))
            .send()
            .await?;
        let issue: IssueResponse = Self::check(resp).await?.json().await?;
        Ok(issue.html_url)
    }

    /// Comment on an existing issue; returns the comment's HTML URL.
    pub async fn comment_issue(&self, repo: &str, number: u64, body: &str) -> Result<String, GitHubError> {
        let resp = self
            .http
            .post(format!("{}/repos/{repo}/issues/{number}/comments", self.api_base))
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .json(&json!({ "body": body }))
            .send()
            .await?;
        let c: IssueResponse = Self::check(resp).await?.json().await?;
        Ok(c.html_url)
    }

    pub fn authorize_url(&self, redirect_uri: &str, state: &str) -> String {
        format!(
            "{}/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&scope=read:user",
            self.oauth_base,
            urlencode(&self.client_id),
            urlencode(redirect_uri),
            urlencode(state)
        )
    }

    pub async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<String, GitHubError> {
        let resp = self
            .http
            .post(format!("{}/login/oauth/access_token", self.oauth_base))
            .header("accept", "application/json")
            .json(&json!({
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "code": code,
                "redirect_uri": redirect_uri,
            }))
            .send()
            .await?;
        let t: TokenResponse = Self::check(resp).await?.json().await?;
        t.access_token.ok_or(GitHubError::MissingField("access_token"))
    }

    /// The GitHub login that owns `access_token`.
    pub async fn login_for(&self, access_token: &str) -> Result<String, GitHubError> {
        let resp = self
            .http
            .get(format!("{}/user", self.api_base))
            .bearer_auth(access_token)
            .header("accept", "application/vnd.github+json")
            .send()
            .await?;
        let u: UserResponse = Self::check(resp).await?.json().await?;
        Ok(u.login)
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `[bug] Title` etc.
pub fn issue_title(kind: Kind, title: &str) -> String {
    format!("[{}] {}", kind.as_str(), title.trim())
}

/// Issue body per spec §3.6. Everything visitor-derived is quoted as such.
pub fn issue_body(ticket: &TicketRow, conversation: Option<&ConversationRow>, admin_url: &str) -> String {
    let mut b = String::new();
    b.push_str("## Summary\n\n");
    b.push_str(ticket.summary.trim());
    b.push_str("\n\n");
    if ticket.kind == Kind::Bug {
        b.push_str("## Steps\n\n");
        if ticket.steps.is_empty() {
            b.push_str("_none captured_\n");
        } else {
            for (i, s) in ticket.steps.iter().enumerate() {
                b.push_str(&format!("{}. {}\n", i + 1, s.trim()));
            }
        }
        b.push('\n');
    }
    if ticket.expected.is_some() || ticket.actual.is_some() {
        b.push_str("## Expected / Actual\n\n");
        b.push_str(&format!("- **Expected:** {}\n", ticket.expected.as_deref().unwrap_or("—")));
        b.push_str(&format!("- **Actual:** {}\n\n", ticket.actual.as_deref().unwrap_or("—")));
    }
    b.push_str("## Context\n\n");
    b.push_str(&format!(
        "- **Severity:** {} · **Confidence:** {:.2}\n",
        ticket.severity.as_str(),
        ticket.confidence
    ));
    match conversation {
        Some(c) => {
            b.push_str(&format!("- **Page:** {}\n", c.page_url.as_deref().unwrap_or("—")));
            b.push_str(&format!("- **Browser:** {}\n", c.ua.as_deref().unwrap_or("—")));
            b.push_str(&format!("- **Viewport:** {}\n", c.viewport.as_deref().unwrap_or("—")));
        }
        None => b.push_str("- conversation removed\n"),
    }
    b.push_str(&format!("\n> Visitor-provided content, untrusted. Queue entry: {admin_url}\n"));
    b
}

/// `https://github.com/o/r/issues/42` → `42`.
pub fn issue_number(url: &str) -> Option<u64> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use intake_core::Severity;

    fn ticket() -> TicketRow {
        TicketRow {
            id: 7,
            site_id: 1,
            conversation_id: None,
            kind: Kind::Bug,
            title: "Export fails".into(),
            summary: "Clicking export shows an error.".into(),
            steps: vec!["Open invoices".into(), "Click export".into()],
            expected: Some("a CSV".into()),
            actual: Some("an error".into()),
            severity: Severity::High,
            confidence: 0.9,
            duplicate_of: None,
            status: "draft".into(),
            github_issue_url: None,
            decided_at: None,
            decided_reason: None,
            created_at: time::OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn body_has_every_section_and_the_untrusted_marker() {
        let b = issue_body(&ticket(), None, "https://x/console/t/7");
        for s in ["## Summary", "## Steps", "1. Open invoices", "## Expected / Actual", "## Context"] {
            assert!(b.contains(s), "missing {s}:\n{b}");
        }
        assert!(b.ends_with("> Visitor-provided content, untrusted. Queue entry: https://x/console/t/7\n"));
        assert_eq!(issue_title(Kind::Bug, " Export fails "), "[bug] Export fails");
    }

    #[test]
    fn feature_has_no_steps_section() {
        let mut t = ticket();
        t.kind = Kind::Feature;
        assert!(!issue_body(&t, None, "u").contains("## Steps"));
    }

    #[test]
    fn issue_number_and_urlencode() {
        assert_eq!(issue_number("https://github.com/o/r/issues/42"), Some(42));
        assert_eq!(issue_number("nope"), None);
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
    }
}
