//! A2A integration tests - full Director -> Worker -> result flow.
//!
//! Validates the whole chain: discovery -> resolution -> invocation -> trust
//! model -> result, assembling the real runtime components (EventBus,
//! AgentRegistry, TaskRouter, A2AInvoker).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use apollia_core::{
    A2AConfig, AIPResult, AIPTask, AgentId, AgentManifest, AgentSkill, ProcessState, RuntimeEvent,
    TaskStatus,
};
use apollia_runtime::{
    coordinator::{ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::{AgentRegistry, AgentRegistryHandle},
    router::TaskRouterHandle,
    A2AError, A2AInvokeRequest, A2AInvoker, EventBusSender,
};
use serde_json::json;

// ─── Test backends ───────────────────────────────────────────────────────────

/// Backend that returns a completed result carrying the given text, at once.
#[derive(Clone)]
struct SuccessBackend {
    output: String,
}

impl ExecutionBackend for SuccessBackend {
    fn execute(
        &self,
        _task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        let output = self.output.clone();
        Box::pin(async move { Ok(AIPResult::completed(&output)) })
    }
}

/// Backend that blocks forever - simulates an agent that never answers.
#[derive(Clone)]
struct BlockingBackend;

impl ExecutionBackend for BlockingBackend {
    fn execute(
        &self,
        _task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Err("unreachable: timeout should have fired first".to_string())
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Builds a minimal `AgentSkill` with the given identifier.
fn make_skill(id: &str) -> AgentSkill {
    AgentSkill {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Test skill: {id}"),
        input_modes: vec!["text".to_string()],
        output_modes: vec!["text".to_string()],
        examples: vec![],
        input_schema: None,
    }
}

/// Builds an A2A Worker Agent manifest declaring the given skills.
fn make_worker_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: format!("Test worker agent: {name}"),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: true,
        supports_mailbox: false,
        mailbox_allowlist: None,
        memory_namespace: Some(name.to_string()),
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: skill_ids.iter().map(|id| make_skill(id)).collect(),
        execution_mode: "direct".to_string(),
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

/// Infrastructure A2A minimale : EventBus + AgentRegistry + TaskRouter + worker actif.
///
/// Returns `(invoker, registry_handle, event_sender, worker_agent_id)`.
async fn setup_a2a_runtime<B>(
    worker_manifest: AgentManifest,
    backend: B,
) -> (A2AInvoker, AgentRegistryHandle, EventBusSender, AgentId)
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    let (event_sender, _) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<B> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let worker_id = registry
        .register(worker_manifest)
        .await
        .expect("registering the worker must succeed");

    registry
        .update_state(worker_id.as_str(), ProcessState::Active)
        .await
        .expect("activating the worker must succeed");

    let coordinator =
        ExecutionCoordinator::new(worker_id.clone(), 1, event_sender.clone(), backend);
    router
        .register_coordinator(worker_id.clone(), coordinator)
        .await
        .expect("registering the coordinator must succeed");

    let invoker = A2AInvoker::new(
        registry.clone(),
        router,
        event_sender.clone(),
        A2AConfig::default(),
    );

    (invoker, registry, event_sender, worker_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Full A2A flow: skill resolution -> active agent -> delegation -> result.
#[tokio::test]
async fn test_full_a2a_routing_success() {
    // GIVEN the runtime started with excel-worker active, declaring "read-excel"
    let manifest = make_worker_manifest(
        "excel-worker",
        &["read-excel", "edit-excel", "analyze-excel"],
    );
    let backend = SuccessBackend {
        output: "Excel data processed".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN invoke("read-excel", ...) is called from "director"
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({"text": "test"}),
            caller: "director",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN the result is Ok, with agent_name, skill_id and duration filled in
    let invocation = result.expect("the A2A invocation must succeed");
    assert_eq!(invocation.agent_name, "excel-worker");
    assert_eq!(invocation.skill_id, "read-excel");
    assert!(
        matches!(invocation.result.status, TaskStatus::Completed),
        "the status must be Completed, got: {:?}",
        invocation.result.status
    );
    // duration_ms can be 0 on fast hardware (millisecond precision), so this
    // checks it stays within reasonable bounds rather than an absolute value.
    assert!(
        invocation.duration_ms < 60_000,
        "duration_ms must be < 60 s, got: {}",
        invocation.duration_ms
    );
}

/// Unknown skill -> SkillNotFound error, with the list of available skills.
#[tokio::test]
async fn test_skill_not_found_lists_available() {
    // GIVEN excel-worker registered with three skills
    let manifest = make_worker_manifest(
        "excel-worker",
        &["read-excel", "edit-excel", "analyze-excel"],
    );
    let backend = SuccessBackend {
        output: "ok".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN invoke("nonexistent-skill", ...) is called
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "nonexistent-skill",
            input: json!({}),
            caller: "director",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN Err(SkillNotFound) with the available skills in `available`
    match result {
        Err(A2AError::SkillNotFound {
            skill_id,
            available,
        }) => {
            assert_eq!(skill_id, "nonexistent-skill");
            assert!(
                available.contains(&"read-excel".to_string()),
                "available must hold 'read-excel', got: {available:?}"
            );
        }
        other => panic!("expected SkillNotFound, got: {other:?}"),
    }
}

/// Agent in the Degraded state -> AgentNotActive error, before any delegation.
#[tokio::test]
async fn test_degraded_agent_rejected() {
    // GIVEN excel-worker registered, then moved to the Degraded state
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "ok".to_string(),
    };
    let (invoker, registry, _event_sender, worker_id) = setup_a2a_runtime(manifest, backend).await;

    registry
        .update_state(worker_id.as_str(), ProcessState::Degraded)
        .await
        .expect("the Active -> Degraded transition must succeed");

    // WHEN invoke("read-excel", ...) is called
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({}),
            caller: "director",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN Err(AgentNotActive), with the name and the current state of the agent
    match result {
        Err(A2AError::AgentNotActive { agent_name, state }) => {
            assert_eq!(agent_name, "excel-worker");
            assert!(
                state.to_lowercase().contains("degraded"),
                "state must mention Degraded, got: {state}"
            );
        }
        other => panic!("expected AgentNotActive, got: {other:?}"),
    }
}

/// Timeout honoured: invoking a blocking agent -> Err(Timeout), runtime not blocked.
///
/// `Duration::from_millis(500).as_secs()` = 0 → `tokio::time::timeout(Duration::ZERO, …)`
/// expires on the first poll of the wait loop, which makes the test instant.
#[tokio::test]
async fn test_timeout_respected() {
    // GIVEN an agent that blocks forever
    let manifest = make_worker_manifest("slow-worker", &["slow-skill"]);
    let (invoker, registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, BlockingBackend).await;

    let wall_start = std::time::Instant::now();

    // WHEN invoke(..., timeout: Some(500ms)) is called
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "slow-skill",
            input: json!({}),
            caller: "director",
            a2a_depth: 0,
            timeout: Some(Duration::from_millis(500)),
            chain_deadline: None,
        })
        .await;

    // THEN Err(Timeout) is received, with the right identifiers
    match result {
        Err(A2AError::Timeout {
            skill_id,
            agent_name,
            ..
        }) => {
            assert_eq!(skill_id, "slow-skill");
            assert_eq!(agent_name, "slow-worker");
        }
        other => panic!("expected Timeout, got: {other:?}"),
    }

    // AND the timeout does not block the runtime (under one real second)
    assert!(
        wall_start.elapsed() < Duration::from_secs(1),
        "the timeout must expire quickly, elapsed: {:?}",
        wall_start.elapsed()
    );

    // AND the registry still answers after the timeout
    let agents = registry
        .list_agents()
        .await
        .expect("the registry must still answer after the timeout");
    assert_eq!(agents.len(), 1, "the worker must still be registered");
}

/// A2A trust model: the execution context of an agent invoked through A2A is
/// read-only on the global user memory.
#[tokio::test]
async fn test_trust_model_context_config() {
    // GIVEN a configured A2AInvoker
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "result".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN the context configuration is generated for an agent invoked over A2A
    let ctx_config = invoker.build_a2a_context();

    // THEN user_memory_writable is false - reading `__user__` is handled
    // directly by the `MemoryInterface` (always on when a user_manager is
    // provided), and writing is never granted through A2A.
    assert!(
        !ctx_config.user_memory_writable,
        "A2A invocation must not grant user_memory write access"
    );
}

/// Observability: the A2AInvocationStarted and A2AInvocationCompleted events
/// are emitted on the EventBus during a successful invocation.
#[tokio::test]
async fn test_events_emitted() {
    // GIVEN an EventBus receiver and excel-worker active
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "data".to_string(),
    };
    let (invoker, _registry, event_sender, _worker_id) = setup_a2a_runtime(manifest, backend).await;

    // Subscribe after the setup, so only the invocation events are observed
    let mut event_rx = event_sender.subscribe();

    // WHEN invoke("read-excel", ...) is called and completes
    invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({}),
            caller: "director",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: None,
        })
        .await
        .expect("the invocation must succeed");

    // Collect the events available on the channel
    let mut events: Vec<RuntimeEvent> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        events.push(event);
    }

    // THEN A2AInvocationStarted emitted with caller="director", target="excel-worker"
    let started = events.iter().any(|e| {
        matches!(
            e,
            RuntimeEvent::A2AInvocationStarted {
                caller,
                target,
                skill_id,
            } if caller == "director" && target == "excel-worker" && skill_id == "read-excel"
        )
    });
    assert!(
        started,
        "A2AInvocationStarted must be emitted, events received: {events:?}"
    );

    // AND A2AInvocationCompleted emitted with status="completed"
    // (duration_ms can be 0 on fast hardware - millisecond precision)
    let completed = events.iter().any(|e| {
        matches!(
            e,
            RuntimeEvent::A2AInvocationCompleted { status, .. } if status == "completed"
        )
    });
    assert!(
        completed,
        "A2AInvocationCompleted must be emitted with status='completed', events received: {events:?}"
    );
}
