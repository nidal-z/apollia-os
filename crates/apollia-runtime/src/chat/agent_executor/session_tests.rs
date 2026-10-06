//! What a conversation changes from its agent reaches the run of any agent,
//! Apollia or installed: its instructions after the mission, what its context
//! does at the limit, the accesses it proposes. The profile is placed in the
//! prompt of Apollia alone.

use std::collections::HashSet;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_memory::user_memory::WrittenBy;
use apollia_oria::context_manager::ContextOverflow;
use apollia_tools::{ActionFamily, PlanningMode};

use super::*;
use crate::chat::builtin_agent::SessionOverrides;
use crate::chat::types::{ChatRole, ToolDecision};
use crate::chat::SessionSettings;

/// Records every request streamed through it, answers its script then plain
/// text, and counts the summaries it is asked for.
struct Recording {
    requests: Arc<Mutex<Vec<CompletionRequest>>>,
    script: Mutex<VecDeque<StreamChunk>>,
    summaries: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl CompletionModel for Recording {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.summaries.fetch_add(1, Ordering::SeqCst);
        Ok(CompletionResponse {
            engine_timings: None,
            content: "SUMMARY OF THE OLDER MESSAGES".into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        let chunk = self
            .script
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| StreamChunk::Text("done".into()));
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "default"
    }
    fn model_id(&self) -> &str {
        "default"
    }
}

/// Counts the calls that actually ran.
struct CountingInvoker(Arc<AtomicU32>);

#[async_trait::async_trait]
impl ToolInvoker for CountingInvoker {
    async fn invoke(&self, _tool: &str, _args: &serde_json::Value) -> Result<String, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("read".into())
    }
}

/// One run of an agent under the overrides of a conversation.
struct Rig {
    executor: AgentExecutor,
    requests: Arc<Mutex<Vec<CompletionRequest>>>,
    summaries: Arc<AtomicU32>,
    ran: Arc<AtomicU32>,
    bus: EventBusSender,
    registry: ToolRegistryHandle,
}

impl Rig {
    /// `agent_id` runs `mission`, granted two reading tools, under `session`;
    /// its model answers `script`.
    async fn of(
        agent_id: &str,
        mission: &str,
        granted: &[&str],
        session: SessionOverrides,
        script: Vec<StreamChunk>,
    ) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let summaries = Arc::new(AtomicU32::new(0));
        let ran = Arc::new(AtomicU32::new(0));
        let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
            std::collections::HashMap::new();
        backends.insert(
            "default".into(),
            Arc::new(Recording {
                requests: Arc::clone(&requests),
                script: Mutex::new(script.into()),
                summaries: Arc::clone(&summaries),
            }),
        );
        let registry = ToolRegistryHandle::start();
        for name in ["notes_read", "web_lookup"] {
            let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
            descriptor.name = name.into();
            descriptor.family = ActionFamily::Read;
            descriptor.is_read_only = true;
            descriptor.input_schema = serde_json::json!({ "type": "object" });
            registry.register(descriptor).await.expect("register tool");
        }
        let (bus, _) = tokio::sync::broadcast::channel(64);
        let executor = AgentExecutor::new(
            AgentRunSpec {
                agent_id: agent_id.into(),
                mission: mission.into(),
                granted_tools: granted.iter().map(|t| (*t).to_owned()).collect(),
                model: None,
                autonomy: AutonomyLevel::Assisted,
                planning: PlanningMode::Never,
                version: String::new(),
            },
            AgentExecutorDeps {
                llm_router: Arc::new(LlmRouter::with_backends(backends, "default")),
                tool_registry: registry.clone(),
                tool_invoker: Arc::new(CountingInvoker(Arc::clone(&ran))),
                event_bus: bus.clone(),
                todo: None,
                plan: None,
                plan_cache: None,
            },
            ApolliaContext::default(),
            SessionWiring {
                session,
                ..SessionWiring::default()
            },
            &StepBudgetConfig::chat_default(),
            CancellationToken::new(),
        );
        Self {
            executor,
            requests,
            summaries,
            ran,
            bus,
            registry,
        }
    }

    async fn run(
        &self,
        history: &[ChatMessage],
        approvals: &PendingChatApprovals,
    ) -> Result<ChatAgentResponse, ChatError> {
        self.executor
            .run(RunInput {
                session_id: "s1",
                message_id: "m1",
                run_id: &RunId::new(),
                user_message: "current request",
                history,
                authorized_tools: &HashSet::new(),
                pending_approvals: approvals,
                summary: None,
                context_window_size: usize::MAX,
            })
            .await
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn text_of(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s,
        MessageContent::TextWithImages { text, .. } => text,
        MessageContent::ToolResult { content, .. } => content,
        MessageContent::WithToolCalls { text, .. } => text,
    }
}

