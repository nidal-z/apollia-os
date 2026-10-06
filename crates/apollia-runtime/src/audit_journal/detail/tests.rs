use super::*;
use serde_json::json;

fn entry(seq: u64, kind: JournalEntryKind, payload: Value) -> JournalEntry {
    JournalEntry {
        seq,
        run_id: "run-1".into(),
        ts: "2026-10-04T09:00:00Z".into(),
        kind,
        payload,
        prev_hash: String::new(),
        hash: String::new(),
        signature: None,
        signing_key_id: None,
    }
}

fn response(seq: u64, id: &str, arguments: Value) -> JournalEntry {
    entry(
        seq,
        JournalEntryKind::LlmCompletion,
        json!({ "content": "", "tool_calls": [{ "id": id, "name": "file_write", "arguments": arguments }] }),
    )
}

fn started(seq: u64, id: &str) -> JournalEntry {
    entry(
        seq,
        JournalEntryKind::ToolCallStarted,
        json!({ "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": id }),
    )
}

fn completed(seq: u64, id: &str) -> JournalEntry {
    entry(
        seq,
        JournalEntryKind::ToolCallCompleted,
        json!({ "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": id, "success": true }),
    )
}

fn output(seq: u64, id: &str, value: Value) -> JournalEntry {
    entry(
        seq,
        JournalEntryKind::ToolOutput,
        json!({ "tool_call_id": id, "tool_name": "file_write", "output": value, "status": "success" }),
    )
}

#[test]
fn the_detail_of_a_call_joins_its_arguments_and_its_output() {
    // GIVEN a run where the model asked for a call, which ran and was captured
    let run = vec![
        response(0, "c0", json!({ "path": "/a.md", "content": "the letter" })),
        started(1, "c0"),
        output(2, "c0", json!({ "bytes": 10 })),
        completed(3, "c0"),
    ];
    for seq in [1, 2, 3] {
        // WHEN the detail of the start, of the capture or of the end is read
        let detail = entry_detail(&run, seq).expect("the entry exists");
        // THEN it holds the same full arguments and output
        assert_eq!(
            detail.tool_call,
            Some(ToolCallDetail {
                tool_call_id: "c0".into(),
                arguments: Some(json!({ "path": "/a.md", "content": "the letter" })),
                output: Some(json!({ "bytes": 10 })),
                status: Some("success".into()),
            }),
            "seq {seq}"
        );
        assert_eq!(detail.line.entry.seq, seq);
    }
}

#[test]
fn an_identifier_used_twice_in_a_run_joins_each_call_to_its_own_capture() {
    // GIVEN a run where the model named two successive calls `c0`
    let run = vec![
        response(0, "c0", json!({ "path": "/first.md" })),
        started(1, "c0"),
        output(2, "c0", json!("first output")),
        completed(3, "c0"),
        response(4, "c0", json!({ "path": "/second.md" })),
        started(5, "c0"),
        output(6, "c0", json!("second output")),
        completed(7, "c0"),
    ];
    // WHEN the detail of each end of call is read
    let first = entry_detail(&run, 3)
        .and_then(|d| d.tool_call)
        .expect("first call");
    let second = entry_detail(&run, 7)
        .and_then(|d| d.tool_call)
        .expect("second call");
    // THEN each holds its own arguments and its own output
    assert_eq!(first.arguments, Some(json!({ "path": "/first.md" })));
    assert_eq!(first.output, Some(json!("first output")));
    assert_eq!(second.arguments, Some(json!({ "path": "/second.md" })));
    assert_eq!(second.output, Some(json!("second output")));
}

#[test]
fn a_call_no_model_asked_for_has_no_arguments_to_join() {
    // GIVEN a run where an agent's own code called a tool: no model response
    // names the call and nothing captured its output
    let run = vec![started(0, "evt-1"), completed(1, "evt-1")];
    // WHEN its detail is read
    let call = entry_detail(&run, 1)
        .and_then(|d| d.tool_call)
        .expect("a call");
    // THEN the call is identified, with neither arguments nor output
    assert_eq!(call.tool_call_id, "evt-1");
    assert_eq!(call.arguments, None);
    assert_eq!(call.output, None);
    assert_eq!(call.status, None);
}

#[test]
fn an_entry_that_is_not_a_call_has_its_summary_and_no_call_detail() {
    // GIVEN a run holding a plan change
    let run = vec![entry(
        0,
        JournalEntryKind::PlanMutation,
        json!({ "revision": 2 }),
    )];
    // WHEN its detail is read, then the detail of an entry the run does not have
    let detail = entry_detail(&run, 0).expect("the entry exists");
    let missing = entry_detail(&run, 9);
    // THEN the entry comes whole with its summary and no call, and the
    // missing one is absent
    assert_eq!(detail.tool_call, None);
    assert_eq!(detail.line.summary.what, "Plan changed");
    assert_eq!(detail.line.entry.payload["revision"], 2);
    assert!(missing.is_none());
}
