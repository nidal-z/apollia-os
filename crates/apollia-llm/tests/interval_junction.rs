//! Junction test between the natural-language automation parser (writer) and
//! the trigger interval parser (reader).
//!
//! `parse_automation` emits `ParsedSchedule::Interval { every, .. }` strings
//! that the desktop maps onto a trigger definition; the runtime then feeds the
//! stored string to `apollia_triggers::parse_interval`. Every unit the writer
//! can emit must be accepted by the reader, otherwise a dictated automation is
//! rejected by the validator with an on-screen error.

use apollia_llm::meta::{parse_automation, ParsedSchedule};
use apollia_triggers::parse_interval;
use chrono::{TimeZone, Utc};
use std::time::Duration;

#[test]
fn test_every_interval_unit_emitted_by_parse_automation_is_accepted_by_parse_interval() {
    // GIVEN one dictated phrase per interval unit the writer can emit
    // (seconds, minutes, hours, days; see PATTERNS in parse_automation.rs)
    let now = Utc.with_ymd_and_hms(2026, 8, 13, 10, 0, 0).unwrap();
    let phrases = [
        ("toutes les 30 secondes, lance agent-a", 30u64),
        ("every 30 seconds, run agent-a", 30),
        ("toutes les 5 minutes, lance agent-a", 300),
        ("toutes les 2 heures, lance agent-a", 7_200),
        ("every 2 days, run agent-a", 172_800),
    ];

    for (phrase, expected_seconds) in phrases {
        // WHEN the phrase is parsed into a schedule
        let parsed = parse_automation(phrase, now, &["agent-a".to_string()]);
        let ParsedSchedule::Interval {
            every,
            every_seconds,
            ..
        } = parsed
            .schedule
            .unwrap_or_else(|| panic!("no schedule parsed from '{phrase}'"))
        else {
            panic!("'{phrase}' did not parse as an interval schedule");
        };
        assert_eq!(
            every_seconds, expected_seconds,
            "writer seconds for '{phrase}'"
        );

        // THEN the emitted interval string is accepted by the reader, with
        // the same duration the writer computed
        let duration = parse_interval(&every)
            .unwrap_or_else(|e| panic!("reader rejected '{every}' from '{phrase}': {e}"));
        assert_eq!(
            duration,
            Duration::from_secs(expected_seconds),
            "reader duration for '{every}' from '{phrase}'"
        );
    }
}
