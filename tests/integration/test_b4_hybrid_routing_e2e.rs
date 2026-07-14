//! Integration tests - chantier B4: hybrid frontier routing end-to-end.
//!
//! Exercises the full coupling between the ReAct loop and the hybrid router
//! without a daemon: a local backend emits failing tool calls until the loop
//! crosses the escalation threshold, then the loop asks `route_with_escalation`
//! for a backend. Two scenarios:
//!
//! - Escalation accepted: the cost ceiling is well above the session cost, so
//!   the frontier backend is invoked and `frontier_ceiling_reached` stays false.
//! - Ceiling blocked: the session cost is seeded above the ceiling, so the loop
//!   stays local and `frontier_ceiling_reached` is true.
//!
//! Mocks are injected via `LlmRouter::with_backends(...).with_routing(...)`; no
//! HTTP API, no SQLite, no sidecar.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use apollia_core::{HybridRoutingConfig, LlmRoutingConfig, RunId};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk, ToolCall,
};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_oria::budget::StepBudget;
use apollia_runtime::chat::{
    BuiltInChatAgent, BuiltInChatAgentDeps, PendingChatApprovals, DEFAULT_CONTEXT_WINDOW_SIZE,
};
use apollia_runtime::EventBus;
use apollia_tools::ToolRegistryHandle;
use tokio_util::sync::CancellationToken;

type StreamResult =
    Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>;

/// Local backend: emits a failing tool call for the first `tool_turns` stream
/// calls, then a final text token. Drives the consecutive-failure counter.
struct LocalFailingModel {
    tool_turns: u32,
    iteration: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for LocalFailingModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        unimplemented!("streaming path only")
    }

    async fn stream(&self, _req: CompletionRequest) -> StreamResult {
        let current = self.iteration.fetch_add(1, Ordering::SeqCst);
        if current < self.tool_turns {
            let chunks = vec![Ok(StreamChunk::ToolCall(ToolCall {
                id: format!("c{current}"),
                name: "bash_executor".into(),
                arguments: serde_json::json!({"command": "false"}),
            }))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        } else {
            let chunks = vec![Ok(StreamChunk::Text("local-final".into()))];
            Ok(Box::pin(futures::stream::iter(chunks)))
        }
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "local"
    }
    fn model_id(&self) -> &str {
        "local-mock"
    }
}

/// Frontier backend: records each invocation and emits a final text token so the
/// loop terminates as soon as the escalation reaches it.
struct FrontierModel {
    invocations: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl CompletionModel for FrontierModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        unimplemented!("streaming path only")
    }

    async fn stream(&self, _req: CompletionRequest) -> StreamResult {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        let chunks = vec![Ok(StreamChunk::Text("frontier-final".into()))];
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "frontier"
    }
    fn model_id(&self) -> &str {
        "frontier-mock"
    }
}

/// Tool invoker that always reports a non-zero exit code, so every tool call is
/// counted as a failure by the ReAct loop.
struct AlwaysFailingInvoker;

#[async_trait::async_trait]
impl ToolInvoker for AlwaysFailingInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok(r#"{"exit_code": 1, "stdout": "", "stderr": "boom"}"#.to_string())
    }
}

/// Build a router with `default = precise = fast = "local"`, a `"frontier"`
/// backend, and a hybrid section with the given ceiling. Returns the router and
/// the frontier invocation counter.
fn make_hybrid_router(ceiling_usd: f64) -> (Arc<LlmRouter>, Arc<AtomicU32>) {
    let invocations = Arc::new(AtomicU32::new(0));
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "local".into(),
        Arc::new(LocalFailingModel {
            tool_turns: 4,
            iteration: AtomicU32::new(0),
        }),
    );
    backends.insert(
        "frontier".into(),
        Arc::new(FrontierModel {
            invocations: invocations.clone(),
        }),
    );
    let routing = LlmRoutingConfig {
        precise: "local".into(),
        fast: "local".into(),
        hybrid: Some(HybridRoutingConfig {
            frontier: "frontier".into(),
            cost_ceiling_usd: ceiling_usd,
            ceiling_action: Default::default(),
        }),
    };
    let router = LlmRouter::with_backends(backends, "local").with_routing(routing);
    (Arc::new(router), invocations)
}

/// Build a chat agent over the given router with a failing tool invoker.
fn make_agent(router: Arc<LlmRouter>, tool_registry: ToolRegistryHandle) -> BuiltInChatAgent {
    let (event_bus, _rx) = EventBus::new();
    BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry,
        tool_invoker: Arc::new(AlwaysFailingInvoker),
        event_bus,
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    })
}

