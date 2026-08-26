//! The engine's handle, one method per command.
//!
//! Split out of `engine.rs`: the actor stays in the parent, the cloneable
//! sender callers hold lives here.

use std::collections::HashMap;

use tokio::sync::oneshot;

use apollia_core::{EventBusSender, ObservabilityConfig, TaskId};

use crate::engine::{
    TaskSubmitter, TriggerCommand, TriggerEngine, TriggerEngineError, TriggerEngineHandle,
    TriggerStatus,
};
use crate::persistence::TriggerPersistence;
use crate::types::TriggerDefinition;

impl TriggerEngineHandle {
    /// Starts a `TriggerEngine` and returns its handle.
    ///
    /// `persistence`: `None` disables SQLite persistence (e.g. tests, demos).
    /// `obs_config`: observability configuration for payload truncation.
    /// Equivalent to `TriggerEngine::start`, exposed here for a consistent public API.
    pub async fn spawn<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
        obs_config: ObservabilityConfig,
    ) -> Self {
        TriggerEngine::start(definitions, task_router, event_bus, persistence, obs_config).await
    }
    /// Finds a webhook trigger by ID.
    ///
    /// Returns `None` if no webhook trigger exists with this identifier.
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
    /// Sends a webhook event to the engine (fire-and-forget).
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
    /// Forces a trigger to fire immediately, without waiting for its schedule.
    ///
    /// Returns `Ok(task_id)` if the task was submitted successfully, or
    /// `Err(TriggerEngineError::NotFound)` if the trigger is unknown.
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
    /// Enables a disabled trigger.
    ///
    /// Emits [`RuntimeEvent::TriggerEnabled`] on the EventBus if the transition succeeds.
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
    /// Disables an active trigger.
    ///
    /// Emits [`RuntimeEvent::TriggerDisabled`] on the EventBus if the transition succeeds.
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
    /// Returns the list of all triggers with their current status.
    pub async fn list(&self) -> Vec<TriggerStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(TriggerCommand::List { reply: reply_tx }).await;
        reply_rx.await.unwrap_or_default()
    }
    /// Returns the full definition of a trigger by ID.
    ///
    /// Returns `None` if no trigger matches `id`.
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
    /// Returns the last `limit` history entries for a trigger.
    ///
    /// Returns an empty vec if persistence is not configured or the trigger has
    /// not fired yet.
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
    /// Reloads the trigger definitions (hot reload).
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
    /// Notifies the engine that an agent has become idle.
    ///
    /// Triggers the FIFO drain of that agent's queue. To be called from the
    /// Supervisor on receipt of [`RuntimeEvent::TaskCompleted`] for the relevant
    /// agent. Fire-and-forget: no response expected.
    pub async fn notify_agent_free(&self, agent_id: String) {
        let _ = self
            .tx
            .send(TriggerCommand::NotifyAgentFree { agent_id })
            .await;
    }
    /// Stops the `TriggerEngine` actor cleanly.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(TriggerCommand::Shutdown).await;
    }
}