fn system_prompt_of(request: &CompletionRequest) -> String {
    request
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| text_of(&m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn said(role: ChatRole, seq: u32, content: String) -> ChatMessage {
    ChatMessage {
        id: format!("h{seq}"),
        role,
        content,
        tool_calls: None,
        tool_name: None,
        created_at: "2026-10-06T00:00:00Z".into(),
        seq,
        metadata: None,
    }
}

/// Two earlier exchanges estimated at 210 000 tokens with the request, for a
/// window of 200 000.
fn history_over_the_limit() -> Vec<ChatMessage> {
    vec![
        said(ChatRole::User, 1, format!("first {}", "x".repeat(400_000))),
        said(ChatRole::Assistant, 2, "first answer".into()),
        said(ChatRole::User, 3, format!("second {}", "y".repeat(300_000))),
        said(ChatRole::Assistant, 4, "second answer".into()),
    ]
}

fn overflowing(overflow: ContextOverflow) -> SessionOverrides {
    SessionOverrides {
        overflow,
        ..SessionOverrides::default()
    }
}

#[tokio::test]
async fn test_an_installed_agent_keeps_its_mission_and_receives_the_instructions_after_it() {
    // GIVEN an installed agent with a mission, in a conversation that adds
    // instructions of its own
    let rig = Rig::of(
        "ledger-clerk",
        "Reconcile the ledger.",
        &["notes_read"],
        SessionOverrides {
            instructions: Some("Answer in one sentence.".into()),
            ..SessionOverrides::default()
        },
        Vec::new(),
    )
    .await;

    // WHEN the agent runs one exchange
    rig.run(&[], &PendingChatApprovals::new())
        .await
        .expect("the run ends");

    // THEN its call carries the mission, then the instructions under their
    // heading
    let prompt = system_prompt_of(&rig.requests()[0]);
    let mission_at = prompt.find("Reconcile the ledger.").expect("mission kept");
    let heading_at = prompt
        .find(apollia_prompts::blocks::SESSION_INSTRUCTIONS_HEADING)
        .expect("heading");
    let text_at = prompt
        .find("Answer in one sentence.")
        .expect("instructions");
    assert!(mission_at < heading_at && heading_at < text_at, "{prompt}");
    rig.registry.shutdown().await;
}

#[tokio::test]
async fn test_summarizing_at_the_limit_asks_for_a_summary_and_sends_it() {
    // GIVEN a conversation on the default overflow, over the limit
    let rig = Rig::of(
        "ledger-clerk",
        "Reconcile the ledger.",
        &[],
        overflowing(ContextOverflow::SummarizeMiddle),
        Vec::new(),
    )
    .await;

    // WHEN the agent runs the next exchange
    rig.run(&history_over_the_limit(), &PendingChatApprovals::new())
        .await
        .expect("the run ends");

    // THEN a summary was asked for, and the call carries it in place of the
    // older messages
    let request = &rig.requests()[0];
    assert!(rig.summaries.load(Ordering::SeqCst) >= 1);
    assert!(system_prompt_of(request).contains("SUMMARY OF THE OLDER MESSAGES"));
    assert!(request.messages.len() < 6, "{}", request.messages.len());
    rig.registry.shutdown().await;
}

#[tokio::test]
async fn test_a_sliding_window_drops_the_oldest_exchange_and_asks_for_no_summary() {
    // GIVEN a conversation on the sliding window, over the limit
    let rig = Rig::of(
        "ledger-clerk",
        "Reconcile the ledger.",
        &[],
        overflowing(ContextOverflow::SlidingWindow),
        Vec::new(),
    )
    .await;

    // WHEN the agent runs the next exchange
    rig.run(&history_over_the_limit(), &PendingChatApprovals::new())
        .await
        .expect("the run ends");

    // THEN no summary was asked for, the first exchange is gone and the
    // second and the request are sent as they were said
    let request = &rig.requests()[0];
    let said: Vec<&str> = request
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| text_of(&m.content))
        .collect();
    assert_eq!(rig.summaries.load(Ordering::SeqCst), 0);
    assert_eq!(said.len(), 3, "{}", said.len());
    assert!(said[0].starts_with("second "));
    assert_eq!(said[2], "current request");
    assert!(!system_prompt_of(request).contains("SUMMARY"));
    rig.registry.shutdown().await;
}

#[tokio::test]
async fn test_stopping_at_the_limit_fails_the_turn_with_a_typed_error_and_calls_no_model() {
    // GIVEN a conversation set to stop at the limit, over it
    let rig = Rig::of(
        "ledger-clerk",
        "Reconcile the ledger.",
        &[],
        overflowing(ContextOverflow::StopAtLimit),
        Vec::new(),
    )
    .await;

    // WHEN the agent runs the next exchange
    let outcome = rig
        .run(&history_over_the_limit(), &PendingChatApprovals::new())
        .await;

    // THEN the turn fails with the measure of the limit, a message a person
    // can act on, no summary and no call
    let error = outcome.expect_err("the limit is reached");
    assert!(
        matches!(error, ChatError::ContextLimitReached { tokens, limit } if tokens >= limit && limit == 160_000),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("context limit of its model"), "{message}");
    assert!(message.contains("fork it"), "{message}");
    assert!(rig.requests().is_empty());
    assert_eq!(rig.summaries.load(Ordering::SeqCst), 0);
    rig.registry.shutdown().await;
}

#[tokio::test]
async fn test_a_proposed_reading_access_asks_where_a_granted_one_runs() {
    // GIVEN an agent granted `notes_read`, in a conversation that proposes
    // `web_lookup`: both read, and governance lets a granted reading run
    // unasked. The model calls one, then the other
    let call = |id: &str, name: &str| {
        StreamChunk::ToolCall(ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        })
    };
    let rig = Rig::of(
        "ledger-clerk",
        "Reconcile the ledger.",
        &["notes_read", "web_lookup"],
        SessionOverrides {
            proposed_accesses: vec!["web_lookup".into()],
            ..SessionOverrides::default()
        },
        vec![call("c1", "notes_read"), call("c2", "web_lookup")],
    )
    .await;
    let approvals = PendingChatApprovals::new();
    let mut events = rig.bus.subscribe();
    let answering = {
        let approvals = approvals.clone();
        let ran = Arc::clone(&rig.ran);
        tokio::spawn(async move {
            let (seen, asked) = crate::test_support::collect_events_until(
                Duration::from_secs(5),
                &mut events,
                |event| matches!(event, RuntimeEvent::ChatApprovalRequired { .. }),
            )
            .await;
            let asked_about = match seen.last() {
                Some(RuntimeEvent::ChatApprovalRequired {
                    tool_call_id,
                    tool_name,
                    ..
                }) if asked => {
                    let ran_before = ran.load(Ordering::SeqCst);
                    let key = format!("s1::m1::{tool_call_id}");
                    crate::test_support::poll_until(Duration::from_secs(5), || {
                        approvals.resolve(&key, ToolDecision::Accept)
                    })
                    .await;
                    Some((tool_name.clone(), ran_before))
                }
                _ => None,
            };
            asked_about
        })
    };

    // WHEN the agent runs the exchange and the question is answered "once"
    rig.run(&[], &approvals).await.expect("the run ends");
    let asked_about = answering.await.expect("answering task");

    // THEN the granted access ran without a question, the proposed one asked
    // before it ran, and ran once accepted
    assert_eq!(asked_about, Some(("web_lookup".to_string(), 1)));
    assert_eq!(rig.ran.load(Ordering::SeqCst), 2);
    rig.registry.shutdown().await;
}

