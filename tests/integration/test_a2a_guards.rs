//! End-to-end integration tests - the A2A guards.
//!
//! Validates the three automatic guards A2AInvoker::invoke() applies:
//! - recursion depth (max_depth)
//! - self invocation (self_invocation)
//! - cumulative chain timeout (chain_timeout)
//!
//! Also checks that RuntimeEvent::A2AGuardTriggered is emitted on the bus.
//! No Python required - uses SuccessBackend and BlockingBackend.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use apollia_core::{
    A2AConfig, AIPResult, AIPTask, AgentId, AgentManifest, AgentSkill, ProcessState, RuntimeEvent,
};
use apollia_runtime::{
    a2a::A2AInvokeRequest,
    coordinator::{ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::{AgentRegistry, AgentRegistryHandle},
    router::TaskRouterHandle,
    A2AError, A2AInvoker, EventBusSender,
};
use serde_json::json;

// ─── Test backends ───────────────────────────────────────────────────────────

/// Backend that completes instantly with the given text.
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

/// Builds a minimal `AgentSkill`.
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
        description: format!("Test worker: {name}"),
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

/// Infrastructure A2A minimale (EventBus + AgentRegistry + TaskRouter + worker actif).
///
/// Returns `(invoker, registry_handle, event_sender, worker_agent_id)`.
async fn setup_a2a_runtime<B>(
    worker_manifest: AgentManifest,
    backend: B,
    config: A2AConfig,
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
        .expect("registering the worker");

    registry
        .update_state(worker_id.as_str(), ProcessState::Active)
        .await
        .expect("activating the worker");

    let coordinator =
        ExecutionCoordinator::new(worker_id.clone(), 1, event_sender.clone(), backend);
    router
        .register_coordinator(worker_id.clone(), coordinator)
        .await
        .expect("registering the coordinator");

    let invoker = A2AInvoker::new(registry.clone(), router, event_sender.clone(), config);

    (invoker, registry, event_sender, worker_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The maximum depth blocks the invocation as soon as a2a_depth >= max_depth.
#[tokio::test]
async fn test_max_depth_blocks_deep_recursion() {
    // GIVEN an A2AInvoker with max_depth = 2 and excel-worker active
    let config = A2AConfig {
        max_depth: 2,
        ..A2AConfig::default()
    };
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let (invoker, _, _, _) = setup_a2a_runtime(
        manifest,
        SuccessBackend {
            output: "ok".to_string(),
        },
        config,
    )
    .await;

    // WHEN invoke is called with a2a_depth = 2 (the ceiling is reached)
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({"text": "data"}),
            caller: "director",
            a2a_depth: 2,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN MaxDepthExceeded is returned with the full context
    match result {
        Err(A2AError::MaxDepthExceeded {
            current_depth,
            max_depth,
            caller,
            skill_id,
        }) => {
            assert_eq!(current_depth, 2, "profondeur courante incorrecte");
            assert_eq!(max_depth, 2, "profondeur max incorrecte");
            assert_eq!(caller, "director");
            assert_eq!(skill_id, "read-excel");
        }
        other => panic!("expected MaxDepthExceeded, got: {other:?}"),
    }
}

/// An agent cannot invoke itself through an A2A skill.
#[tokio::test]
async fn test_self_invocation_blocked() {
    // GIVEN excel-worker active, declaring the "read-excel" skill
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let (invoker, _, _, _) = setup_a2a_runtime(
        manifest,
        SuccessBackend {
            output: "ok".to_string(),
        },
        A2AConfig::default(),
    )
    .await;

    // WHEN excel-worker invokes "read-excel" as the caller
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({}),
            caller: "excel-worker",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN SelfInvocation is returned with the agent name and the target skill
    match result {
        Err(A2AError::SelfInvocation {
            agent_name,
            skill_id,
        }) => {
            assert_eq!(agent_name, "excel-worker");
            assert_eq!(skill_id, "read-excel");
        }
        other => panic!("expected SelfInvocation, got: {other:?}"),
    }
}

/// An expired chain_deadline raises ChainTimeoutExceeded before any delegation.
#[tokio::test]
async fn test_chain_timeout_propagated() {
    // GIVEN excel-worker active and a chain_deadline in the past
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let (invoker, _, _, _) =
        setup_a2a_runtime(manifest, BlockingBackend, A2AConfig::default()).await;

    let expired_deadline = Instant::now() - Duration::from_secs(1);

    // WHEN invoke is called with an already expired chain_deadline
    let result = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "read-excel",
            input: json!({}),
            caller: "director",
            a2a_depth: 0,
            timeout: None,
            chain_deadline: Some(expired_deadline),
        })
        .await;

    // THEN ChainTimeoutExceeded (or Timeout) comes back at once, without blocking
    assert!(
        matches!(
            result,
            Err(A2AError::ChainTimeoutExceeded { .. }) | Err(A2AError::Timeout { .. })
        ),
        "expected ChainTimeoutExceeded or Timeout, got: {result:?}"
    );
}

/// RuntimeEvent::A2AGuardTriggered is emitted on the bus before the error returns.
#[tokio::test]
async fn test_guard_event_emitted() {
    // GIVEN an A2AInvoker with max_depth = 1 and a subscriber on the EventBus
    let config = A2AConfig {
        max_depth: 1,
        ..A2AConfig::default()
    };
    let manifest = make_worker_manifest("pdf-worker", &["extract-pdf"]);
    let (invoker, _, event_sender, _) = setup_a2a_runtime(
        manifest,
        SuccessBackend {
            output: "ok".to_string(),
        },
        config,
    )
    .await;

    let mut guard_rx = event_sender.subscribe();

    // WHEN invoke trips the max_depth guard (depth = 1 == max_depth = 1)
    let _ = invoker
        .invoke(A2AInvokeRequest {
            skill_id: "extract-pdf",
            input: json!({}),
            caller: "director",
            a2a_depth: 1,
            timeout: None,
            chain_deadline: None,
        })
        .await;

    // THEN A2AGuardTriggered is in the buffer with guard_type = "max_depth"
    let mut guard_found = false;
    loop {
        match guard_rx.try_recv() {
            Ok(RuntimeEvent::A2AGuardTriggered {
                ref guard_type,
                ref skill_id,
                ..
            }) if guard_type == "max_depth" => {
                assert_eq!(skill_id, "extract-pdf", "wrong skill_id in the event");
                guard_found = true;
                break;
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    assert!(
        guard_found,
        "RuntimeEvent::A2AGuardTriggered with guard_type=max_depth must be emitted"
    );
}
