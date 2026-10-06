//! Characterization tests for two chat-loop capabilities whose existing tests
//! stopped short of the chat path: the LLM critic driving a correction retry
//! (every other verification test here runs with the critic disabled), and the
//! context compaction performed by the agent itself, with its event.
//!
//! They pin today's behaviour so a change of execution engine can be checked
//! against it.

use super::*;
use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason as LlmFinishReason,
    MessageContent, StreamChunk as LlmStreamChunk,
};
use std::sync::Mutex as StdMutex;

/// Returns its scripted responses in order, then the last one forever.
struct ScriptedModel {
    responses: StdMutex<Vec<String>>,
}

impl ScriptedModel {
    fn new(responses: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            responses: StdMutex::new(responses.iter().map(|r| r.to_string()).collect()),
        })
    }

    fn next(&self) -> String {
        let mut queue = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() > 1 {
            queue.remove(0)
        } else {
            queue.first().cloned().unwrap_or_default()
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for ScriptedModel {
    async fn complete(
        &self,
        _req: CompletionRequest,
    ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.next(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: LlmFinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
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
        Err(apollia_llm::types::LlmError::InferenceError(
            "scripted model does not stream".into(),
        ))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "scripted"
    }
    fn model_id(&self) -> &str {
        "scripted"
    }
}

fn router_of(model: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    Arc::new(LlmRouter::with_backends(backends, "default"))
}

/// Check invoker for a verification loop that declares no command.
struct NoCheck;

impl CheckInvoker for NoCheck {
    async fn invoke_check(&self, _command: &str) -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code: 0,
            stderr: String::new(),
        })
    }
}

#[tokio::test]
async fn test_chat_critic_correction_drives_one_retry_then_passes() {
    // GIVEN the chat wiring at the supervised tier: no declared check, an
    // enabled critic that flags the first answer and accepts the corrected one
    let critic_model = ScriptedModel::new(&[
        r#"{"corrections":[{"kind":"missing_total","description":"the total is absent","suggestion":"state the total"}]}"#,
        r#"{"corrections":[]}"#,
    ]);
    let critic = CriticPass::new(router_of(critic_model));
    let verification = VerificationLoop::new(Vec::new(), Vec::new());
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });
    let corrections_seen: Vec<String> = Vec::new();

    // WHEN the post-loop verification runs on the first answer
    let (report, corrections_seen) = run_verification_with_retry(
        &AutonomyLevel::Supervised,
        Some(&verification),
        Some(&critic),
        &NoCheck,
        "Sum the invoices",
        "Here are the invoices.",
        &budget,
        VERIFICATION_MAX_RETRIES,
        corrections_seen,
        |mut seen: Vec<String>, correction: String| async move {
            seen.push(correction);
            (Ok("The total is 42.".to_string()), seen)
        },
    )
    .await;

    // THEN exactly one retry ran, it carried the critic's suggestion, and the
    // corrected answer passed
    let report = report.expect("a report at the supervised tier");
    assert_eq!((report.passed, report.retry_iterations), (true, 1));
    assert_eq!(corrections_seen.len(), 1);
    assert!(
        corrections_seen[0].contains("state the total"),
        "the retry prompt must carry the critic's suggestion: {}",
        corrections_seen[0]
    );
}

#[tokio::test]
async fn test_chat_critic_that_never_accepts_stops_at_the_retry_bound() {
    // GIVEN an enabled critic that flags every answer
    let critic_model = ScriptedModel::new(&[
        r#"{"corrections":[{"kind":"wrong","description":"still wrong","suggestion":"fix it"}]}"#,
    ]);
    let critic = CriticPass::new(router_of(critic_model));
    let verification = VerificationLoop::new(Vec::new(), Vec::new());
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });

    // WHEN the verification runs
    let (report, ()) = run_verification_with_retry(
        &AutonomyLevel::Supervised,
        Some(&verification),
        Some(&critic),
        &NoCheck,
        "objective",
        "answer",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("answer again".to_string()), ()) },
    )
    .await;

    // THEN it gives up failed after the bounded number of retries
    let report = report.expect("a report at the supervised tier");
    assert_eq!(
        (report.passed, report.retry_iterations),
        (false, VERIFICATION_MAX_RETRIES)
    );
}

