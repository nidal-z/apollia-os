//! A run outside any conversation goes through governance like the chat: a
//! sensitive call nobody consented to asks, or waits when nobody watches, and
//! never runs on its own.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk, ToolCall,
};

use super::*;
use crate::chat::types::ToolCallStatus;
use crate::chat::types::ToolDecision;
use crate::governance::{AccessCall, AccessDecision, GovernanceContract, GovernanceState};

/// Streams one call to `file_write`, then final text.
struct WriteOnce {
    turn: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for WriteOnce {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::InferenceError("streaming path only".into()))
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let chunk = if self.turn.fetch_add(1, Ordering::SeqCst) == 0 {
            StreamChunk::ToolCall(ToolCall {
                id: "w1".into(),
                name: "file_write".into(),
                arguments: serde_json::json!({ "path": "/tmp/report.txt" }),
            })
        } else {
            StreamChunk::Text("done".into())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "write-once"
    }
    fn model_id(&self) -> &str {
        "write-once"
    }
}

/// Counts the calls that actually ran.
struct CountingInvoker(Arc<AtomicU32>);

#[async_trait::async_trait]
impl ToolInvoker for CountingInvoker {
    async fn invoke(&self, _tool: &str, _args: &serde_json::Value) -> Result<String, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("written".into())
    }
}

/// What a run of an agent writing one file, outside any conversation,
/// produced: the approvals it raised, how many calls ran before the answer,
/// and how many ran in all.
struct OffChatRun {
    approvals: Vec<String>,
    ran_before_answer: u32,
    ran: u32,
    status: ToolCallStatus,
}

async fn run_off_chat_write(unattended: bool) -> OffChatRun {
    let mut backends = std::collections::HashMap::new();
    backends.insert(
        "default".to_string(),
        Arc::new(WriteOnce {
            turn: AtomicU32::new(0),
        }) as Arc<dyn CompletionModel>,
    );
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let registry = ToolRegistryHandle::start();
    let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    descriptor.name = "file_write".into();
    descriptor.family = apollia_tools::ActionFamily::WriteOrDelete;
    descriptor.is_read_only = false;
    descriptor.input_schema = serde_json::json!({ "type": "object" });
    registry.register(descriptor).await.unwrap();
    let ran = Arc::new(AtomicU32::new(0));
    let spec = AgentRunSpec {
        agent_id: "ledger-clerk".into(),
        mission: "Write the report.".into(),
        granted_tools: vec!["file_write".into()],
        model: None,
        autonomy: AutonomyLevel::Assisted,
    };
    let executor = AgentExecutor::new(
        spec,
        AgentExecutorDeps {
            llm_router: router,
            tool_registry: registry.clone(),
            tool_invoker: Arc::new(CountingInvoker(Arc::clone(&ran))),
            event_bus: bus,
            todo: None,
            plan: None,
        },
        ApolliaContext::default(),
        SessionWiring {
            governance: Some(crate::governance::GovernanceHandle::detached(
                GovernanceState::default(),
                None,
            )),
            unattended,
            ..SessionWiring::default()
        },
        &StepBudgetConfig::chat_default(),
        CancellationToken::new(),
    );
    let approvals = PendingChatApprovals::new();
    let answering = {
        let approvals = approvals.clone();
        let ran = Arc::clone(&ran);
        tokio::spawn(async move {
            let mut raised = Vec::new();
            let mut ran_before_answer = 0;
            let (seen, _) = crate::test_support::collect_events_until(
                Duration::from_secs(5),
                &mut events,
                |event| matches!(event, RuntimeEvent::ChatApprovalRequired { .. }),
            )
            .await;
            if let Some(RuntimeEvent::ChatApprovalRequired { tool_call_id, .. }) = seen.last() {
                ran_before_answer = ran.load(Ordering::SeqCst);
                raised.push(tool_call_id.clone());
                let key = format!("task-1::task-1::{tool_call_id}");
                crate::test_support::poll_until(Duration::from_secs(5), || {
                    approvals.resolve(&key, ToolDecision::refuse())
                })
                .await;
            }
            (raised, ran_before_answer)
        })
    };
    let response = executor
        .run(RunInput {
            session_id: "task-1",
            message_id: "task-1",
            run_id: &RunId::new(),
            user_message: "write the report",
            history: &[],
            authorized_tools: &HashSet::new(),
            pending_approvals: &approvals,
            summary: None,
            context_window_size: 20,
        })
        .await
        .expect("the run ends");
    let (approvals, ran_before_answer) = answering.await.unwrap();
    registry.shutdown().await;
    OffChatRun {
        approvals,
        ran_before_answer,
        ran: ran.load(Ordering::SeqCst),
        status: response.tool_calls[0].status.clone(),
    }
}

// GIVEN an agent run outside any conversation, holding no "always" for the
//       family that writes
// WHEN the model asks to write a file
// THEN the run raises an approval before anything runs, and the refusal
//      leaves the file unwritten
#[tokio::test]
async fn test_off_chat_write_asks_and_never_runs_on_its_own() {
    // GIVEN / WHEN
    let run = run_off_chat_write(false).await;

    // THEN
    assert_eq!(run.approvals, vec!["w1".to_string()]);
    assert_eq!(run.ran_before_answer, 0);
    assert_eq!(run.ran, 0);
    assert_eq!(run.status, ToolCallStatus::Refused);
}

