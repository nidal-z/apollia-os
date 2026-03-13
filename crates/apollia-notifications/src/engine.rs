use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use apollia_core::{EventBusSender, RuntimeEvent};

use crate::{config::NotificationConfig, config::Severity, event_filter};

/// Erreur retournée par un canal de notification lors de l'envoi.
///
/// Chaque variante correspond à une catégorie de canal. Les erreurs sont
/// loggées en `warn!` par le [`NotificationEngine`] — elles n'interrompent
/// jamais le dispatch vers les autres canaux.
#[derive(Debug, thiserror::Error)]
pub enum NotifError {
    /// Canal desktop indisponible (notifications OS non supportées ou permission refusée).
    #[error("canal desktop indisponible : {0}")]
    DesktopUnavailable(String),
    /// Appel webhook échoué (erreur réseau, timeout, code HTTP non-2xx).
    #[error("webhook échoué : {0}")]
    WebhookFailed(String),
    /// Erreur interne du canal (sérialisation, état incohérent, etc.).
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Notification prête à être envoyée via un ou plusieurs canaux.
///
/// Produite par [`crate::event_filter::map_event`] à partir d'un [`RuntimeEvent`].
/// Distribuée à chaque [`NotificationChannel`] qui l'accepte.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Nom de l'événement déclencheur (ex: `"task.input_required"`, `"task.failed"`).
    pub event: String,
    /// Horodatage UTC de la notification.
    pub timestamp: DateTime<Utc>,
    /// Identifiant de la tâche concernée, si applicable.
    pub task_id: Option<String>,
    /// Nom ou identifiant de l'agent concerné, si applicable.
    pub agent: Option<String>,
    /// Message lisible destiné à l'utilisateur.
    pub message: String,
    /// Métadonnées additionnelles (URLs d'action, identifiants, contexte).
    pub metadata: HashMap<String, String>,
    /// Sévérité de la notification.
    pub severity: Severity,
}

/// Trait à implémenter par chaque canal de notification.
///
/// Un canal est object-safe (`Box<dyn NotificationChannel>`) et thread-safe (`Send + Sync`).
/// Les canaux concrets sont implémentés dans :
/// - [`crate::channels::desktop`] — notifications natives OS (STORY-100)
/// - [`crate::channels::webhook`] — requêtes HTTP POST (STORY-101)
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Identifiant unique du canal tel que configuré dans `apollia.toml`.
    fn id(&self) -> &str;

    /// Retourne `true` si ce canal accepte l'événement nommé.
    ///
    /// Déléguer la logique à [`crate::config::channel_accepts_event`] en passant
    /// l'état propre du canal (`enabled`, `events`) et la config globale.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool;

    /// Envoie la notification via ce canal.
    ///
    /// En cas d'erreur, retourner un [`NotifError`] — le [`NotificationEngine`]
    /// logge l'erreur et continue avec les autres canaux sans panic.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError>;
}

/// Handle returned by [`NotificationEngine::spawn`] to stop the engine gracefully.
///
/// Call [`NotificationEngineHandle::shutdown`] to signal the engine to stop and
/// wait for its task to finish. Dropping the handle without calling `shutdown`
/// sends the stop signal but does not wait for completion.
pub struct NotificationEngineHandle {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl NotificationEngineHandle {
    /// Signal the engine to stop and wait for it to finish.
    ///
    /// Idempotent — if the engine already stopped (bus closed), this returns immediately.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.join_handle.await;
    }
}

/// Moteur de notification : s'abonne à l'EventBus et dispatche les événements
/// aux canaux configurés.
///
/// Démarré par le Supervisor via [`NotificationEngine::spawn`] (STORY-102).
/// Chaque événement reçu est transformé en [`Notification`] via
/// [`map_event`], puis dispatché à chaque canal dont [`NotificationChannel::accepts`]
/// retourne `true`.
///
/// Les erreurs de canal sont loggées en `warn!` et n'interrompent pas le dispatch.
pub struct NotificationEngine {
    config: NotificationConfig,
    channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
    /// Chemin vers la base SQLite `hitl.db` pour l'écriture dans `notification_logs`.
    ///
    /// `None` → logging désactivé (tests, dev sans data_dir). En production, le
    /// Supervisor passe `Some(data_dir.join("hitl.db"))`.
    log_db_path: Option<PathBuf>,
}

