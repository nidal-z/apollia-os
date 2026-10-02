//! The agent of a step runs through the agent executor, as a task of its own.

use std::path::PathBuf;

use super::{AgentRunner, BoxFuture};
use crate::agents::task::{resolve_runnable, AgentTaskLauncher, Nesting};
use crate::coordinator::ExecutionBackend;

/// Runs the agent of a step outside any conversation, recorded in `tasks`
/// like any other agent run, stopped with the step and spending from the
/// workflow's budget.
pub struct LaunchedAgentRunner<B: ExecutionBackend> {
    launcher: AgentTaskLauncher<B>,
    data_dir: PathBuf,
}

impl<B: ExecutionBackend> LaunchedAgentRunner<B> {
    /// A runner over `launcher`, resolving agents and their model from the
    /// stores under `data_dir`.
    pub fn new(launcher: AgentTaskLauncher<B>, data_dir: PathBuf) -> Self {
        Self { launcher, data_dir }
    }

    async fn run_to_end(
        &self,
        agent: String,
        message: String,
        nesting: Nesting,
    ) -> Result<String, String> {
        let resolved = resolve_runnable(self.data_dir.clone(), &agent, "a workflow step").await?;
        let input = serde_json::json!({ "message": message });
        let (_task, outcome) = self
            .launcher
            .launch_nested(&resolved, &input, message, nesting)
            .await
            .map_err(|e| e.to_string())?;
        let outcome = outcome
            .await
            .map_err(|_| format!("the run of agent '{agent}' was dropped"))?;
        match outcome.status {
            "completed" => Ok(outcome.output),
            status => Err(format!("agent '{agent}' {status}: {}", outcome.output)),
        }
    }
}

impl<B: ExecutionBackend> AgentRunner for LaunchedAgentRunner<B> {
    fn run(
        &self,
        agent: String,
        message: String,
        nesting: Nesting,
    ) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(self.run_to_end(agent, message, nesting))
    }
}

#[cfg(test)]
mod tests;