#[tokio::test]
async fn test_the_profile_is_injected_for_apollia_and_only_readable_for_an_installed_agent() {
    // GIVEN a profile store, and the two cases of the profile switch of a
    // conversation: Apollia with its switch on, and an installed agent whose
    // profile source is cleared while the conversation opens it
    let dir = tempfile::tempdir().unwrap();
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).expect("store");
    repo.set("role", "accountant", WrittenBy::User)
        .expect("set role");
    let store = Arc::new(std::sync::Mutex::new(repo));
    let context = || ApolliaContext {
        user_memory: Some(Arc::clone(&store)),
        profile_id: None,
        past_sessions: None,
    };
    let executor_of = |agent_id: &str| {
        let (bus, _) = tokio::sync::broadcast::channel(16);
        AgentExecutor::new(
            AgentRunSpec {
                agent_id: agent_id.into(),
                mission: "Help with the books.".into(),
                granted_tools: vec![],
                model: None,
                autonomy: AutonomyLevel::Assisted,
                planning: PlanningMode::Never,
                version: String::new(),
            },
            AgentExecutorDeps {
                llm_router: Arc::new(LlmRouter::empty()),
                tool_registry: ToolRegistryHandle::start(),
                tool_invoker: Arc::new(CountingInvoker(Arc::new(AtomicU32::new(0)))),
                event_bus: bus,
                todo: None,
                plan: None,
                plan_cache: None,
            },
            context(),
            SessionWiring::default(),
            &StepBudgetConfig::chat_default(),
            CancellationToken::new(),
        )
    };
    let opened = SessionSettings {
        profile_access: Some(true),
        ..SessionSettings::default()
    };

    // WHEN the prompt of each is built
    let apollia = executor_of(APOLLIA_CHAT_AGENT_ID)
        .agent()
        .build_system_prompt(Some("Help with the books."), AutonomyLevel::Assisted);
    let installed = executor_of("ledger-clerk")
        .agent()
        .build_system_prompt(Some("Help with the books."), AutonomyLevel::Assisted);

    // THEN Apollia receives the profile in its prompt; the installed agent
    // never does, and the conversation only opens the reading its source
    // had cleared
    assert!(apollia.contains("## User Persona"), "{apollia}");
    assert!(apollia.contains("accountant"), "{apollia}");
    assert!(!installed.contains("## User Persona"), "{installed}");
    assert!(!installed.contains("accountant"), "{installed}");
    assert!(opened.profile_readable(false));
    assert!(!SessionSettings::default().profile_readable(false));
}
