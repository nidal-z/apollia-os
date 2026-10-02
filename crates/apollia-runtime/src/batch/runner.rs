//! The items of a batch run as agent runs outside any conversation.

use std::path::PathBuf;

use apollia_core::paths::DataFile;
use apollia_core::LlmBackendRepository;
use apollia_tools::ResolvedAgent;

use super::{item_message, ItemEnd, ItemRunner, ItemStatus, StartedItem};
use crate::agents::task::{resolve_without_code, AgentTaskLauncher};
use crate::coordinator::ExecutionBackend;

/// Runs each item through the agent executor, as a task of its own.
pub struct AgentItemRunner<B: ExecutionBackend> {
    launcher: AgentTaskLauncher<B>,
    data_dir: PathBuf,
}

impl<B: ExecutionBackend> AgentItemRunner<B> {
    /// A runner over `launcher`, resolving agents and their model from the
    /// stores under `data_dir`.
    pub fn new(launcher: AgentTaskLauncher<B>, data_dir: PathBuf) -> Self {
        Self { launcher, data_dir }
    }

    /// The agent as resolved now, so that each item reads the overrides and
    /// the model in force when it starts. The stores are opened for the
    /// occasion, as the CLI opens them: the batch engine shares no lock.
    async fn resolve(&self, agent: &str) -> Result<ResolvedAgent, String> {
        let data_dir = self.data_dir.clone();
        let name = agent.to_string();
        let resolved = tokio::task::spawn_blocking(move || {
            let system = DataFile::System.path(&data_dir);
            let models = if system.exists() {
                Some(LlmBackendRepository::open(&system).map_err(|e| e.to_string())?)
            } else {
                None
            };
            resolve_without_code(&data_dir, models.as_ref(), &name).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        match resolved {
            Some(agent) if agent.enabled => Ok(agent),
            Some(agent) => Err(format!("agent '{}' is disabled", agent.id)),
            None => Err(format!(
                "agent '{agent}' is not a stored agent without code, which is what a batch runs"
            )),
        }
    }
}

impl<B: ExecutionBackend> ItemRunner for AgentItemRunner<B> {
    async fn check(&self, agent: &str) -> Result<(), String> {
        self.resolve(agent).await.map(|_| ())
    }

    async fn start(&self, agent: &str, input: &serde_json::Value) -> Result<StartedItem, String> {
        let resolved = self.resolve(agent).await?;
        let (task_id, outcome) = self
            .launcher
            .launch_watched(&resolved, input, item_message(input))
            .await
            .map_err(|e| e.to_string())?;
        let (tx, end) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let end = match outcome.await {
                Ok(outcome) => ItemEnd {
                    status: match outcome.status {
                        "completed" => ItemStatus::Completed,
                        "canceled" => ItemStatus::Canceled,
                        _ => ItemStatus::Failed,
                    },
                    output: outcome.output,
                },
                Err(_) => ItemEnd {
                    status: ItemStatus::Failed,
                    output: "the run was dropped".into(),
                },
            };
            let _ = tx.send(end);
        });
        Ok(StartedItem {
            task_id: task_id.to_string(),
            end,
        })
    }
}
