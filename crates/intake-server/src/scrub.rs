//! Server-side scrub: belt and braces behind the widget's own pass. Only the scrubbed text
//! and per-label counts leave this module; the reversible map is never requested.

use std::collections::BTreeMap;

use veilkit_core::{Engine, RedactOptions, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrubbed {
    pub text: String,
    /// Label → how many spans were replaced, e.g. `{"SSN": 1, "EMAIL": 2}`.
    pub redactions: BTreeMap<String, u32>,
}

pub fn scrub(engine: &Engine, text: &str) -> Scrubbed {
    let out =
        engine.redact(text, &RedactOptions { style: Style::Label, reversible: false, ..Default::default() });
    debug_assert!(out.map.is_none(), "reversible map must never be produced");
    let mut redactions = BTreeMap::new();
    for s in &out.spans {
        *redactions.entry(s.span.label.to_string()).or_insert(0) += 1;
    }
    Scrubbed { text: out.text, redactions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssn_and_email_are_replaced_and_counted() {
        let engine = Engine::all_packs().expect("engine");
        let s = scrub(&engine, "my ssn is 123-45-6789 and mail me at jane.doe@example.com");
        assert!(!s.text.contains("123-45-6789"), "{}", s.text);
        assert!(!s.text.contains("jane.doe@example.com"), "{}", s.text);
        assert_eq!(s.redactions.values().sum::<u32>(), 2, "{:?}", s.redactions);
        assert!(s.redactions.contains_key("SSN"), "{:?}", s.redactions);
    }

    #[test]
    fn clean_text_is_untouched() {
        let engine = Engine::all_packs().expect("engine");
        let s = scrub(&engine, "the export button is broken");
        assert_eq!(s.text, "the export button is broken");
        assert!(s.redactions.is_empty());
    }
}
