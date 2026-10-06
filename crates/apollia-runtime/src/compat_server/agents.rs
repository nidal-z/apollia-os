//! The agents and workflows the compatible server serves as models.
//!
//! An agent or a workflow is served when the user marked it exposed, the one
//! flag the MCP bridge reads too (`exposed_items` in `agents.db`), and when it
//! still exists: a stored agent without code, enabled, or a workflow an
//! installed package declares. A client names it `apollia/<id>`: an agent by
//! its identifier, a workflow by `<package>/<workflow>`. An agent identifier
//! never carries `/`, so the two never answer to the same name.
//!
//! A call runs the agent as a batch of one item, the path every run nobody
//! watches takes, and a workflow through the workflow engine; nothing here
//! runs an agent any other way.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_tools::{ExposedItem, ExposedItemRepository, ExposedKind, TaskRepository};

use super::request_mcp_run::RequestMcp;
use crate::agents::task::resolve_stored;
use crate::batch::BatchHandle;
use crate::eventbus::EventBusSender;
use crate::workflows::{WorkflowDescriptor, WorkflowHandle};

/// The prefix of every model that is an agent or a workflow.
pub(crate) const MODEL_PREFIX: &str = "apollia/";

/// What the compatible server reaches to serve agents and workflows, and to
/// run the tool loop of a request that brings an MCP server. Without it, the
/// server lists none and answers a call to one `run_unavailable`.
#[derive(Clone, Default)]
pub struct ExposedAgents {
    data_dir: Option<PathBuf>,
    bus: Option<EventBusSender>,
    batches: Option<BatchHandle>,
    tasks: Option<Arc<TaskRepository>>,
    workflows: Option<WorkflowHandle>,
    request_mcp: Option<RequestMcp>,
}

impl std::fmt::Debug for ExposedAgents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExposedAgents")
            .field("data_dir", &self.data_dir)
            .field("batches", &self.batches.is_some())
            .field("workflows", &self.workflows.is_some())
            .field("request_mcp", &self.request_mcp.is_some())
            .finish()
    }
}

/// An agent or a workflow, as `GET /v1/models` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExposedModel {
    /// `apollia/<id>`.
    pub(crate) id: String,
    pub(crate) kind: ExposedKind,
}

/// What a call to an `apollia/...` model runs.
#[derive(Debug, Clone)]
pub(crate) enum ExposedTarget {
    /// The stored agent of this identifier.
    Agent(String),
    /// The workflow, as its package declares it.
    Workflow(WorkflowDescriptor),
}

/// What runs a call: the batch engine for an agent, the workflow engine for a
/// workflow, and the bus their progress is read from.
pub(crate) struct Engines<'a> {
    pub(crate) bus: &'a EventBusSender,
    pub(crate) batches: Option<&'a BatchHandle>,
    pub(crate) tasks: Option<&'a Arc<TaskRepository>>,
    pub(crate) workflows: Option<&'a WorkflowHandle>,
}

impl ExposedAgents {
    /// The agents stored under `data_dir`; their progress is read on `bus`.
    #[must_use]
    pub fn new(data_dir: PathBuf, bus: EventBusSender) -> Self {
        Self {
            data_dir: Some(data_dir),
            bus: Some(bus),
            ..Self::default()
        }
    }

    /// The same, running an agent as a batch of one item through `batches`,
    /// whose tasks `tasks` records.
    #[must_use]
    pub fn with_batches(
        mut self,
        batches: Option<BatchHandle>,
        tasks: Option<Arc<TaskRepository>>,
    ) -> Self {
        self.batches = batches;
        self.tasks = tasks;
        self
    }

    /// The same, running a workflow through `workflows`.
    #[must_use]
    pub fn with_workflows(mut self, workflows: Option<WorkflowHandle>) -> Self {
        self.workflows = workflows;
        self
    }

    /// The same, running the tool loop of a request that brings an MCP server
    /// through `request_mcp`.
    #[must_use]
    pub fn with_request_mcp(mut self, request_mcp: Option<RequestMcp>) -> Self {
        self.request_mcp = request_mcp;
        self
    }

    /// What runs a request that brings an MCP server, when this runtime can.
    pub(crate) fn request_mcp(&self) -> Option<&RequestMcp> {
        self.request_mcp.as_ref()
    }

    /// What runs a call, `None` when this runtime has no bus to follow one.
    pub(crate) fn engines(&self) -> Option<Engines<'_>> {
        Some(Engines {
            bus: self.bus.as_ref()?,
            batches: self.batches.as_ref(),
            tasks: self.tasks.as_ref(),
            workflows: self.workflows.as_ref(),
        })
    }

    /// The items marked exposed, none when `agents.db` does not exist yet.
    async fn marked(&self) -> Vec<ExposedItem> {
        let Some(data_dir) = &self.data_dir else {
            return Vec::new();
        };
        let path = apollia_core::paths::DataFile::Agents.path(data_dir);
        if !path.exists() {
            return Vec::new();
        }
        let read =
            tokio::task::spawn_blocking(move || ExposedItemRepository::open(&path)?.list()).await;
        match read {
            Ok(Ok(items)) => items,
            Ok(Err(error)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "compat_server.agents.unreadable"
                );
                Vec::new()
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "compat_server.agents.join_failed"
                );
                Vec::new()
            }
        }
    }

    /// What `item` runs, when it still exists and can run.
    async fn target(&self, item: ExposedItem) -> Option<ExposedTarget> {
        match item.kind {
            ExposedKind::Agent => {
                let data_dir = self.data_dir.clone()?;
                match resolve_stored(data_dir, &item.item_id).await {
                    Ok(Some(agent)) if agent.enabled => Some(ExposedTarget::Agent(agent.id)),
                    Ok(_) => None,
                    Err(error) => {
                        tracing::event!(
                            tracing::Level::WARN,
                            agent_id = %item.item_id,
                            error = %error,
                            "compat_server.agents.unresolved"
                        );
                        None
                    }
                }
            }
            ExposedKind::Workflow => self
                .workflows
                .as_ref()?
                .describe(&item.item_id)
                .await
                .map(ExposedTarget::Workflow),
        }
    }

    /// Every exposed agent and workflow that exists now, agents first.
    pub(crate) async fn listed(&self) -> Vec<ExposedModel> {
        let mut listed = Vec::new();
        for item in self.marked().await {
            let kind = item.kind;
            let id = format!("{MODEL_PREFIX}{}", item.item_id);
            if self.target(item).await.is_some() {
                listed.push(ExposedModel { id, kind });
            }
        }
        listed
    }

    /// What the model `model`, `apollia/<id>`, runs: `None` when no exposed
    /// agent or workflow that exists carries that identifier.
    pub(crate) async fn find(&self, model: &str) -> Option<ExposedTarget> {
        let id = model.strip_prefix(MODEL_PREFIX)?;
        let item = self
            .marked()
            .await
            .into_iter()
            .find(|item| item.item_id == id)?;
        self.target(item).await
    }
}
