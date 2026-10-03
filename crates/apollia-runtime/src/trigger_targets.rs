//! What a trigger starts, as the runtime starts it.
//!
//! A trigger names an agent or a workflow. An agent stored without code runs
//! on the agent executor, outside any conversation, as a task the router
//! follows; an agent that runs code keeps the coordinator path. A workflow is
//! launched by the workflow engine on its rendered inputs, which the engine
//! checks against the schema the workflow declares.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use apollia_core::{AIPInput, TaskId};
use apollia_triggers::TaskSubmitter;

use crate::agents::task::{message_of, resolve_stored, AgentTaskLauncher};
use crate::coordinator::ExecutionBackend;
use crate::router::TaskRouterHandle;
use crate::workflows::WorkflowHandle;

type Submitted<'a> = Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>>;

/// The targets a trigger may start: the task router, the agent executor and
/// the workflow engine.
pub struct TriggerTargets<B: ExecutionBackend> {
    /// The coordinator path, for an agent that runs code.
    pub router: TaskRouterHandle<B>,
    /// The agent executor; `None` sends every agent to the router.
    pub agents: Option<AgentTaskLauncher<B>>,
    /// The workflow engine; `None` refuses every workflow.
    pub workflows: Option<WorkflowHandle>,
    /// Where the agents are stored.
    pub data_dir: PathBuf,
}

impl<B> TriggerTargets<B>
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    /// Runs `agent` on `input`: on the executor when it is stored without
    /// code, through the router otherwise.
    async fn start_agent(&self, agent: &str, input: AIPInput) -> Result<TaskId, String> {
        if let Some(launcher) = &self.agents {
            if let Some(resolved) = resolve_stored(self.data_dir.clone(), agent).await? {
                if !resolved.enabled {
                    return Err(format!("agent '{agent}' is disabled"));
                }
                let given = serde_json::to_value(&input).map_err(|e| e.to_string())?;
                return launcher
                    .launch(&resolved, &given, message_of(&input))
                    .await
                    .map_err(|e| e.to_string());
            }
        }
        self.router
            .submit(agent, input)
            .await
            .map_err(|e| e.to_string())
    }
}

impl<B> TaskSubmitter for TriggerTargets<B>
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    fn submit<'a>(&'a self, agent: &'a str, input: AIPInput) -> Submitted<'a> {
        Box::pin(self.start_agent(agent, input))
    }

    fn pending_count<'a>(
        &'a self,
        agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        apollia_triggers::TaskSubmitter::pending_count(&self.router, agent)
    }

    fn submit_workflow<'a>(
        &'a self,
        workflow: &'a str,
        inputs: serde_json::Value,
    ) -> Submitted<'a> {
        Box::pin(async move {
            let Some(engine) = &self.workflows else {
                return Err(format!(
                    "workflow '{workflow}' cannot start: this runtime runs no workflow"
                ));
            };
            engine
                .launch(workflow, inputs)
                .await
                .map(|run| TaskId::from(run.run_id))
                .map_err(|e| e.to_string())
        })
    }
}

#[cfg(test)]
mod tests;