impl NotificationEngine {
    /// Crée un nouveau moteur de notification.
    ///
    /// `log_db_path` : chemin vers `hitl.db` pour écrire la table `notification_logs`.
    /// Passer `None` pour désactiver le logging SQLite (tests).
    pub fn new(
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
        event_bus: EventBusSender,
        log_db_path: Option<PathBuf>,
    ) -> Self {
        Self {
            config,
            channels,
            event_bus,
            log_db_path,
        }
    }

    /// Spawns the engine as a Tokio task and returns a [`NotificationEngineHandle`].
    ///
    /// Preferred over [`run`] in production — the handle allows the Supervisor to
    /// stop the engine gracefully before the EventBus closes (fixes race condition
    /// where late notifications are delivered after `apollia-os stop`).
    pub fn spawn(self) -> NotificationEngineHandle {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let join_handle = tokio::spawn(self.run_with_shutdown(shutdown_rx));
        NotificationEngineHandle {
            shutdown_tx,
            join_handle,
        }
    }

    /// Boucle principale de l'engine (variante avec signal d'arrêt explicite).
    ///
    /// Utilisée par [`spawn`]. Réagit à deux sources de terminaison :
    /// - le signal oneshot envoyé par [`NotificationEngineHandle::shutdown`]
    /// - la fermeture implicite de l'EventBus (`RecvError::Closed`)
    async fn run_with_shutdown(
        self,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let NotificationEngine {
            config,
            channels,
            event_bus,
            log_db_path,
        } = self;
        let mut rx = event_bus.subscribe();
        drop(event_bus);

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown_rx => {
                    tracing::info!("NotificationEngine : signal d'arrêt reçu — arrêt propre");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Some(notif) = map_event_with(&config, &channels, &event) {
                                let channel_results =
                                    dispatch_notif(&config, &channels, &notif).await;
                                if let Some(ref db_path) = log_db_path {
                                    let db_path = db_path.clone();
                                    let notif_clone = notif.clone();
                                    tokio::task::spawn_blocking(move || {
                                        write_notification_log(
                                            &db_path,
                                            &notif_clone,
                                            &channel_results,
                                        );
                                    });
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                skipped = n,
                                "NotificationEngine a raté des événements (bus saturé)"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("NotificationEngine : bus fermé — arrêt propre");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Boucle principale de l'engine (variante sans signal d'arrêt).
    ///
    /// Conservée pour la compatibilité des tests unitaires. En production,
    /// préférer [`spawn`] qui retourne un [`NotificationEngineHandle`].
    pub async fn run(self) {
        let NotificationEngine {
            config,
            channels,
            event_bus,
            log_db_path,
        } = self;
        let mut rx = event_bus.subscribe();
        // Libérer le sender pour permettre la fermeture du bus quand tous les
        // senders externes sont également droppés.
        drop(event_bus);

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(notif) = map_event_with(&config, &channels, &event) {
                        let channel_results =
                            dispatch_notif(&config, &channels, &notif).await;
                        if let Some(ref db_path) = log_db_path {
                            let db_path = db_path.clone();
                            let notif_clone = notif.clone();
                            tokio::task::spawn_blocking(move || {
                                write_notification_log(
                                    &db_path,
                                    &notif_clone,
                                    &channel_results,
                                );
                            });
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        skipped = n,
                        "NotificationEngine a raté des événements (bus saturé)"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("NotificationEngine : bus fermé — arrêt propre");
                    break;
                }
            }
        }
    }

    /// Transforme un [`RuntimeEvent`] en [`Notification`].
    ///
    /// Fonction pure — délègue à [`event_filter::map_event`].
    /// Testable sans infrastructure.
    pub fn map_event(&self, event: &RuntimeEvent) -> Option<Notification> {
        event_filter::map_event(event)
    }
}

/// Transforme un [`RuntimeEvent`] en [`Notification`] dans le contexte de `run()`.
///
/// Fonctions libres utilisées après destructuration de `self` dans [`NotificationEngine::run`].
fn map_event_with(
    _config: &NotificationConfig,
    _channels: &[Box<dyn NotificationChannel>],
    event: &RuntimeEvent,
) -> Option<Notification> {
    event_filter::map_event(event)
}

/// Dispatche une notification à tous les canaux qui l'acceptent.
///
/// Pour chaque canal, appelle [`NotificationChannel::accepts`] puis
/// [`NotificationChannel::send`]. Les erreurs sont loggées en `warn!` sans
/// interrompre le dispatch vers les canaux suivants.
///
/// Retourne une map `channel_id → Option<error_message>` pour les canaux
/// qui ont accepté la notification (`None` = succès, `Some(msg)` = erreur).
async fn dispatch_notif(
    config: &NotificationConfig,
    channels: &[Box<dyn NotificationChannel>],
    notif: &Notification,
) -> HashMap<String, Option<String>> {
    let mut results = HashMap::new();
    for channel in channels {
        if channel.accepts(&notif.event, config) {
            match channel.send(notif).await {
                Ok(()) => {
                    results.insert(channel.id().to_string(), None);
                }
                Err(err) => {
                    tracing::warn!(
                        channel_id = channel.id(),
                        error = %err,
                        event = %notif.event,
                        "Canal de notification en erreur — dispatch continue"
                    );
                    results.insert(channel.id().to_string(), Some(err.to_string()));
                }
            }
        }
    }
    results
}

/// Écrit une entrée dans `notification_logs` (table SQLite dans `hitl.db`).
///
/// `channel_results` : map `channel_id → None` (succès) ou `Some(msg)` (erreur).
/// La table est créée idempotentiellement si elle n'existe pas.
/// Les erreurs sont loggées en `warn!` sans propagation — le logging est best-effort.
fn write_notification_log(
    db_path: &std::path::Path,
    notif: &Notification,
    channel_results: &HashMap<String, Option<String>>,
) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "notification_logs : impossible d'ouvrir la base");
            return;
        }
    };

    if let Err(e) = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notification_logs (
            id          TEXT    PRIMARY KEY,
            event_name  TEXT    NOT NULL,
            task_id     TEXT,
            agent_id    TEXT,
            sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
            channels    TEXT    NOT NULL DEFAULT '{}',
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at ON notification_logs(sent_at);",
    ) {
        tracing::warn!(error = %e, "notification_logs : migration échouée");
        return;
    }

    // Sérialise les résultats par canal : { "desktop": "ok" | "erreur..." }
    let channels_json: serde_json::Map<String, serde_json::Value> = channel_results
        .iter()
        .map(|(id, err)| {
            let status = match err {
                None => serde_json::Value::String("ok".into()),
                Some(msg) => serde_json::Value::String(msg.clone()),
            };
            (id.clone(), status)
        })
        .collect();

    // Premier canal en erreur → champ `error` global
    let global_error: Option<String> = channel_results
        .values()
        .find_map(|e| e.as_deref().map(str::to_string));

    let id = uuid::Uuid::new_v4().to_string();
    let sent_at = notif.timestamp.to_rfc3339();
    let channels_str = serde_json::to_string(&channels_json).unwrap_or_else(|_| "{}".into());

    if let Err(e) = conn.execute(
        "INSERT INTO notification_logs (id, event_name, task_id, agent_id, sent_at, channels, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            notif.event,
            notif.task_id,
            notif.agent,
            sent_at,
            channels_str,
            global_error,
        ],
    ) {
        tracing::warn!(error = %e, "notification_logs : INSERT échoué");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{channel_accepts_event, ChannelConfig, ChannelKind};
    use apollia_core::{AgentId, TaskId};
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    /// Canal mock pour les tests unitaires.
    struct MockChannel {
        name: String,
        enabled: bool,
        events: Option<Vec<String>>,
        should_fail: bool,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl NotificationChannel for MockChannel {
        fn id(&self) -> &str {
            &self.name
        }

        fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
            channel_accepts_event(self.enabled, &self.events, event, &config.events)
        }

        async fn send(&self, _notif: &Notification) -> Result<(), NotifError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(NotifError::Internal("erreur simulée".into()))
            } else {
                Ok(())
            }
        }
    }

    fn make_config(global_events: Vec<String>) -> NotificationConfig {
        NotificationConfig {
            events: global_events,
            channels: vec![],
        }
    }

    #[test]
    fn test_ac3_accepts_subset_events_filters_correctly() {
        // GIVEN canal configuré avec un sous-ensemble d'événements
        let channel = MockChannel {
            name: "slack".into(),
            enabled: true,
            events: Some(vec!["task.input_required".into(), "task.failed".into()]),
            should_fail: false,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let config = make_config(vec![
            "task.input_required".into(),
            "task.failed".into(),
            "agent.degraded".into(),
        ]);

        // WHEN / THEN — agent.degraded rejeté car absent de la liste du canal
        assert!(!channel.accepts("agent.degraded", &config));
        // ET — les événements listés sont acceptés
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
    }

    #[test]
    fn test_ac4_accepts_wildcard_all_events() {
        // GIVEN canal configuré avec le wildcard "*"
        let channel = MockChannel {
            name: "monitoring".into(),
            enabled: true,
            events: Some(vec!["*".into()]),
            should_fail: false,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let config = make_config(vec![
            "task.input_required".into(),
            "task.failed".into(),
            "agent.degraded".into(),
        ]);

        // WHEN / THEN — tous les événements de la liste globale sont acceptés
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        assert!(channel.accepts("agent.degraded", &config));
    }

    #[tokio::test]
    async fn test_ac5_channel_error_does_not_stop_other_channels() {
        // GIVEN deux canaux : "desktop" OK, "slack" retourne une erreur
        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);

        let desktop_count = Arc::new(AtomicU32::new(0));
        let desktop_count_clone = desktop_count.clone();

        let config = make_config(vec!["task.input_required".into()]);

        let channels: Vec<Box<dyn NotificationChannel>> = vec![
            Box::new(MockChannel {
                name: "desktop".into(),
                enabled: true,
                events: None,
                should_fail: false,
                call_count: desktop_count_clone,
            }),
            Box::new(MockChannel {
                name: "slack".into(),
                enabled: true,
                events: None,
                should_fail: true,
                call_count: Arc::new(AtomicU32::new(0)),
            }),
        ];

        let engine = NotificationEngine::new(config, channels, tx.clone(), None);
        let handle = tokio::spawn(engine.run());

        // Laisser l'engine s'abonner au bus
        tokio::task::yield_now().await;

        // WHEN — envoi d'un événement
        tx.send(RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-001"),
            prompt: "Confirmer ?".into(),
            step_id: None,
        })
        .expect("envoi échoue");

        // Fermer le bus → engine.run() se termine proprement
        drop(tx);

        // Attendre la fin de la tâche engine — pas de panic
        handle.await.expect("engine a paniqué");

        // THEN — desktop a bien reçu la notification malgré l'erreur slack
        assert_eq!(desktop_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_map_event_delegates_to_event_filter() {
        // GIVEN — NotificationEngine délègue map_event à event_filter::map_event
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(make_config(vec![]), vec![], tx, None);

        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-test"),
            prompt: "Test ?".into(),
            step_id: None,
        };

        // WHEN
        let notif = engine.map_event(&event);

        // THEN
        assert!(notif.is_some());
        assert_eq!(notif.unwrap().event, "task.input_required");
    }

    #[test]
    fn test_map_event_returns_none_for_unknown_event() {
        // GIVEN
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(make_config(vec![]), vec![], tx, None);

        let event = RuntimeEvent::AgentRegistered(AgentId::from("agent-1"));

        // WHEN / THEN
        assert!(engine.map_event(&event).is_none());
    }

    /// Vérifie que ChannelConfig se désérialise correctement depuis TOML-compatible JSON.
    #[test]
    fn test_channel_config_deserialize() {
        let json = r#"{
            "id": "desktop",
            "type": "desktop",
            "enabled": true,
            "events": ["task.input_required", "task.failed"]
        }"#;
        let config: ChannelConfig = serde_json::from_str(json).expect("désérialisation échoue");
        assert_eq!(config.id, "desktop");
        assert!(matches!(config.kind, ChannelKind::Desktop));
        assert!(config.enabled);
        assert_eq!(
            config.events.as_deref(),
            Some(&["task.input_required".to_string(), "task.failed".to_string()][..])
        );
    }
}
