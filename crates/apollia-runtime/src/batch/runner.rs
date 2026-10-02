//! The items of a batch run as agent runs outside any conversation.

use std::path::PathBuf;

use apollia_tools::ResolvedAgent;

use super::{item_message, ItemEnd, ItemRunner, ItemStatus, StartedItem};
use crate::agents::task::{resolve_runnable, AgentTaskLauncher};
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
    /// the model in force when it starts.
    async fn resolve(&self, agent: &str) -> Result<ResolvedAgent, String> {
        resolve_runnable(self.data_dir.clone(), agent, "a batch").await
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
