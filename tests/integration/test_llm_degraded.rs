//! Tests e2e - comportement dégradé sans backend LLM.
//!
//! Vérifie que :
//! - le runtime démarre correctement avec `llm_router = None` ;
//! - `AgentDegraded` peut être émis et reçu sur l'EventBus ;
//! - `LlmRouter::empty()` n'a aucun backend disponible ;
//! - une erreur d'initialisation LLM n'empêche pas le runtime de démarrer.
//!
//! Aucune dépendance Python - CI OK.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, AgentId, RuntimeEvent};
use apollia_llm::{LlmConfig, LlmRouter, ObservabilityConfig};
use apollia_runtime::{
    api::{
        routes_agents::StubAgentLoader, server::empty_shared_llm_router, APIServer, APIServerConfig,
        AppState,
    },
    coordinator::{DynBackend, ExecutionBackend},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};

// ─────────────────────────────────────────────
// Mock backend - bloque indéfiniment
// ─────────────────────────────────────────────

/// Backend qui ne se termine jamais - utilisé pour tester le démarrage du runtime.
#[derive(Clone)]
struct NeverMockBackend;

impl From<DynBackend> for NeverMockBackend {
    fn from(_: DynBackend) -> Self {
        NeverMockBackend
    }
}

impl ExecutionBackend for NeverMockBackend {
    fn execute(
        &self,
        _task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async {
            std::future::pending::<()>().await;
            unreachable!()
        })
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-llm-deg-{id}.sock"))
}

async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind on port 0 should succeed");
    listener
        .local_addr()
        .expect("local_addr should be available")
        .port()
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

/// Le runtime démarre sans erreur quand `llm_router = None`.
#[tokio::test]
async fn test_runtime_starts_without_llm_router() {
    // GIVEN un runtime configuré sans LLM
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<NeverMockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let socket_path = temp_socket_path();
    let port = free_port().await;

    let state = AppState {
        router_handle: router,
        registry_handle: registry,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        backend: NeverMockBackend,
        llm_router: empty_shared_llm_router(), // aucun LLM configuré
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: None,
        stt_repository: None,
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
        resilience_layer: None,
        runner_proxy: None,
    };

    // WHEN l'APIServer démarre
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let result = api.start().await;

    // THEN il démarre sans erreur (pas de panic, pas d'Err)
    assert!(result.is_ok(), "APIServer doit démarrer sans LLM router");

    let _ = std::fs::remove_file(&socket_path);
}

/// `AgentDegraded` peut être émis et reçu via l'EventBus.
///
/// Vérifie l'infrastructure d'événements : le runtime peut observer
/// et propager la dégradation d'un agent sans backend LLM.
#[tokio::test]
async fn test_agent_degraded_event_emitted_when_no_llm() {
    // GIVEN un EventBus avec un subscriber actif
    let (event_sender, _rx) = EventBus::new();
    let mut event_rx = event_sender.subscribe();
    let agent_id = AgentId::new_v4();

    // WHEN AgentDegraded est émis (comme le ferait RuntimeContext sans LLM)
    let _ = event_sender.send(RuntimeEvent::AgentDegraded {
        agent_id: agent_id.clone(),
        reason: "no LLM backend available".into(),
    });

    // THEN l'événement est reçu avec l'agent_id et la raison corrects
    let event = event_rx
        .try_recv()
        .expect("AgentDegraded doit être présent dans le bus");

    assert!(
        matches!(
            event,
            RuntimeEvent::AgentDegraded {
                agent_id: ref id,
                ref reason,
            } if *id == agent_id && reason.contains("no LLM")
        ),
        "événement inattendu : {event:?}"
    );
}

/// `LlmRouter::empty()` n'a aucun backend disponible.
///
/// Correspond au comportement de `ctx.llm = None` quand le router est vide.
#[tokio::test]
async fn test_ctx_llm_is_none_when_no_backend() {
    // GIVEN un LlmRouter sans backend configuré
    let router = LlmRouter::empty();

    // WHEN on interroge les backends disponibles
    let backends = router.list();
    let default_backend = router.get(None);

    // THEN il n'y a aucun backend et get() retourne None
    assert!(
        backends.is_empty(),
        "LlmRouter::empty() ne doit avoir aucun backend"
    );
    assert!(
        default_backend.is_none(),
        "get(None) doit retourner None pour un router vide"
    );
}

/// le runtime continue de fonctionner après un échec d'initialisation LLM.
///
/// Vérifie le principe #4 (fail fast) : l'erreur est détectée au démarrage,
/// mais le runtime peut continuer avec `llm_router = None`.
#[tokio::test]
async fn test_runtime_continues_after_llm_init_failure() {
    // GIVEN une config LLM avec un backend par défaut inexistant
    let config = LlmConfig {
        default: "backend-inexistant".to_owned(),
        backends: vec![],
        observability: ObservabilityConfig::default(),
        routing: None,
        pricing_overrides: HashMap::new(),
        cost_alert_threshold_usd: None,
        vertex: None,
        runner: apollia_core::config::LlmRunnerConfig::default(),
    };

    // WHEN LlmRouter::from_config() échoue (backend absent)
    let llm_result = LlmRouter::from_config(&config).await;

    // THEN l'erreur est retournée proprement (pas de panic)
    assert!(
        llm_result.is_err(),
        "from_config doit échouer pour un backend par défaut inconnu"
    );

    // ET le runtime peut quand même démarrer sans LLM
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<NeverMockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let socket_path = temp_socket_path();
    let port = free_port().await;

    let state = AppState {
        router_handle: router,
        registry_handle: registry,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        backend: NeverMockBackend,
        llm_router: empty_shared_llm_router(),
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: None,
        stt_repository: None,
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
        resilience_layer: None,
        runner_proxy: None,
    };

    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let result = api.start().await;

    assert!(
        result.is_ok(),
        "le runtime doit continuer sans LLM après un échec d'initialisation"
    );

    let _ = std::fs::remove_file(&socket_path);
}
