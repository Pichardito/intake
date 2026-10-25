//! Domain types shared by the server and the tests.
//!
//! Everything here is plain data: `serde` round-trips, no I/O, no clocks.

use serde::{Deserialize, Serialize};

/// What a conversation turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bug,
    Feature,
    Feedback,
    Praise,
    Noise,
}

impl Kind {
    /// Stable lowercase name, used for GitHub labels and the `tickets.kind` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Bug => "bug",
            Kind::Feature => "feature",
            Kind::Feedback => "feedback",
            Kind::Praise => "praise",
            Kind::Noise => "noise",
        }
    }
}

impl std::str::FromStr for Kind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bug" => Ok(Kind::Bug),
            "feature" => Ok(Kind::Feature),
            "feedback" => Ok(Kind::Feedback),
            "praise" => Ok(Kind::Praise),
            "noise" => Ok(Kind::Noise),
            other => Err(format!("unknown kind `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Severity::Low),
            "medium" => Ok(Severity::Medium),
            "high" => Ok(Severity::High),
            other => Err(format!("unknown severity `{other}`")),
        }
    }
}

/// Where a conversation is. The machine in [`crate::state`] owns every transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    /// No visitor turn yet.
    Open,
    /// The persona has asked `asked` follow-up questions so far (1..=3).
    Clarifying { asked: u8 },
    /// Enough gathered (or the cap hit); triage runs next.
    Closing,
    /// A draft ticket exists for this conversation.
    Triaged,
}

impl State {
    /// Compact column form, e.g. `clarifying:2`. Inverse of [`State::parse_column`].
    pub fn to_column(self) -> String {
        match self {
            State::Open => "open".into(),
            State::Clarifying { asked } => format!("clarifying:{asked}"),
            State::Closing => "closing".into(),
            State::Triaged => "triaged".into(),
        }
    }

    pub fn parse_column(s: &str) -> Result<State, String> {
        match s {
            "open" => Ok(State::Open),
            "closing" => Ok(State::Closing),
            "triaged" => Ok(State::Triaged),
            other => match other.strip_prefix("clarifying:") {
                Some(n) => n
                    .parse::<u8>()
                    .map(|asked| State::Clarifying { asked })
                    .map_err(|e| format!("bad clarifying count `{n}`: {e}")),
                None => Err(format!("unknown state `{other}`")),
            },
        }
    }

    /// The visitor may still send messages.
    pub fn accepts_messages(self) -> bool {
        matches!(self, State::Open | State::Clarifying { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisitorInput {
    pub text: String,
    #[serde(default)]
    pub source: InputSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSource {
    #[default]
    Text,
    /// Speech-to-text output (the voice seam); `confidence` is 0..=1.
    Transcript { confidence: f32 },
}

/// Fields the persona fills in across turns. `None` = not yet known.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extracted {
    #[serde(default)]
    pub kind_hint: Option<Kind>,
    /// Bug: what were you doing.
    #[serde(default)]
    pub doing: Option<String>,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub actual: Option<String>,
    /// Feature: who benefits.
    #[serde(default)]
    pub benefit: Option<String>,
    /// Feature: how often.
    #[serde(default)]
    pub frequency: Option<String>,
}

impl Extracted {
    /// Later turns refine earlier ones: a `Some` in `newer` wins, a `None` keeps the old value.
    /// Empty strings count as `None` so the model cannot "fill" a field with nothing.
    pub fn merge(&self, newer: &Extracted) -> Extracted {
        fn pick(old: &Option<String>, new: &Option<String>) -> Option<String> {
            match new.as_deref().map(str::trim) {
                Some(s) if !s.is_empty() => Some(s.to_string()),
                _ => old.clone(),
            }
        }
        Extracted {
            kind_hint: newer.kind_hint.or(self.kind_hint),
            doing: pick(&self.doing, &newer.doing),
            expected: pick(&self.expected, &newer.expected),
            actual: pick(&self.actual, &newer.actual),
            benefit: pick(&self.benefit, &newer.benefit),
            frequency: pick(&self.frequency, &newer.frequency),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateHint {
    AskMore,
    Enough,
}

/// One parsed persona turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonaReply {
    pub text: String,
    pub state_hint: StateHint,
    pub extracted: Extracted,
}

/// The triage schema. This is the *only* thing the model's second call can produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Triage {
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub actual: Option<String>,
    pub severity: Severity,
    pub confidence: f32,
    #[serde(default)]
    pub duplicate_of: Option<i64>,
}

/// An open ticket as seen by the dedup step: just enough text to compare.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TicketStub {
    pub id: i64,
    pub title: String,
    pub summary: String,
}

/// A chat role in the shape the Messages API wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// One message in a model request. The server lifts `System` into the API's `system` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// A stored turn of a conversation (already scrubbed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}
