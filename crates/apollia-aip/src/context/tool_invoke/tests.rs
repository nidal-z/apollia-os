use super::*;
use apollia_core::events::{RunId, RuntimeEvent};

fn completion(run: &RunId) -> ToolCompletionEvent {
    ToolCompletionEvent {
        parent_id: "evt-1".into(),
        task_id: "t-1".into(),
        agent_id: "quotes".into(),
        tool_name: "http_fetch".into(),
        duration_ms: 12,
        run_id: Some(run.clone()),
        family: Some("send_external".into()),
        target: Some("example.org".into()),
    }
}

#[test]
fn the_end_of_a_call_carries_its_identifier_family_and_target() {
    for (result, expected) in [
        (Ok(serde_json::json!({ "status": 200 })), true),
        (
            Err(ToolProxyError::ExecutionFailed("timeout".into())),
            false,
        ),
    ] {
        // GIVEN a bus, and a call of an agent's own code that ended
        let (bus, mut events) = tokio::sync::broadcast::channel(4);
        let run = RunId::new();

        // WHEN its completion is emitted
        emit_tool_completion_events(Some(&bus), &result, completion(&run));

        // THEN the event carries the identifier its start was emitted under,
        // the family of the tool and the target of the call
        match events.try_recv().expect("one event") {
            RuntimeEvent::ToolCallCompleted {
                parent_event_id,
                tool_call_id,
                family,
                target,
                success,
                run_id,
                ..
            } => {
                assert_eq!(tool_call_id.as_deref(), Some(parent_event_id.as_str()));
                assert_eq!(family.as_deref(), Some("send_external"));
                assert_eq!(target.as_deref(), Some("example.org"));
                assert_eq!(success, expected);
                assert_eq!(run_id, Some(run));
            }
            other => panic!("expected ToolCallCompleted, got {other:?}"),
        }
    }
}
