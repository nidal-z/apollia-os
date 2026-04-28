use std::time::Duration;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

type HmacSha256 = Hmac<Sha256>;

/// Configuration d'un canal webhook.
///
/// Correspond à une entrée `[[notifications.channels]]` de type `webhook`
/// dans `apollia.toml`. Le champ `url` est obligatoire (contrairement au
/// type générique [`crate::config::ChannelConfig`] qui l'a en `Option`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WebhookChannelConfig {
    /// Identifiant unique du canal (ex: `"slack"`, `"monitoring"`).
    pub id: String,
    /// URL du endpoint webhook (ex: `"https://hooks.slack.com/services/..."`).
    pub url: String,
    /// Si `false`, le canal est ignoré même s'il est présent dans la config.
    pub enabled: bool,
    /// Sous-ensemble d'événements à recevoir sur ce canal.
    ///
    /// - `None` → utilise la liste globale (`[notifications].events`)
    /// - `Some(["*"])` → tous les événements de la liste globale
    /// - `Some(liste)` → sous-ensemble d'événements explicites
    pub events: Option<Vec<String>>,
    /// Secret partagé pour signer les payloads sortants avec HMAC-SHA256.
    ///
    /// Si `Some`, chaque requête reçoit un header `X-Apollia-Signature: sha256=<hex>`.
    /// Si `None`, le webhook est envoyé sans signature (rétrocompatible).
    pub signing_secret: Option<String>,
    /// Sévérité minimale acceptée par ce canal.
    ///
    /// Les notifications dont la sévérité est inférieure à ce seuil sont silencieusement
    /// ignorées. Défaut lors de la désérialisation : [`Severity::Info`] (toutes les
    /// notifications non-debug sont transmises).
    #[serde(default)]
    pub min_severity: Severity,
}

/// Canal de notification via HTTP POST — format JSON fixe Apollia.
///
/// Construit un [`reqwest::Client`] avec un timeout de 5 secondes au moment de
/// la création. Chaque appel à [`send`] effectue un `POST` vers l'URL configurée
/// avec le payload JSON documenté dans la spec Apollia.
///
/// En cas d'erreur réseau, de timeout ou de réponse HTTP non-2xx,
/// [`NotifError::WebhookFailed`] est retourné. L'appelant
/// ([`crate::engine::NotificationEngine`]) logge l'erreur en `warn!` et
/// continue sans interrompre le dispatch vers les autres canaux.
pub struct WebhookChannel {
    config: WebhookChannelConfig,
    client: Client,
    ssrf_guard: bool,
}

