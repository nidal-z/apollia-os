//! Tests d'intégration E2E - garde-fous A2A.
//!
//! Valide les trois garde-fous automatiques appliqués par A2AInvoker::invoke() :
//! - Profondeur de récursivité (max_depth)
//! - Auto-invocation (self_invocation)
//! - Timeout cumulé de chaîne (chain_timeout)
//!
//! Vérifie également l'émission de RuntimeEvent::A2AGuardTriggered sur le bus.
//! Aucun Python requis - utilise SuccessBackend et BlockingBackend.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use apollia_core::{
    A2AConfig, AIPResult, AIPTask, AgentId, AgentManifest, AgentSkill, ProcessState, RuntimeEvent,
};
use apollia_runtime::{
    coordinator::{ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::{AgentRegistry, AgentRegistryHandle},
    router::TaskRouterHandle,
    A2AError, A2AInvoker, EventBusSender,
};
use serde_json::json;

// ─── Backends de test ────────────────────────────────────────────────────────

/// Backend qui complète instantanément avec le texte fourni.
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

/// Backend qui bloque indéfiniment - simule un agent qui ne répond jamais.
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

/// Construit un `AgentSkill` minimal.
fn make_skill(id: &str) -> AgentSkill {
    AgentSkill {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Skill de test : {id}"),
        input_modes: vec!["text".to_string()],
        output_modes: vec!["text".to_string()],
    }
}

/// Construit un manifest de Worker Agent A2A déclarant les skills fournis.
fn make_worker_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
    AgentManifest {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: format!("Worker de test : {name}"),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: true,
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
    }
}

/// Infrastructure A2A minimale (EventBus + AgentRegistry + TaskRouter + worker actif).
///
/// Retourne `(invoker, registry_handle, event_sender, worker_agent_id)`.
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
        .expect("enregistrement du worker");

    registry
        .update_state(worker_id.as_str(), ProcessState::Active)
        .await
        .expect("activation du worker");

    let coordinator =
        ExecutionCoordinator::new(worker_id.clone(), 1, event_sender.clone(), backend);
    router
        .register_coordinator(worker_id.clone(), coordinator)
        .await
        .expect("enregistrement du coordinator");

    let invoker = A2AInvoker::new(registry.clone(), router, event_sender.clone(), config);

    (invoker, registry, event_sender, worker_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// La profondeur maximale bloque l'invocation dès que a2a_depth >= max_depth.
#[tokio::test]
async fn test_max_depth_blocks_deep_recursion() {
    // GIVEN A2AInvoker avec max_depth = 2 et excel-worker actif
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

    // WHEN invoke est appelé avec a2a_depth = 2 (atteint le plafond)
    let result = invoker
        .invoke(
            "read-excel",
            json!({"text": "data"}),
            "director",
            2,
            None,
            None,
        )
        .await;

    // THEN MaxDepthExceeded est retourné avec le contexte complet
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
        other => panic!("attendu MaxDepthExceeded, obtenu : {other:?}"),
    }
}

/// Un agent ne peut pas s'invoquer lui-même via un skill A2A.
#[tokio::test]
async fn test_self_invocation_blocked() {
    // GIVEN excel-worker actif déclarant le skill "read-excel"
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let (invoker, _, _, _) = setup_a2a_runtime(
        manifest,
        SuccessBackend {
            output: "ok".to_string(),
        },
        A2AConfig::default(),
    )
    .await;

    // WHEN excel-worker invoque "read-excel" en tant que caller
    let result = invoker
        .invoke("read-excel", json!({}), "excel-worker", 0, None, None)
        .await;

    // THEN SelfInvocation est retourné avec le nom de l'agent et le skill ciblé
    match result {
        Err(A2AError::SelfInvocation {
            agent_name,
            skill_id,
        }) => {
            assert_eq!(agent_name, "excel-worker");
            assert_eq!(skill_id, "read-excel");
        }
        other => panic!("attendu SelfInvocation, obtenu : {other:?}"),
    }
}

/// Un chain_deadline expiré déclenche ChainTimeoutExceeded avant toute délégation.
#[tokio::test]
async fn test_chain_timeout_propagated() {
    // GIVEN excel-worker actif et un chain_deadline dans le passé
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let (invoker, _, _, _) =
        setup_a2a_runtime(manifest, BlockingBackend, A2AConfig::default()).await;

    let expired_deadline = Instant::now() - Duration::from_secs(1);

    // WHEN invoke est appelé avec un chain_deadline déjà expiré
    let result = invoker
        .invoke(
            "read-excel",
            json!({}),
            "director",
            0,
            None,
            Some(expired_deadline),
        )
        .await;

    // THEN ChainTimeoutExceeded (ou Timeout) est retourné immédiatement sans bloquer
    assert!(
        matches!(
            result,
            Err(A2AError::ChainTimeoutExceeded { .. }) | Err(A2AError::Timeout { .. })
        ),
        "attendu ChainTimeoutExceeded ou Timeout, obtenu : {result:?}"
    );
}

/// RuntimeEvent::A2AGuardTriggered est émis sur le bus avant que l'erreur soit retournée.
#[tokio::test]
async fn test_guard_event_emitted() {
    // GIVEN A2AInvoker avec max_depth = 1 et un subscriber sur l'EventBus
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

    // WHEN invoke déclenche le garde-fou max_depth (depth = 1 == max_depth = 1)
    let _ = invoker
        .invoke("extract-pdf", json!({}), "director", 1, None, None)
        .await;

    // THEN A2AGuardTriggered est dans le buffer avec guard_type = "max_depth"
    let mut guard_found = false;
    loop {
        match guard_rx.try_recv() {
            Ok(RuntimeEvent::A2AGuardTriggered {
                ref guard_type,
                ref skill_id,
                ..
            }) if guard_type == "max_depth" => {
                assert_eq!(
                    skill_id, "extract-pdf",
                    "skill_id incorrect dans l'événement"
                );
                guard_found = true;
                break;
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    assert!(
        guard_found,
        "RuntimeEvent::A2AGuardTriggered avec guard_type=max_depth doit être émis"
    );
}
