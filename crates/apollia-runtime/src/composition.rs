//! Composition: an agent calls another agent, or a workflow, it was granted.
//!
//! Two access families make such a call possible, `agent:<id>` and
//! `workflow:<package>/<workflow>`. Granted to an agent, each becomes a tool
//! of its runs, described by the mission of the agent or the description of
//! the workflow; calling it runs the agent executor or the workflow engine.
//! Nothing is offered that the access list does not name, and an agent is
//! never offered itself.
//!
//! A call is nested in the run that makes it: it stops when that run is
//! stopped, it spends from the same budget, and the nesting is bounded at
//! [`MAX_DEPTH`] levels, the first run counting as one. An agent calling
//! itself is refused.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_llm::types::ToolSpec;
use apollia_llm::ToolInvoker;
use apollia_oria::budget::StepBudget;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::agents::task::{resolve_runnable, Nesting};
use crate::workflows::{
    AgentRunner, BoxFuture, ParentRun, WorkflowDescriptor, WorkflowHandle, MAX_DEPTH,
};

/// Prefix of the access to an agent: `agent:<id>`.
pub const AGENT_ACCESS_PREFIX: &str = "agent:";

/// Prefix of the access to a workflow: `workflow:<package>/<workflow>`.
pub const WORKFLOW_ACCESS_PREFIX: &str = "workflow:";

/// The run a call is made from.
#[derive(Clone)]
pub struct CallerRun {
    /// The agent that calls.
    pub agent_id: String,
    /// Its level in the nesting, 1 for a run started on its own.
    pub depth: u32,
    /// Its cancellation token, which the call inherits.
    pub cancel: CancellationToken,
    /// Its budget, which the call spends from.
    pub budget: Arc<StepBudget>,
}

/// What the agents and the workflows an agent calls are read and run through.
pub trait Composer: Send + Sync {
    /// The mission of `agent` when it can be called; `None` for an unknown,
    /// disabled or code-running agent.
    fn agent_mission(&self, agent: String) -> BoxFuture<'_, Option<String>>;

    /// The workflow `workflow`, `<package>/<workflow>`, when a package
    /// declares it.
    fn workflow(&self, workflow: String) -> BoxFuture<'_, Option<WorkflowDescriptor>>;

    /// Run `agent` on `message`, nested in `caller`, and return its answer.
    fn run_agent(
        &self,
        agent: String,
        message: String,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<String, String>>;

    /// Run `workflow` on `inputs`, nested in `caller`, and return its output.
    fn run_workflow(
        &self,
        workflow: String,
        inputs: Value,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<Value, String>>;
}

/// The composer of a running runtime: an agent runs through the agent
/// executor, as a task of its own; a workflow through the workflow engine.
pub struct RuntimeComposer {
    agents: Arc<dyn AgentRunner>,
    workflows: WorkflowHandle,
    data_dir: PathBuf,
}

impl RuntimeComposer {
    /// A composer running agents through `agents`, workflows through
    /// `workflows`, and reading agents from the stores under `data_dir`.
    pub fn new(agents: Arc<dyn AgentRunner>, workflows: WorkflowHandle, data_dir: PathBuf) -> Self {
        Self {
            agents,
            workflows,
            data_dir,
        }
    }
}

impl Composer for RuntimeComposer {
    fn agent_mission(&self, agent: String) -> BoxFuture<'_, Option<String>> {
        Box::pin(async move {
            resolve_runnable(self.data_dir.clone(), &agent, "an agent call")
                .await
                .ok()
                .map(|resolved| resolved.mission)
        })
    }

    fn workflow(&self, workflow: String) -> BoxFuture<'_, Option<WorkflowDescriptor>> {
        Box::pin(async move { self.workflows.describe(&workflow).await })
    }

