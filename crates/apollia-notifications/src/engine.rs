use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

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
    /// URL du webhook malformée — parsing impossible.
    #[error("URL webhook invalide : {0}")]
    InvalidUrl(String),
    /// SSRF guard a refusé l'envoi (URL pointant sur loopback / RFC1918 /
    /// link-local / metadata cloud / domaine `.local|.internal|localhost`).
    #[error("SSRF bloqué : {0}")]
    Ssrf(String),
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
/// - [`crate::channels::desktop`] — notifications natives OS.
/// - [`crate::channels::webhook`] — requêtes HTTP POST.
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

/// Commande interne envoyée au [`NotificationEngine`] via son handle.
enum NotifEngineCommand {
    /// Remplace la configuration et les canaux actifs (hot-reload).
    Reload {
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
    },
    /// Publie une notification directement sans passer par l'EventBus.
    Publish { notification: Notification },
    /// Demande un arrêt propre du moteur.
    Shutdown,
}

/// Handle returned by [`NotificationEngine::spawn`] to control the engine.
///
/// Cloneable — stockable dans `AppState` (routes REST) et `SupervisorHandles`
/// (shutdown gracieux) simultanément.
///
/// Call [`NotificationEngineHandle::shutdown`] to signal the engine to stop.
/// Call [`NotificationEngineHandle::reload`] to hot-reload configuration.
pub struct NotificationEngineHandle {
    tx: mpsc::Sender<NotifEngineCommand>,
}

impl Clone for NotificationEngineHandle {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl NotificationEngineHandle {
    /// Hot-reload la configuration et les canaux du moteur.
    ///
    /// Le moteur remplace immédiatement ses canaux internes. Les événements
    /// reçus après le reload utiliseront la nouvelle configuration.
    pub async fn reload(
        &self,
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
    ) {
        let _ = self
            .tx
            .send(NotifEngineCommand::Reload { config, channels })
            .await;
    }

    /// Publie une notification directement, sans passer par l'EventBus.
    ///
    /// La notification est dispatchée à tous les canaux dont [`NotificationChannel::accepts`]
    /// retourne `true`. Utilisé par [`crate::inactivity_watcher::InactivityWatcher`].
    pub async fn publish(&self, notification: Notification) {
        let _ = self
            .tx
            .send(NotifEngineCommand::Publish { notification })
            .await;
    }

    /// Signal the engine to stop gracefully.
    ///
    /// Fire-and-forget — the engine arrête sa boucle dès réception.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(NotifEngineCommand::Shutdown).await;
    }
}

/// Moteur de notification : s'abonne à l'EventBus et dispatche les événements
/// aux canaux configurés.
///
/// Démarré par le Supervisor via [`NotificationEngine::spawn`].
/// Chaque événement reçu est transformé en [`Notification`] via
/// [`map_event`], puis dispatché à chaque canal dont [`NotificationChannel::accepts`]
/// retourne `true`.
///
/// Les erreurs de canal sont loggées en `warn!` et n'interrompent pas le dispatch.
pub struct NotificationEngine {
    config: NotificationConfig,
    channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
    /// URL de base de l'API REST locale (ex : `http://127.0.0.1:7771`).
    ///
    /// Construite depuis `ApiConfig` au démarrage. Utilisée pour produire
    /// les URLs de reprise HITL dans les métadonnées des notifications.
    api_base_url: String,
    /// Chemin vers la base SQLite `hitl.db` pour l'écriture dans `notification_logs`.
    ///
    /// `None` → logging désactivé (tests, dev sans data_dir). En production, le
    /// Supervisor passe `Some(data_dir.join("hitl.db"))`.
    log_db_path: Option<PathBuf>,
    /// `true` si le seuil de coût LLM a déjà déclenché une notification pour cette session.
    ///
    /// Edge trigger : remis à `false` quand `threshold_exceeded` repasse à `false`
    /// (nouvelle session ou coût redescendu sous le seuil).
    cost_threshold_already_notified: bool,
}

