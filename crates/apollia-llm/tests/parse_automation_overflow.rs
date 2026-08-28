//! Regression test for the interval overflow found by the first public fuzzing
//! run of `fuzz_targets/parse_automation.rs`.
//!
//! The crashing input asked for an interval of 7_777_777_777 days. The parser
//! turned it into `now + Duration::seconds(672_000_000_000_000)`, and chrono's
//! `Add` panics on overflow instead of returning an error, so a thread died on
//! operator text.
//!
//! The bytes are read from the fuzz seed corpus rather than copied here, so the
//! seed and this test can never drift: deleting the seed is a compile error.

use apollia_llm::meta::parse_automation;
use chrono::{TimeZone, Utc};

/// The exact libFuzzer artifact of the first public CI run
/// (`crash-68f4a44d75ad953d7574080d0cd8b95ec98a7ece`), promoted to a seed.
const CRASH_INPUT: &[u8] = include_bytes!("../../../fuzz/seeds/parse_automation/interval_overflow");

#[test]
fn test_parse_automation_absurd_interval_returns_no_schedule() {
    // GIVEN the fuzzer input whose interval overflows a UTC timestamp
    let input = String::from_utf8_lossy(CRASH_INPUT);
    let now = Utc.with_ymd_and_hms(2026, 8, 28, 10, 0, 0).unwrap();

    // WHEN it is parsed with the same arguments the fuzz target uses
    let parsed = parse_automation(&input, now, &["scheduler".to_string()]);

    // THEN the parser refuses it like any unreadable phrase, without panicking
    assert_eq!(parsed.schedule, None);
}

#[test]
fn test_parse_automation_absurd_interval_is_refused_like_any_unreadable_phrase() {
    // GIVEN the same absurd interval dictated as plain readable text
    let now = Utc.with_ymd_and_hms(2026, 8, 28, 10, 0, 0).unwrap();

    // WHEN it is parsed
    let parsed = parse_automation(
        "every 7777777777 days, ask scheduler to run the report",
        now,
        &["scheduler".to_string()],
    );

    // THEN the operator gets the refusal the wizard already knows how to show:
    // no schedule, low confidence, and the "state a frequency" ambiguity
    assert_eq!(
        (parsed.schedule, parsed.confidence, parsed.ambiguities),
        (
            None,
            apollia_llm::meta::Confidence::Low,
            vec![
                "No schedule detected - state a frequency (for instance 'every day at 8am')."
                    .to_string()
            ]
        )
    );
}
