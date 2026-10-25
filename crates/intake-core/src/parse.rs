//! Strict parsing of model output. Anything that is not exactly the expected shape is an
//! error; the caller decides whether to retry or fall back.

use serde::Deserialize;

use crate::types::{Extracted, Kind, PersonaReply, Severity, StateHint, Triage};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    #[error("no fenced ```json block found")]
    MissingJsonBlock,
    #[error("the reply text before the JSON block is empty")]
    MissingReply,
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("confidence {0} is outside 0..=1")]
    ConfidenceOutOfRange(f32),
    #[error("title is empty")]
    EmptyTitle,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersonaJson {
    state_hint: StateHint,
    #[serde(default)]
    extracted: Extracted,
}

/// Split `raw` into the prose before the last fenced ```json block and the block's body.
fn split_fenced_json(raw: &str) -> Result<(&str, &str), ParseError> {
    let open = raw.rfind("```json").ok_or(ParseError::MissingJsonBlock)?;
    let after = &raw[open + "```json".len()..];
    let close = after.find("```").ok_or(ParseError::MissingJsonBlock)?;
    Ok((raw[..open].trim(), after[..close].trim()))
}

/// A persona turn: a reply paragraph followed by a fenced ```json block holding
/// `{"state_hint": "ask_more"|"enough", "extracted": {...}}`.
pub fn parse_persona(raw: &str) -> Result<PersonaReply, ParseError> {
    let (text, json) = split_fenced_json(raw)?;
    if text.is_empty() {
        return Err(ParseError::MissingReply);
    }
    let parsed: PersonaJson =
        serde_json::from_str(json).map_err(|e| ParseError::InvalidJson(e.to_string()))?;
    Ok(PersonaReply { text: text.to_string(), state_hint: parsed.state_hint, extracted: parsed.extracted })
}

/// A triage result: a JSON object, either bare or inside a fenced ```json block. Unknown
/// fields, a missing required field, an empty title, or a confidence outside 0..=1 are errors.
pub fn parse_triage(raw: &str) -> Result<Triage, ParseError> {
    let json = match split_fenced_json(raw) {
        Ok((_, json)) => json,
        Err(_) => raw.trim(),
    };
    let t: Triage = serde_json::from_str(json).map_err(|e| ParseError::InvalidJson(e.to_string()))?;
    if !(0.0..=1.0).contains(&t.confidence) || t.confidence.is_nan() {
        return Err(ParseError::ConfidenceOutOfRange(t.confidence));
    }
    if t.title.trim().is_empty() {
        return Err(ParseError::EmptyTitle);
    }
    Ok(t)
}

/// What the server stores when the model failed to produce a valid triage twice: `Noise`
/// with zero confidence and the raw output as the summary, for a human to read.
pub fn fallback_triage(raw: &str) -> Triage {
    const MAX: usize = 4000;
    let mut summary: String = raw.chars().take(MAX).collect();
    if raw.chars().count() > MAX {
        summary.push('…');
    }
    Triage {
        kind: Kind::Noise,
        title: "Unparseable triage output".into(),
        summary,
        steps: vec![],
        expected: None,
        actual: None,
        severity: Severity::Low,
        confidence: 0.0,
        duplicate_of: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixtures(sub: &str) -> Vec<(PathBuf, String)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(sub);
        let mut files: Vec<_> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no fixtures in {}", dir.display());
        files.into_iter().map(|p| (p.clone(), fs::read_to_string(&p).unwrap())).collect()
    }

    fn expected_for(txt: &Path) -> Option<String> {
        fs::read_to_string(txt.with_extension("json")).ok()
    }

    #[test]
    fn persona_golden_fixtures() {
        for (path, raw) in fixtures("persona") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let got = parse_persona(&raw);
            match expected_for(&path) {
                Some(want) => {
                    let want: PersonaReply =
                        serde_json::from_str(&want).unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert_eq!(got.as_ref(), Ok(&want), "{name}");
                }
                None => assert!(got.is_err(), "{name}: expected a parse error, got {got:?}"),
            }
        }
    }

    #[test]
    fn triage_golden_fixtures() {
        for (path, raw) in fixtures("triage") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let got = parse_triage(&raw);
            match expected_for(&path) {
                Some(want) => {
                    let want: Triage = serde_json::from_str(&want).unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert_eq!(got.as_ref(), Ok(&want), "{name}");
                }
                None => assert!(got.is_err(), "{name}: expected a parse error, got {got:?}"),
            }
        }
    }

    #[test]
    fn persona_specific_errors() {
        assert_eq!(parse_persona("just prose"), Err(ParseError::MissingJsonBlock));
        assert_eq!(parse_persona("```json\n{\"state_hint\":\"enough\"}\n```"), Err(ParseError::MissingReply));
        assert!(matches!(
            parse_persona("hi\n```json\n{\"state_hint\":\"enough\",\"tool\":\"rm\"}\n```"),
            Err(ParseError::InvalidJson(_))
        ));
    }

    #[test]
    fn triage_rejects_out_of_range_confidence_and_unknown_fields() {
        let base = r#"{"kind":"bug","title":"t","summary":"s","severity":"low","confidence":1.5}"#;
        assert_eq!(parse_triage(base), Err(ParseError::ConfidenceOutOfRange(1.5)));
        let extra = r#"{"kind":"bug","title":"t","summary":"s","severity":"low","confidence":0.5,"action":"open_pr"}"#;
        assert!(matches!(parse_triage(extra), Err(ParseError::InvalidJson(_))));
        let empty = r#"{"kind":"bug","title":"  ","summary":"s","severity":"low","confidence":0.5}"#;
        assert_eq!(parse_triage(empty), Err(ParseError::EmptyTitle));
    }

    #[test]
    fn fallback_is_noise_with_raw_summary() {
        let t = fallback_triage("garbage <script>");
        assert_eq!(t.kind, Kind::Noise);
        assert_eq!(t.confidence, 0.0);
        assert_eq!(t.summary, "garbage <script>");
        assert!(t.duplicate_of.is_none());
        let long = "x".repeat(5000);
        assert!(fallback_triage(&long).summary.chars().count() <= 4001);
    }
}
