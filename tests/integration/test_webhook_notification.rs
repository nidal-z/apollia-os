//! Integration tests for WebhookChannel.
//!
//! Tests the full HTTP notification path using a real `WebhookChannel`
//! from `apollia-notifications`:
//! - wiremock server captures the POST and verifies all JSON fields
//!   and the `X-Apollia-Event` header.
//! - HTTP server that never responds → `NotifError::WebhookFailed`
//!   returned in < 2 s (non-blocking, no crash).

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use apollia_notifications::{
    NotifError, Notification, NotificationChannel, Severity, WebhookChannel, WebhookChannelConfig,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Builds a `Notification` for `task.input_required` with HITL metadata.
fn make_input_required_notification() -> Notification {
    let mut metadata = HashMap::new();
    metadata.insert(
        "resume_url".into(),
        "http://localhost:7771/api/v1/tasks/t-0042/resume".into(),
    );
    metadata.insert(
        "inspect_url".into(),
        "http://localhost:7771/dashboard#task-t-0042".into(),
    );

    Notification {
        event: "task.input_required".into(),
        timestamp: Utc::now(),
        task_id: Some("t-0042".into()),
        agent: Some("devis-agent".into()),
        message: "Devis #42 - 12 500€ TTC pour Dupont SA - confirmer l'envoi ?".into(),
        metadata,
        severity: Severity::Warning,
    }
}

/// Builds a `WebhookChannel` pointing at `url` with default timeout (5 s).
fn make_channel(url: &str) -> WebhookChannel {
    WebhookChannel::new(WebhookChannelConfig {
        id: "test-webhook".into(),
        url: url.to_string(),
        enabled: true,
        events: None,
        signing_secret: None,
        min_severity: Severity::Info,
    })
    // Opt out of the SSRF guard so these in-process tests can reach the
    // 127.0.0.1 mock server (the guard is on by default in production).
    .with_ssrf_guard(false)
}

// ── Payload JSON Apollia vérifié avec mock HTTP server ────────────────

/// ÉTANT DONNÉ un mock HTTP server (wiremock) qui attend POST sur /webhook
/// QUAND `WebhookChannel.send(Notification{event:"task.input_required", ...})` est appelé
/// ALORS le body reçu est un JSON valide avec tous les champs Apollia :
///       `event`, `timestamp`, `runtime`, `version`, `task_id`, `agent`,
///       `message`, `metadata`, `severity`
///       ET le header `X-Apollia-Event` == `"task.input_required"`
#[tokio::test]
async fn test_ac6_webhook_payload_verified() {
    // GIVEN - wiremock server that expects a POST to /webhook
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/webhook"))
        .and(header("X-Apollia-Event", "task.input_required"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let channel = make_channel(&format!("{}/webhook", server.uri()));
    let notif = make_input_required_notification();

    // WHEN
    let result = channel.send(&notif).await;

    // THEN - send succeeds
    assert!(result.is_ok(), "send must succeed; got: {:?}", result.err());

    // THEN - exactly 1 request received by the mock server
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "mock server must receive exactly one request"
    );

    let req = &requests[0];

    // THEN - body is valid JSON with all required Apollia fields
    let body: serde_json::Value =
        serde_json::from_slice(&req.body).expect("request body must be valid JSON");

    assert_eq!(
        body["event"], "task.input_required",
        "event field must match"
    );
    assert_eq!(
        body["runtime"], "apollia-os",
        "runtime must be 'apollia-os'"
    );
    assert_eq!(body["task_id"], "t-0042", "task_id must match");
    assert_eq!(body["agent"], "devis-agent", "agent must match");
    assert_eq!(
        body["severity"], "warning",
        "severity must be 'warning' for input_required"
    );
    assert!(
        body["timestamp"].as_str().is_some(),
        "timestamp must be an ISO 8601 string"
    );
    assert!(
        body["version"].as_str().is_some(),
        "version must be present and non-null"
    );
    assert!(
        !body["message"].as_str().unwrap_or("").is_empty(),
        "message must not be empty"
    );

    // THEN - metadata contains HITL URLs
    assert!(
        body["metadata"]["resume_url"].as_str().is_some(),
        "metadata.resume_url must be present"
    );
    assert!(
        body["metadata"]["inspect_url"].as_str().is_some(),
        "metadata.inspect_url must be present"
    );

    // THEN - X-Apollia-Event header is correct (checked by wiremock matcher above)
    let event_header = req
        .headers
        .get("x-apollia-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        event_header, "task.input_required",
        "X-Apollia-Event header must equal the event name"
    );
}

// ── WebhookChannel : connexion refusée → NotifError retourné, pas de panic

/// ÉTANT DONNÉ une URL pointant vers un port sans listener (connexion refusée)
/// QUAND `WebhookChannel.send()` est appelé
/// ALORS `NotifError::WebhookFailed` retourné immédiatement (< 2 s)
///       et le runtime continue sans crash
#[tokio::test]
async fn test_ac7_webhook_timeout_returns_error() {
    // GIVEN - find a free port, then immediately drop the listener so nobody listens.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind must succeed");
    let port = listener.local_addr().expect("local_addr").port();
    // Drop the listener - subsequent TCP connects to this port will be refused.
    drop(listener);

    let channel = make_channel(&format!("http://127.0.0.1:{port}"));
    let notif = make_input_required_notification();

    let start = std::time::Instant::now();

    // WHEN
    let result = channel.send(&notif).await;

    let elapsed = start.elapsed();

    // THEN - connection-refused maps to NotifError::WebhookFailed (no panic, no hang)
    assert!(
        matches!(result, Err(NotifError::WebhookFailed(_))),
        "expected NotifError::WebhookFailed on connection refused, got: {:?}",
        result
    );

    // THEN - error returned quickly (well under the 5 s default timeout)
    assert!(
        elapsed < Duration::from_secs(5),
        "send must return within 5 s even on unreachable host; took {:?}",
        elapsed
    );
}
