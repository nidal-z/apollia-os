//! A tool of an MCP server the user added is decided like any other tool: by
//! its action family, through governance, with the three answers of a consent
//! prompt. Nothing the server declares about approval takes part.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk, ToolCall,
};
use apollia_mcp::classify::classify_mcp_tool;
use apollia_mcp::protocol::McpToolAnnotations;

use super::*;
use crate::chat::types::{ToolCallStatus, ToolDecision};

/// The tool a user-added server publishes, under its registry name.
const DELETE_CONTACT: &str = "mcp:crm/delete_contact";

/// Streams two calls to [`DELETE_CONTACT`], one per turn, then final text.
struct DeleteTwice {
    turn: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for DeleteTwice {
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
                id: format!("d{}", turn + 1),
                name: DELETE_CONTACT.into(),
                arguments: serde_json::json!({ "id": 42 + turn }),
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
        "delete-twice"
    }
    fn model_id(&self) -> &str {
        "delete-twice"
    }
}

/// Counts the calls that actually reached the server.
struct CountingInvoker(Arc<AtomicU32>);

#[async_trait::async_trait]
impl ToolInvoker for CountingInvoker {
    async fn invoke(&self, _tool: &str, _args: &serde_json::Value) -> Result<String, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("deleted".into())
    }
}

/// What a run deleting two contacts produced: the approvals it raised, how
/// many calls had run when each was raised, how many ran in all, and the
/// status of each call.
struct McpRun {
    approvals: Vec<String>,
    ran_when_asked: Vec<u32>,
    ran: u32,
    statuses: Vec<ToolCallStatus>,
}

/// A governance backed by a database, so that an "always" can be recorded.
fn governance(dir: &std::path::Path) -> crate::governance::GovernanceHandle {
    apollia_tools::GovernanceDb::open(dir).unwrap();
    crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
        governance_db: Some(dir.join("governance.db")),
        config_path: None,
        policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
        journal: None,
        events: None,
        forbidden_connections: Default::default(),
    })
}

/// Runs an agent that deletes two contacts through a server the user added,
/// answering every approval it raises with `answer`.
async fn run_mcp_delete(answer: ToolDecision) -> McpRun {
    let dir = tempfile::tempdir().unwrap();
    let mut backends = std::collections::HashMap::new();
    backends.insert(
        "default".to_string(),
        Arc::new(DeleteTwice {
            turn: AtomicU32::new(0),
        }) as Arc<dyn CompletionModel>,
    );
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let end_of_run = bus.clone();
    let registry = ToolRegistryHandle::start();

    // The family is the one the MCP registration decides for a tool the
    // server declares destructive, with no entry in the curated selection.
    let declared = McpToolAnnotations {
        destructive_hint: Some(true),
        ..McpToolAnnotations::default()
    };
    let (family, family_source) = classify_mcp_tool(None, Some(&declared), false);
    let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    descriptor.name = DELETE_CONTACT.into();
    descriptor.family = family;
    descriptor.family_source = family_source;
    descriptor.is_read_only = false;
    descriptor.input_schema = serde_json::json!({ "type": "object" });
    registry.register(descriptor).await.unwrap();

    let ran = Arc::new(AtomicU32::new(0));
    let executor = AgentExecutor::new(
        AgentRunSpec {
            agent_id: "crm-clerk".into(),
            mission: "Remove the duplicate contacts.".into(),
            granted_tools: vec![DELETE_CONTACT.into()],
            model: None,
            autonomy: AutonomyLevel::Assisted,
            planning: apollia_tools::PlanningMode::Never,
            version: String::new(),
        },
        AgentExecutorDeps {
            llm_router: router,
            tool_registry: registry.clone(),
            tool_invoker: Arc::new(CountingInvoker(Arc::clone(&ran))),
            event_bus: bus,
            todo: None,
            plan: None,
            plan_cache: None,
        },
        ApolliaContext::default(),
        SessionWiring {
            governance: Some(governance(dir.path())),
            unattended: false,
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
            let mut ran_when_asked = Vec::new();
            loop {
                let (seen, _) = crate::test_support::collect_events_until(
                    Duration::from_secs(5),
                    &mut events,
                    |event| {
                        matches!(
                            event,
                            RuntimeEvent::ChatApprovalRequired { .. } | RuntimeEvent::AllReady
                        )
                    },
                )
                .await;
                let Some(RuntimeEvent::ChatApprovalRequired { tool_call_id, .. }) = seen.last()
                else {
                    break;
                };
                ran_when_asked.push(ran.load(Ordering::SeqCst));
                raised.push(tool_call_id.clone());
                let key = format!("task-1::task-1::{tool_call_id}");
                let answer = answer.clone();
                crate::test_support::poll_until(Duration::from_secs(5), || {
                    approvals.resolve(&key, answer.clone())
                })
                .await;
            }
            (raised, ran_when_asked)
        })
    };
    let response = executor
        .run(RunInput {
            session_id: "task-1",
            message_id: "task-1",
            run_id: &RunId::new(),
            user_message: "remove the duplicates",
            history: &[],
            authorized_tools: &HashSet::new(),
            pending_approvals: &approvals,
            summary: None,
            context_window_size: 20,
        })
        .await
        .expect("the run ends");
    // Ends the wait of the answering task.
    let _ = end_of_run.send(RuntimeEvent::AllReady);
    let (approvals, ran_when_asked) = answering.await.unwrap();
    registry.shutdown().await;
    McpRun {
        approvals,
        ran_when_asked,
        ran: ran.load(Ordering::SeqCst),
        statuses: response
            .tool_calls
            .iter()
            .map(|call| call.status.clone())
            .collect(),
    }
}

// GIVEN an agent holding a destructive tool of a server the user added, and
//       no standing consent
// WHEN it deletes two contacts and each approval is answered "once"
// THEN every call asks before it runs, and each runs once answered
#[tokio::test]
async fn test_a_user_added_mcp_tool_asks_at_every_call_when_answered_once() {
    // GIVEN / WHEN
    let run = run_mcp_delete(ToolDecision::Accept).await;

    // THEN
    assert_eq!(run.approvals, vec!["d1".to_string(), "d2".to_string()]);
    assert_eq!(run.ran_when_asked, vec![0, 1]);
    assert_eq!(run.ran, 2);
}

// GIVEN the same agent and the same tool
// WHEN the first approval is answered "always for this agent"
// THEN the first call asks, and the second runs on the standing consent
//      governance recorded for the family, without asking
#[tokio::test]
async fn test_a_user_added_mcp_tool_stops_asking_once_answered_always() {
    // GIVEN / WHEN
    let run = run_mcp_delete(ToolDecision::AlwaysAccept).await;

    // THEN
    assert_eq!(run.approvals, vec!["d1".to_string()]);
    assert_eq!(run.ran_when_asked, vec![0]);
    assert_eq!(run.ran, 2);
}

// GIVEN the same agent and the same tool
// WHEN every approval is refused
// THEN nothing reaches the server
#[tokio::test]
async fn test_a_user_added_mcp_tool_never_runs_when_refused() {
    // GIVEN / WHEN
    let run = run_mcp_delete(ToolDecision::refuse()).await;

    // THEN
    assert_eq!(run.approvals, vec!["d1".to_string(), "d2".to_string()]);
    assert_eq!(run.ran_when_asked, vec![0, 0]);
    assert_eq!(run.ran, 0);
    assert_eq!(
        run.statuses,
        vec![ToolCallStatus::Refused, ToolCallStatus::Refused]
    );
}