    fn run_agent(
        &self,
        agent: String,
        message: String,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<String, String>> {
        let nesting = Nesting {
            parent: caller.cancel,
            budget: caller.budget,
            depth: caller.depth + 1,
        };
        self.agents.run(agent, message, nesting)
    }

    fn run_workflow(
        &self,
        workflow: String,
        inputs: Value,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<Value, String>> {
        let parent = ParentRun {
            run_id: None,
            workflow_id: None,
            depth: caller.depth,
            cancel: caller.cancel,
            budget: caller.budget,
        };
        Box::pin(async move { self.workflows.run_nested(&workflow, inputs, parent).await })
    }
}

/// Whether `access` is an access to an agent or to a workflow.
pub fn is_call_access(access: &str) -> bool {
    access.starts_with(AGENT_ACCESS_PREFIX) || access.starts_with(WORKFLOW_ACCESS_PREFIX)
}

/// The tools of the agents and workflows `granted` names, for the agent
/// `caller`: one per agent other than `caller` that can be called, described
/// by its mission, and one per declared workflow, described by its
/// description and taking its inputs.
pub async fn tool_specs(
    composer: &dyn Composer,
    granted: &[String],
    caller: &str,
) -> Vec<ToolSpec> {
    let mut specs = Vec::new();
    for access in granted {
        if let Some(agent) = access.strip_prefix(AGENT_ACCESS_PREFIX) {
            if agent == caller {
                continue;
            }
            match composer.agent_mission(agent.to_string()).await {
                Some(mission) => specs.push(agent_spec(access, agent, mission)),
                None => tracing::event!(
                    tracing::Level::INFO,
                    access = %access,
                    "composition.agent.unavailable"
                ),
            }
        } else if let Some(workflow) = access.strip_prefix(WORKFLOW_ACCESS_PREFIX) {
            match composer.workflow(workflow.to_string()).await {
                Some(descriptor) => specs.push(ToolSpec {
                    name: access.clone(),
                    description: descriptor.description,
                    parameters: descriptor
                        .input_schema
                        .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                }),
                None => tracing::event!(
                    tracing::Level::INFO,
                    access = %access,
                    "composition.workflow.unavailable"
                ),
            }
        }
    }
    specs
}

fn agent_spec(access: &str, agent: &str, mission: String) -> ToolSpec {
    let description = if mission.trim().is_empty() {
        format!("Hand a request to the agent {agent} and get its answer.")
    } else {
        mission
    };
    ToolSpec {
        name: access.to_string(),
        description,
        parameters: json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The request handed to the agent."
                }
            },
            "required": ["message"]
        }),
    }
}

/// A tool invoker that runs the agent and workflow calls of `caller` and
/// hands every other call to `inner`.
pub struct ComposingInvoker {
    inner: Arc<dyn ToolInvoker>,
    composer: Arc<dyn Composer>,
    caller: CallerRun,
}

impl ComposingInvoker {
    /// Calls of `caller` run through `composer`; the rest through `inner`.
    pub fn new(
        inner: Arc<dyn ToolInvoker>,
        composer: Arc<dyn Composer>,
        caller: CallerRun,
    ) -> Self {
        Self {
            inner,
            composer,
            caller,
        }
    }

    fn nested_depth(&self) -> Result<u32, String> {
        let depth = self.caller.depth + 1;
        if depth > MAX_DEPTH {
            return Err(format!(
                "a run may nest {MAX_DEPTH} levels deep, this call would be level {depth}"
            ));
        }
        Ok(depth)
    }

    async fn call_agent(&self, agent: &str, arguments: &Value) -> Result<String, String> {
        if agent == self.caller.agent_id {
            return Err(format!("agent '{agent}' cannot call itself"));
        }
        let depth = self.nested_depth()?;
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("the call to agent '{agent}' needs a `message` string"))?;
        tracing::event!(
            tracing::Level::INFO,
            caller = %self.caller.agent_id,
            agent = %agent,
            depth,
            "composition.agent.called"
        );
        self.composer
            .run_agent(agent.to_string(), message.to_string(), self.caller.clone())
            .await
    }

    async fn call_workflow(&self, workflow: &str, arguments: &Value) -> Result<String, String> {
        let depth = self.nested_depth()?;
        tracing::event!(
            tracing::Level::INFO,
            caller = %self.caller.agent_id,
            workflow = %workflow,
            depth,
            "composition.workflow.called"
        );
        let output = self
            .composer
            .run_workflow(workflow.to_string(), arguments.clone(), self.caller.clone())
            .await?;
        Ok(match output {
            Value::String(text) => text,
            other => other.to_string(),
        })
    }
}

// REASON: `ToolInvoker` is declared with `#[async_trait]` in `apollia-llm`;
// an implementation has to follow its signature.
#[async_trait::async_trait]
impl ToolInvoker for ComposingInvoker {
    async fn invoke(&self, tool_name: &str, arguments: &Value) -> Result<String, String> {
        if let Some(agent) = tool_name.strip_prefix(AGENT_ACCESS_PREFIX) {
            return self.call_agent(agent, arguments).await;
        }
        if let Some(workflow) = tool_name.strip_prefix(WORKFLOW_ACCESS_PREFIX) {
            return self.call_workflow(workflow, arguments).await;
        }
        self.inner.invoke(tool_name, arguments).await
    }
}

#[cfg(test)]
mod tests;
