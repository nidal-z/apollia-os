//! Plan first through the agent executor: when it engages, the plan cache,
//! the tools a step may call, the decision governance takes on each call of a
//! step, and the stop.
//!
//! The first test is the characterization of the plan cache that pinned the
//! ORIA engine, aimed at the executor with the same assertions.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use apollia_core::{AutonomyLevel, RunId, RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_tools::{PlanningMode, ToolRegistryHandle};
use tokio_util::sync::CancellationToken;

use crate::chat::agent_executor::{
    AgentExecutor, AgentExecutorDeps, AgentRunSpec, ApolliaContext, RunInput, SessionWiring,
};
use crate::chat::types::{PendingChatApprovals, ToolDecision};
use crate::governance::{GovernanceHandle, GovernanceState, PolicyRefusals};

const PLAN_JSON: &str = r#"{"steps":[
    {"step_id":"s1","description":"read the input","tool_hint":"file_io","depends_on":[]},
    {"step_id":"s2","description":"run the check","tool_hint":"bash_executor","depends_on":["s1"]}
]}"#;

/// A plan of one step, which writes.
const WRITE_PLAN_JSON: &str = r#"{"steps":[
    {"step_id":"s1","description":"write the report","tool_hint":"file_write","depends_on":[]}
]}"#;

/// Answers every completion with the same plan and counts the calls.
struct CountingPlanModel {
    calls: AtomicU32,
    plan: &'static str,
}

impl CountingPlanModel {
    fn new(plan: &'static str) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicU32::new(0),
            plan,
        })
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl CompletionModel for CountingPlanModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.plan.to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError("mock does not stream".into()))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "counting-plan"
    }

    fn model_id(&self) -> &str {
        "counting-plan"
    }
}

