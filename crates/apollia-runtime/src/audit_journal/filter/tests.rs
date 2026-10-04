use super::*;
use crate::audit_journal::summarize;
use serde_json::json;

fn entry(
    run_id: &str,
    ts: &str,
    kind: JournalEntryKind,
    payload: serde_json::Value,
) -> JournalEntry {
    JournalEntry {
        seq: 0,
        run_id: run_id.into(),
        ts: ts.into(),
        kind,
        payload,
        prev_hash: String::new(),
        hash: String::new(),
        signature: None,
        signing_key_id: None,
    }
}

fn completed(agent: &str, family: &str, success: bool, ts: &str) -> JournalEntry {
    entry(
        "run-1",
        ts,
        JournalEntryKind::ToolCallCompleted,
        json!({ "tool_name": "file_write", "agent_id": agent, "family": family, "success": success }),
    )
}

/// The filter of six criteria, in the order agents, families, natures,
/// results, since, until; an empty one is absent.
fn filter_of(criteria: [&str; 6]) -> Result<JournalFilter, JournalFilterError> {
    let [agents, families, natures, outcomes, since, until] =
        criteria.map(|raw| (!raw.is_empty()).then_some(raw));
    JournalFilter::from_criteria(JournalCriteria {
        agents,
        families,
        natures,
        outcomes,
        since,
        until,
    })
}

fn passes(filter: &JournalFilter, entry: &JournalEntry) -> bool {
    filter.matches(entry, &summarize(entry))
}

#[test]
fn the_default_filter_keeps_everything() {
    // GIVEN no criterion
    let filter = JournalFilter::default();
    // WHEN an action and a system entry are checked
    let action = completed("quotes", "read", true, "2026-10-04T09:00:00Z");
    let system = entry(
        "run-1",
        "not a date",
        JournalEntryKind::ClockSample,
        json!({}),
    );
    // THEN both pass, even the one whose date cannot be read
    assert!(passes(&filter, &action));
    assert!(passes(&filter, &system));
}

#[test]
fn each_criterion_keeps_its_matches_only() {
    // GIVEN a failed write by `quotes` on the fourth of October
    let write = completed("quotes", "write_or_delete", false, "2026-10-04T09:00:00Z");
    let cases = [
        (("quotes", "", "", "", "", ""), true),
        (("invoices", "", "", "", "", ""), false),
        (("", "write_or_delete,read", "", "", "", ""), true),
        (("", "read", "", "", "", ""), false),
        (("", "", "action", "", "", ""), true),
        (("", "", "policy,access_change", "", "", ""), false),
        (("", "", "", "failure", "", ""), true),
        (("", "", "", "success,refused", "", ""), false),
        (("", "", "", "", "2026-10-04", "2026-10-05"), true),
        (("", "", "", "", "2026-10-05", ""), false),
        (("", "", "", "", "", "2026-10-04"), false),
        (
            (
                "",
                "",
                "",
                "",
                "2026-10-04T09:00:00Z",
                "2026-10-04T09:00:01Z",
            ),
            true,
        ),
        (("", "", "", "", "", "2026-10-04T09:00:00Z"), false),
        (
            (
                "quotes",
                "write_or_delete",
                "action",
                "failure",
                "2026-10-04",
                "",
            ),
            true,
        ),
        (
            (
                "quotes",
                "write_or_delete",
                "action",
                "success",
                "2026-10-04",
                "",
            ),
            false,
        ),
    ];
    for ((agents, families, natures, outcomes, since, until), expected) in cases {
        // WHEN the filter of each set of criteria is applied
        let filter = filter_of([agents, families, natures, outcomes, since, until])
            .expect("readable criteria");
        // THEN the entry passes exactly when every criterion holds
        assert_eq!(
            passes(&filter, &write),
            expected,
            "{agents}|{families}|{natures}|{outcomes}|{since}|{until}"
        );
    }
}

#[test]
fn an_agent_is_matched_by_its_name_or_by_the_runs_of_its_tasks() {
    // GIVEN a filter on `quotes`, whose tasks ran under `run-quotes`
    let filter = JournalFilter {
        agents: vec!["quotes".into()],
        agent_runs: vec!["run-quotes".into()],
        ..JournalFilter::default()
    };
    // WHEN an entry naming the agent, an entry of its run that names nobody,
    // and an entry of another agent are checked
    let named = completed("quotes", "read", true, "2026-10-04T09:00:00Z");
    let of_its_run = entry(
        "run-quotes",
        "2026-10-04T09:00:00Z",
        JournalEntryKind::LlmCompletion,
        json!({ "content": "x" }),
    );
    let other = completed("invoices", "read", true, "2026-10-04T09:00:00Z");
    // THEN the first two pass and the third does not
    assert!(passes(&filter, &named));
    assert!(passes(&filter, &of_its_run));
    assert!(!passes(&filter, &other));
}

#[test]
fn an_entry_without_a_family_or_a_result_fails_a_filter_on_them() {
    // GIVEN the start of a call, which has a family and no result yet, and a
    // system entry, which has neither
    let started = entry(
        "run-1",
        "2026-10-04T09:00:00Z",
        JournalEntryKind::ToolCallStarted,
        json!({ "tool_name": "file_read", "agent_id": "quotes", "family": "read" }),
    );
    let system = entry(
        "run-1",
        "2026-10-04T09:00:00Z",
        JournalEntryKind::PlanMutation,
        json!({}),
    );
    let by_family = filter_of(["", "read", "", "", "", ""]).expect("readable");
    let by_outcome = filter_of(["", "", "", "success", "", ""]).expect("readable");
    // WHEN both filters are applied
    // THEN the start passes the family filter only, the system entry neither
    assert!(passes(&by_family, &started));
    assert!(!passes(&by_outcome, &started));
    assert!(!passes(&by_family, &system));
    assert!(!passes(&by_outcome, &system));
}

#[test]
fn a_kind_or_a_date_that_rules_an_entry_out_is_told_without_its_payload() {
    // GIVEN a filter on the policy nature since the fourth of October
    let filter = filter_of(["", "", "policy", "", "2026-10-04", ""]).expect("readable");
    // WHEN kinds and dates are checked alone
    // THEN another nature, an earlier date and an unreadable date are ruled out
    assert!(filter.admits(&JournalEntryKind::PolicyApplied, "2026-10-04T00:00:00Z"));
    assert!(filter.admits(
        &JournalEntryKind::PolicyApplied,
        "2026-10-04T02:00:00+02:00"
    ));
    assert!(!filter.admits(&JournalEntryKind::ToolCallStarted, "2026-10-04T00:00:00Z"));
    assert!(!filter.admits(&JournalEntryKind::PolicyApplied, "2026-10-03T23:59:59Z"));
    assert!(!filter.admits(&JournalEntryKind::PolicyApplied, "yesterday"));
}

#[test]
fn an_unreadable_criterion_is_refused_and_named() {
    // GIVEN a nature, a result and a date nobody defined
    // WHEN each is turned into a filter
    let nature = filter_of(["", "", "action,other", "", "", ""]);
    let outcome = filter_of(["", "", "", "ok", "", ""]);
    let date = filter_of(["", "", "", "", "last week", ""]);
    // THEN each is refused with the value that could not be read
    assert_eq!(nature, Err(JournalFilterError::Nature("other".into())));
    assert_eq!(outcome, Err(JournalFilterError::Outcome("ok".into())));
    assert_eq!(date, Err(JournalFilterError::Date("last week".into())));
}
