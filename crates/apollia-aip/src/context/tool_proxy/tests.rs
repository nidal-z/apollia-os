use super::*;
use apollia_core::events::{RunId, RuntimeEvent};

#[test]
fn the_start_of_a_call_gains_its_family_and_target_and_keeps_the_rest() {
    // GIVEN the start of a call, built before its descriptor was read
    let run = RunId::new();
    let started = RuntimeEvent::ToolCallStarted {
        event_id: "evt-1".into(),
        task_id: "t-1".into(),
        agent_id: "quotes".into(),
        tool_name: "file_read".into(),
        args_json: Some(r#"{"path":"/home/u/a.md"}"#.into()),
        run_id: Some(run.clone()),
        tool_call_id: Some("evt-1".into()),
        family: None,
        target: None,
    };

    // WHEN the family and the target read from the descriptor are added
    let completed =
        with_family_and_target(started, Some("read".into()), Some("/home/u/a.md".into()));

    // THEN the event carries both, under the same identifier and run
    match completed {
        RuntimeEvent::ToolCallStarted {
            event_id,
            tool_call_id,
            family,
            target,
            run_id,
            tool_name,
            ..
        } => {
            assert_eq!(event_id, "evt-1");
            assert_eq!(tool_call_id.as_deref(), Some("evt-1"));
            assert_eq!(family.as_deref(), Some("read"));
            assert_eq!(target.as_deref(), Some("/home/u/a.md"));
            assert_eq!(run_id, Some(run));
            assert_eq!(tool_name, "file_read");
        }
        other => panic!("expected ToolCallStarted, got {other:?}"),
    }
}
