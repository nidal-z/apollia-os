use super::*;
use apollia_core::plan::StepOrigin;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Streams one `bash_executor` tool call on the first turn, then final text.
struct OneToolThenText {
    iteration: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for OneToolThenText {
    async fn complete(
        &self,
        _req: CompletionRequest,
    ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
        Err(apollia_llm::types::LlmError::InferenceError(
            "streaming path only".into(),
        ))
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                    + Send,
            >,
        >,
        apollia_llm::types::LlmError,
    > {
        let current = self.iteration.fetch_add(1, Ordering::SeqCst);
        if current == 0 {
            let chunks = vec![Ok(LlmStreamChunk::ToolCall(LlmToolCall {
                id: "c1".into(),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "echo hi"}),
            }))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        } else {
            let chunks = vec![Ok(LlmStreamChunk::Text("done".to_string()))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        }
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-one-tool"
    }
    fn model_id(&self) -> &str {
        "mock"
    }
}

/// A tool invoker whose `invoke` awaits briefly then records completion.
///
/// `cancel_on_invoke` lets the test request a pause exactly while the tool
/// future is in flight, so the loop must finish the tool before stopping.
struct SlowRecordingInvoker {
    completed: Arc<AtomicBool>,
    cancel_on_invoke: Option<CancellationToken>,
}

#[async_trait::async_trait]
impl ToolInvoker for SlowRecordingInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        // Request the pause now: the future is mid-flight, so the loop must
        // wait for completion below before observing the cancellation.
        if let Some(token) = &self.cancel_on_invoke {
            token.cancel();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        self.completed.store(true, Ordering::SeqCst);
        Ok(r#"{"exit_code": 0, "stdout": "ran"}"#.to_string())
    }
}

fn make_router(model: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    Arc::new(LlmRouter::with_backends(backends, "default"))
}

fn make_event_bus() -> EventBusSender {
    let (tx, _rx) = tokio::sync::broadcast::channel(128);
    tx
}

#[tokio::test]
async fn pause_stops_loop_at_checkpoint() {
    // GIVEN a ReAct loop with a token cancelled before the first iteration
    let model: Arc<dyn CompletionModel> = Arc::new(OneToolThenText {
        iteration: AtomicU32::new(0),
    });
    let tool_registry = ToolRegistryHandle::start();
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: make_router(model),
        tool_registry: tool_registry.clone(),
        tool_invoker: Arc::new(MockToolInvoker::ok()),
        event_bus: make_event_bus(),
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    });
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let cancel = CancellationToken::new();

    // WHEN the token is cancelled before the turn runs
    cancel.cancel();
    let resp = agent
        .execute(
            "sess-pause",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &HashSet::new(),
            &approvals,
            &budget,
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
            None,
            None,
            None,
            None,
            cancel,
        )
        .await
        .expect("paused turn returns Ok, not an error");

    // THEN the loop returns a paused outcome, not an error, and spent no step
    assert_eq!(resp.turn_outcome(), TurnOutcome::Paused);
    assert!(resp.paused);
    assert!(resp.tool_calls.is_empty());
    tool_registry.shutdown().await;
}

#[tokio::test]
async fn running_tool_completes_before_loop_stops() {
    // GIVEN a model that issues a tool call and a slow tool that requests the
    // pause while its future is in flight
    let model: Arc<dyn CompletionModel> = Arc::new(OneToolThenText {
        iteration: AtomicU32::new(0),
    });
    let tool_registry = ToolRegistryHandle::start();
    let cancel = CancellationToken::new();
    let completed = Arc::new(AtomicBool::new(false));
    let invoker: Arc<dyn ToolInvoker> = Arc::new(SlowRecordingInvoker {
        completed: completed.clone(),
        cancel_on_invoke: Some(cancel.clone()),
    });
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: make_router(model),
        tool_registry: tool_registry.clone(),
        tool_invoker: invoker,
        event_bus: make_event_bus(),
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    });
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let mut authorized = HashSet::new();
    authorized.insert("bash_executor".to_string());

    // WHEN the turn runs (the tool triggers the pause during its await)
    let resp = agent
        .execute(
            "sess-slow",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &authorized,
            &approvals,
            &budget,
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
            None,
            None,
            None,
            None,
            cancel,
        )
        .await
        .expect("paused turn returns Ok");

    // THEN the in-flight tool ran to completion before the loop stopped, and
    // the turn is paused (no half-applied tool)
    assert!(
        completed.load(Ordering::SeqCst),
        "the tool future must complete before the pause checkpoint"
    );
    assert_eq!(resp.turn_outcome(), TurnOutcome::Paused);
    assert_eq!(resp.tool_calls.len(), 1, "the completed tool is recorded");
    tool_registry.shutdown().await;
}

