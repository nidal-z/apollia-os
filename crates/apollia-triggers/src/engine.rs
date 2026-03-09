//! `TriggerEngine` — acteur Tokio central du système de déclenchement.
//!
//! Le `TriggerEngine` est un acteur Tokio standard : struct interne, handle
//! clonable exposé via [`TriggerEngineHandle`], boucle `run_loop` dans un
//! `tokio::spawn`. Les sources envoient des [`crate::TriggerEvent`] sur le
//! channel interne ; le moteur évalue l'[`crate::OnBusyPolicy`], rend le
//! template d'entrée et soumet une tâche au [`TaskSubmitter`].
//!
//! **Cette story N'implémente PAS** :
//! - Les sources concrètes (`CronTrigger`, `FileWatchTrigger`) → STORY-067/068
//! - La route webhook → STORY-069
//! - La persistance SQLite réelle → STORY-070

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, oneshot};

use apollia_core::{AIPInput, AIPPart, EventBusSender, RuntimeEvent, TaskId, TextPart};

use crate::persistence::TriggerPersistence;
use crate::sources::spawn_source;
use crate::types::{
    OnBusyPolicy, TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig,
};

// ─── TaskSubmitter trait ───────────────────────────────────────────────────

/// Abstraction sur `TaskRouterHandle` pour la soumission de tâches depuis le `TriggerEngine`.
///
/// Pattern cohérent avec ADR-015 (`ToolExecutor`) et ADR-016 (`AgentRunner`) :
/// le crate `apollia-triggers` ne dépend pas de `apollia-runtime`, ce qui évite
/// les dépendances circulaires. Le concret `TaskRouterHandle<B>` implémentera
/// ce trait dans STORY-072 lors de l'intégration au Supervisor.
pub trait TaskSubmitter: Send + Sync + 'static {
    /// Soumet une tâche pour l'agent désigné.
    ///
    /// Retourne le `TaskId` généré si la soumission réussit, ou un message
    /// d'erreur sous forme de `String` en cas d'échec.
    fn submit<'a>(
        &'a self,
        agent: &'a str,
        input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>>;

    /// Retourne le nombre de tâches en attente ou actives pour l'agent désigné.
    ///
    /// Utilisé par [`OnBusyPolicy::Drop`] pour décider si le trigger doit être ignoré.
    fn pending_count<'a>(
        &'a self,
        agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>>;
}

// ─── TriggerCommand ───────────────────────────────────────────────────────

/// Commandes envoyées au `TriggerEngine` via son handle.
enum TriggerCommand {
    /// Trouve un trigger webhook par ID.
    FindWebhook {
        id: String,
        reply: oneshot::Sender<Option<TriggerDefinition>>,
    },
    /// Envoie un événement webhook au moteur (fire-and-forget).
    SendWebhookEvent {
        trigger_id: String,
        body: String,
        headers: HashMap<String, String>,
    },
    /// Force le déclenchement immédiat d'un trigger.
    FireNow {
        id: String,
        reply: oneshot::Sender<Result<TaskId, TriggerEngineError>>,
    },
    /// Active un trigger.
    Enable {
        id: String,
        reply: oneshot::Sender<Result<(), TriggerEngineError>>,
    },
    /// Désactive un trigger.
    Disable {
        id: String,
        reply: oneshot::Sender<Result<(), TriggerEngineError>>,
    },
    /// Liste tous les triggers avec leur statut courant.
    List {
        reply: oneshot::Sender<Vec<TriggerStatus>>,
    },
    /// Recharge les définitions de triggers (hot reload, implémenté STORY-073).
    Reload {
        definitions: Vec<TriggerDefinition>,
        reply: oneshot::Sender<()>,
    },
    /// Arrête l'acteur proprement.
    Shutdown,
}

// ─── Public types ─────────────────────────────────────────────────────────

