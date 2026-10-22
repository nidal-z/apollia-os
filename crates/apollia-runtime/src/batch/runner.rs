//! The items of a batch run as agent runs outside any conversation.
//!
//! Each item runs unattended: the accesses of the agent that are not
//! sensitive, and the sensitive ones granted to it (for this batch or for
//! good), run without asking; the first other access the agent reaches for
//! stops the item, which then waits for a decision. An item the user allowed
//! to execute code once runs again with one call of that tool let through.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::paths::DataFile;
use apollia_permissions::{ConsentFamily, ConsentScope, FamilyConsentStore};
use apollia_tools::{ActionFamily, ResolvedAgent};

use super::access::{classify, known_tools, sensitive, BatchAccess, KnownTool};
use super::write_back::SOURCE_WRITE_ACCESS;
use super::{item_message_for, ItemEnd, ItemRunner, ItemStatus, StartedItem};
use crate::agents::task::{resolve_runnable, AgentTaskLauncher};
use crate::coordinator::ExecutionBackend;
use crate::governance::{
    AccessCall, AccessChange, AccessDecision, DecisionReason, GovernanceContract, GovernanceEvent,
};

/// Who records a consent given for a batch.
const GRANTED_BY: &str = "batch";

/// Who grants writing into the original file: the user, at launch.
const WRITE_GRANTED_BY: &str = "user";

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

    async fn allow_original_write(&self, agent: &str, location: &str) -> Result<(), String> {
        decide_original_write(self.governance.as_deref(), agent, location)
    }

    async fn record_original_write(
        &self,
        batch_id: &str,
        agent: &str,
        location: &str,
    ) -> Result<(), String> {
        journal_original_write(self.governance.as_deref(), batch_id, agent, location)
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
        item: super::ItemStart<'_>,
    ) -> Result<StartedItem, String> {
        let super::ItemStart { input, once, task } = item;
        let resolved = self.resolve(agent).await?;
        let mut authorized = self.authorized(&resolved, batch_id).await?;
        if let Some(access) = once {
            authorized.push(crate::governance::once_grant(access));
        }
        let template = apollia_oria::plan_template::PlanTemplate {
            key: format!("{batch_id}:{agent}"),
            fields: item_fields(input),
            outputs: task
                .map(|t| {
                    t.outputs
                        .iter()
                        .map(|o| (o.name.trim().to_string(), o.description.trim().to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let (task_id, outcome) = self
            .launcher
            .launch_batch_item(
                &resolved,
                input,
                item_message_for(task, input),
                crate::agents::task::BatchItemRun {
                    authorized,
                    template,
                },
            )
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

    async fn synthesize(&self, agent: &str, message: String) -> Result<String, String> {
        let resolved = self.resolve(agent).await?;
        let (_, outcome) = self
            .launcher
            .launch_unattended(&resolved, &serde_json::json!({}), message, Vec::new())
            .await
            .map_err(|e| e.to_string())?;
        match outcome.await {
            Ok(outcome) if outcome.status == "completed" => Ok(outcome.output),
            Ok(outcome) => Err(outcome.output),
            Err(_) => Err("the run of the synthesis was dropped".into()),
        }
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

/// Why writing into the original file cannot be decided without governance.
const NO_GOVERNANCE: &str =
    "writing into the original file needs the governance journal, which is not open";

/// Asks `governance` whether `agent` may write the results of a batch into
/// the original file at `location`. The grant at launch is the user's answer:
/// a decision that would ask is answered, a refusal of the policy stands.
pub(crate) fn decide_original_write(
    governance: Option<&dyn GovernanceContract>,
    agent: &str,
    location: &str,
) -> Result<(), String> {
    let governance = governance.ok_or_else(|| NO_GOVERNANCE.to_string())?;
    let decision = governance.decide(&AccessCall {
        agent_id: agent,
        access_id: SOURCE_WRITE_ACCESS,
        argument: Some(location),
        family: ActionFamily::WriteOrDelete,
        unattended: false,
        granted_for_run: true,
    });
    match decision {
        AccessDecision::Allowed { .. }
        | AccessDecision::AskUser { .. }
        | AccessDecision::Pending { .. } => Ok(()),
        AccessDecision::Denied { reason } => Err(match reason {
            DecisionReason::Policy { detail } => {
                format!("writing into the original file is refused by the policy: {detail}")
            }
            DecisionReason::Rule { rule_id } => {
                format!("writing into the original file is refused by rule {rule_id}")
            }
            DecisionReason::Paused { connection_id } => {
                format!("writing into the original file is refused: {connection_id} is paused")
            }
            DecisionReason::FailClosed
            | DecisionReason::Consent
            | DecisionReason::NoConsentNeeded => {
                "writing into the original file is refused: governance cannot be read".to_string()
            }
        }),
    }
}

/// Records in the journal of `governance` that the batch `batch_id` of
/// `agent` writes its results into the original file at `location`, for its
/// duration.
pub(crate) fn journal_original_write(
    governance: Option<&dyn GovernanceContract>,
    batch_id: &str,
    agent: &str,
    location: &str,
) -> Result<(), String> {
    let governance = governance.ok_or_else(|| NO_GOVERNANCE.to_string())?;
    governance
        .record(&GovernanceEvent::AccessChanged {
            agent_id: agent.to_string(),
            access_id: SOURCE_WRITE_ACCESS.to_string(),
            change: AccessChange::Granted,
            actor: WRITE_GRANTED_BY.to_string(),
            reason: Some(format!(
                "write_or_delete for lot:{batch_id}: the results are written into {location}, \
                 after a dated backup copy"
            )),
        })
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        batch_id,
        "batch.original_write.granted"
    );
    Ok(())
}

/// The fields of an item with their values as text, in the order of the
/// source: what fills the plan template of its batch.
fn item_fields(input: &serde_json::Value) -> Vec<(String, String)> {
    match input.as_object() {
        Some(fields) => fields
            .iter()
            .map(|(name, value)| {
                let text = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned);
                (name.clone(), text)
            })
            .collect(),
        None => Vec::new(),
    }
}