/// Answers every call with the tool's name, and keeps the names called.
struct EchoInvoker {
    called: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl ToolInvoker for EchoInvoker {
    async fn invoke(&self, tool_name: &str, _args: &serde_json::Value) -> Result<String, String> {
        self.called
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(tool_name.to_string());
        Ok(format!("{tool_name} ok"))
    }
}

fn spec(planning: PlanningMode, granted: &[&str]) -> AgentRunSpec {
    AgentRunSpec {
        agent_id: "parity-agent".into(),
        mission: "Plan the necessary steps.".into(),
        granted_tools: granted.iter().map(|t| t.to_string()).collect(),
        model: None,
        autonomy: AutonomyLevel::Assisted,
        planning,
        version: "1.0.0".into(),
    }
}

fn ceiling() -> StepBudgetConfig {
    StepBudgetConfig {
        max_steps: 20,
        max_tool_calls: 50,
        wall_clock_secs: 600,
    }
}

struct Harness {
    executor: AgentExecutor,
    model: Arc<CountingPlanModel>,
    called: Arc<Mutex<Vec<String>>>,
    bus: crate::eventbus::EventBusSender,
    events: tokio::sync::broadcast::Receiver<RuntimeEvent>,
}

impl Harness {
    fn called(&self) -> Vec<String> {
        self.called
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Governance in developer mode with an allow rule on an empty prefix for
/// each code executor, so the single simple command of a step runs unasked.
fn code_runs() -> GovernanceHandle {
    let rules = apollia_permissions::CODE_EXECUTOR_TOOLS
        .iter()
        .enumerate()
        .map(|(i, tool)| apollia_permissions::PrefixRule {
            id: i as i64 + 1,
            tool_name: (*tool).to_owned(),
            arg_prefix: Some(String::new()),
            action: apollia_permissions::RuleAction::Allow,
            scope: apollia_permissions::PermissionScope::Global,
            ..apollia_permissions::PrefixRule::default()
        })
        .collect();
    GovernanceHandle::detached(
        GovernanceState {
            rules,
            developer_mode: true,
            ..GovernanceState::default()
        },
        None,
    )
}

/// Governance that holds nothing: reading runs, everything sensitive asks.
fn empty_governance() -> GovernanceHandle {
    GovernanceHandle::detached(GovernanceState::default(), None)
}

/// Registers `name` in `family`, with an open schema.
async fn describe(registry: &ToolRegistryHandle, name: &str, family: apollia_tools::ActionFamily) {
    let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    descriptor.name = name.to_owned();
    descriptor.family = family;
    descriptor.is_read_only = family == apollia_tools::ActionFamily::Read;
    descriptor.input_schema = serde_json::json!({ "type": "object" });
    registry.register(descriptor).await.expect("register tool");
}

/// A harness whose code executor runs unasked: see [`code_runs`].
async fn harness(
    spec: AgentRunSpec,
    plan: &'static str,
    plan_cache: Option<Arc<Mutex<PlanCacheRepository>>>,
    cancel: CancellationToken,
) -> Harness {
    harness_under(spec, plan, plan_cache, cancel, code_runs()).await
}

/// A harness whose tool calls are decided by `governance`. `file_io` reads,
/// `file_write` writes, `bash_executor` runs code.
async fn harness_under(
    spec: AgentRunSpec,
    plan: &'static str,
    plan_cache: Option<Arc<Mutex<PlanCacheRepository>>>,
    cancel: CancellationToken,
    governance: GovernanceHandle,
) -> Harness {
    let tool_registry = ToolRegistryHandle::start();
    describe(&tool_registry, "file_io", apollia_tools::ActionFamily::Read).await;
    describe(
        &tool_registry,
        "file_write",
        apollia_tools::ActionFamily::WriteOrDelete,
    )
    .await;
    let model = CountingPlanModel::new(plan);
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("planner".into(), model.clone());
    let (bus, events) = tokio::sync::broadcast::channel::<RuntimeEvent>(256);
    let called = Arc::new(Mutex::new(Vec::new()));
    let executor = AgentExecutor::new(
        spec,
        AgentExecutorDeps {
            llm_router: Arc::new(LlmRouter::with_backends(backends, "planner")),
            tool_registry,
            tool_invoker: Arc::new(EchoInvoker {
                called: Arc::clone(&called),
            }),
            event_bus: bus.clone(),
            todo: None,
            plan: None,
            plan_cache,
        },
        ApolliaContext::default(),
        SessionWiring {
            governance: Some(governance),
            ..SessionWiring::default()
        },
        &ceiling(),
        cancel,
    );
    Harness {
        executor,
        model,
        called,
        bus,
        events,
    }
}

async fn run(
    executor: &AgentExecutor,
    message: &str,
    authorized: &HashSet<String>,
    approvals: &PendingChatApprovals,
) -> Result<crate::chat::builtin_agent::ChatAgentResponse, crate::chat::types::ChatError> {
    executor
        .run(RunInput {
            session_id: "task-1",
            message_id: "task-1",
            run_id: &RunId::new(),
            user_message: message,
            history: &[],
            authorized_tools: authorized,
            pending_approvals: approvals,
            summary: None,
            context_window_size: 20,
        })
        .await
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn count_cache_hits(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::PlanCacheHit { .. }))
        .count()
}

fn count_plans_generated(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::PlanGenerated { .. }))
        .count()
}

fn both_tools() -> HashSet<String> {
    HashSet::from(["file_io".to_string(), "bash_executor".to_string()])
}

#[tokio::test]
async fn test_orchestrated_second_identical_task_is_served_by_the_plan_cache() {
    // GIVEN an agent that plans first, a shared plan cache and a counting model
    let dir = tempfile::tempdir().expect("tempdir");
    let repo =
        PlanCacheRepository::open(&dir.path().join("plan_cache.db")).expect("open plan cache");
    let mut h = harness(
        spec(PlanningMode::Always, &["file_io", "bash_executor"]),
        PLAN_JSON,
        Some(Arc::new(Mutex::new(repo))),
        CancellationToken::new(),
    )
    .await;
    let approvals = PendingChatApprovals::new();
    let first = run(&h.executor, "check the input", &both_tools(), &approvals).await;
    let first_events = drain(&mut h.events);
    let calls_after_first = h.model.calls();

    // WHEN the same task runs a second time
    let second = run(&h.executor, "check the input", &both_tools(), &approvals).await;
    let second_events = drain(&mut h.events);

    // THEN both runs complete, the first plans through the Reasoner and the
    // second is a cache hit that never asks the model for a plan
    assert!(
        first.is_ok() && second.is_ok(),
        "both runs must complete: {:?} / {:?}",
        first.as_ref().err(),
        second.as_ref().err()
    );
    assert_eq!(
        (
            count_plans_generated(&first_events),
            count_cache_hits(&first_events)
        ),
        (1, 0),
        "the first run plans through the Reasoner"
    );
    // The cached path still publishes `PlanGenerated`, so the interface shows
    // the plan it runs; what tells the two apart is the `PlanCacheHit` before it.
    assert_eq!(
        (
            count_plans_generated(&second_events),
            count_cache_hits(&second_events)
        ),
        (1, 1),
        "the second run is served from the cache"
    );
    assert!(
        calls_after_first >= 1,
        "the first run must call the model to plan"
    );
    assert_eq!(
        h.model.calls(),
        calls_after_first,
        "a cache hit runs the cached plan without one more model call"
    );
}

