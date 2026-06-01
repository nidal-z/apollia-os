//! Integration tests - route `POST /webhooks/:id`.
//!
//! Couvre 3 scénarios : POST valide (HMAC correct), POST invalide (signature erronée),
//! et POST trigger inconnu (404).
//!
//! **POST valide** : HMAC correct → HTTP 200 + 1 submit()
//! **POST invalide** : signature erronée → HTTP 401 + 0 submit()
//! **Trigger inconnu** : HTTP 404 + 0 submit()
//!
//! Pattern : serveur TCP réel (port éphémère) + requêtes HTTP brutes.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use apollia_core::{AIPInput, ObservabilityConfig, TaskId};
use apollia_runtime::{
    api::routes_agents::StubAgentLoader, api::APIServer, api::APIServerConfig,
    api::APIServerHandle, api::AppState, coordinator::DynBackend, coordinator::ExecutionBackend,
    eventbus::EventBus, registry::AgentRegistry, router::TaskRouterHandle,
};
use apollia_triggers::{
    InputTemplate, OnBusyPolicy, TaskSubmitter, TriggerDefinition, TriggerEngineHandle,
    TriggerSourceConfig,
};

// ─── MockTaskSubmitter ────────────────────────────────────────────────────

/// Mock [`TaskSubmitter`] qui compte les appels à `submit()`.
///
/// `pending_count` retourne toujours 0 (agent libre) dans les tests webhook -
/// le but est de vérifier que le handler HTTP atteint (ou non) la soumission.
struct MockTaskSubmitter {
    submit_count: Arc<AtomicU32>,
}

impl MockTaskSubmitter {
    fn new() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Self {
                submit_count: Arc::clone(&count),
            },
            count,
        )
    }
}

impl TaskSubmitter for MockTaskSubmitter {
    fn submit<'a>(
        &'a self,
        _agent: &'a str,
        _input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        self.submit_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(TaskId::new_v4()) })
    }

    fn pending_count<'a>(
        &'a self,
        _agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        Box::pin(async { 0_usize })
    }
}

// ─── MockBackend ──────────────────────────────────────────────────────────

/// Backend d'exécution minimal - jamais appelé dans les tests webhook.
#[derive(Clone)]
struct MockBackend;

impl From<DynBackend> for MockBackend {
    fn from(_: DynBackend) -> Self {
        MockBackend
    }
}

impl ExecutionBackend for MockBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<apollia_core::AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Calcule la signature HMAC-SHA256 au format `sha256=<hex>`.
fn compute_hmac(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC key creation failed in test helper");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Construit une `TriggerDefinition` de type webhook.
fn webhook_def(id: &str, secret: &str) -> TriggerDefinition {
    TriggerDefinition {
        id: id.into(),
        agent: "crm-agent".into(),
        enabled: true,
        on_busy: OnBusyPolicy::Queue { max_depth: 16 },
        source: TriggerSourceConfig::Webhook {
            secret: secret.into(),
        },
        input_template: InputTemplate("{{body}}".into()),
    }
}

/// Construit un `AppState<MockBackend>` avec un `TriggerEngine` démarré.
///
/// Retourne `(state, submit_count)`.
async fn build_webhook_state(
    defs: Vec<TriggerDefinition>,
) -> (AppState<MockBackend>, Arc<AtomicU32>) {
    let (event_sender, _rx) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_sender.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 64);

    let (mock_submitter, submit_count) = MockTaskSubmitter::new();
    let engine_handle = TriggerEngineHandle::spawn(
        defs,
        mock_submitter,
        event_sender.clone(),
        None,
        ObservabilityConfig::default(),
    )
    .await;

    let state = AppState {
        router_handle,
        registry_handle,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        backend: MockBackend,
        llm_router: None,
        trigger_engine: Some(engine_handle),
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
    };
    (state, submit_count)
}

