//! The agents and workflows of the A2A card, beside the skills of agent code.
//!
//! An agent that takes a message is listed as `agent:<id>`, with the schema
//! of the message it takes; a workflow as `workflow:<package>/<name>`, with
//! the schema of the inputs it declares. `POST /api/v1/a2a/invoke` launches
//! either by that identifier and answers once it has ended. An agent runs as
//! a task of its own, as any run outside a conversation does; a workflow runs
//! through the workflow engine. The `invocation_id` a client gives is bound
//! to the task or to the run for the length of the call, so
//! `DELETE /api/v1/tasks/{invocation_id}` stops either.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use apollia_core::{AIPPart, AIPResult, DataPart};
use apollia_tools::{ResolvedAgent, WorkflowRunRow};

use super::{
    a2a_invoker_err_response, A2aAgentDto, A2aSkillDto, InvokeRequest,
    DEFAULT_DELEGATE_TIMEOUT_SECS,
};
use crate::a2a::invoker::{A2AError, A2AInvocationResult, SkillListing};
use crate::a2a::{A2AInvoker, A2aErrorResponse, AliasTarget, InvocationAliases};
use crate::agents::task::{
    resolve_runnable, resolve_without_code, AgentTaskError, AgentTaskLauncher,
};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::workflows::{WorkflowDescriptor, WorkflowError};

/// Prefix of the identifier of an agent that takes a message.
const AGENT_PREFIX: &str = "agent:";
/// Prefix of the identifier of a workflow.
const WORKFLOW_PREFIX: &str = "workflow:";
/// `kind` of an agent that takes a message.
const AGENT_KIND: &str = "agent";
/// `kind` of a workflow.
const WORKFLOW_KIND: &str = "workflow";

type RouteError = (StatusCode, Json<A2aErrorResponse>);

fn refusal(status: StatusCode, message: impl ToString, skill_id: Option<&str>) -> RouteError {
    (
        status,
        Json(A2aErrorResponse {
            error: message.to_string(),
            skill_id: skill_id.map(str::to_string),
            available_skills: None,
            conflicting_agents: None,
        }),
    )
}

/// The schema of what an agent takes: one message.
fn message_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": false
    })
}

/// The schema of a workflow's inputs; any object when it declares none.
fn inputs_schema(workflow: &WorkflowDescriptor) -> Value {
    workflow
        .input_schema
        .clone()
        .unwrap_or_else(|| json!({"type": "object"}))
}

/// What an identifier of the card launches, when it is not a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    /// An agent that takes a message, by its id.
    Agent(String),
    /// A workflow, `<package>/<name>`.
    Workflow(String),
}

impl Target {
    /// `agent:<id>` or `workflow:<package>/<name>`; `None` for a skill id.
    pub(super) fn parse(id: &str) -> Option<Self> {
        if let Some(agent) = id.strip_prefix(AGENT_PREFIX) {
            return Some(Self::Agent(agent.to_string()));
        }
        id.strip_prefix(WORKFLOW_PREFIX)
            .map(|workflow| Self::Workflow(workflow.to_string()))
    }
}

/// The agents that take a message and the workflows, as the card lists them.
pub(super) struct Catalogue {
    agents: Vec<ResolvedAgent>,
    workflows: Vec<WorkflowDescriptor>,
}