#[tokio::test]
async fn test_a_planned_run_calls_its_steps_tools_in_order_and_answers_their_outputs() {
    // GIVEN an agent that always plans, both tools pre-authorized
    let h = harness(
        spec(PlanningMode::Always, &["file_io", "bash_executor"]),
        PLAN_JSON,
        None,
        CancellationToken::new(),
    )
    .await;

    // WHEN it runs a task
    let response = run(
        &h.executor,
        "check the input",
        &both_tools(),
        &PendingChatApprovals::new(),
    )
    .await
    .expect("the plan completes");

    // THEN each step called its tool once, in plan order, and the answer
    // carries their outputs
    assert_eq!(h.called(), vec!["file_io", "bash_executor"]);
    let names: Vec<_> = response
        .tool_calls
        .iter()
        .map(|c| c.tool_name.as_str())
        .collect();
    assert_eq!(names, vec!["file_io", "bash_executor"]);
    assert!(response.content.contains("file_io ok"));
    assert!(response.content.contains("bash_executor ok"));
}

#[tokio::test]
async fn test_a_step_on_a_tool_the_agent_is_not_granted_never_reaches_the_tool() {
    // GIVEN an agent that always plans but is granted file_io alone
    let h = harness(
        spec(PlanningMode::Always, &["file_io"]),
        PLAN_JSON,
        None,
        CancellationToken::new(),
    )
    .await;

    // WHEN its plan reaches the bash_executor step
    let result = run(
        &h.executor,
        "check the input",
        &both_tools(),
        &PendingChatApprovals::new(),
    )
    .await;

    // THEN the run fails, and bash_executor was never called
    assert!(result.is_err());
    let called = h.called();
    assert!(called.contains(&"file_io".to_string()), "{called:?}");
    assert!(!called.contains(&"bash_executor".to_string()), "{called:?}");
}

#[tokio::test]
async fn test_a_step_that_runs_code_waits_for_the_human_and_a_read_does_not() {
    // GIVEN an agent that always plans, under a governance that holds
    // nothing: reading a file runs, running code asks
    let h = harness_under(
        spec(PlanningMode::Always, &["file_io", "bash_executor"]),
        PLAN_JSON,
        None,
        CancellationToken::new(),
        empty_governance(),
    )
    .await;
    let approvals = PendingChatApprovals::new();
    let answering = approvals.clone();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let mut events = h.bus.subscribe();
    let human = tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            if let RuntimeEvent::ChatApprovalRequired {
                session_id,
                message_id,
                tool_call_id,
                tool_name,
                ..
            } = event
            {
                let key = format!("{session_id}::{message_id}::{tool_call_id}");
                seen.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(tool_name);
                answering.resolve(
                    &key,
                    ToolDecision::Refuse {
                        reason: Some("not now".into()),
                    },
                );
            }
        }
    });

    // WHEN the task runs, the human refusing bash_executor
    let result = run(&h.executor, "check the input", &HashSet::new(), &approvals).await;
    human.abort();
    let asked = asked
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();

    // THEN file_io ran unasked, bash_executor never ran, and the run ends on
    // the refusal
    assert!(!asked.is_empty(), "running code asks");
    assert!(
        asked.iter().all(|tool| tool == "bash_executor"),
        "a read asks nobody: {asked:?}"
    );
    assert!(result.is_err());
    let called = h.called();
    assert!(called.contains(&"file_io".to_string()), "{called:?}");
    assert!(!called.contains(&"bash_executor".to_string()), "{called:?}");
}