/// État observé d'un trigger, retourné par [`TriggerEngineHandle::list`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerStatus {
    /// Identifiant du trigger.
    pub id: String,
    /// Nom de l'agent cible.
    pub agent: String,
    /// Type de source (`"cron"` | `"interval"` | `"file_watch"` | `"webhook"` | `"oneshot"`).
    pub source_kind: String,
    /// Indique si le trigger est actif.
    pub enabled: bool,
    /// Nombre total de fires réussis depuis le démarrage.
    pub fire_count: u64,
    /// Nombre total de skips depuis le démarrage.
    pub skip_count: u64,
    /// Horodatage du dernier fire (None si jamais déclenché).
    pub last_fired: Option<DateTime<Utc>>,
}

/// Erreurs du `TriggerEngine`.
#[derive(thiserror::Error, Debug)]
pub enum TriggerEngineError {
    /// Aucun trigger trouvé pour l'identifiant fourni.
    #[error("trigger '{id}' not found")]
    NotFound {
        /// Identifiant introuvable.
        id: String,
    },

    /// Le trigger est déjà désactivé.
    #[error("trigger '{id}' already disabled")]
    AlreadyDisabled {
        /// Identifiant du trigger.
        id: String,
    },

    /// Le trigger est déjà activé.
    #[error("trigger '{id}' already enabled")]
    AlreadyEnabled {
        /// Identifiant du trigger.
        id: String,
    },

    /// La soumission de tâche a échoué (erreur TaskRouter ou policy Drop).
    #[error("submit failed: {0}")]
    SubmitFailed(String),
}

// ─── TriggerEngine (internal actor) ───────────────────────────────────────

/// Acteur central `TriggerEngine`.
///
/// Reçoit les [`TriggerEvent`] des sources, évalue l'[`OnBusyPolicy`],
/// rend le template d'entrée et soumet les tâches au [`TaskSubmitter`].
/// Jamais exposé directement — accessible uniquement via [`TriggerEngineHandle`].
struct TriggerEngine {
    definitions: Vec<TriggerDefinition>,
    /// Canal interne sources → moteur.
    ///
    /// Conservé pour être cloné et transmis aux nouvelles sources lors du hot reload
    /// ([`TriggerCommand::Reload`]).
    event_tx: mpsc::Sender<TriggerEvent>,
    task_router: Arc<dyn TaskSubmitter>,
    event_bus: EventBusSender,
    /// JoinHandles des sources actives — abortés lors du hot reload (STORY-073).
    handles: Vec<tokio::task::JoinHandle<()>>,
    fire_counts: HashMap<String, u64>,
    skip_counts: HashMap<String, u64>,
    last_fired: HashMap<String, DateTime<Utc>>,
    /// Persistance SQLite — `None` si non configurée (ex : tests unitaires).
    persistence: Option<TriggerPersistence>,
}