impl Catalogue {
    /// The enabled agents without code, when the runtime has an executor to
    /// run them, and the workflows the engine can launch.
    pub(super) async fn read<B: ExecutionBackend + Clone>(
        state: &AppState<B>,
    ) -> Result<Self, RouteError> {
        let agents = if state.chat_manager.is_some() {
            let data_dir = state.data_dir.clone();
            tokio::task::spawn_blocking(move || runnable_agents(&data_dir))
                .await
                .map_err(|e| e.to_string())
                .and_then(|listed| listed)
                .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, e, None))?
        } else {
            Vec::new()
        };
        let workflows = match &state.workflows {
            Some(engine) => engine
                .catalogue()
                .await
                .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, e, None))?,
            None => Vec::new(),
        };
        Ok(Self { agents, workflows })
    }

    /// One card entry per agent, then one per workflow.
    pub(super) fn agent_dtos(&self) -> Vec<A2aAgentDto> {
        let agents = self.agents.iter().map(|agent| {
            let id = format!("{AGENT_PREFIX}{}", agent.id);
            let name = display_name(agent);
            A2aAgentDto {
                agent_id: id.clone(),
                name: name.clone(),
                version: agent.version.clone(),
                state: "active".to_string(),
                skills: vec![A2aSkillDto {
                    id,
                    name,
                    description: agent.mission.clone(),
                    input_modes: vec!["text".to_string()],
                    output_modes: vec!["text".to_string()],
                    kind: AGENT_KIND.to_string(),
                    input_schema: Some(message_schema()),
                }],
                kind: AGENT_KIND.to_string(),
                input_schema: Some(message_schema()),
            }
        });
        let workflows = self.workflows.iter().map(|workflow| {
            let id = format!("{WORKFLOW_PREFIX}{}", workflow.id);
            A2aAgentDto {
                agent_id: id.clone(),
                name: workflow.id.clone(),
                version: workflow.version.clone(),
                state: "active".to_string(),
                skills: vec![A2aSkillDto {
                    id,
                    name: workflow.id.clone(),
                    description: workflow.description.clone(),
                    input_modes: vec!["data".to_string()],
                    output_modes: vec!["data".to_string()],
                    kind: WORKFLOW_KIND.to_string(),
                    input_schema: Some(inputs_schema(workflow)),
                }],
                kind: WORKFLOW_KIND.to_string(),
                input_schema: Some(inputs_schema(workflow)),
            }
        });
        agents.chain(workflows).collect()
    }

    /// One listing per agent, then one per workflow.
    pub(super) fn skill_listings(&self) -> Vec<SkillListing> {
        let agents = self.agents.iter().map(|agent| SkillListing {
            skill_id: format!("{AGENT_PREFIX}{}", agent.id),
            agent_name: agent.id.clone(),
            skill_name: display_name(agent),
            description: agent.mission.clone(),
            input_schema: Some(message_schema()),
            kind: AGENT_KIND.to_string(),
        });
        let workflows = self.workflows.iter().map(|workflow| SkillListing {
            skill_id: format!("{WORKFLOW_PREFIX}{}", workflow.id),
            agent_name: workflow.id.clone(),
            skill_name: workflow.id.clone(),
            description: workflow.description.clone(),
            input_schema: Some(inputs_schema(workflow)),
            kind: WORKFLOW_KIND.to_string(),
        });
        agents.chain(workflows).collect()
    }
}

fn display_name(agent: &ResolvedAgent) -> String {
    if agent.name.is_empty() {
        agent.id.clone()
    } else {
        agent.name.clone()
    }
}

/// The enabled stored agents that run no code, by id.
fn runnable_agents(data_dir: &std::path::Path) -> Result<Vec<ResolvedAgent>, String> {
    let path = apollia_core::paths::DataFile::Agents.path(data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let names: Vec<String> = apollia_tools::AgentRepository::open(&path)
        .map_err(|e| e.to_string())?
        .list_stored()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|stored| stored.name)
        .collect();
    let mut agents = Vec::new();
    for name in names {
        if let Some(agent) =
            resolve_without_code(data_dir, None, &name).map_err(|e| e.to_string())?
        {
            if agent.enabled {
                agents.push(agent);
            }
        }
    }
    Ok(agents)
}

/// Holds a client name bound to a task or a run for the length of one call.
struct Binding<'a> {
    aliases: &'a InvocationAliases,
    name: &'a str,
    /// `false` when another live invocation held the name: nothing to release.
    owned: bool,
}

impl<'a> Binding<'a> {
    fn hold(aliases: &'a InvocationAliases, name: &'a str, target: AliasTarget) -> Self {
        let owned = aliases.bind_target(name, target.clone());
        if !owned {
            tracing::event!(
                tracing::Level::WARN,
                invocation_id = %name,
                target = ?target,
                "a2a.invocation_id.taken"
            );
        }
        Self {
            aliases,
            name,
            owned,
        }
    }
}

impl Drop for Binding<'_> {
    fn drop(&mut self) {
        if self.owned {
            self.aliases.release(self.name);
        }
    }
}

/// Launch `target` and answer once it has ended.
pub(super) async fn invoke<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    invoker: &Arc<A2AInvoker>,
    target: Target,
    req: InvokeRequest,
) -> Result<A2AInvocationResult, RouteError> {
    let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(DEFAULT_DELEGATE_TIMEOUT_SECS));
    match target {
        Target::Agent(agent) => invoke_agent(state, invoker.aliases(), agent, req, timeout).await,
        Target::Workflow(workflow) => {
            invoke_workflow(state, invoker.aliases(), workflow, req, timeout).await
        }
    }
}