#[tokio::test]
async fn test_a_stopped_planned_run_ends_paused_without_calling_a_tool() {
    // GIVEN an agent that always plans, its run already stopped
    let cancel = CancellationToken::new();
    cancel.cancel();
    let h = harness(
        spec(PlanningMode::Always, &["file_io", "bash_executor"]),
        PLAN_JSON,
        None,
        cancel,
    )
    .await;

    // WHEN it runs
    let response = run(
        &h.executor,
        "check the input",
        &both_tools(),
        &PendingChatApprovals::new(),
    )
    .await
    .expect("a stop is not a failure");

    // THEN it ends paused, before planning and before any tool
    assert!(response.paused);
    assert_eq!(h.model.calls(), 0);
    assert!(h.called().is_empty());
}

#[tokio::test]
async fn test_planning_mode_decides_when_a_run_plans_first() {
    // GIVEN an agent with one tool and a plain mission, a short request and
    // a long multi-step one
    let short = "What time is it?";
    let long = "Plan the work: first read every invoice of the month in the ledger \
                folder, then compare each total with the bank statement, then list \
                every gap with its supplier, then draft a reminder for each late \
                supplier, and finally write a summary of what was found and done.";
    let modes = |mode| async move {
        let mut agent = spec(mode, &["file_io"]);
        agent.mission = "Keep the ledger.".into();
        let h = harness(agent, PLAN_JSON, None, CancellationToken::new()).await;
        (h.executor.plans_first(short), h.executor.plans_first(long))
    };

    // WHEN each mode decides for both
    let never = modes(PlanningMode::Never).await;
    let auto = modes(PlanningMode::Auto).await;
    let always = modes(PlanningMode::Always).await;

    // THEN never and always hold whatever the request, auto plans the long one
    assert_eq!(never, (false, false));
    assert_eq!(auto, (false, true));
    assert_eq!(always, (true, true));
}

#[tokio::test]
async fn test_a_conversation_in_plan_mode_never_plans_first() {
    // GIVEN an agent that always plans, run in a conversation's plan mode
    let model = CountingPlanModel::new(PLAN_JSON);
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("planner".into(), model);
    let (bus, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
    let executor = AgentExecutor::new(
        spec(PlanningMode::Always, &["file_io"]),
        AgentExecutorDeps {
            llm_router: Arc::new(LlmRouter::with_backends(backends, "planner")),
            tool_registry: ToolRegistryHandle::start(),
            tool_invoker: Arc::new(EchoInvoker {
                called: Arc::new(Mutex::new(Vec::new())),
            }),
            event_bus: bus,
            todo: None,
            plan: None,
            plan_cache: None,
        },
        ApolliaContext::default(),
        SessionWiring {
            plan_mode: true,
            ..SessionWiring::default()
        },
        &ceiling(),
        CancellationToken::new(),
    );

    // WHEN it decides for a request
    let plans_first = executor.plans_first("check the input");

    // THEN the plan mode of the conversation is the plan, not plan first
    assert!(!plans_first);
}

/// What one run of the single writing step produced under a governance: the
/// answers each approval offered, whether the write ran, and the result.
struct GovernedWrite {
    offered: Vec<Vec<String>>,
    ran: bool,
    result: Result<crate::chat::builtin_agent::ChatAgentResponse, crate::chat::types::ChatError>,
}

/// Runs the plan of one writing step for `parity-agent` under `governance`,
/// a human giving `answer` to every approval raised.
async fn governed_write(governance: GovernanceHandle, answer: ToolDecision) -> GovernedWrite {
    let h = harness_under(
        spec(PlanningMode::Always, &["file_write"]),
        WRITE_PLAN_JSON,
        None,
        CancellationToken::new(),
        governance,
    )
    .await;
    let approvals = PendingChatApprovals::new();
    let answering = approvals.clone();
    let offered = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&offered);
    let called = Arc::clone(&h.called);
    let ran_early = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let early = Arc::clone(&ran_early);
    let mut events = h.bus.subscribe();
    let human = tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            if let RuntimeEvent::ChatApprovalRequired {
                session_id,
                message_id,
                tool_call_id,
                answers,
                ..
            } = event
            {
                if !called
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_empty()
                {
                    early.store(true, Ordering::SeqCst);
                }
                seen.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(answers);
                let key = format!("{session_id}::{message_id}::{tool_call_id}");
                answering.resolve(&key, answer.clone());
            }
        }
    });
    let result = run(&h.executor, "write the report", &HashSet::new(), &approvals).await;
    human.abort();
    assert!(
        !ran_early.load(Ordering::SeqCst),
        "the write ran before its answer"
    );
    let offered = offered
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    GovernedWrite {
        offered,
        ran: h.called().contains(&"file_write".to_string()),
        result,
    }
}