impl NotificationEngine {
    /// Crée un nouveau moteur de notification.
    ///
    /// `api_base_url` : URL de base de l'API REST locale (ex : `http://127.0.0.1:7771`),
    /// construite depuis `ApiConfig` au démarrage du Supervisor.
    ///
    /// `log_db_path` : chemin vers `hitl.db` pour écrire la table `notification_logs`.
    /// Passer `None` pour désactiver le logging SQLite (tests).
    ///
    /// Émet un `tracing::warn!` pour chaque nom d'événement présent dans `config.events`
    /// ou dans les listes de canaux qui n'est pas reconnu par le moteur. Le démarrage
    /// n'est pas bloqué par ces avertissements.
    pub fn new(
        config: NotificationConfig,
        channels: Vec<Box<dyn NotificationChannel>>,
        event_bus: EventBusSender,
        api_base_url: String,
        log_db_path: Option<PathBuf>,
    ) -> Self {
        // Validate global event names and per-channel event names at startup.
        event_filter::warn_unknown_events(&config.events);
        for channel_cfg in &config.channels {
            if let Some(events) = &channel_cfg.events {
                event_filter::warn_unknown_events(events);
            }
        }

        Self {
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified: false,
        }
    }

    /// Spawns the engine as a Tokio task and returns a [`NotificationEngineHandle`].
    ///
    /// The handle allows the Supervisor to stop the engine gracefully before the
    /// EventBus closes, and REST routes to hot-reload configuration after CRUD
    /// mutations.
    pub fn spawn(self) -> NotificationEngineHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let NotificationEngine {
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified,
        } = self;
        tokio::spawn(run_engine_loop(
            config,
            channels,
            event_bus,
            api_base_url,
            log_db_path,
            cost_threshold_already_notified,
            cmd_rx,
        ));
        NotificationEngineHandle { tx: cmd_tx }
    }

    /// Réaction à [`RuntimeEvent::TokenBudgetUpdated`].
    ///
    /// Edge trigger : émet une notification OS uniquement à la transition `false → true`
    /// de `threshold_exceeded`. Le flag est réarmé lorsque `threshold_exceeded` repasse
    /// à `false` (nouvelle session ou coût redescendu sous le seuil).
    ///
    /// N'émet rien si le seuil n'est pas encore dépassé ou si la notification
    /// a déjà été envoyée pour ce dépassement.
    pub async fn handle_budget_update(&mut self, event: &RuntimeEvent) {
        process_budget_alert(
            &mut self.cost_threshold_already_notified,
            event,
            &self.config,
            &self.channels,
            &self.api_base_url,
            self.log_db_path.as_deref(),
        )
        .await;
    }

    /// Transforme un [`RuntimeEvent`] en [`Notification`].
    ///
    /// Fonction pure — délègue à [`event_filter::map_event`].
    /// Testable sans infrastructure.
    pub fn map_event(&self, event: &RuntimeEvent) -> Option<Notification> {
        event_filter::map_event(&self.api_base_url, event)
    }
}