// GIVEN the same run, watched by nobody
// WHEN the model asks to write a file
// THEN governance answers that the call waits, the run raises the approval
//      the batch engine turns into a waiting item, and nothing runs
#[tokio::test]
async fn test_unattended_off_chat_write_waits_and_never_runs() {
    // GIVEN
    let governance =
        crate::governance::GovernanceHandle::detached(GovernanceState::default(), None);
    let call = AccessCall {
        agent_id: "ledger-clerk",
        access_id: "file_write",
        argument: Some("/tmp/report.txt"),
        family: apollia_tools::ActionFamily::WriteOrDelete,
        unattended: true,
        granted_for_run: false,
    };

    // WHEN
    let decision = governance.decide(&call);
    let run = run_off_chat_write(true).await;

    // THEN
    assert!(matches!(decision, AccessDecision::Pending { .. }));
    assert_eq!(run.approvals, vec!["w1".to_string()]);
    assert_eq!(run.ran_before_answer, 0);
    assert_eq!(run.ran, 0);
}

/// Streams two calls to `bash_executor`, one per turn, then final text.
struct CodeTwice {
    turn: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for CodeTwice {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::InferenceError("streaming path only".into()))
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let turn = self.turn.fetch_add(1, Ordering::SeqCst);
        let chunk = if turn < 2 {
            StreamChunk::ToolCall(ToolCall {
                id: format!("c{}", turn + 1),
                name: "bash_executor".into(),
                arguments: serde_json::json!({ "command": "ls" }),
            })
        } else {
            StreamChunk::Text("done".into())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "code-twice"
    }
    fn model_id(&self) -> &str {
        "code-twice"
    }
}

// GIVEN an unattended run whose caller allowed one call of `bash_executor`
// WHEN the model executes code twice
// THEN the first call runs without asking, the second raises an approval and
//      does not run: the grant was spent by the first
#[tokio::test]
async fn test_an_unattended_run_allowed_code_once_runs_one_call_and_asks_at_the_next() {
    // GIVEN
    let mut backends = std::collections::HashMap::new();
    backends.insert(
        "default".to_string(),
        Arc::new(CodeTwice {
            turn: AtomicU32::new(0),
        }) as Arc<dyn CompletionModel>,
    );
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let registry = ToolRegistryHandle::start();
    let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    descriptor.name = "bash_executor".into();
    descriptor.family = apollia_tools::ActionFamily::ExecuteCode;
    descriptor.is_read_only = false;
    descriptor.input_schema = serde_json::json!({ "type": "object" });
    registry.register(descriptor).await.unwrap();
    let ran = Arc::new(AtomicU32::new(0));
    let executor = AgentExecutor::new(
        AgentRunSpec {
            agent_id: "ledger-clerk".into(),
            mission: "List the files.".into(),
            granted_tools: vec!["bash_executor".into()],
            model: None,
            autonomy: AutonomyLevel::Assisted,
        },
        AgentExecutorDeps {
            llm_router: router,
            tool_registry: registry.clone(),
            tool_invoker: Arc::new(CountingInvoker(Arc::clone(&ran))),
            event_bus: bus,
            todo: None,
            plan: None,
        },
        ApolliaContext::default(),
        SessionWiring {
            governance: Some(crate::governance::GovernanceHandle::detached(
                GovernanceState::default(),
                None,
            )),
            unattended: true,
            ..SessionWiring::default()
        },
        &StepBudgetConfig::chat_default(),
        CancellationToken::new(),
    );
    let approvals = PendingChatApprovals::new();
    let answering = {
        let approvals = approvals.clone();
        let ran = Arc::clone(&ran);
        tokio::spawn(async move {
            let (seen, _) = crate::test_support::collect_events_until(
                Duration::from_secs(5),
                &mut events,
                |event| matches!(event, RuntimeEvent::ChatApprovalRequired { .. }),
            )
            .await;
            let Some(RuntimeEvent::ChatApprovalRequired { tool_call_id, .. }) = seen.last() else {
                return (Vec::new(), 0);
            };
            let ran_before_answer = ran.load(Ordering::SeqCst);
            let key = format!("task-1::task-1::{tool_call_id}");
            crate::test_support::poll_until(Duration::from_secs(5), || {
                approvals.resolve(&key, ToolDecision::refuse())
            })
            .await;
            (vec![tool_call_id.clone()], ran_before_answer)
        })
    };
    let authorized: HashSet<String> = [crate::governance::once_grant("bash_executor")].into();

    // WHEN
    let response = executor
        .run(RunInput {
            session_id: "task-1",
            message_id: "task-1",
            run_id: &RunId::new(),
            user_message: "list the files",
            history: &[],
            authorized_tools: &authorized,
            pending_approvals: &approvals,
            summary: None,
            context_window_size: 20,
        })
        .await
        .expect("the run ends");
    let (raised, ran_before_answer) = answering.await.unwrap();
    registry.shutdown().await;

    // THEN
    assert_eq!(raised, vec!["c2".to_string()]);
    assert_eq!(ran_before_answer, 1);
    assert_eq!(ran.load(Ordering::SeqCst), 1);
    assert_eq!(response.tool_calls[1].status, ToolCallStatus::Refused);
}
