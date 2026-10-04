//! The items of a batch run as agent runs outside any conversation.
//!
//! Each item runs unattended: the accesses of the agent that are not
//! sensitive, and the sensitive ones granted to it (for this batch or for
//! good), run without asking; the first other access the agent reaches for
//! stops the item, which then waits for a decision.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::paths::DataFile;
use apollia_permissions::{ConsentFamily, ConsentScope, FamilyConsentStore};
use apollia_tools::ResolvedAgent;

use super::access::{classify, known_tools, sensitive, BatchAccess, KnownTool};
use super::{item_message, ItemEnd, ItemRunner, ItemStatus, StartedItem};
use crate::agents::task::{resolve_runnable, AgentTaskLauncher};
use crate::coordinator::ExecutionBackend;
use crate::governance::GovernanceContract;

/// Who records a consent given for a batch.
const GRANTED_BY: &str = "batch";

/// Runs each item through the agent executor, as a task of its own.
pub struct AgentItemRunner<B: ExecutionBackend> {
    launcher: AgentTaskLauncher<B>,
    data_dir: PathBuf,
    governance: Option<Arc<dyn GovernanceContract>>,
    tools: Arc<HashMap<String, KnownTool>>,
}

impl<B: ExecutionBackend> AgentItemRunner<B> {
    /// A runner over `launcher`, resolving agents and their model from the
    /// stores under `data_dir`.
    pub fn new(launcher: AgentTaskLauncher<B>, data_dir: PathBuf) -> Self {
        Self {
            launcher,
            data_dir,
            governance: None,
            tools: Arc::new(known_tools()),
        }
    }

    /// The same runner, asking `governance` which accesses are sensitive.
    #[must_use]
    pub fn with_governance(mut self, governance: Arc<dyn GovernanceContract>) -> Self {
        self.governance = Some(governance);
        self
    }

    /// Runs `f` on the consent store of `governance.db`, off the runtime.
    async fn consents<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut FamilyConsentStore) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let path = DataFile::Governance.path(&self.data_dir);
        tokio::task::spawn_blocking(move || {
            let mut store = FamilyConsentStore::new(&path).map_err(|e| e.to_string())?;
            f(&mut store)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// The accesses of `agent` that run without asking in the batch
    /// `batch_id`: those not sensitive, and the sensitive ones whose family is
    /// granted, for this batch or for good.
    async fn authorized(
        &self,
        agent: &ResolvedAgent,
        batch_id: &str,
    ) -> Result<Vec<String>, String> {
        let classified = classify(agent, &self.tools, self.governance.as_deref());
        let mut authorized = Vec::new();
        let mut to_check = Vec::new();
        for (access, is_sensitive) in classified {
            if !is_sensitive {
                authorized.push(access.access_id);
            } else if let Ok(family) = ConsentFamily::parse(&access.family) {
                to_check.push((access.access_id, family));
            }
        }
        let agent_id = agent.id.clone();
        let batch_id = batch_id.to_string();
        let granted = self
            .consents(move |store| {
                let mut granted = Vec::new();
                for (access_id, family) in to_check {
                    if store
                        .is_granted(&agent_id, family, Some(&batch_id))
                        .map_err(|e| e.to_string())?
                    {
                        granted.push(access_id);
                    }
                }
                Ok(granted)
            })
            .await?;
        authorized.extend(granted);
        Ok(authorized)
    }

    /// The agent as resolved now, so that each item reads the overrides and
    /// the model in force when it starts.
    async fn resolve(&self, agent: &str) -> Result<ResolvedAgent, String> {
        resolve_runnable(self.data_dir.clone(), agent, "a batch").await
    }
}

impl<B: ExecutionBackend> ItemRunner for AgentItemRunner<B> {
    async fn check(&self, agent: &str) -> Result<Vec<BatchAccess>, String> {
        let resolved = self.resolve(agent).await?;
        Ok(sensitive(&classify(
            &resolved,
            &self.tools,
            self.governance.as_deref(),
        )))
    }

    async fn grant(
        &self,
        batch_id: &str,
        agent: &str,
        grants: &[BatchAccess],
    ) -> Result<(), String> {
        if grants.is_empty() {
            return Ok(());
        }
        let families = grants
            .iter()
            .map(|g| ConsentFamily::parse(&g.family).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let scope = ConsentScope::Lot(batch_id.to_string());
        let agent = agent.to_string();
        self.consents(move |store| {
            for family in families {
                store
                    .grant(&agent, family, &scope, GRANTED_BY)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
        .await?;
        tracing::event!(
            tracing::Level::INFO,
            batch_id,
            granted = grants.len(),
            "batch.accesses.granted"
        );
        Ok(())
    }

    async fn release(&self, batch_id: &str) -> Result<(), String> {
        let lot = batch_id.to_string();
        let withdrawn = self
            .consents(move |store| store.revoke_lot(&lot).map_err(|e| e.to_string()))
            .await?;
        tracing::event!(
            tracing::Level::INFO,
            batch_id,
            withdrawn,
            "batch.accesses.withdrawn"
        );
        Ok(())
    }

    async fn start(
        &self,
        batch_id: &str,
        agent: &str,
        input: &serde_json::Value,
    ) -> Result<StartedItem, String> {
        let resolved = self.resolve(agent).await?;
        let authorized = self.authorized(&resolved, batch_id).await?;
        let (task_id, outcome) = self
            .launcher
            .launch_unattended(&resolved, input, item_message(input), authorized)
            .await
            .map_err(|e| e.to_string())?;
        let (tx, end) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let end = match outcome.await {
                Ok(outcome) => ItemEnd {
                    status: match outcome.status {
                        "completed" => ItemStatus::Completed,
                        "canceled" => ItemStatus::Canceled,
                        "awaiting" => ItemStatus::Awaiting,
                        _ => ItemStatus::Failed,
                    },
                    output: outcome.output,
                    awaiting: outcome.awaiting,
                },
                Err(_) => ItemEnd {
                    status: ItemStatus::Failed,
                    output: "the run was dropped".into(),
                    awaiting: None,
                },
            };
            let _ = tx.send(end);
        });
        Ok(StartedItem {
            task_id: task_id.to_string(),
            end,
        })
    }

    async fn cancel(&self, task_id: &str) {
        if let Err(e) = self.launcher.router.cancel(task_id).await {
            tracing::event!(
                tracing::Level::WARN,
                task_id,
                error = %e,
                "batch.item.cancel_failed"
            );
        }
    }
}
