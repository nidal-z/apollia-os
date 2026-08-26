//! The engine's command handlers.
//!
//! Split out of `engine.rs`: the actor loop stays in the parent, the arm of
//! each `TriggerCommand` (webhook, fire-now, enable, disable, list, reload)
//! lives here.

use std::collections::HashMap;

use chrono::Utc;

use apollia_core::{RuntimeEvent, TaskId};

use crate::engine::{
    source_config_str, source_kind_str, TriggerCommand, TriggerEngine, TriggerEngineError,
    TriggerStatus,
};
use crate::sources::spawn_source;
use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

impl TriggerEngine {
    /// Handles a command from the handle.
    ///
    /// Returns `true` to signal that the loop should stop.
    pub(super) async fn handle_command(&mut self, cmd: TriggerCommand) -> bool {
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
                self.cmd_send_webhook(trigger_id, body, headers).await;
                false
            }

            TriggerCommand::FireNow { id, reply } => {
                let result = self.cmd_fire_now(id).await;
                let _ = reply.send(result);
                false
            }

            TriggerCommand::Enable { id, reply } => {
                let outcome = self.cmd_enable(id);
                let _ = reply.send(outcome.map(|_| ()));
                false
            }

            TriggerCommand::Disable { id, reply } => {
                let outcome = self.cmd_disable(id);
                let _ = reply.send(outcome.map(|_| ()));
                false
            }

            TriggerCommand::List { reply } => {
                let _ = reply.send(self.build_statuses());
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

            TriggerCommand::NotifyAgentFree { agent_id } => {
                self.drain_agent_queue(&agent_id).await;
                false
            }

            TriggerCommand::Shutdown => true,
        }
    }
    /// Handles a [`TriggerCommand::SendWebhookEvent`]: resolves the agent then
    /// delegates to [`Self::handle_event`].
    async fn cmd_send_webhook(
        &mut self,
        trigger_id: String,
        body: String,
        headers: HashMap<String, String>,
    ) {
        // Read the agent name before any mutable borrow.
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
                "trigger.webhook.event.unknown"
            );
        }
    }
    /// Handles a [`TriggerCommand::FireNow`]: builds a Timer payload and submits
    /// the event.
    async fn cmd_fire_now(&mut self, id: String) -> Result<TaskId, TriggerEngineError> {
        match self.definitions.iter().find(|d| d.id == id).cloned() {
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
        }
    }
    /// Enables a trigger and emits [`RuntimeEvent::TriggerEnabled`] when applicable.
    ///
    /// Returns the identifier of the enabled trigger on success.
    fn cmd_enable(&mut self, id: String) -> Result<String, TriggerEngineError> {
        // Phase 1: mutate the definition (scoped mutable borrow).
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
        }; // mutable borrow released here

        // Phase 2: emit the event (after releasing the borrow).
        if let Ok(ref trigger_id) = outcome {
            let _ = self.event_bus.send(RuntimeEvent::TriggerEnabled {
                trigger_id: trigger_id.clone(),
            });
        }
        outcome
    }
    /// Disables a trigger and emits [`RuntimeEvent::TriggerDisabled`] when applicable.
    ///
    /// Returns the identifier of the disabled trigger on success.
    fn cmd_disable(&mut self, id: String) -> Result<String, TriggerEngineError> {
        // Phase 1: mutate the definition (scoped mutable borrow).
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
        }; // mutable borrow released here

        // Phase 2: emit the event.
        if let Ok(ref trigger_id) = outcome {
            let _ = self.event_bus.send(RuntimeEvent::TriggerDisabled {
                trigger_id: trigger_id.clone(),
            });
        }
        outcome
    }
    /// Builds the list of current [`TriggerStatus`] for [`TriggerCommand::List`].
    fn build_statuses(&self) -> Vec<TriggerStatus> {
        self.definitions
            .iter()
            .map(|d| TriggerStatus {
                id: d.id.clone(),
                agent: d.agent.clone(),
                source_kind: source_kind_str(&d.source),
                source_config: source_config_str(&d.source),
                enabled: d.enabled,
                fire_count: self.fire_counts.get(&d.id).copied().unwrap_or(0),
                skip_count: self.skip_counts.get(&d.id).copied().unwrap_or(0),
                last_fired: self.last_fired.get(&d.id).copied(),
            })
            .collect()
    }
    /// Reloads the trigger definitions (hot reload).
    ///
    /// Gives each active source 2 seconds to terminate cleanly before using
    /// [`tokio::task::AbortHandle`] to force the stop. This window lets
    /// `notify::Watcher` drop correctly.
    ///
    /// The in-memory counters (`fire_counts`, `skip_counts`, `last_fired`) and
    /// the SQLite data are preserved; only the definitions and JoinHandles are
    /// replaced.
    async fn do_reload(&mut self, new_definitions: Vec<TriggerDefinition>) {
        // 1. Stop all active sources with a 2s timeout.
        let handles = std::mem::take(&mut self.handles);
        for handle in handles {
            // Save the AbortHandle before timeout consumes the JoinHandle.
            let abort_handle = handle.abort_handle();
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                Ok(_) => {}
                Err(_) => {
                    // Timeout exceeded: force the abort via the AbortHandle.
                    abort_handle.abort();
                }
            }
        }

        // 2. Replace the definitions (in-memory counters are preserved).
        //    The queues are cleared, so queued triggers are lost.
        self.definitions = new_definitions;
        self.agent_queues.clear();

        // 3. Respawn the enabled sources.
        self.handles = self
            .definitions
            .iter()
            .filter(|d| d.enabled)
            .map(|d| spawn_source(d.clone(), self.event_tx.clone()))
            .collect();

        // 4. Emit the TriggersReloaded event.
        let count = self.definitions.iter().filter(|d| d.enabled).count();
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::TriggersReloaded { count });
        tracing::info!(count, "trigger.definitions.reloaded");
    }
}
