//! `intake-core`: the pure heart of the feedback-intake persona.
//!
//! No I/O, no async, no clocks, no randomness. The server crate feeds it text and gets
//! back decisions; that is what makes every rule here unit-testable.
//!
//! * [`state`] — the conversation state machine (the persona asks at most three questions).
//! * [`prompt`] — assembles model messages; visitor text only ever appears inside the
//!   untrusted block.
//! * [`parse`] — strict parsing of model output; the model returns a schema, never an action.
//! * [`dedup`] — duplicate scoring against open tickets.

#![forbid(unsafe_code)]

pub mod dedup;
pub mod parse;
pub mod prompt;
pub mod state;
pub mod types;

pub use dedup::{rank_duplicates, resolve_duplicate};
pub use parse::{fallback_triage, parse_persona, parse_triage, ParseError};
pub use prompt::{persona_messages, triage_messages, untrusted_block, PersonaCtx, PromptSet, TriageCtx};
pub use state::{advance, required_missing, MAX_QUESTIONS};
pub use types::*;