impl TriggerEngine {
    /// Démarre le moteur et retourne son handle clonable.
    ///
    /// `persistence` : `None` désactive la persistance SQLite (utile pour les tests unitaires).
    /// Les sources dans `definitions` sont actuellement des stubs no-op (STORY-067/068).
    pub async fn start<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
    ) -> TriggerEngineHandle {
        let (event_tx, event_rx) = mpsc::channel::<TriggerEvent>(256);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TriggerCommand>(64);

        // Spawner les sources pour chaque définition active
        let handles: Vec<tokio::task::JoinHandle<()>> = definitions
            .iter()
            .filter(|d| d.enabled)
            .map(|d| spawn_source(d.clone(), event_tx.clone()))
            .collect();

        let engine = TriggerEngine {
            definitions,
            event_tx: event_tx.clone(),
            task_router: Arc::new(task_router),
            event_bus,
            handles,
            fire_counts: HashMap::new(),
            skip_counts: HashMap::new(),
            last_fired: HashMap::new(),
            persistence,
        };

        tokio::spawn(engine.run_loop(event_rx, cmd_rx));

        TriggerEngineHandle { tx: cmd_tx }
    }

    /// Boucle principale de l'acteur — sélectionne sur events ET commandes.
    async fn run_loop(
        mut self,
        mut event_rx: mpsc::Receiver<TriggerEvent>,
        mut cmd_rx: mpsc::Receiver<TriggerCommand>,
    ) {
        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    self.handle_event(event).await;
                }
                Some(cmd) = cmd_rx.recv() => {
                    if self.handle_command(cmd).await {
                        break;
                    }
                }
            }
        }
        // Annuler les JoinHandles des sources actives
        for handle in self.handles {
            handle.abort();
        }
        tracing::info!("TriggerEngine arrêté");
    }

    /// Traite un `TriggerEvent` — délègue à `process_event` et ignore le résultat.
    async fn handle_event(&mut self, event: TriggerEvent) {
        let _ = self.process_event(event).await;
    }

    /// Traite une commande du handle.
    ///
    /// Retourne `true` pour signaler l'arrêt de la boucle.
    async fn handle_command(&mut self, cmd: TriggerCommand) -> bool {
        match cmd {
            TriggerCommand::FindWebhook { id, reply } => {
                let def = self
                    .definitions
                    .iter()
                    .find(|d| d.id == id && matches!(d.source, TriggerSourceConfig::Webhook { .. }))
                    .cloned();
                let _ = reply.send(def);
                false
            }

            TriggerCommand::SendWebhookEvent {
                trigger_id,
                body,
                headers,
            } => {
                // Récupère le nom de l'agent avant tout emprunt mutable
                let agent = self
                    .definitions
                    .iter()
                    .find(|d| d.id == trigger_id)
                    .map(|d| d.agent.clone());

                if let Some(agent) = agent {
                    let now = Utc::now();
                    let event = TriggerEvent {
                        trigger_id,
                        agent,
                        payload: TriggerPayload::Webhook { body, headers },
                        fired_at: now,
                    };
                    self.handle_event(event).await;
                } else {
                    tracing::warn!(
                        trigger_id = %trigger_id,
                        "SendWebhookEvent reçu pour un trigger inconnu"
                    );
                }
                false
            }

            TriggerCommand::FireNow { id, reply } => {
                let result = match self.definitions.iter().find(|d| d.id == id).cloned() {
                    None => Err(TriggerEngineError::NotFound { id }),
                    Some(def) => {
                        let now = Utc::now();
                        let event = TriggerEvent {
                            trigger_id: def.id.clone(),
                            agent: def.agent.clone(),
                            payload: TriggerPayload::Timer {
                                scheduled_at: now,
                                fired_at: now,
                            },
                            fired_at: now,
                        };
                        self.process_event(event).await
                    }
                };
                let _ = reply.send(result);
                false
            }

            TriggerCommand::Enable { id, reply } => {
                // Phase 1 : mutation de la définition (emprunt mutable borné)
                let outcome = {
                    match self.definitions.iter_mut().find(|d| d.id == id) {
                        None => Err(TriggerEngineError::NotFound { id }),
                        Some(def) => {
                            if def.enabled {
                                Err(TriggerEngineError::AlreadyEnabled { id: def.id.clone() })
                            } else {
                                def.enabled = true;
                                Ok(def.id.clone())
                            }
                        }
                    }
                }; // emprunt mutable libéré ici

                // Phase 2 : émission d'événement (après libération de l'emprunt)
                if let Ok(ref trigger_id) = outcome {
                    let _ = self.event_bus.send(RuntimeEvent::TriggerEnabled {
                        trigger_id: trigger_id.clone(),
                    });
                }
                let _ = reply.send(outcome.map(|_| ()));
                false
            }

            TriggerCommand::Disable { id, reply } => {
                // Phase 1 : mutation de la définition (emprunt mutable borné)
                let outcome = {
                    match self.definitions.iter_mut().find(|d| d.id == id) {
                        None => Err(TriggerEngineError::NotFound { id }),
                        Some(def) => {
                            if !def.enabled {
                                Err(TriggerEngineError::AlreadyDisabled { id: def.id.clone() })
                            } else {
                                def.enabled = false;
                                Ok(def.id.clone())
                            }
                        }
                    }
                }; // emprunt mutable libéré ici

                // Phase 2 : émission d'événement
                if let Ok(ref trigger_id) = outcome {
                    let _ = self.event_bus.send(RuntimeEvent::TriggerDisabled {
                        trigger_id: trigger_id.clone(),
                    });
                }
                let _ = reply.send(outcome.map(|_| ()));
                false
            }

            TriggerCommand::List { reply } => {
                let statuses = self
                    .definitions
                    .iter()
                    .map(|d| TriggerStatus {
                        id: d.id.clone(),
                        agent: d.agent.clone(),
                        source_kind: source_kind_str(&d.source),
                        enabled: d.enabled,
                        fire_count: self.fire_counts.get(&d.id).copied().unwrap_or(0),
                        skip_count: self.skip_counts.get(&d.id).copied().unwrap_or(0),
                        last_fired: self.last_fired.get(&d.id).copied(),
                    })
                    .collect();
                let _ = reply.send(statuses);
                false
            }

            TriggerCommand::Reload { definitions, reply } => {
                // Annule les sources existantes
                for handle in self.handles.drain(..) {
                    handle.abort();
                }
                self.definitions = definitions;
                // Spawner les nouvelles sources pour les définitions actives
                self.handles = self
                    .definitions
                    .iter()
                    .filter(|d| d.enabled)
                    .map(|d| spawn_source(d.clone(), self.event_tx.clone()))
                    .collect();
                let _ = reply.send(());
                false
            }

            TriggerCommand::Shutdown => true,
        }
    }

    /// Traitement complet d'un événement : évaluation de la policy, soumission,
    /// émission des `RuntimeEvent` et persistance (stub).
    ///
    /// Retourne `Ok(task_id)` si une tâche a été soumise, `Err` sinon.
    async fn process_event(&mut self, event: TriggerEvent) -> Result<TaskId, TriggerEngineError> {
        // 1. Trouver la définition
        let def = match self
            .definitions
            .iter()
            .find(|d| d.id == event.trigger_id)
            .cloned()
        {
            Some(d) => d,
            None => {
                tracing::warn!(
                    trigger_id = %event.trigger_id,
                    "événement reçu pour un trigger inconnu"
                );
                return Err(TriggerEngineError::NotFound {
                    id: event.trigger_id.clone(),
                });
            }
        };

        // Ignore si désactivé
        if !def.enabled {
            let reason = "trigger disabled".to_string();
            tracing::debug!(trigger_id = %event.trigger_id, "trigger désactivé, skip");
            self.persist_skipped(&event, &reason).await;
            *self
                .skip_counts
                .entry(event.trigger_id.clone())
                .or_insert(0) += 1;
            return Err(TriggerEngineError::SubmitFailed(reason));
        }

        // 3. Vérifier OnBusyPolicy::Drop
        if def.on_busy == OnBusyPolicy::Drop {
            let pending = self.task_router.pending_count(&def.agent).await;
            if pending > 0 {
                let reason = "agent busy, on_busy=drop".to_string();
                let _ = self.event_bus.send(RuntimeEvent::TriggerSkipped {
                    trigger_id: event.trigger_id.clone(),
                    reason: reason.clone(),
                });
                self.persist_skipped(&event, &reason).await;
                *self
                    .skip_counts
                    .entry(event.trigger_id.clone())
                    .or_insert(0) += 1;
                return Err(TriggerEngineError::SubmitFailed(reason));
            }
        }

        // 2. Rendre le template d'entrée
        let text = def.input_template.render(&event.payload);
        let input = AIPInput {
            parts: vec![AIPPart::Text(TextPart { text })],
        };

        // 4. Soumettre la tâche
        match self.task_router.submit(&def.agent, input).await {
            Ok(task_id) => {
                let _ = self.event_bus.send(RuntimeEvent::TriggerFired {
                    trigger_id: event.trigger_id.clone(),
                    task_id: task_id.clone(),
                });
                self.persist_fired(&event, &task_id).await;
                *self
                    .fire_counts
                    .entry(event.trigger_id.clone())
                    .or_insert(0) += 1;
                self.last_fired.insert(event.trigger_id.clone(), Utc::now());
                Ok(task_id)
            }
            Err(e) => {
                let _ = self.event_bus.send(RuntimeEvent::TriggerError {
                    trigger_id: event.trigger_id.clone(),
                    error: e.clone(),
                });
                self.persist_error(&event, &e).await;
                tracing::error!(
                    trigger_id = %event.trigger_id,
                    error = %e,
                    "soumission de tâche échouée"
                );
                Err(TriggerEngineError::SubmitFailed(e))
            }
        }
    }

    /// Persiste un fire réussi dans `trigger_history` via [`TriggerPersistence`].
    ///
    /// Si la persistance n'est pas configurée ou échoue, un avertissement est loggué
    /// sans interrompre le traitement (fire-and-forget).
    async fn persist_fired(&mut self, event: &TriggerEvent, task_id: &TaskId) {
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) = p.record_fired(
                &event.trigger_id,
                &event.agent,
                task_id.as_ref(),
                event.fired_at,
            ) {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "failed to persist trigger fire"
                );
            }
        } else {
            tracing::debug!(trigger = %event.trigger_id, task = %task_id, "trigger fired (no persistence)");
        }
    }

    /// Persiste un skip dans `trigger_history` via [`TriggerPersistence`].
    async fn persist_skipped(&mut self, event: &TriggerEvent, reason: &str) {
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) =
                p.record_skipped(&event.trigger_id, &event.agent, reason, event.fired_at)
            {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "failed to persist trigger skip"
                );
            }
        } else {
            tracing::debug!(trigger = %event.trigger_id, %reason, "trigger skipped (no persistence)");
        }
    }

    /// Persiste une erreur de soumission dans `trigger_history` via [`TriggerPersistence`].
    async fn persist_error(&mut self, event: &TriggerEvent, error: &str) {
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) = p.record_error(&event.trigger_id, &event.agent, error, event.fired_at) {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "failed to persist trigger error"
                );
            }
        } else {
            tracing::warn!(trigger = %event.trigger_id, %error, "trigger error (no persistence)");
        }
    }
}

