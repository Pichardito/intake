//! The conversation state machine. Code decides when to stop asking; the model only
//! writes the words.

use crate::types::{Extracted, Kind, State, StateHint};

/// The persona never asks more than this many follow-up questions.
pub const MAX_QUESTIONS: u8 = 3;

/// Fields the persona still has to collect for the hinted kind.
///
/// Bugs need `doing`, `expected`, `actual`; features need `benefit`, `frequency`; the other
/// kinds (and an unknown kind) need nothing. Returned in the order the persona should ask.
pub fn required_missing(kind: Option<Kind>, e: &Extracted) -> Vec<&'static str> {
    let present = |s: &Option<String>| s.as_deref().is_some_and(|v| !v.trim().is_empty());
    let mut missing = Vec::new();
    match kind {
        Some(Kind::Bug) => {
            if !present(&e.doing) {
                missing.push("doing");
            }
            if !present(&e.expected) {
                missing.push("expected");
            }
            if !present(&e.actual) {
                missing.push("actual");
            }
        }
        Some(Kind::Feature) => {
            if !present(&e.benefit) {
                missing.push("benefit");
            }
            if !present(&e.frequency) {
                missing.push("frequency");
            }
        }
        Some(Kind::Feedback) | Some(Kind::Praise) | Some(Kind::Noise) | None => {}
    }
    missing
}

