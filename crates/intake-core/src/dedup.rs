//! Duplicate scoring: token Jaccard over title + summary, with the model's suggestion as a
//! bounded nudge. Pure; the server supplies the candidates (top FTS hits).

use std::collections::HashSet;

use crate::types::{TicketStub, Triage};

/// Below this Jaccard score a candidate is not offered as a duplicate.
pub const DUPLICATE_THRESHOLD: f32 = 0.35;
/// A draft that is probably a duplicate never looks confident as a new ticket.
pub const DUPLICATE_CONFIDENCE_CAP: f32 = 0.5;
/// Extra score for the candidate the model itself named.
const SUGGESTION_BONUS: f32 = 0.2;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "when", "from", "but", "not", "are", "was", "you", "have",
    "has", "can", "cant", "does", "doesnt", "into", "after", "before", "then", "than", "there", "their",
    "they", "will", "would", "should", "could", "about", "just", "also",
];

/// Lowercase alphanumeric tokens of three or more characters, minus stopwords.
pub fn tokens(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(t))
        .map(str::to_string)
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    inter / union
}

/// Candidates ranked best first as `(ticket_id, score)`, score in 0..=1.
pub fn rank_duplicates(candidates: &[TicketStub], triage: &Triage) -> Vec<(i64, f32)> {
    let mine = tokens(&format!("{} {}", triage.title, triage.summary));
    let mut ranked: Vec<(i64, f32)> = candidates
        .iter()
        .map(|c| {
            let theirs = tokens(&format!("{} {}", c.title, c.summary));
            let mut score = jaccard(&mine, &theirs);
            if triage.duplicate_of == Some(c.id) {
                score = (score + SUGGESTION_BONUS).min(1.0);
            }
            (c.id, score)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    ranked
}

/// Apply the ranking to a triage: a `duplicate_of` the model invented (not among the
/// candidates) is dropped; the best candidate over the threshold becomes `duplicate_of`;
/// when a duplicate is set, confidence is capped.
pub fn resolve_duplicate(mut triage: Triage, candidates: &[TicketStub]) -> Triage {
    let ranked = rank_duplicates(candidates, &triage);
    triage.duplicate_of = match ranked.first() {
        Some(&(id, score)) if score >= DUPLICATE_THRESHOLD => Some(id),
        _ => None,
    };
    if triage.duplicate_of.is_some() {
        triage.confidence = triage.confidence.min(DUPLICATE_CONFIDENCE_CAP);
    }
    triage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Kind, Severity};

    fn triage(title: &str, summary: &str, dup: Option<i64>) -> Triage {
        Triage {
            kind: Kind::Bug,
            title: title.into(),
            summary: summary.into(),
            steps: vec![],
            expected: None,
            actual: None,
            severity: Severity::Medium,
            confidence: 0.9,
            duplicate_of: dup,
        }
    }

    fn stub(id: i64, title: &str, summary: &str) -> TicketStub {
        TicketStub { id, title: title.into(), summary: summary.into() }
    }

    #[test]
    fn tokens_drop_short_words_and_stopwords() {
        let t = tokens("The export button does not work, it's broken!");
        assert!(t.contains("export") && t.contains("button") && t.contains("broken"));
        assert!(!t.contains("the") && !t.contains("it") && !t.contains("not"));
    }

    #[test]
    fn identical_text_scores_one_and_unrelated_scores_zero() {
        let tr = triage("Export CSV fails", "Clicking export returns a server error", None);
        let c = [
            stub(1, "Export CSV fails", "Clicking export returns a server error"),
            stub(2, "Dark mode please", "Would love a dark theme"),
        ];
        let r = rank_duplicates(&c, &tr);
        assert_eq!(r[0].0, 1);
        assert!((r[0].1 - 1.0).abs() < 1e-6);
        assert_eq!(r[1], (2, 0.0));
    }

    #[test]
    fn model_suggestion_is_a_bounded_nudge_not_a_verdict() {
        let tr = triage("Export CSV fails", "server error on export", Some(2));
        let c = [
            stub(1, "Export CSV fails", "server error on export"),
            stub(2, "Dark mode please", "dark theme"),
        ];
        let r = rank_duplicates(&c, &tr);
        assert_eq!(r[0].0, 1, "lexical match beats the suggestion when the suggestion is unrelated");
        let (_, s2) = r.iter().find(|(id, _)| *id == 2).copied().unwrap();
        assert!((s2 - 0.2).abs() < 1e-6);
    }

    #[test]
    fn resolve_drops_invented_ids_and_caps_confidence() {
        let tr = triage("Export CSV fails", "server error on export", Some(999));
        let resolved = resolve_duplicate(tr.clone(), &[stub(5, "Unrelated", "nothing alike here")]);
        assert_eq!(resolved.duplicate_of, None);
        assert_eq!(resolved.confidence, 0.9);

        let resolved = resolve_duplicate(tr, &[stub(5, "Export CSV fails", "server error when exporting")]);
        assert_eq!(resolved.duplicate_of, Some(5));
        assert_eq!(resolved.confidence, DUPLICATE_CONFIDENCE_CAP);
    }

    #[test]
    fn empty_candidates_is_fine() {
        let tr = triage("a", "b", Some(1));
        assert!(rank_duplicates(&[], &tr).is_empty());
        assert_eq!(resolve_duplicate(tr, &[]).duplicate_of, None);
    }
}