/// Démarre un `APIServer` sur un port éphémère.
///
/// Retourne `(handle, port, socket_path)`.
async fn start_test_server(state: AppState<MockBackend>) -> (APIServerHandle, u16, PathBuf) {
    // Port 0 → OS choisit un port libre
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind port 0 failed");
    let port = listener.local_addr().expect("local_addr failed").port();
    drop(listener); // Libérer le listener temporaire

    let id = &uuid::Uuid::new_v4().to_string()[..8];
    let socket_path = PathBuf::from(format!("/tmp/ap-webhook-{id}.sock"));

    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port: port,
        bind_addr: "127.0.0.1".to_string(),
        api_token: None,
    };
    let server = APIServer::new(config, state);
    let handle = server.start().await.expect("APIServer start failed");

    // Laisser le listener TCP être prêt
    tokio::time::sleep(Duration::from_millis(50)).await;
    (handle, port, socket_path)
}

/// Envoie une requête `POST` HTTP/1.1 avec des en-têtes personnalisés.
///
/// Retourne le code de statut HTTP.
async fn http_post_webhook(
    port: u16,
    path: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut header = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        header.push_str(&format!("{name}: {value}\r\n"));
    }
    header.push_str("\r\n");

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed");
    stream
        .write_all(header.as_bytes())
        .await
        .expect("write header failed");
    stream.write_all(body).await.expect("write body failed");

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .expect("read response failed");

    // Extraire le code de statut depuis la première ligne : "HTTP/1.1 200 OK"
    String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// POST avec signature HMAC valide → HTTP 200 + 1 submit().
#[tokio::test]
async fn test_ac3_valid_webhook_returns_200() {
    // GIVEN - serveur avec trigger webhook "crm-sync" (secret "test-secret")
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{\"event\": \"lead_created\"}";
    let sig = compute_hmac("test-secret", body);

    // WHEN - POST valide avec signature HMAC correcte
    let status = http_post_webhook(
        port,
        "/webhooks/crm-sync",
        body,
        &[
            ("Content-Type", "application/json"),
            ("X-Apollia-Signature", &sig),
        ],
    )
    .await;

    // THEN - HTTP 200 retourné immédiatement
    assert_eq!(status, 200, "expected HTTP 200 for valid webhook POST");

    // ET - 1 submit() effectué dans le TriggerEngine (fire-and-forget, poll avec timeout)
    let submitted = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if submit_count.load(Ordering::SeqCst) >= 1 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        submitted.unwrap_or(false),
        "expected 1 submit() after valid webhook, got {}",
        submit_count.load(Ordering::SeqCst)
    );
}

/// POST avec signature invalide → HTTP 401 + zéro submit().
#[tokio::test]
async fn test_ac4_invalid_signature_returns_401() {
    // GIVEN - même serveur avec trigger webhook "crm-sync"
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{\"event\": \"lead_created\"}";
    // Signature incorrecte - 64 zéros hexadécimaux
    let bad_sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";

    // WHEN - POST avec signature invalide
    let status = http_post_webhook(
        port,
        "/webhooks/crm-sync",
        body,
        &[
            ("Content-Type", "application/json"),
            ("X-Apollia-Signature", bad_sig),
        ],
    )
    .await;

    // THEN - HTTP 401 retourné, aucune soumission
    assert_eq!(
        status, 401,
        "expected HTTP 401 for invalid webhook signature"
    );
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        0,
        "submit() must NOT be called when HMAC signature is invalid"
    );
}

// ─── Extra : 404 pour trigger inconnu ─────────────────────────────────────

/// Extra : POST sur un trigger inconnu → HTTP 404 + zéro submit().
#[tokio::test]
async fn test_webhook_unknown_trigger_returns_404() {
    // GIVEN - serveur avec trigger "crm-sync" uniquement
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{}";
    let sig = compute_hmac("test-secret", body);

    // WHEN - POST sur un ID de trigger inexistant
    let status = http_post_webhook(
        port,
        "/webhooks/nonexistent-trigger",
        body,
        &[("X-Apollia-Signature", &sig)],
    )
    .await;

    // THEN - HTTP 404 retourné, aucune soumission
    assert_eq!(status, 404, "expected HTTP 404 for unknown webhook trigger");
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        0,
        "submit() must NOT be called for unknown trigger"
    );
}
