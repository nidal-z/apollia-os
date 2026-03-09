use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

use crate::{
    config::{channel_accepts_event, NotificationConfig},
    engine::{NotifError, Notification, NotificationChannel},
};

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
        Self { config, client }
    }
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
    /// - **Headers** : `Content-Type: application/json` (via `.json()`),
    ///   `X-Apollia-Event: <event>`, `User-Agent: apollia-os/<version>` (via le client)
    /// - **Timeout** : 5 s (configuré sur le client dans [`new`])
    ///
    /// Retourne [`NotifError::WebhookFailed`] pour toute erreur réseau ou
    /// réponse HTTP non-2xx. L'erreur est non-critique : le runtime la logge en
    /// `warn!` sans interrompre le dispatch.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        let payload = build_payload(notif);

        let resp = self
            .client
            .post(&self.config.url)
            .header("X-Apollia-Event", &notif.event)
            .json(&payload)
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
        })
    }

    // ─── AC-5 — canal désactivé ───────────────────────────────────────────────

    #[test]
    fn test_ac5_disabled_channel_accepts_false() {
        // GIVEN canal webhook configuré avec enabled=false
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "slack".into(),
            url: "http://test".into(),
            enabled: false,
            events: None,
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
        });
        let config = make_config(vec!["task.input_required", "task.failed", "agent.degraded"]);

        // WHEN / THEN — agent.degraded rejeté car absent de la liste du canal
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        assert!(!channel.accepts("agent.degraded", &config));
    }

    // ─── AC-1 — structure payload JSON ───────────────────────────────────────

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

    // ─── AC-3 — timeout → NotifError::WebhookFailed ──────────────────────────

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
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("client build");
        let channel = WebhookChannel::with_client(config, client);

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

    // ─── AC-4 — réponse HTTP 500 → NotifError::WebhookFailed ─────────────────

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
        });

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

    // ─── AC-2 — headers corrects envoyés ─────────────────────────────────────

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
}