#[tokio::test]
async fn live_token_lets_turn_converge() {
    // GIVEN a turn with a token that is never cancelled (error-case baseline:
    // the pause machinery must not alter the normal convergence path)
    let model: Arc<dyn CompletionModel> = Arc::new(OneToolThenText {
        iteration: AtomicU32::new(0),
    });
    let tool_registry = ToolRegistryHandle::start();
    let completed = Arc::new(AtomicBool::new(false));
    let invoker: Arc<dyn ToolInvoker> = Arc::new(SlowRecordingInvoker {
        completed: completed.clone(),
        cancel_on_invoke: None,
    });
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: make_router(model),
        tool_registry: tool_registry.clone(),
        tool_invoker: invoker,
        event_bus: make_event_bus(),
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    });
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let mut authorized = HashSet::new();
    authorized.insert("bash_executor".to_string());

    // WHEN the turn runs with a live (uncancelled) token
    let resp = agent
        .execute(
            "sess-live",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &authorized,
            &approvals,
            &budget,
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
            None,
            None,
            None,
            None,
            CancellationToken::new(),
        )
        .await
        .expect("turn converges");

    // THEN the turn converges normally, not paused
    assert_eq!(resp.turn_outcome(), TurnOutcome::Completed);
    assert!(!resp.paused);
    assert_eq!(resp.content, "done");
    tool_registry.shutdown().await;
}

/// Minimal always-ok invoker for the no-tool checkpoint test.
struct MockToolInvoker;
impl MockToolInvoker {
    fn ok() -> Self {
        Self
    }
}
#[async_trait::async_trait]
impl ToolInvoker for MockToolInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok("ok".to_string())
    }
}

// ── provenance stamping on injected plan steps ────────────

#[test]
fn stamp_inject_provenance_overrides_step_origin() {
    // GIVEN a plan_add_step payload whose step declares an Initial provenance
    // and depends on the target X (the "before step X" ordering)
    let args = serde_json::json!({
        "step": {
            "step_id": "y",
            "description": "do Y",
            "depends_on": [],
            "provenance": { "origin": "initial", "at": 0 }
        },
        "reason": "before step X, do Y"
    });
    let prov = InjectedInstruction {
        session_id: "s1".into(),
        text: "before step X, do Y".into(),
    }
    .provenance();

    // WHEN the runtime stamps the inject provenance
    let stamped = stamp_inject_provenance(&args, &prov);

    // THEN the step provenance is forced to UserInject with the operator reason,
    // regardless of what the model emitted
    let step = &stamped["step"];
    let parsed: apollia_core::plan::PlanStep =
        serde_json::from_value(step.clone()).expect("step parses");
    assert_eq!(parsed.provenance.origin, StepOrigin::UserInject);
    assert_eq!(
        parsed.provenance.reason.as_deref(),
        Some("before step X, do Y")
    );
}

#[test]
fn stamp_inject_provenance_leaves_malformed_payload_untouched() {
    // GIVEN a payload with no object `step` field (error case)
    let args = serde_json::json!({ "reason": "do Y" });
    let prov = InjectedInstruction {
        session_id: "s1".into(),
        text: "do Y".into(),
    }
    .provenance();

    // WHEN stamping
    let stamped = stamp_inject_provenance(&args, &prov);

    // THEN the args are returned unchanged so the parser still reports the error
    assert_eq!(stamped, args);
}