fn authorized_bash() -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert("bash_executor".to_string());
    set
}

// GIVEN a hybrid router whose ceiling is well above the session cost
//   AND a local backend that fails three consecutive tool calls
// WHEN the ReAct loop reaches the escalation threshold
// THEN the frontier backend is invoked and the response does not flag the ceiling
#[tokio::test]
async fn b4_escalation_accepted_routes_to_frontier() {
    // GIVEN a hybrid router with a high ceiling and a fresh agent
    let (router, frontier_invocations) = make_hybrid_router(100.0);
    let tool_registry = ToolRegistryHandle::start();
    let agent = make_agent(router, tool_registry.clone());
    let budget = StepBudget::with_max(20);
    let approvals = PendingChatApprovals::new();

    // WHEN the agent runs (3 failures, then the escalated step hits the frontier)
    let resp = agent
        .execute(
            "sess-b4-accept",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &authorized_bash(),
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
        .expect("escalated run should produce a final response");

    // THEN the frontier backend was invoked exactly once and the ceiling flag is false
    assert_eq!(
        frontier_invocations.load(Ordering::SeqCst),
        1,
        "the escalated step should route to the frontier backend"
    );
    assert_eq!(resp.content, "frontier-final");
    assert!(!resp.frontier_ceiling_reached);

    tool_registry.shutdown().await;
}

// GIVEN a hybrid router whose ceiling has already been reached
//   AND a local backend that fails consecutive tool calls then stops
// WHEN the ReAct loop reaches the escalation threshold
// THEN the loop stays local, the frontier is never invoked, and the ceiling is flagged
#[tokio::test]
async fn b4_ceiling_blocked_stays_local_and_flags() {
    // GIVEN a hybrid router whose session cost is seeded above the ceiling
    let (router, frontier_invocations) = make_hybrid_router(1.0);
    router.seed_session_cost_usd(1.50);
    let tool_registry = ToolRegistryHandle::start();
    let agent = make_agent(router, tool_registry.clone());
    let budget = StepBudget::with_max(20);
    let approvals = PendingChatApprovals::new();

    // WHEN the agent runs (escalation requested but blocked by the ceiling)
    let resp = agent
        .execute(
            "sess-b4-block",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &authorized_bash(),
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
        .expect("blocked run should still produce a final local response");

    // THEN the frontier was never invoked and the ceiling flag is set
    assert_eq!(
        frontier_invocations.load(Ordering::SeqCst),
        0,
        "the ceiling should keep the loop on the local backend"
    );
    assert_eq!(resp.content, "local-final");
    assert!(resp.frontier_ceiling_reached);

    tool_registry.shutdown().await;
}

// GIVEN a hybrid router and a run that never crosses the failure threshold
// WHEN the loop completes normally
// THEN the frontier is never invoked and the ceiling flag stays false
#[tokio::test]
async fn b4_below_threshold_never_escalates() {
    // Sanity guard: a clean run must not touch the frontier path. A local model
    // that stops immediately (zero failing turns) never escalates.
    let invocations = Arc::new(AtomicU32::new(0));
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "local".into(),
        Arc::new(LocalFailingModel {
            tool_turns: 0,
            iteration: AtomicU32::new(0),
        }),
    );
    backends.insert(
        "frontier".into(),
        Arc::new(FrontierModel {
            invocations: invocations.clone(),
        }),
    );
    let routing = LlmRoutingConfig {
        precise: "local".into(),
        fast: "local".into(),
        hybrid: Some(HybridRoutingConfig {
            frontier: "frontier".into(),
            cost_ceiling_usd: 100.0,
            ceiling_action: Default::default(),
        }),
    };
    let router = Arc::new(LlmRouter::with_backends(backends, "local").with_routing(routing));
    let tool_registry = ToolRegistryHandle::start();
    let agent = make_agent(router, tool_registry.clone());
    let budget = StepBudget::with_max(20);
    let approvals = PendingChatApprovals::new();

    let resp = agent
        .execute(
            "sess-b4-clean",
            "msg-1",
            &RunId::new(),
            "go",
            &[],
            "",
            &[],
            &authorized_bash(),
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
        .expect("clean run should produce a final response");

    assert_eq!(invocations.load(Ordering::SeqCst), 0);
    assert_eq!(resp.content, "local-final");
    assert!(!resp.frontier_ceiling_reached);

    tool_registry.shutdown().await;
}