impl WebhookChannel {
    /// Crée un canal webhook avec timeout 5 s et User-Agent `apollia-os/<version>`.
    ///
    /// # Panics
    ///
    /// Panics si la construction du [`reqwest::Client`] échoue — cela ne peut
    /// pas arriver sur les systèmes supportés (aucun TLS personnalisé ni proxy
    /// système incompatible requis).
    pub fn new(config: WebhookChannelConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent(format!("apollia-os/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest::Client build ne peut pas échouer avec la config par défaut");
        Self::with_client(config, client)
    }

    /// Crée un canal avec un client reqwest fourni.
    ///
    /// Permet d'injecter un client avec un timeout court ou un serveur mock
    /// dans les tests.
    pub(crate) fn with_client(config: WebhookChannelConfig, client: Client) -> Self {
        Self {
            config,
            client,
            ssrf_guard: true,
        }
    }

    /// Active ou désactive le SSRF guard sur ce canal.
    ///
    /// Activé par défaut. L'opt-out existe uniquement pour les tests
    /// d'intégration in-process qui doivent dialoguer avec un serveur mock
    /// sur `127.0.0.1`. Tout site d'appel en production doit le laisser à
    /// `true`.
    #[must_use]
    pub fn with_ssrf_guard(mut self, enabled: bool) -> Self {
        self.ssrf_guard = enabled;
        self
    }
}

/// Calcule la signature HMAC-SHA256 d'un body avec le secret donné.
///
/// Retourne la chaîne au format `sha256=<hex>` tel qu'attendu dans le header
/// `X-Apollia-Signature`. Conforme à la convention GitHub/Stripe.
///
/// HMAC accepte des clés de toute taille, le `expect` interne ne peut donc
/// jamais se déclencher en pratique.
pub fn compute_signature(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any size");
    mac.update(body);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Construit le payload JSON Apollia à partir d'une [`Notification`].
///
/// Le format est fixe et documenté :
/// `event`, `timestamp`, `runtime`, `version`, `task_id`, `agent`, `message`,
/// `metadata`, `severity`.
pub(crate) fn build_payload(notif: &Notification) -> serde_json::Value {
    serde_json::json!({
        "event":     notif.event,
        "timestamp": notif.timestamp.to_rfc3339(),
        "runtime":   "apollia-os",
        "version":   env!("CARGO_PKG_VERSION"),
        "task_id":   notif.task_id,
        "agent":     notif.agent,
        "message":   notif.message,
        "metadata":  notif.metadata,
        "severity":  notif.severity.as_str(),
    })
}

#[async_trait]
impl NotificationChannel for WebhookChannel {
    /// Retourne l'identifiant du canal tel que configuré dans `apollia.toml`.
    fn id(&self) -> &str {
        &self.config.id
    }

    /// Retourne `true` si ce canal est activé et accepte l'événement donné.
    ///
    /// Délègue la logique de filtrage à [`channel_accepts_event`].
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(
            self.config.enabled,
            &self.config.events,
            event,
            &config.events,
        )
    }

    /// Envoie la notification via HTTP POST vers l'URL configurée.
    ///
    /// - **Payload** : format JSON fixe Apollia (voir [`build_payload`])
    /// - **Headers** : `Content-Type: application/json`, `X-Apollia-Event: <event>`,
    ///   `User-Agent: apollia-os/<version>` (via le client),
    ///   `X-Apollia-Signature: sha256=<hex>` si `signing_secret` est configuré
    /// - **Timeout** : 5 s (configuré sur le client dans [`new`])
    ///
    /// La signature est calculée sur le body JSON sérialisé final avant envoi.
    ///
    /// Retourne [`NotifError::WebhookFailed`] pour toute erreur réseau ou
    /// réponse HTTP non-2xx. L'erreur est non-critique : le runtime la logge en
    /// `warn!` sans interrompre le dispatch.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        if notif.severity < self.config.min_severity {
            return Ok(());
        }

        // SSRF guard — must precede the HTTP request. Opérateur peut configurer
        // une URL pointant sur 127.0.0.1 ou un endpoint metadata cloud ;
        // refus avant tout octet émis.
        if self.ssrf_guard {
            let parsed_url = url::Url::parse(&self.config.url)
                .map_err(|e| NotifError::InvalidUrl(e.to_string()))?;
            apollia_tools::ssrf::assert_public(&parsed_url)
                .map_err(|e| NotifError::Ssrf(e.to_string()))?;
        }

        let payload = build_payload(notif);
        let body_bytes =
            serde_json::to_vec(&payload).map_err(|e| NotifError::WebhookFailed(e.to_string()))?;

        let mut builder = self
            .client
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .header("X-Apollia-Event", &notif.event);

        if let Some(secret) = &self.config.signing_secret {
            let signature = compute_signature(secret, &body_bytes);
            tracing::debug!(channel = %self.config.id, "webhook request signed with HMAC-SHA256");
            builder = builder.header("X-Apollia-Signature", signature);
        }

