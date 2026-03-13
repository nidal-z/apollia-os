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
use apollia_pipelines::engine::PipelineEngineHandle;

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
    /// Retourne la définition complète d'un trigger par ID (STORY-074).
    GetDefinition {
        id: String,
        reply: oneshot::Sender<Option<TriggerDefinition>>,
    },
    /// Retourne l'historique SQLite d'un trigger (STORY-074).
    QueryHistory {
        trigger_id: String,
        limit: usize,
        reply: oneshot::Sender<Vec<crate::persistence::TriggerHistoryEntry>>,
    },
    /// Recharge les définitions de triggers (hot reload, implémenté STORY-073).
    Reload {
        definitions: Vec<TriggerDefinition>,
        reply: oneshot::Sender<()>,
    },
    /// Injecte (ou retire) le `PipelineEngine` après le démarrage — résout la
    /// dépendance circulaire TriggerEngine ↔ PipelineEngine (STORY-119).
    SetPipelineEngine {
        handle: Option<PipelineEngineHandle>,
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
    /// Handle optionnel vers le `PipelineEngine` — `None` si Sprint 12 non déployé.
    ///
    /// Injecté au démarrage. Si absent et qu'un trigger définit `pipeline`, un
    /// `tracing::warn!` est émis et `TriggerSkipped` est envoyé sur l'EventBus
    /// (AC-3 STORY-117). Jamais de panic.
    pipeline_engine: Option<PipelineEngineHandle>,
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
    /// `pipeline_engine` : `None` désactive le dispatch vers les pipelines — un trigger
    /// avec `pipeline` émettra `TriggerSkipped` au lieu de paniquer (AC-3 STORY-117).
    /// Les sources dans `definitions` sont actuellement des stubs no-op (STORY-067/068).
    pub async fn start<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
        pipeline_engine: Option<PipelineEngineHandle>,
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
            pipeline_engine,
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

            TriggerCommand::GetDefinition { id, reply } => {
                let def = self.definitions.iter().find(|d| d.id == id).cloned();
                let _ = reply.send(def);
                false
            }

            TriggerCommand::QueryHistory {
                trigger_id,
                limit,
                reply,
            } => {
                let entries = match self.persistence.as_ref() {
                    Some(p) => p.query_history(&trigger_id, limit).unwrap_or_default(),
                    None => vec![],
                };
                let _ = reply.send(entries);
                false
            }

            TriggerCommand::Reload { definitions, reply } => {
                self.do_reload(definitions).await;
                let _ = reply.send(());
                false
            }

            TriggerCommand::SetPipelineEngine { handle } => {
                self.pipeline_engine = handle;
                tracing::info!(
                    has_pipeline_engine = self.pipeline_engine.is_some(),
                    "TriggerEngine: pipeline_engine mis à jour"
                );
                false
            }

            TriggerCommand::Shutdown => true,
        }
    }

    /// Traitement complet d'un événement : évaluation de la policy, soumission,
    /// émission des `RuntimeEvent` et persistance (stub).
    ///
    /// Retourne `Ok(task_id)` si une tâche a été soumise, `Err` sinon.
    /// Pour les triggers `pipeline`, le `task_id` retourné est le `RunId` converti.
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

        // ── Dispatch pipeline (STORY-117) ─────────────────────────────────────
        // Si `pipeline` est défini, dispatche vers `PipelineEngine` au lieu du `TaskRouter`.
        // L'exclusivité `agent XOR pipeline` est validée dans STORY-118.
        if let Some(ref pipeline_id) = def.pipeline.clone() {
            return self.dispatch_to_pipeline(&event, pipeline_id, &def).await;
        }

        // ── Dispatch agent (chemin existant) ──────────────────────────────────

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
                    agent: def.agent.clone(),
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

    /// Dispatche un événement vers le `PipelineEngine` (STORY-117).
    ///
    /// - Si `pipeline_engine` est `Some`, appelle `run_pipeline()` et retourne le
    ///   `RunId` converti en `TaskId`.
    /// - Si `pipeline_engine` est `None`, émet `TriggerSkipped` avec
    ///   `reason = "pipeline_engine_unavailable"` et retourne `Err(SubmitFailed)`.
    ///   Aucune panic dans les deux cas (AC-3).
    async fn dispatch_to_pipeline(
        &mut self,
        event: &TriggerEvent,
        pipeline_id: &str,
        def: &TriggerDefinition,
    ) -> Result<TaskId, TriggerEngineError> {
        match &self.pipeline_engine {
            Some(pe) => {
                // Rendre le payload du trigger depuis le template
                let trigger_payload = def.input_template.render(&event.payload);
                let pe = pe.clone();
                match pe
                    .run_pipeline(
                        pipeline_id,
                        Some(event.trigger_id.clone()),
                        Some(trigger_payload),
                    )
                    .await
                {
                    Ok(run_id) => {
                        tracing::info!(
                            trigger_id = %event.trigger_id,
                            pipeline_id = %pipeline_id,
                            run_id = %run_id,
                            "trigger dispatched to pipeline"
                        );
                        // Convertir RunId → TaskId pour homogénéité du type de retour
                        let task_id = TaskId::from(run_id.0.as_str());
                        let _ = self.event_bus.send(RuntimeEvent::TriggerFired {
                            trigger_id: event.trigger_id.clone(),
                            agent: format!("pipeline:{pipeline_id}"),
                            task_id: task_id.clone(),
                        });
                        *self
                            .fire_counts
                            .entry(event.trigger_id.clone())
                            .or_insert(0) += 1;
                        self.last_fired.insert(event.trigger_id.clone(), Utc::now());
                        Ok(task_id)
                    }
                    Err(e) => {
                        let reason = format!("pipeline_start_failed: {e}");
                        tracing::warn!(
                            trigger_id = %event.trigger_id,
                            pipeline_id = %pipeline_id,
                            error = %e,
                            "failed to start pipeline"
                        );
                        let _ = self.event_bus.send(RuntimeEvent::TriggerSkipped {
                            trigger_id: event.trigger_id.clone(),
                            reason: reason.clone(),
                        });
                        *self
                            .skip_counts
                            .entry(event.trigger_id.clone())
                            .or_insert(0) += 1;
                        Err(TriggerEngineError::SubmitFailed(reason))
                    }
                }
            }
            None => {
                tracing::warn!(
                    trigger_id = %event.trigger_id,
                    pipeline_id = %pipeline_id,
                    "pipeline engine not available — trigger skipped"
                );
                let _ = self.event_bus.send(RuntimeEvent::TriggerSkipped {
                    trigger_id: event.trigger_id.clone(),
                    reason: "pipeline_engine_unavailable".into(),
                });
                *self
                    .skip_counts
                    .entry(event.trigger_id.clone())
                    .or_insert(0) += 1;
                Err(TriggerEngineError::SubmitFailed(
                    "pipeline engine not available".into(),
                ))
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

    /// Recharge les définitions de triggers (hot reload — STORY-073).
    ///
    /// Donne à chaque source active 2 secondes pour se terminer proprement avant
    /// d'utiliser [`tokio::task::AbortHandle`] pour forcer l'arrêt. Cette fenêtre
    /// permet à `notify::Watcher` de se drop correctement (ADR-021).
    ///
    /// Les compteurs en mémoire (`fire_counts`, `skip_counts`, `last_fired`)
    /// et les données SQLite sont **préservés** — seules les définitions et les
    /// JoinHandles sont remplacés (AC-2).
    async fn do_reload(&mut self, new_definitions: Vec<TriggerDefinition>) {
        // 1. Arrêter toutes les sources actives avec timeout 2s.
        let handles = std::mem::take(&mut self.handles);
        for handle in handles {
            // Sauvegarder l'AbortHandle avant que timeout consomme le JoinHandle.
            let abort_handle = handle.abort_handle();
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                Ok(_) => {}
                Err(_) => {
                    // Timeout dépassé — forcer l'abort via l'AbortHandle.
                    abort_handle.abort();
                }
            }
        }

        // 2. Remplacer les définitions (les compteurs en mémoire sont préservés).
        self.definitions = new_definitions;

        // 3. Respawn les sources activées.
        self.handles = self
            .definitions
            .iter()
            .filter(|d| d.enabled)
            .map(|d| spawn_source(d.clone(), self.event_tx.clone()))
            .collect();

        // 4. Émettre l'événement TriggersReloaded.
        let count = self.definitions.iter().filter(|d| d.enabled).count();
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::TriggersReloaded { count });
        tracing::info!(count, "triggers reloaded");
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
    /// `pipeline_engine` : `None` désactive le dispatch pipeline — triggers avec `pipeline`
    /// émettront `TriggerSkipped` au lieu de paniquer (AC-3 STORY-117).
    /// Équivalent à `TriggerEngine::start` — exposé ici pour une API publique cohérente.
    pub async fn spawn<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
        pipeline_engine: Option<PipelineEngineHandle>,
    ) -> Self {
        TriggerEngine::start(
            definitions,
            task_router,
            event_bus,
            persistence,
            pipeline_engine,
        )
        .await
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

    /// Retourne la définition complète d'un trigger par ID (STORY-074).
    ///
    /// Retourne `None` si aucun trigger ne correspond à `id`.
    pub async fn get_definition(&self, id: &str) -> Option<TriggerDefinition> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(TriggerCommand::GetDefinition {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await;
        reply_rx.await.unwrap_or(None)
    }

    /// Retourne les `limit` dernières entrées d'historique pour un trigger (STORY-074).
    ///
    /// Retourne un vec vide si la persistance n'est pas configurée ou si le trigger
    /// n'a pas encore été déclenché.
    pub async fn query_history(
        &self,
        trigger_id: &str,
        limit: usize,
    ) -> Vec<crate::persistence::TriggerHistoryEntry> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(TriggerCommand::QueryHistory {
                trigger_id: trigger_id.to_string(),
                limit,
                reply: reply_tx,
            })
            .await;
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

    /// Injecte (ou retire) le `PipelineEngine` après le démarrage du moteur.
    ///
    /// Résout la dépendance circulaire TriggerEngine ↔ PipelineEngine (STORY-119) :
    /// le TriggerEngine démarre sans PipelineEngine, puis reçoit le handle dès que
    /// le PipelineEngine est prêt. Fire-and-forget — pas de réponse attendue.
    pub async fn set_pipeline_engine(&self, handle: Option<PipelineEngineHandle>) {
        let _ = self
            .tx
            .send(TriggerCommand::SetPipelineEngine { handle })
            .await;
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

    /// Construit une `TriggerDefinition` minimale pour les tests (trigger agent).
    fn make_definition(id: &str, on_busy: OnBusyPolicy) -> TriggerDefinition {
        TriggerDefinition {
            id: id.into(),
            agent: "test-agent".into(),
            pipeline: None,
            enabled: true,
            on_busy,
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("test {{scheduled_at}}".into()),
        }
    }

    /// Construit une `TriggerDefinition` pour un trigger pipeline (STORY-117).
    fn make_pipeline_definition(id: &str, pipeline_id: &str) -> TriggerDefinition {
        TriggerDefinition {
            id: id.into(),
            agent: String::new(),
            pipeline: Some(pipeline_id.into()),
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("{{scheduled_at}}".into()),
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
        let handle = TriggerEngine::start(vec![], router, make_bus(), None, None).await;
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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;
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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;
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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;
        // WHEN
        let result = handle.fire_now("rapport-hebdo").await;
        // THEN Ok(task_id)
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[tokio::test]
    async fn test_ac4_fire_now_unknown_id_returns_error() {
        // GIVEN aucun trigger enregistré
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![], router, make_bus(), None, None).await;
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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;

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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;

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
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;

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

    // ── STORY-073 — Hot reload ─────────────────────────────────────────────

    /// AC-1 : reload() remplace toutes les définitions existantes.
    #[tokio::test]
    async fn test_ac1_reload_replaces_all_triggers() {
        // GIVEN un moteur avec 1 trigger
        let def1 = make_definition("trigger-1", OnBusyPolicy::Queue);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![def1], router, make_bus(), None, None).await;
        assert_eq!(handle.list().await.len(), 1);

        // WHEN reload avec 2 nouveaux triggers
        let def2 = make_definition("trigger-2", OnBusyPolicy::Drop);
        let def3 = make_definition("trigger-3", OnBusyPolicy::Queue);
        handle.reload(vec![def2, def3]).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // THEN 2 triggers actifs, trigger-1 absent
        let list = handle.list().await;
        assert_eq!(list.len(), 2, "list() doit retourner 2 triggers");
        assert!(
            !list.iter().any(|t| t.id == "trigger-1"),
            "trigger-1 ne doit plus être présent"
        );
        assert!(list.iter().any(|t| t.id == "trigger-2"));
        assert!(list.iter().any(|t| t.id == "trigger-3"));
    }

    /// AC-1 : reload() émet RuntimeEvent::TriggersReloaded { count }.
    #[tokio::test]
    async fn test_ac1_triggers_reloaded_event_emitted() {
        // GIVEN un bus avec subscriber actif
        let (bus_tx, mut bus_rx) = broadcast::channel::<apollia_core::RuntimeEvent>(64);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![], router, bus_tx, None, None).await;

        // WHEN reload avec 1 trigger activé
        let def = make_definition("new-trigger", OnBusyPolicy::Queue);
        handle.reload(vec![def]).await;

        // THEN TriggersReloaded { count: 1 } reçu dans les 500ms
        let received = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                match bus_rx.recv().await {
                    Ok(apollia_core::RuntimeEvent::TriggersReloaded { count: 1 }) => {
                        return true;
                    }
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        })
        .await;
        assert_eq!(
            received,
            Ok(true),
            "TriggersReloaded {{ count: 1 }} doit être émis"
        );
    }

    // ── STORY-117 — Triggers → Pipelines ──────────────────────────────────────

    /// AC-3 : trigger avec `pipeline` sans PipelineEngineHandle → TriggerSkipped, pas de panic.
    #[tokio::test]
    async fn test_ac3_trigger_pipeline_no_engine_warning_no_panic() {
        // GIVEN TriggerEngine sans pipeline_engine (None)
        //   ET un trigger avec pipeline = "traitement-facture"
        let def = make_pipeline_definition("factures-auto", "traitement-facture");
        let (bus_tx, mut bus_rx) = broadcast::channel::<apollia_core::RuntimeEvent>(64);
        let (router, task_calls) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(vec![def], router, bus_tx, None, None).await;

        // WHEN le trigger fire
        let result = handle.fire_now("factures-auto").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN Err(SubmitFailed) — aucune panic
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed, got {result:?}"
        );

        // ET TaskRouter n'est pas appelé
        assert_eq!(
            task_calls.load(Ordering::SeqCst),
            0,
            "TaskRouter ne doit pas être appelé pour un trigger pipeline"
        );

        // ET TriggerSkipped est émis sur EventBus avec reason pipeline_engine_unavailable
        let skipped = tokio::time::timeout(std::time::Duration::from_millis(200), async {
            loop {
                match bus_rx.recv().await {
                    Ok(apollia_core::RuntimeEvent::TriggerSkipped { reason, .. }) => {
                        return reason;
                    }
                    Ok(_) => {}
                    Err(_) => return String::new(),
                }
            }
        })
        .await
        .unwrap_or_default();

        assert!(
            skipped.contains("pipeline_engine_unavailable"),
            "reason doit contenir 'pipeline_engine_unavailable', got: '{skipped}'"
        );

        // ET l'acteur est toujours vivant (pas de panic)
        let list = handle.list().await;
        assert_eq!(list.len(), 1, "l'acteur doit encore répondre après le skip");
    }

    /// AC-5 : trigger avec `agent` non affecté par l'ajout de la fonctionnalité pipeline.
    #[tokio::test]
    async fn test_ac5_agent_trigger_unaffected() {
        // GIVEN trigger existant avec agent="hello-agent" (pipeline = None)
        let def = make_definition("rapport-hebdo", OnBusyPolicy::Queue);
        let (router, calls) = MockTaskRouterHandle::new_with_tracking();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;

        // WHEN fire
        let result = handle.fire_now("rapport-hebdo").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN TaskRouter.submit() appelé exactement une fois — comportement inchangé
        assert!(result.is_ok(), "fire_now doit réussir, got {result:?}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "TaskRouter doit être appelé exactement une fois"
        );
    }

    /// AC-1 : le champ `pipeline` de TriggerDefinition est backward-compatible.
    ///
    /// Un moteur créé avec des définitions sans `pipeline` fonctionne exactement
    /// comme avant STORY-117 — aucune régression.
    #[tokio::test]
    async fn test_ac1_pipeline_none_no_regression() {
        // GIVEN trigger sans pipeline (None)
        let def = make_definition("ancien-trigger", OnBusyPolicy::Queue);
        assert!(def.pipeline.is_none());

        let (router, calls) = MockTaskRouterHandle::new_with_tracking();
        let handle = TriggerEngine::start(vec![def], router, make_bus(), None, None).await;

        // WHEN fire
        handle
            .fire_now("ancien-trigger")
            .await
            .expect("fire_now must succeed");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN submit appelé — même comportement qu'avant
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