/// Advance after one persona turn.
///
/// * `Open` → `Clarifying{asked: 1}` (the first reply asked a question), unless the model
///   said `Enough` and nothing is missing, in which case the first reply already closed.
/// * `Clarifying{asked}` → `Closing` once nothing is missing or `asked == MAX_QUESTIONS`;
///   otherwise `Clarifying{asked + 1}`. The hint cannot push past the cap, and `Enough`
///   cannot end early while a required field is still missing.
/// * `Closing` → `Triaged`; `Triaged` is absorbing.
pub fn advance(state: State, kind_hint: Option<Kind>, extracted: &Extracted, hint: StateHint) -> State {
    let complete = required_missing(kind_hint, extracted).is_empty();
    match state {
        State::Open => {
            if complete && hint == StateHint::Enough {
                State::Closing
            } else {
                State::Clarifying { asked: 1 }
            }
        }
        State::Clarifying { asked } => {
            if complete || asked >= MAX_QUESTIONS {
                State::Closing
            } else {
                State::Clarifying { asked: asked + 1 }
            }
        }
        State::Closing | State::Triaged => State::Triaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    fn full_bug() -> Extracted {
        Extracted {
            kind_hint: Some(Kind::Bug),
            doing: s("exporting"),
            expected: s("a csv"),
            actual: s("a 500"),
            ..Default::default()
        }
    }

    #[test]
    fn required_fields_per_kind() {
        let empty = Extracted::default();
        assert_eq!(required_missing(Some(Kind::Bug), &empty), vec!["doing", "expected", "actual"]);
        assert_eq!(required_missing(Some(Kind::Feature), &empty), vec!["benefit", "frequency"]);
        assert!(required_missing(Some(Kind::Feedback), &empty).is_empty());
        assert!(required_missing(Some(Kind::Praise), &empty).is_empty());
        assert!(required_missing(Some(Kind::Noise), &empty).is_empty());
        assert!(required_missing(None, &empty).is_empty());
    }

    #[test]
    fn whitespace_only_field_counts_as_missing() {
        let e = Extracted { doing: s("   "), ..full_bug() };
        assert_eq!(required_missing(Some(Kind::Bug), &e), vec!["doing"]);
    }

    #[test]
    fn transition_table() {
        use State::*;
        use StateHint::*;
        let empty = Extracted::default();
        let full = full_bug();
        let bug_partial = Extracted { kind_hint: Some(Kind::Bug), doing: s("x"), ..Default::default() };
        let cases: Vec<(State, Option<Kind>, &Extracted, StateHint, State)> = vec![
            // first turn always asks unless the model is done and nothing is required
            (Open, Some(Kind::Bug), &empty, AskMore, Clarifying { asked: 1 }),
            (Open, Some(Kind::Bug), &empty, Enough, Clarifying { asked: 1 }),
            (Open, Some(Kind::Praise), &empty, AskMore, Clarifying { asked: 1 }),
            (Open, Some(Kind::Praise), &empty, Enough, Closing),
            (Open, Some(Kind::Bug), &full, Enough, Closing),
            (Open, None, &empty, Enough, Closing),
            // clarifying: keep going while something is missing and under the cap
            (Clarifying { asked: 1 }, Some(Kind::Bug), &bug_partial, AskMore, Clarifying { asked: 2 }),
            (Clarifying { asked: 1 }, Some(Kind::Bug), &bug_partial, Enough, Clarifying { asked: 2 }),
            (Clarifying { asked: 2 }, Some(Kind::Bug), &bug_partial, AskMore, Clarifying { asked: 3 }),
            // the cap closes regardless of hint or completeness
            (Clarifying { asked: 3 }, Some(Kind::Bug), &bug_partial, AskMore, Closing),
            (Clarifying { asked: 3 }, Some(Kind::Bug), &bug_partial, Enough, Closing),
            // completeness closes regardless of hint
            (Clarifying { asked: 1 }, Some(Kind::Bug), &full, AskMore, Closing),
            (Clarifying { asked: 1 }, Some(Kind::Bug), &full, Enough, Closing),
            (Clarifying { asked: 1 }, Some(Kind::Feedback), &empty, AskMore, Closing),
            // tail
            (Closing, None, &empty, AskMore, Triaged),
            (Triaged, None, &empty, Enough, Triaged),
        ];
        for (from, kind, e, hint, want) in cases {
            assert_eq!(advance(from, kind, e, hint), want, "from {from:?} kind {kind:?} hint {hint:?}");
        }
    }

    fn arb_kind() -> impl Strategy<Value = Option<Kind>> {
        prop_oneof![
            Just(None),
            Just(Some(Kind::Bug)),
            Just(Some(Kind::Feature)),
            Just(Some(Kind::Feedback)),
            Just(Some(Kind::Praise)),
            Just(Some(Kind::Noise)),
        ]
    }

    fn arb_field() -> impl Strategy<Value = Option<String>> {
        prop_oneof![Just(None), Just(Some(String::new())), Just(Some("v".to_string()))]
    }

    fn arb_extracted() -> impl Strategy<Value = Extracted> {
        (arb_kind(), arb_field(), arb_field(), arb_field(), arb_field(), arb_field()).prop_map(
            |(kind_hint, doing, expected, actual, benefit, frequency)| Extracted {
                kind_hint,
                doing,
                expected,
                actual,
                benefit,
                frequency,
            },
        )
    }

    fn arb_hint() -> impl Strategy<Value = StateHint> {
        prop_oneof![Just(StateHint::AskMore), Just(StateHint::Enough)]
    }

    proptest! {
        /// Whatever the model says and whatever it extracts, the persona asks at most
        /// MAX_QUESTIONS questions and every run terminates in Triaged.
        #[test]
        fn asked_never_exceeds_cap(steps in prop::collection::vec((arb_kind(), arb_extracted(), arb_hint()), 1..12)) {
            let mut state = State::Open;
            for (kind, e, hint) in steps {
                state = advance(state, kind, &e, hint);
                if let State::Clarifying { asked } = state {
                    prop_assert!((1..=MAX_QUESTIONS).contains(&asked), "asked = {asked}");
                }
            }
            // MAX_QUESTIONS clarifying turns + closing + triaged = at most 5 steps to absorb
            for _ in 0..5 {
                state = advance(state, None, &Extracted::default(), StateHint::AskMore);
            }
            prop_assert_eq!(state, State::Triaged);
        }
    }
}