fn text_of(message: &LlmChatMessage) -> String {
    match &message.content {
        MessageContent::Text(t) => t.clone(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn test_chat_agent_compacts_its_buffer_and_emits_context_compacted() {
    // GIVEN a chat agent whose buffer holds an old turn far over the window,
    // followed by the current request
    let (event_bus, mut rx) = tokio::sync::broadcast::channel(16);
    let mut agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router_of(ScriptedModel::new(&["summary of the old turn"])),
        tool_registry: ToolRegistryHandle::start(),
        tool_invoker: Arc::new(NoInvoker),
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    });
    agent.context_manager = ContextManager::new(0.80, 4000, 1, None, 8000);
    let mut messages = vec![
        LlmChatMessage::system("system"),
        LlmChatMessage::user("x".repeat(600_000)),
        LlmChatMessage::assistant("ok"),
        LlmChatMessage::user("latest request"),
    ];

    // WHEN the loop's compaction step runs on it
    let compacted = agent
        .maybe_compact_context(&mut messages, "s1", 0)
        .await
        .expect("summarizing never refuses");

    // THEN the buffer shrinks, the current request is still last, and the
    // compaction is published with the original message count
    assert!(compacted);
    assert!(
        messages.len() < 4,
        "buffer not compacted: {}",
        messages.len()
    );
    assert_eq!(
        text_of(messages.last().expect("a message")),
        "latest request"
    );
    let event = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|e| match e {
        apollia_core::RuntimeEvent::ContextCompacted {
            original_messages, ..
        } => Some(original_messages),
        _ => None,
    });
    assert_eq!(event, Some(4));
}

#[tokio::test]
async fn test_chat_agent_below_threshold_leaves_buffer_untouched() {
    // GIVEN a chat agent with a short buffer
    let (event_bus, mut rx) = tokio::sync::broadcast::channel(16);
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router_of(ScriptedModel::new(&["unused"])),
        tool_registry: ToolRegistryHandle::start(),
        tool_invoker: Arc::new(NoInvoker),
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    });
    let mut messages = vec![
        LlmChatMessage::system("system"),
        LlmChatMessage::user("hello"),
    ];

    // WHEN the compaction step runs
    let compacted = agent
        .maybe_compact_context(&mut messages, "s1", 0)
        .await
        .expect("summarizing never refuses");

    // THEN nothing changes and nothing is published
    assert!(!compacted);
    assert_eq!(messages.len(), 2);
    assert!(rx.try_recv().is_err());
}

struct NoInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err("no tool in this test".to_string())
    }
}

#[test]
fn test_chat_tool_output_under_the_cap_reaches_the_model_unchanged() {
    // GIVEN a plain tool output exactly at the cap
    let output = "a".repeat(TOOL_OUTPUT_MAX_LEN);

    // WHEN it is prepared for the model's context
    let prepared = truncate_tool_output(&output);

    // THEN it is passed through byte for byte
    assert_eq!(prepared, output);
}

#[test]
fn test_chat_tool_output_over_the_cap_is_cut_with_a_notice() {
    // GIVEN a plain (non-JSON) tool output one character over the cap
    let output = "b".repeat(TOOL_OUTPUT_MAX_LEN + 1);

    // WHEN it is prepared for the model's context
    let prepared = truncate_tool_output(&output);

    // THEN the model sees the cut and the original size, so it can refine
    assert!(prepared.len() < output.len() + 200);
    assert!(
        prepared.contains(&format!(
            "[Output truncated - {} chars total.",
            TOOL_OUTPUT_MAX_LEN + 1
        )),
        "missing truncation notice: {}",
        &prepared[prepared.len().saturating_sub(120)..]
    );
}