        let resp = builder
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| NotifError::WebhookFailed(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(NotifError::WebhookFailed(format!("HTTP {}", resp.status())));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::config::{NotificationConfig, Severity};

    fn make_config(events: Vec<&str>) -> NotificationConfig {
        NotificationConfig {
            events: events.into_iter().map(String::from).collect(),
            channels: vec![],
            inactivity_timeout_secs: 30,
        }
    }

    fn make_notif(event: &str, task_id: Option<&str>, severity: Severity) -> Notification {
        let mut metadata = HashMap::new();
        if event == "task.input_required" {
            metadata.insert(
                "resume_url".into(),
                "http://localhost:7771/api/v1/tasks/t-0042/resume".into(),
            );
            metadata.insert(
                "inspect_url".into(),
                "http://localhost:7771/dashboard#tasks/t-0042".into(),
            );
        }
        Notification {
            event: event.into(),
            timestamp: Utc::now(),
            task_id: task_id.map(String::from),
            agent: Some("devis-agent".into()),
            message: "Message de test".into(),
            metadata,
            severity,
        }
    }

    fn make_channel_url(url: &str) -> WebhookChannel {
        WebhookChannel::new(WebhookChannelConfig {
            id: "test-webhook".into(),
            url: url.into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false)
    }

    fn make_channel_with_secret(url: &str, secret: &str) -> WebhookChannel {
        WebhookChannel::new(WebhookChannelConfig {
            id: "test-signed-webhook".into(),
            url: url.into(),
            enabled: true,
            events: None,
            signing_secret: Some(secret.into()),
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false)
    }

    // ─── canal désactivé ───────────────────────────────────────────────

    #[test]
    fn test_ac5_disabled_channel_accepts_false() {
        // GIVEN canal webhook configuré avec enabled=false
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "slack".into(),
            url: "http://test".into(),
            enabled: false,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let config = make_config(vec!["task.input_required"]);

        // WHEN / THEN — accepts() retourne false sans que send() soit jamais appelé
        assert!(!channel.accepts("task.input_required", &config));
    }

    #[test]
    fn test_ac5_enabled_channel_accepts_matching_event() {
        // GIVEN canal activé sans liste propre → délègue à la liste globale
        let channel = make_channel_url("http://example.com");
        let config = make_config(vec!["task.input_required", "task.failed"]);

        // WHEN / THEN
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        // Événement absent de la liste globale → refusé
        assert!(!channel.accepts("agent.degraded", &config));
    }

    #[test]
    fn test_ac5_channel_with_per_channel_events_subset() {
        // GIVEN canal avec sous-ensemble d'événements explicites
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "slack".into(),
            url: "http://example.com".into(),
            enabled: true,
            events: Some(vec!["task.input_required".into(), "task.failed".into()]),
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let config = make_config(vec!["task.input_required", "task.failed", "agent.degraded"]);

        // WHEN / THEN — agent.degraded rejeté car absent de la liste du canal
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        assert!(!channel.accepts("agent.degraded", &config));
    }

    // ─── filtrage par sévérité ────────────────────────────────────────

    #[tokio::test]
    async fn test_webhook_filters_below_min_severity() {
        // GIVEN canal webhook avec min_severity = Warning
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "webhook-warn".into(),
            url: "http://127.0.0.1:1".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Warning,
        });

        // WHEN notification Info (< Warning) dispatchée
        let notif = make_notif("task.completed", None, Severity::Info);

        // THEN Ok(()) immédiat sans tentative réseau
        let result = channel.send(&notif).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_webhook_default_min_severity_is_info() {
        // GIVEN WebhookChannelConfig avec min_severity par défaut (serde default)
        let json = r#"{
            "id": "webhook",
            "url": "http://example.com",
            "enabled": true
        }"#;
        let cfg: WebhookChannelConfig = serde_json::from_str(json).expect("déserialisation");

        // WHEN / THEN — min_severity = Info (Severity::default())
        assert_eq!(cfg.min_severity, Severity::Info);
    }

    // ─── structure payload JSON ───────────────────────────────────────

    #[test]
    fn test_ac1_payload_json_structure_task_input_required() {
        // GIVEN une Notification task.input_required
        let notif = make_notif("task.input_required", Some("t-0042"), Severity::Warning);

        // WHEN
        let payload = build_payload(&notif);

        // THEN — tous les champs du format JSON fixe Apollia sont présents
        assert_eq!(payload["event"], "task.input_required");
        assert_eq!(payload["runtime"], "apollia-os");
        assert_eq!(payload["task_id"], "t-0042");
        assert_eq!(payload["agent"], "devis-agent");
        assert_eq!(payload["severity"], "warning");
        assert!(
            payload["timestamp"].as_str().is_some(),
            "timestamp doit être une chaîne ISO8601"
        );
        assert!(
            payload["version"].as_str().is_some(),
            "version doit être présente"
        );
        // metadata contient les URLs HITL
        assert!(
            payload["metadata"]["resume_url"].as_str().is_some(),
            "resume_url absent des metadata"
        );
        assert!(
            payload["metadata"]["inspect_url"].as_str().is_some(),
            "inspect_url absent des metadata"
        );
    }

    #[test]
    fn test_ac1_payload_task_failed_severity_error() {
        // GIVEN une notification task.failed
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let payload = build_payload(&notif);

        // THEN
        assert_eq!(payload["event"], "task.failed");
        assert_eq!(payload["severity"], "error");
        assert_eq!(payload["agent"], "devis-agent");
    }

    #[test]
    fn test_ac1_payload_null_fields_when_no_task_id() {
        // GIVEN une notification sans task_id (ex: agent.degraded)
        let notif = Notification {
            event: "agent.degraded".into(),
            timestamp: Utc::now(),
            task_id: None,
            agent: Some("mon-agent".into()),
            message: "Agent dégradé".into(),
            metadata: HashMap::new(),
            severity: Severity::Warning,
        };

        // WHEN
        let payload = build_payload(&notif);

        // THEN — task_id est null (JSON null), pas absent
        assert!(payload["task_id"].is_null());
        assert_eq!(payload["event"], "agent.degraded");
    }

    // ─── timeout → NotifError::WebhookFailed ──────────────────────────

    #[tokio::test]
    async fn test_ac3_webhook_timeout_returns_error() {
        // GIVEN un serveur TCP qui accepte la connexion mais ne répond jamais
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        tokio::spawn(async move {
            // Accepte et tient la connexion ouverte sans jamais répondre
            if let Ok((_stream, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });

        // Client avec timeout court pour ne pas bloquer le test
        let config = WebhookChannelConfig {
            id: "timeout-test".into(),
            url: format!("http://127.0.0.1:{}", addr.port()),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("client build");
        let channel = WebhookChannel::with_client(config, client).with_ssrf_guard(false);

        // WHEN
        let result = channel
            .send(&make_notif(
                "task.input_required",
                Some("t-0042"),
                Severity::Warning,
            ))
            .await;

        // THEN — NotifError::WebhookFailed retourné, pas de panic
        assert!(
            matches!(result, Err(NotifError::WebhookFailed(_))),
            "attendu Err(WebhookFailed), obtenu {:?}",
            result
        );
    }

    // ─── réponse HTTP 500 → NotifError::WebhookFailed ─────────────────

    #[tokio::test]
    async fn test_ac4_webhook_500_returns_error() {
        // GIVEN un serveur HTTP minimal qui répond 500
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                // Consomme la requête (lecture partielle suffit)
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "500-test".into(),
            url: format!("http://127.0.0.1:{}", addr.port()),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false);

        // WHEN
        let result = channel
            .send(&make_notif("task.failed", Some("t-001"), Severity::Error))
            .await;

        // THEN — NotifError::WebhookFailed contenant "500"
        match result {
            Err(NotifError::WebhookFailed(msg)) => {
                assert!(
                    msg.contains("500"),
                    "message doit contenir '500', obtenu: {msg}"
                );
            }
            other => panic!("attendu Err(WebhookFailed), obtenu {:?}", other),
        }
    }

    // ─── headers corrects envoyés ─────────────────────────────────────

    #[tokio::test]
    async fn test_ac2_headers_sent_correctly() {
        // GIVEN un serveur HTTP qui capture la requête brute
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (captured_tx, captured_rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = captured_tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = make_channel_url(&format!("http://127.0.0.1:{}", addr.port()));
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let _ = channel.send(&notif).await;

        // THEN — headers X-Apollia-Event, Content-Type et User-Agent présents
        let request = captured_rx
            .await
            .expect("requête non capturée par le serveur mock");

        // X-Apollia-Event doit contenir le nom de l'événement
        assert!(
            request
                .to_lowercase()
                .contains("x-apollia-event: task.failed"),
            "header X-Apollia-Event absent ou incorrect\n---\n{request}"
        );
        // Content-Type application/json (positionné par reqwest via .json())
        assert!(
            request
                .to_lowercase()
                .contains("content-type: application/json"),
            "header Content-Type absent\n---\n{request}"
        );
        // User-Agent contient le préfixe apollia-os
        assert!(
            request.to_lowercase().contains("apollia-os/"),
            "header User-Agent absent ou incorrect\n---\n{request}"
        );
    }

    // ─── HMAC-SHA256 : calcul de signature ────────────────────────────

    #[test]
    fn test_hmac_signature_matches_expected_value() {
        // GIVEN secret = "secret", body = b"payload"
        let signature = compute_signature("secret", b"payload");

        // WHEN recompute independently with raw HMAC
        let mut mac = HmacSha256::new_from_slice(b"secret").expect("valid key");
        mac.update(b"payload");
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        // THEN both computations produce the same result
        assert_eq!(
            signature, expected,
            "compute_signature doit correspondre au calcul HMAC direct"
        );
    }

    #[test]
    fn test_empty_body_produces_valid_signature() {
        // GIVEN secret = "secret", body vide
        let signature = compute_signature("secret", b"");

        // WHEN recompute with empty body
        let mut mac = HmacSha256::new_from_slice(b"secret").expect("valid key");
        mac.update(b"");
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        // THEN signature est valide et correspond (edge case : body vide)
        assert_eq!(signature, expected);
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), "sha256=".len() + 64);
    }

    #[test]
    fn test_different_secrets_produce_different_signatures() {
        // GIVEN même body, secrets différents
        let body = b"same payload";
        let sig1 = compute_signature("secret_a", body);
        let sig2 = compute_signature("secret_b", body);

        // THEN signatures distinctes
        assert_ne!(
            sig1, sig2,
            "des secrets différents doivent produire des signatures différentes"
        );
    }

    #[test]
    fn test_signature_format_is_sha256_prefix_hex() {
        // GIVEN un secret et un body quelconques
        let signature = compute_signature("whsec_test123", b"{\"event\":\"agent.started\"}");

        // THEN format "sha256=<64 hex chars>"
        assert!(
            signature.starts_with("sha256="),
            "la signature doit commencer par 'sha256='"
        );
        let hex_part = &signature["sha256=".len()..];
        assert_eq!(
            hex_part.len(),
            64,
            "la partie hex doit faire 64 caractères (HMAC-SHA256 = 32 octets)"
        );
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "la partie hex ne doit contenir que des caractères hexadécimaux"
        );
    }

    // ─── header X-Apollia-Signature dans les requêtes HTTP ───────────

    #[tokio::test]
    async fn test_webhook_with_secret_adds_signature_header() {
        // GIVEN un serveur HTTP qui capture la requête brute
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = make_channel_with_secret(
            &format!("http://127.0.0.1:{}", addr.port()),
            "whsec_test123",
        );
        let notif = make_notif("agent.started", None, Severity::Info);

        // WHEN
        let _ = channel.send(&notif).await;
        let request = rx.await.expect("requête non capturée");

        // THEN X-Apollia-Signature présent avec le bon format
        let request_lower = request.to_lowercase();
        assert!(
            request_lower.contains("x-apollia-signature: sha256="),
            "header X-Apollia-Signature absent ou malformé\n---\n{request}"
        );
    }

    #[tokio::test]
    async fn test_webhook_without_secret_no_signature_header() {
        // GIVEN un serveur HTTP qui capture la requête brute
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        // Canal sans signing_secret
        let channel = make_channel_url(&format!("http://127.0.0.1:{}", addr.port()));
        let notif = make_notif("agent.started", None, Severity::Info);

        // WHEN
        let _ = channel.send(&notif).await;
        let request = rx.await.expect("requête non capturée");

        // THEN X-Apollia-Signature absent
        assert!(
            !request.to_lowercase().contains("x-apollia-signature"),
            "X-Apollia-Signature ne doit pas être présent sans secret\n---\n{request}"
        );
    }

    // ─── SSRF guard ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_webhook_channel_blocks_internal_url() {
        // GIVEN un canal webhook configuré (par erreur de l'opérateur) avec
        // une URL pointant sur le réseau privé RFC1918. Aucun serveur n'écoute,
        // mais le SSRF guard doit refuser l'envoi avant tout I/O réseau.
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "exfil-test".into(),
            url: "http://10.0.0.1/exfil".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let result = channel.send(&notif).await;

        // THEN — NotifError::Ssrf, jamais de tentative HTTP
        assert!(
            matches!(result, Err(NotifError::Ssrf(_))),
            "attendu Err(Ssrf), obtenu {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_webhook_channel_blocks_metadata_endpoint() {
        // GIVEN URL pointant sur l'endpoint metadata cloud (link-local).
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "metadata-test".into(),
            url: "http://169.254.169.254/latest/meta-data/".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let result = channel.send(&notif).await;

        // THEN
        assert!(
            matches!(result, Err(NotifError::Ssrf(_))),
            "attendu Err(Ssrf), obtenu {:?}",
            result
        );
    }
}