// ─── source_kind_str ──────────────────────────────────────────────────────

/// Retourne la chaîne représentant le type de source d'un trigger.
fn source_kind_str(source: &TriggerSourceConfig) -> String {
    match source {
        TriggerSourceConfig::Cron { .. } => "cron",
        TriggerSourceConfig::Interval { .. } => "interval",
        TriggerSourceConfig::Oneshot { .. } => "oneshot",
        TriggerSourceConfig::FileWatch { .. } => "file_watch",
        TriggerSourceConfig::Webhook { .. } => "webhook",
    }
    .to_string()
}

// ─── TriggerEngineHandle ──────────────────────────────────────────────────

/// Handle clonable du `TriggerEngine` — injectable dans `AppState<B>`.
///
/// `Clone + Send + Sync` : même pattern que `AgentRegistryHandle` et
/// `TaskRouterHandle`. Toutes les méthodes sont `async` et communiquent
/// avec l'acteur via `mpsc::Sender<TriggerCommand>`.
#[derive(Clone)]
pub struct TriggerEngineHandle {
    tx: mpsc::Sender<TriggerCommand>,
}

impl TriggerEngineHandle {
    /// Démarre un `TriggerEngine` et retourne son handle.
    ///
    /// `persistence` : `None` désactive la persistance SQLite (ex : tests, démonstrations).
    /// Équivalent à `TriggerEngine::start` — exposé ici pour une API publique cohérente.
    pub async fn spawn<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
    ) -> Self {
        TriggerEngine::start(definitions, task_router, event_bus, persistence).await
    }

    /// Trouve un trigger webhook par ID.
    ///
    /// Retourne `None` si aucun trigger webhook n'existe avec cet identifiant.
    pub async fn find_webhook(&self, id: &str) -> Option<TriggerDefinition> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(TriggerCommand::FindWebhook {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await;
        reply_rx.await.unwrap_or(None)
    }

    /// Envoie un événement webhook au moteur (fire-and-forget).
    pub async fn send_webhook_event(
        &self,
        trigger_id: String,
        body: String,
        headers: HashMap<String, String>,
    ) {
        let _ = self
            .tx
            .send(TriggerCommand::SendWebhookEvent {
                trigger_id,
                body,
                headers,
            })
            .await;
    }

    /// Force le déclenchement immédiat d'un trigger, sans attendre son schedule.
    ///
    /// Retourne `Ok(task_id)` si la tâche a été soumise avec succès,
    /// ou `Err(TriggerEngineError::NotFound)` si le trigger est inconnu.
    pub async fn fire_now(&self, id: &str) -> Result<TaskId, TriggerEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(TriggerCommand::FireNow {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?;
        reply_rx
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?
    }

    /// Active un trigger désactivé.
    ///
    /// Émet [`RuntimeEvent::TriggerEnabled`] sur l'EventBus si la transition réussit.
    pub async fn enable(&self, id: &str) -> Result<(), TriggerEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(TriggerCommand::Enable {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?;
        reply_rx
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?
    }

    /// Désactive un trigger actif.
    ///
    /// Émet [`RuntimeEvent::TriggerDisabled`] sur l'EventBus si la transition réussit.
    pub async fn disable(&self, id: &str) -> Result<(), TriggerEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(TriggerCommand::Disable {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?;
        reply_rx
            .await
            .map_err(|_| TriggerEngineError::SubmitFailed("actor dead".into()))?
    }

    /// Retourne la liste de tous les triggers avec leur statut courant.
    pub async fn list(&self) -> Vec<TriggerStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(TriggerCommand::List { reply: reply_tx }).await;
        reply_rx.await.unwrap_or_default()
    }

    /// Recharge les définitions de triggers (hot reload — implémenté STORY-073).
    pub async fn reload(&self, definitions: Vec<TriggerDefinition>) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(TriggerCommand::Reload {
                definitions,
                reply: reply_tx,
            })
            .await;
        let _ = reply_rx.await;
    }

    /// Arrête l'acteur `TriggerEngine` proprement.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(TriggerCommand::Shutdown).await;
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, TriggerSourceConfig};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::sync::broadcast;

    /// Construit un `EventBusSender` de test.
    fn make_bus() -> EventBusSender {
        broadcast::channel(64).0
    }

    /// Construit une `TriggerDefinition` minimale pour les tests.
    fn make_definition(id: &str, on_busy: OnBusyPolicy) -> TriggerDefinition {
        TriggerDefinition {
            id: id.into(),
            agent: "test-agent".into(),
            enabled: true,
            on_busy,
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("test {{scheduled_at}}".into()),
        }
    }

    // ── Mock TaskSubmitter ─────────────────────────────────────────────────

    /// Mock du `TaskSubmitter` pour les tests.
    struct MockTaskRouterHandle {
        calls: Arc<AtomicUsize>,
        should_fail: bool,
        pending: usize,
    }

    impl MockTaskRouterHandle {
        /// Crée un mock qui réussit toujours.
        fn new() -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: false,
                    pending: 0,
                },
                calls,
            )
        }

        /// Même comportement que `new()` — explicite pour les tests de comptage.
        fn new_with_tracking() -> (Self, Arc<AtomicUsize>) {
            Self::new()
        }

        /// Crée un mock qui échoue toujours à la soumission.
        fn new_always_fail() -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: true,
                    pending: 0,
                },
                calls,
            )
        }

        /// Crée un mock qui simule un agent occupé (pending_count > 0).
        fn new_with_pending(pending: usize) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: false,
                    pending,
                },
                calls,
            )
        }
    }

    impl TaskSubmitter for MockTaskRouterHandle {
        fn submit<'a>(
            &'a self,
            _agent: &'a str,
            _input: AIPInput,
        ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
            let calls = self.calls.clone();
            let should_fail = self.should_fail;
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if should_fail {
                    Err("mock failure".into())
                } else {
                    Ok(TaskId::new_v4())
                }
            })
        }

        fn pending_count<'a>(
            &'a self,
            _agent: &'a str,
        ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
            let pending = self.pending;
            Box::pin(async move { pending })
        }
    }

    // ── AC-1 ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac1_start_empty_definitions() {
        // GIVEN une liste vide de TriggerDefinition
        let (router, _) = MockTaskRouterHandle::new();
        // WHEN
        let handle = TriggerEngine::start(vec![], router, make_bus(), None).await;
        // THEN list() retourne un vec vide
        let list = handle.list().await;
        assert!(list.is_empty(), "liste attendue vide, got {:?}", list);
    }

    // ── AC-2 ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac2_handle_event_queue_submits_task() {
        // GIVEN un trigger avec OnBusyPolicy::Queue et un mock en succès
        let def = make_definition("test-trigger", OnBusyPolicy::Queue);
        let (router, calls) = MockTaskRouterHandle::new_with_tracking();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;
        // WHEN
        handle
            .fire_now("test-trigger")
            .await
            .expect("fire_now failed");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // THEN submit a été appelé exactement une fois
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // ── AC-3 (OnBusyPolicy::Drop) ──────────────────────────────────────────

    #[tokio::test]
    async fn test_ac3_drop_policy_skips_when_agent_busy() {
        // GIVEN un trigger Drop et un agent occupé (pending_count = 1)
        let def = make_definition("busy-trigger", OnBusyPolicy::Drop);
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;
        // WHEN
        let result = handle.fire_now("busy-trigger").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // THEN submit N'a PAS été appelé
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "submit ne doit pas être appelé"
        );
        // ET fire_now retourne une erreur (SubmitFailed)
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed, got {:?}",
            result
        );
    }

    // ── AC-4 ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac4_fire_now_returns_task_id() {
        // GIVEN un trigger enregistré
        let def = make_definition("rapport-hebdo", OnBusyPolicy::Queue);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;
        // WHEN
        let result = handle.fire_now("rapport-hebdo").await;
        // THEN Ok(task_id)
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[tokio::test]
    async fn test_ac4_fire_now_unknown_id_returns_error() {
        // GIVEN aucun trigger enregistré
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![], router, make_bus(), None).await;
        // WHEN
        let result = handle.fire_now("unknown-trigger").await;
        // THEN NotFound
        assert!(
            matches!(result, Err(TriggerEngineError::NotFound { .. })),
            "expected NotFound, got {:?}",
            result
        );
    }

    // ── AC-5 ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac5_enable_disable_toggle() {
        // GIVEN un trigger actif
        let def = make_definition("factures", OnBusyPolicy::Drop);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;

        // WHEN disable
        handle.disable("factures").await.expect("disable failed");
        let list = handle.list().await;
        // THEN enabled = false
        assert!(!list[0].enabled, "trigger doit être désactivé");

        // WHEN re-enable
        handle.enable("factures").await.expect("enable failed");
        let list = handle.list().await;
        // THEN enabled = true
        assert!(list[0].enabled, "trigger doit être réactivé");
    }

    // ── AC-6 ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac6_submit_error_does_not_panic() {
        // GIVEN un trigger qui échoue toujours à la soumission
        let def = make_definition("failing-trigger", OnBusyPolicy::Queue);
        let (router, _) = MockTaskRouterHandle::new_always_fail();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;

        // WHEN — ne doit pas paniquer
        let result = handle.fire_now("failing-trigger").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN l'acteur est toujours en vie
        let list = handle.list().await;
        assert_eq!(list.len(), 1, "l'acteur doit encore répondre");
        // fire_now retourne Err(SubmitFailed) car la soumission a échoué
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed, got {:?}",
            result
        );
    }

    // ── Extra ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_fire_count_increments_on_success() {
        // GIVEN un trigger
        let def = make_definition("compteur", OnBusyPolicy::Queue);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None).await;

        // WHEN fire × 2
        handle
            .fire_now("compteur")
            .await
            .expect("first fire failed");
        handle
            .fire_now("compteur")
            .await
            .expect("second fire failed");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let list = handle.list().await;
        assert_eq!(list[0].fire_count, 2, "fire_count doit être 2");
    }

    #[tokio::test]
    async fn test_handle_is_clone_send_sync() {
        // THEN TriggerEngineHandle est Clone + Send + Sync (vérifié à la compilation)
        fn assert_send_sync<T: Clone + Send + Sync>() {}
        assert_send_sync::<TriggerEngineHandle>();
    }
}