#[tokio::test]
async fn test_a_sensitive_step_nobody_consented_to_asks_with_the_three_answers() {
    // GIVEN a governance that holds no consent for the agent
    // WHEN the writing step of its plan is reached and the human answers once
    let run = governed_write(empty_governance(), ToolDecision::Accept).await;

    // THEN governance asked, offering once, always and refuse, and the write
    // ran after the answer
    assert_eq!(run.offered, vec![vec!["once", "always", "refuse"]]);
    assert!(run.ran);
    assert!(run.result.is_ok(), "{:?}", run.result.err());
}

#[tokio::test]
async fn test_a_sensitive_step_the_human_refuses_never_runs() {
    // GIVEN a governance that holds no consent for the agent
    // WHEN the human refuses each time the writing step asks
    let run = governed_write(empty_governance(), ToolDecision::refuse()).await;

    // THEN the write was asked about, never ran, and the run fails
    assert!(!run.offered.is_empty());
    assert!(!run.ran);
    assert!(run.result.is_err());
}

#[tokio::test]
async fn test_a_sensitive_step_runs_unasked_on_a_standing_consent_of_its_family() {
    // GIVEN the agent holding the standing consent of the family that writes
    let governance = GovernanceHandle::detached(
        GovernanceState {
            consents: HashSet::from([(
                "parity-agent".to_string(),
                apollia_permissions::ConsentFamily::WriteOrDelete,
            )]),
            ..GovernanceState::default()
        },
        None,
    );

    // WHEN the writing step of its plan is reached
    let run = governed_write(governance, ToolDecision::refuse()).await;

    // THEN nobody was asked and the write ran
    assert!(run.offered.is_empty(), "{:?}", run.offered);
    assert!(run.ran);
    assert!(run.result.is_ok(), "{:?}", run.result.err());
}

#[tokio::test]
async fn test_a_step_the_policy_refuses_never_runs_and_asks_nobody() {
    // GIVEN an organisation policy that refuses the tool, and an agent that
    // holds the standing consent of its family
    let governance = GovernanceHandle::detached(
        GovernanceState {
            policy: PolicyRefusals {
                denied: HashSet::from(["file_write".to_string()]),
                ..PolicyRefusals::default()
            },
            consents: HashSet::from([(
                "parity-agent".to_string(),
                apollia_permissions::ConsentFamily::WriteOrDelete,
            )]),
            ..GovernanceState::default()
        },
        None,
    );

    // WHEN the writing step of its plan is reached
    let run = governed_write(governance, ToolDecision::Accept).await;

    // THEN the policy decides: nobody is asked, the write never runs, the run
    // fails
    assert!(run.offered.is_empty(), "{:?}", run.offered);
    assert!(!run.ran);
    assert!(run.result.is_err());
}

#[tokio::test]
async fn test_a_step_a_deny_rule_matches_never_runs_and_asks_nobody() {
    // GIVEN a fine rule that denies the tool to every agent
    let governance = GovernanceHandle::detached(
        GovernanceState {
            rules: vec![apollia_permissions::PrefixRule {
                id: 7,
                tool_name: "file_write".into(),
                arg_prefix: None,
                action: apollia_permissions::RuleAction::Deny,
                scope: apollia_permissions::PermissionScope::Global,
                ..apollia_permissions::PrefixRule::default()
            }],
            ..GovernanceState::default()
        },
        None,
    );

    // WHEN the writing step of its plan is reached
    let run = governed_write(governance, ToolDecision::Accept).await;

    // THEN the rule decides: nobody is asked, the write never runs, the run
    // fails
    assert!(run.offered.is_empty(), "{:?}", run.offered);
    assert!(!run.ran);
    assert!(run.result.is_err());
}
