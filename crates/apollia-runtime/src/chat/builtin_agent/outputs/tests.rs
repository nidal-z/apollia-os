//! A long output is kept on disk rather than cut, and a failing tool trips
//! its breaker.

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

/// Fails every call with a transient error and counts the calls.
struct FlakyInvoker {
    calls: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl ToolInvoker for FlakyInvoker {
    async fn invoke(&self, _tool: &str, _args: &serde_json::Value) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err("connection reset by peer".to_string())
    }
}

fn agent_with(invoker: Arc<dyn ToolInvoker>) -> BuiltInChatAgent {
    let (event_bus, _rx) = tokio::sync::broadcast::channel(16);
    BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: Arc::new(LlmRouter::empty()),
        tool_registry: ToolRegistryHandle::start(),
        tool_invoker: invoker,
        event_bus,
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    })
}

fn call(name: &str) -> ToolCall {
    ToolCall {
        id: format!("call-{name}"),
        name: name.into(),
        arguments: serde_json::json!({}),
    }
}

#[test]
fn test_a_long_output_is_kept_whole_on_disk_and_the_model_gets_its_path() {
    // GIVEN an output twice the cap, and a tool output directory not yet made
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join(TOOL_OUTPUT_DIR);
    let output = format!(
        "{}{}",
        "a".repeat(TOOL_OUTPUT_MAX_LEN),
        "#".repeat(TOOL_OUTPUT_MAX_LEN)
    );

    // WHEN it is prepared for the model's context
    let prepared = context_tool_output(&output, Some(&dir));

    // THEN the model gets the beginning and a path, and that file holds all of
    // the output, its end included
    assert!(prepared.starts_with(&"a".repeat(TOOL_OUTPUT_MAX_LEN)));
    assert!(!prepared.contains('#'));
    let path = std::fs::read_dir(&dir)
        .expect("the directory is created")
        .next()
        .expect("one file")
        .expect("entry")
        .path();
    assert!(prepared.contains(&path.display().to_string()));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), output);
}

#[test]
fn test_a_short_output_reaches_the_model_unchanged_and_nothing_is_written() {
    // GIVEN an output at the cap
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join(TOOL_OUTPUT_DIR);
    let output = "a".repeat(TOOL_OUTPUT_MAX_LEN);

    // WHEN it is prepared for the model's context
    let prepared = context_tool_output(&output, Some(&dir));

    // THEN it is passed as is, and no file is written
    assert_eq!(prepared, output);
    assert!(!dir.exists());
}

#[test]
fn test_without_a_directory_a_long_output_is_cut_with_a_notice() {
    // GIVEN an output over the cap, and no directory to keep it in
    let output = "b".repeat(TOOL_OUTPUT_MAX_LEN + 1);

    // WHEN it is prepared for the model's context
    let prepared = context_tool_output(&output, None);

    // THEN it is cut as before, the model told so
    assert!(prepared.contains("[Output truncated -"));
}

#[tokio::test]
async fn test_a_tool_failing_three_times_is_refused_without_being_called() {
    // GIVEN a tool that changes something, failing on every call
    let calls = Arc::new(AtomicU32::new(0));
    let agent = agent_with(Arc::new(FlakyInvoker {
        calls: Arc::clone(&calls),
    }));

    // WHEN it is called four times
    let mut outcomes = Vec::new();
    for _ in 0..4 {
        outcomes.push(agent.invoke_through_breaker(&call("deploy")).await.0);
    }

    // THEN each of the first three failures is one call, never retried, and
    // the fourth is refused by the open breaker before reaching the tool
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        outcomes[0].as_ref().expect_err("failure"),
        "connection reset by peer"
    );
    let refusal = outcomes[3].as_ref().expect_err("refusal");
    assert!(refusal.contains("circuit open"), "{refusal}");
}
