//! An "always accept" is a standing consent for the agent and the family of
//! the tool, never a grant by name; on a code executor or on a tool nobody
//! classified it covers the current call only.
//!
//! These tests drive the chat loop with a scripted model that asks for the same
//! tool twice in one exchange and count the approval prompts it raises.

use super::*;
use crate::test_support::poll_until;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Upper bound on the wait for an approval prompt that should appear. Generous
/// so a loaded machine never turns a live prompt into a false negative.
const PROMPT_WAIT: Duration = Duration::from_secs(5);

/// Streams one call to `tool` on each of the first two iterations (ids `c0`,
/// `c1`), then final text.
struct TwoCallsThenText {
    tool: &'static str,
    iteration: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for TwoCallsThenText {
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
        let chunk = if current < 2 {
            LlmStreamChunk::ToolCall(LlmToolCall {
                id: format!("c{current}"),
                name: self.tool.into(),
                arguments: serde_json::json!({"command": format!("echo {current}")}),
            })
        } else {
            LlmStreamChunk::Text("done".to_string())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-two-calls"
    }
    fn model_id(&self) -> &str {
        "mock"
    }
}

struct OkInvoker;

#[async_trait::async_trait]
impl ToolInvoker for OkInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok(r#"{"exit_code": 0, "stdout": "ok"}"#.to_string())
    }
}

/// What one scripted exchange produced.
struct ExchangeRun {
    response: ChatAgentResponse,
    prompts: Vec<String>,
    second_resolved: bool,
}

/// Run one exchange where the model calls `tool` twice. The first call is
/// answered "always accept"; the second call is answered "accept" if, and only
/// if, the loop raises a prompt for it within [`PROMPT_WAIT`].
///
/// `family`, when given, registers the tool under that family; `governance`
/// decides the calls, the empty state when absent.
async fn run_two_call_exchange(
    tool: &'static str,
    family: Option<apollia_tools::ActionFamily>,
    governance: Option<crate::governance::GovernanceHandle>,
) -> ExchangeRun {
    let model: Arc<dyn CompletionModel> = Arc::new(TwoCallsThenText {
        tool,
        iteration: AtomicU32::new(0),
    });
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (event_bus, mut events) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    if let Some(family) = family {
        let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
        descriptor.name = tool.to_owned();
        descriptor.family = family;
        descriptor.is_read_only = false;
        descriptor.input_schema = serde_json::json!({ "type": "object" });
        tool_registry.register(descriptor).await.unwrap();
    }
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry: tool_registry.clone(),
        tool_invoker: Arc::new(OkInvoker),
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    })
    .with_governance(governance);
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let second_resolved = Arc::new(AtomicBool::new(false));
    let operator = tokio::spawn({
        let approvals = approvals.clone();
        let second_resolved = second_resolved.clone();
        async move {
            poll_until(PROMPT_WAIT, || {
                approvals.resolve("sess-1::msg-1::c0", ToolDecision::AlwaysAccept)
            })
            .await;
            let resolved = poll_until(PROMPT_WAIT, || {
                approvals.resolve("sess-1::msg-1::c1", ToolDecision::Accept)
            })
            .await;
            second_resolved.store(resolved, Ordering::SeqCst);
        }
    });

    let response = agent
        .execute(
            "sess-1",
            "msg-1",
            &RunId::new(),
            "run it twice",
            &[],
            "assistant",
            &[tool.to_string()],
            &HashSet::new(),
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
        .expect("the exchange completes");
    operator.await.expect("operator task joins");
    tool_registry.shutdown().await;

    let mut prompts = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::ChatApprovalRequired { tool_call_id, .. } = event {
            prompts.push(tool_call_id);
        }
    }
    ExchangeRun {
        response,
        prompts,
        second_resolved: second_resolved.load(Ordering::SeqCst),
    }
}

#[tokio::test]
async fn always_on_bash_executor_prompts_again_for_the_next_call() {
    // GIVEN an exchange where the model calls bash_executor on two successive
    // iterations, and the operator answers "always" to the first prompt
    // WHEN the exchange runs
    let run = run_two_call_exchange("bash_executor", None, None).await;

    // THEN both calls ran, and the second one went through its own prompt
    assert_eq!(run.prompts, vec!["c0".to_string(), "c1".to_string()]);
    assert!(run.second_resolved, "the second call raised no prompt");
    assert_eq!(run.response.tool_calls.len(), 2);
    assert!(run
        .response
        .tool_calls
        .iter()
        .all(|c| c.status == ToolCallStatus::Executed));
    // AND the loop reports no name grant for the code executor
    assert!(run.response.newly_authorized.is_empty());
}

#[tokio::test]
async fn always_on_python_executor_is_not_granted_by_name() {
    // GIVEN the same exchange shape with python_executor
    // WHEN the operator answers "always" to the first prompt
    let run = run_two_call_exchange("python_executor", None, None).await;

    // THEN the second call is prompted and no name grant leaves the loop
    assert_eq!(run.prompts, vec!["c0".to_string(), "c1".to_string()]);
    assert!(run.response.newly_authorized.is_empty());
}

#[tokio::test]
async fn always_on_a_writing_tool_is_a_family_consent() {
    // GIVEN governance over an empty governance.db, and an exchange where the
    // model calls a writing tool twice, the operator answering "always" to
    // the first prompt
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("governance.db");
    let governance =
        crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
            governance_db: Some(db.clone()),
            config_path: None,
            policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
            journal: None,
            events: None,
            forbidden_connections: Default::default(),
        });

    // WHEN the exchange runs
    let run = run_two_call_exchange(
        "file_write",
        Some(apollia_tools::ActionFamily::WriteOrDelete),
        Some(governance),
    )
    .await;

    // THEN only the first call prompted, the second ran under the consent
    assert_eq!(run.prompts, vec!["c0".to_string()]);
    assert!(!run.second_resolved);
    assert_eq!(run.response.tool_calls.len(), 2);
    assert!(run
        .response
        .tool_calls
        .iter()
        .all(|c| c.status == ToolCallStatus::Executed));
    // AND the consent is held for the agent and the family, no rule by name
    // is written and no name grant leaves the loop
    let consents = apollia_permissions::FamilyConsentStore::new(&db)
        .unwrap()
        .list_permanent()
        .unwrap();
    assert_eq!(
        consents,
        vec![(
            crate::chat::manager::APOLLIA_CHAT_AGENT_ID.to_owned(),
            apollia_permissions::ConsentFamily::WriteOrDelete
        )]
    );
    assert!(apollia_permissions::PrefixRuleEngine::new(&db)
        .unwrap()
        .list_rules()
        .unwrap()
        .is_empty());
    assert!(run.response.newly_authorized.is_empty());
}

#[tokio::test]
async fn always_on_an_unclassified_tool_asks_again() {
    // GIVEN an exchange where the model calls a tool nothing classifies twice,
    // the operator answering "always" to the first prompt
    // WHEN the exchange runs
    let run = run_two_call_exchange("mcp:files/move", None, None).await;

    // THEN the second call is prompted as well
    assert_eq!(run.prompts, vec!["c0".to_string(), "c1".to_string()]);
    assert!(run.second_resolved, "the second call raised no prompt");
}