/// Construit la notification d'alerte de seuil de coût LLM.
fn build_cost_alert_notification(
    session_cost_usd: f64,
    threshold_usd: f64,
    api_base_url: &str,
) -> Notification {
    let mut metadata = HashMap::new();
    metadata.insert("dashboard_url".into(), format!("{api_base_url}/dashboard"));
    Notification {
        event: "llm.cost_alert".into(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: format!("Coût session : ${session_cost_usd:.3} (seuil : ${threshold_usd:.3})"),
        metadata,
        severity: crate::config::Severity::Warning,
    }
}

/// Évalue un [`RuntimeEvent::TokenBudgetUpdated`] contre l'état de l'edge trigger.
///
/// Si `threshold_exceeded` passe de `false` à `true`, dispatche une notification
/// via les canaux configurés et met à jour `already_notified`.
/// Réarme `already_notified` à `false` quand `threshold_exceeded` repasse à `false`.
async fn process_budget_alert(
    already_notified: &mut bool,
    event: &RuntimeEvent,
    config: &NotificationConfig,
    channels: &[Box<dyn NotificationChannel>],
    api_base_url: &str,
    log_db_path: Option<&std::path::Path>,
) {
    let RuntimeEvent::TokenBudgetUpdated {
        session_cost_usd,
        threshold_usd,
        threshold_exceeded,
        ..
    } = event
    else {
        return;
    };

    if *threshold_exceeded && !*already_notified {
        *already_notified = true;
        let notif = build_cost_alert_notification(*session_cost_usd, *threshold_usd, api_base_url);
        let results = dispatch_notif(config, channels, &notif).await;
        if let Some(db_path) = log_db_path {
            let db_path = db_path.to_path_buf();
            tokio::task::spawn_blocking(move || {
                write_notification_log(&db_path, &notif, &results);
            });
        }
    } else if !threshold_exceeded {
        *already_notified = false;
    }
}

/// Boucle principale du moteur de notification (fonction libre).
///
/// Écoute simultanément l'EventBus (événements runtime) et le canal de commande
/// (reload / shutdown). Le reload remplace la config et les canaux à chaud,
/// sans interrompre l'écoute de l'EventBus.
async fn run_engine_loop(
    mut config: NotificationConfig,
    mut channels: Vec<Box<dyn NotificationChannel>>,
    event_bus: EventBusSender,
    api_base_url: String,
    log_db_path: Option<PathBuf>,
    mut cost_threshold_already_notified: bool,
    mut cmd_rx: mpsc::Receiver<NotifEngineCommand>,
) {
    let mut rx = event_bus.subscribe();
    drop(event_bus);

    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(NotifEngineCommand::Reload { config: new_config, channels: new_channels }) => {
                        let count = new_channels.len();
                        config = new_config;
                        channels = new_channels;
                        tracing::info!(channels = count, "NotificationEngine : configuration rechargée");
                    }
                    Some(NotifEngineCommand::Publish { notification }) => {
                        let channel_results = dispatch_notif(&config, &channels, &notification).await;
                        if let Some(ref db_path) = log_db_path {
                            let db_path = db_path.clone();
                            let notif_clone = notification.clone();
                            tokio::task::spawn_blocking(move || {
                                write_notification_log(&db_path, &notif_clone, &channel_results);
                            });
                        }
                    }
                    Some(NotifEngineCommand::Shutdown) | None => {
                        tracing::info!("NotificationEngine : signal d'arrêt reçu — arrêt propre");
                        break;
                    }
                }
            }
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Some(notif) = map_event_with(&config, &channels, &api_base_url, &event) {
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
                        process_budget_alert(
                            &mut cost_threshold_already_notified,
                            &event,
                            &config,
                            &channels,
                            &api_base_url,
                            log_db_path.as_deref(),
                        )
                        .await;
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

/// Transforme un [`RuntimeEvent`] en [`Notification`] dans le contexte de `run()`.
///
/// Fonctions libres utilisées après destructuration de `self` dans [`NotificationEngine::run`].
fn map_event_with(
    _config: &NotificationConfig,
    _channels: &[Box<dyn NotificationChannel>],
    base_url: &str,
    event: &RuntimeEvent,
) -> Option<Notification> {
    event_filter::map_event(base_url, event)
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
            inactivity_timeout_secs: 30,
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

        let engine = NotificationEngine::new(
            config,
            channels,
            tx.clone(),
            "http://127.0.0.1:7771".to_owned(),
            None,
        );
        let handle = engine.spawn();

        // Laisser l'engine s'abonner au bus
        tokio::task::yield_now().await;

        // WHEN — envoi d'un événement
        tx.send(RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-001"),
            prompt: "Confirmer ?".into(),
            step_id: None,
        })
        .expect("envoi échoue");

        // Laisser le dispatch s'exécuter
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Arrêter proprement via handle
        handle.shutdown().await;

        // THEN — desktop a bien reçu la notification malgré l'erreur slack
        assert_eq!(desktop_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_map_event_delegates_to_event_filter() {
        // GIVEN — NotificationEngine délègue map_event à event_filter::map_event
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let engine = NotificationEngine::new(
            make_config(vec![]),
            vec![],
            tx,
            "http://127.0.0.1:7771".to_owned(),
            None,
        );

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
        let engine = NotificationEngine::new(
            make_config(vec![]),
            vec![],
            tx,
            "http://127.0.0.1:7771".to_owned(),
            None,
        );

        let event = RuntimeEvent::AgentRegistered(AgentId::from("agent-1"));

        // WHEN / THEN
        assert!(engine.map_event(&event).is_none());
    }

    #[test]
    fn test_unknown_notification_event_emits_warning() {
        // GIVEN a set of event names where one is unknown and one is the wildcard
        let events = vec![
            "task.failed".to_string(),
            "agent.exploded".to_string(),
            "*".to_string(),
        ];

        // WHEN finding unknown events from the known set
        let unknown: Vec<&str> = events
            .iter()
            .filter(|e| *e != "*" && !crate::event_filter::KNOWN_EVENT_NAMES.contains(&e.as_str()))
            .map(String::as_str)
            .collect();

        // THEN "agent.exploded" is identified as unknown; "*" and "task.failed" are not
        assert_eq!(unknown, vec!["agent.exploded"]);

        // AND calling warn_unknown_events does not panic or block
        crate::event_filter::warn_unknown_events(&events);
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

    fn make_budget_event(threshold_exceeded: bool) -> RuntimeEvent {
        RuntimeEvent::TokenBudgetUpdated {
            session_cost_usd: 0.75,
            total_input_tokens: 1000,
            total_output_tokens: 500,
            total_cache_read_tokens: 200,
            threshold_usd: 0.50,
            threshold_exceeded,
        }
    }

    #[tokio::test]
    async fn handle_budget_update_notifies_on_threshold_exceeded() {
        // GIVEN engine with a mock channel that accepts llm.cost_alert
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let call_count = Arc::new(AtomicU32::new(0));
        let config = make_config(vec!["llm.cost_alert".into()]);
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: call_count.clone(),
        })];
        let mut engine =
            NotificationEngine::new(config, channels, tx, "http://127.0.0.1:7771".into(), None);

        // WHEN threshold is exceeded and not yet notified
        let event = make_budget_event(true);
        engine.handle_budget_update(&event).await;

        // THEN notification dispatched once
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert!(engine.cost_threshold_already_notified);
    }

    #[tokio::test]
    async fn handle_budget_update_no_duplicate_notification() {
        // GIVEN engine already notified (flag set by a prior exceeded event)
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let call_count = Arc::new(AtomicU32::new(0));
        let config = make_config(vec!["llm.cost_alert".into()]);
        let channels: Vec<Box<dyn NotificationChannel>> = vec![Box::new(MockChannel {
            name: "desktop".into(),
            enabled: true,
            events: None,
            should_fail: false,
            call_count: call_count.clone(),
        })];
        let mut engine =
            NotificationEngine::new(config, channels, tx, "http://127.0.0.1:7771".into(), None);

        // Set the flag via the first exceeded event
        engine.handle_budget_update(&make_budget_event(true)).await;
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // WHEN threshold exceeded again (still above threshold)
        engine.handle_budget_update(&make_budget_event(true)).await;

        // THEN no second notification dispatched
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn handle_budget_update_rearms_on_false() {
        // GIVEN engine already notified
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let mut engine = NotificationEngine::new(
            make_config(vec!["llm.cost_alert".into()]),
            vec![],
            tx,
            "http://127.0.0.1:7771".into(),
            None,
        );
        engine.handle_budget_update(&make_budget_event(true)).await;
        assert!(engine.cost_threshold_already_notified);

        // WHEN threshold no longer exceeded (new session or cost dropped)
        engine.handle_budget_update(&make_budget_event(false)).await;

        // THEN flag is reset
        assert!(!engine.cost_threshold_already_notified);
    }
}