/// The message an agent is invoked with: `{"message": ...}` or a bare string.
fn message_of(input: &Value) -> Option<String> {
    match input {
        Value::String(text) => Some(text.clone()),
        Value::Object(fields) => fields
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

async fn invoke_agent<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    aliases: &InvocationAliases,
    agent: String,
    req: InvokeRequest,
    timeout: Duration,
) -> Result<A2AInvocationResult, RouteError> {
    let skill_id = req.skill_id.as_str();
    let message = message_of(&req.input).ok_or_else(|| {
        refusal(
            StatusCode::BAD_REQUEST,
            "an agent takes {\"message\": <text>}",
            Some(skill_id),
        )
    })?;
    let Some(chat) = state.chat_manager.clone() else {
        return Err(refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "no agent executor in this runtime",
            Some(skill_id),
        ));
    };
    let resolved = resolve_runnable(state.data_dir.clone(), &agent, "an A2A invocation")
        .await
        .map_err(|e| refusal(StatusCode::NOT_FOUND, e, Some(skill_id)))?;
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: state.task_repository.clone(),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
    };
    let started = Instant::now();
    let input = json!({ "message": message });
    let (task_id, outcome) = launcher
        .launch_watched(&resolved, &input, message)
        .await
        .map_err(|e| match e {
            AgentTaskError::Store(_) => refusal(StatusCode::INTERNAL_SERVER_ERROR, e, None),
            other => refusal(StatusCode::SERVICE_UNAVAILABLE, other, Some(skill_id)),
        })?;
    let task_id = task_id.to_string();
    let _binding = req
        .invocation_id
        .as_deref()
        .map(|name| Binding::hold(aliases, name, AliasTarget::Task(task_id.clone())));

    let outcome = match tokio::time::timeout(timeout, outcome).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(_)) => {
            return Err(a2a_invoker_err_response(A2AError::ExecutionFailed {
                agent_name: agent,
                message: "the run was dropped".to_string(),
            }))
        }
        Err(_) => {
            let _ = state.router_handle.cancel(&task_id).await;
            return Err(a2a_invoker_err_response(A2AError::Timeout {
                skill_id: req.skill_id.clone(),
                agent_name: agent,
                timeout_secs: timeout.as_secs(),
            }));
        }
    };
    match outcome.status {
        "completed" => {
            let mut result = AIPResult::completed(&outcome.output);
            result.task_id = task_id;
            Ok(A2AInvocationResult {
                result,
                agent_name: agent,
                skill_id: req.skill_id.clone(),
                duration_ms: started.elapsed().as_millis() as u64,
                run_id: None,
            })
        }
        "canceled" => Err(a2a_invoker_err_response(A2AError::Canceled { task_id })),
        _ => Err(a2a_invoker_err_response(A2AError::ExecutionFailed {
            agent_name: agent,
            message: outcome.output,
        })),
    }
}

fn workflow_refusal(e: WorkflowError, skill_id: &str) -> RouteError {
    let status = match &e {
        WorkflowError::InvalidInput(_) | WorkflowError::TooDeep(_) => StatusCode::BAD_REQUEST,
        WorkflowError::NotFound(_) | WorkflowError::BadIdentifier(_) => StatusCode::NOT_FOUND,
        WorkflowError::NoHost | WorkflowError::Stopped => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refusal(status, e, Some(skill_id))
}

async fn invoke_workflow<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    aliases: &InvocationAliases,
    workflow: String,
    req: InvokeRequest,
    timeout: Duration,
) -> Result<A2AInvocationResult, RouteError> {
    let skill_id = req.skill_id.as_str();
    let Some(engine) = state.workflows.clone() else {
        return Err(refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "the workflow engine is not running in this runtime",
            Some(skill_id),
        ));
    };
    let started = Instant::now();
    let launched = engine
        .start(&workflow, req.input.clone(), None)
        .await
        .map_err(|e| workflow_refusal(e, skill_id))?;
    let run_id = launched.row.run_id.clone();
    let _binding = req
        .invocation_id
        .as_deref()
        .map(|name| Binding::hold(aliases, name, AliasTarget::WorkflowRun(run_id.clone())));

    let row = match tokio::time::timeout(timeout, launched.end).await {
        Ok(Ok(end)) => end.row,
        Ok(Err(_)) => {
            return Err(a2a_invoker_err_response(A2AError::RegistryError(
                "the workflow engine stopped before the run ended".to_string(),
            )))
        }
        Err(_) => {
            let _ = engine.cancel(&run_id).await;
            return Err(a2a_invoker_err_response(A2AError::Timeout {
                skill_id: req.skill_id.clone(),
                agent_name: workflow,
                timeout_secs: timeout.as_secs(),
            }));
        }
    };
    match row.status.as_str() {
        "completed" => Ok(A2AInvocationResult {
            result: workflow_result(&row),
            agent_name: workflow,
            skill_id: req.skill_id.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            run_id: None,
        }),
        "canceled" => Err(a2a_invoker_err_response(A2AError::Canceled {
            task_id: row.run_id,
        })),
        _ => Err(a2a_invoker_err_response(A2AError::ExecutionFailed {
            agent_name: workflow,
            message: row.error.unwrap_or_else(|| row.status.clone()),
        })),
    }
}

/// What a completed run returned, as an AIP result keyed by its run id: a
/// text part for a string, a data part for any other value.
fn workflow_result(row: &WorkflowRunRow) -> AIPResult {
    let mut result = match &row.output {
        Some(Value::String(text)) => AIPResult::completed(text),
        Some(value) => {
            let mut result = AIPResult::completed("");
            result.output = vec![AIPPart::Data(DataPart {
                data: value.clone(),
            })];
            result
        }
        None => AIPResult::completed(""),
    };
    result.task_id = row.run_id.clone();
    result
}
