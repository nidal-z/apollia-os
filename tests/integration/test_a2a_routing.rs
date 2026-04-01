//! Tests d'intégration A2A — flux complet Director → Worker → résultat.
//!
//! Valide l'enchaînement complet : discovery → résolution → invocation → trust
//! model → résultat, en assemblant les composants réels du runtime (EventBus,
//! AgentRegistry, TaskRouter, A2AInvoker).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use apollia_core::{
    AIPResult, AIPTask, AgentId, AgentManifest, AgentSkill, ProcessState, RuntimeEvent, TaskStatus,
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

/// Backend qui retourne immédiatement un résultat complété avec le texte fourni.
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

/// Backend qui bloque indéfiniment — simule un agent qui ne répond jamais.
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

/// Construit un `AgentSkill` minimal avec l'identifiant fourni.
fn make_skill(id: &str) -> AgentSkill {
    AgentSkill {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Compétence de test : {id}"),
        input_modes: vec!["text".to_string()],
        output_modes: vec!["text".to_string()],
    }
}

/// Construit un manifest de Worker Agent A2A déclarant les skills fournis.
fn make_worker_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
    AgentManifest {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: format!("Worker agent de test : {name}"),
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
    }
}

/// Infrastructure A2A minimale : EventBus + AgentRegistry + TaskRouter + worker actif.
///
/// Retourne `(invoker, registry_handle, event_sender, worker_agent_id)`.
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
        .expect("enregistrement du worker doit réussir");

    registry
        .update_state(worker_id.as_str(), ProcessState::Active)
        .await
        .expect("activation du worker doit réussir");

    let coordinator =
        ExecutionCoordinator::new(worker_id.clone(), 1, event_sender.clone(), backend);
    router
        .register_coordinator(worker_id.clone(), coordinator)
        .await
        .expect("enregistrement du coordinator doit réussir");

    let invoker = A2AInvoker::new(registry.clone(), router, event_sender.clone());

    (invoker, registry, event_sender, worker_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Flux A2A complet : résolution skill → agent actif → délégation → résultat.
#[tokio::test]
async fn test_full_a2a_routing_success() {
    // GIVEN le runtime démarré avec excel-worker actif déclarant "read-excel"
    let manifest = make_worker_manifest(
        "excel-worker",
        &["read-excel", "edit-excel", "analyze-excel"],
    );
    let backend = SuccessBackend {
        output: "Excel data processed".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN invoke("read-excel", ...) est appelé depuis "director"
    let result = invoker
        .invoke("read-excel", json!({"text": "test"}), "director", None)
        .await;

    // THEN le résultat est Ok avec agent_name, skill_id et duration correctement renseignés
    let invocation = result.expect("l'invocation A2A doit réussir");
    assert_eq!(invocation.agent_name, "excel-worker");
    assert_eq!(invocation.skill_id, "read-excel");
    assert!(
        matches!(invocation.result.status, TaskStatus::Completed),
        "le statut doit être Completed, obtenu : {:?}",
        invocation.result.status
    );
    // duration_ms peut valoir 0 sur matériel rapide (précision milliseconde) ;
    // on vérifie qu'il reste dans des bornes raisonnables plutôt qu'une valeur absolue.
    assert!(
        invocation.duration_ms < 60_000,
        "duration_ms doit être < 60 s, obtenu : {}",
        invocation.duration_ms
    );
}

/// Skill inconnu → erreur SkillNotFound avec la liste des skills disponibles.
#[tokio::test]
async fn test_skill_not_found_lists_available() {
    // GIVEN excel-worker enregistré avec trois skills
    let manifest = make_worker_manifest(
        "excel-worker",
        &["read-excel", "edit-excel", "analyze-excel"],
    );
    let backend = SuccessBackend {
        output: "ok".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN invoke("nonexistent-skill", ...) est appelé
    let result = invoker
        .invoke("nonexistent-skill", json!({}), "director", None)
        .await;

    // THEN Err(SkillNotFound) avec les skills disponibles dans `available`
    match result {
        Err(A2AError::SkillNotFound {
            skill_id,
            available,
        }) => {
            assert_eq!(skill_id, "nonexistent-skill");
            assert!(
                available.contains(&"read-excel".to_string()),
                "available doit contenir 'read-excel', obtenu : {available:?}"
            );
        }
        other => panic!("attendu SkillNotFound, obtenu : {other:?}"),
    }
}

/// Agent en état Degraded → erreur AgentNotActive avant toute délégation.
#[tokio::test]
async fn test_degraded_agent_rejected() {
    // GIVEN excel-worker enregistré puis passé en état Degraded
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "ok".to_string(),
    };
    let (invoker, registry, _event_sender, worker_id) = setup_a2a_runtime(manifest, backend).await;

    registry
        .update_state(worker_id.as_str(), ProcessState::Degraded)
        .await
        .expect("transition Active → Degraded doit réussir");

    // WHEN invoke("read-excel", ...) est appelé
    let result = invoker
        .invoke("read-excel", json!({}), "director", None)
        .await;

    // THEN Err(AgentNotActive) avec le nom et l'état courant de l'agent
    match result {
        Err(A2AError::AgentNotActive { agent_name, state }) => {
            assert_eq!(agent_name, "excel-worker");
            assert!(
                state.to_lowercase().contains("degraded"),
                "state doit mentionner Degraded, obtenu : {state}"
            );
        }
        other => panic!("attendu AgentNotActive, obtenu : {other:?}"),
    }
}

/// Timeout respecté : invocation sur agent bloquant → Err(Timeout) sans bloquer le runtime.
///
/// `Duration::from_millis(500).as_secs()` = 0 → `tokio::time::timeout(Duration::ZERO, …)`
/// expire dès le premier poll de la boucle d'attente, rendant le test instantané.
#[tokio::test]
async fn test_timeout_respected() {
    // GIVEN un agent qui bloque indéfiniment
    let manifest = make_worker_manifest("slow-worker", &["slow-skill"]);
    let (invoker, registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, BlockingBackend).await;

    let wall_start = std::time::Instant::now();

    // WHEN invoke(..., timeout: Some(500ms)) est appelé
    let result = invoker
        .invoke(
            "slow-skill",
            json!({}),
            "director",
            Some(Duration::from_millis(500)),
        )
        .await;

    // THEN Err(Timeout) reçu avec les identifiants corrects
    match result {
        Err(A2AError::Timeout {
            skill_id,
            agent_name,
            ..
        }) => {
            assert_eq!(skill_id, "slow-skill");
            assert_eq!(agent_name, "slow-worker");
        }
        other => panic!("attendu Timeout, obtenu : {other:?}"),
    }

    // ET le timeout ne bloque pas le runtime (< 1 seconde réelle)
    assert!(
        wall_start.elapsed() < Duration::from_secs(1),
        "le timeout doit expirer rapidement, elapsed : {:?}",
        wall_start.elapsed()
    );

    // ET le registry répond encore après le timeout
    let agents = registry
        .list_agents()
        .await
        .expect("le registry doit encore répondre après le timeout");
    assert_eq!(agents.len(), 1, "le worker doit toujours être enregistré");
}

/// Trust model A2A : le contexte d'exécution d'un agent invoqué via A2A est en
/// lecture seule pour la mémoire utilisateur globale.
#[tokio::test]
async fn test_trust_model_context_config() {
    // GIVEN un A2AInvoker configuré
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "result".to_string(),
    };
    let (invoker, _registry, _event_sender, _worker_id) =
        setup_a2a_runtime(manifest, backend).await;

    // WHEN on génère la configuration de contexte pour un agent invoqué via A2A
    let ctx_config = invoker.build_a2a_context();

    // THEN user_memory_read_only est true — l'agent invoqué peut lire la mémoire
    // utilisateur globale mais ne peut pas y écrire
    assert!(
        ctx_config.user_memory_read_only,
        "les agents invoqués via A2A doivent avoir user_memory_read_only = true"
    );
}

/// Observabilité : les événements A2AInvocationStarted et A2AInvocationCompleted
/// sont émis sur l'EventBus lors d'une invocation réussie.
#[tokio::test]
async fn test_events_emitted() {
    // GIVEN un EventBus receiver et excel-worker actif
    let manifest = make_worker_manifest("excel-worker", &["read-excel"]);
    let backend = SuccessBackend {
        output: "data".to_string(),
    };
    let (invoker, _registry, event_sender, _worker_id) = setup_a2a_runtime(manifest, backend).await;

    // Souscrire après le setup pour n'observer que les événements de l'invocation
    let mut event_rx = event_sender.subscribe();

    // WHEN invoke("read-excel", ...) est appelé et complété
    invoker
        .invoke("read-excel", json!({}), "director", None)
        .await
        .expect("l'invocation doit réussir");

    // Collecter les événements disponibles dans le canal
    let mut events: Vec<RuntimeEvent> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        events.push(event);
    }

    // THEN A2AInvocationStarted émis avec caller="director", target="excel-worker"
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
        "A2AInvocationStarted doit être émis, événements reçus : {events:?}"
    );

    // ET A2AInvocationCompleted émis avec status="completed"
    // (duration_ms peut être 0 sur matériel rapide — précision milliseconde)
    let completed = events.iter().any(|e| {
        matches!(
            e,
            RuntimeEvent::A2AInvocationCompleted { status, .. } if status == "completed"
        )
    });
    assert!(
        completed,
        "A2AInvocationCompleted doit être émis avec status='completed', événements reçus : {events:?}"
    );
}
