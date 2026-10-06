//! A request that brings an MCP server: the runtime runs the tool loop.
//!
//! Such a request is never relayed to the engine of the model it names. The
//! runtime connects to each server over HTTP, hands its tools to the agent
//! executor under an agent that exists for this call only, on the model the
//! request names, and answers in the format of the endpoint. The connection
//! lasts as long as the call: it is removed when the run ends, and when the
//! caller goes away before the end.
//!
//! What bounds it:
//!
//! - `[server] allow_request_mcp`, off by default, read at every request;
//! - a model only: an agent or a workflow works with its own connections;
//! - the organisation policy admits each server as it admits a connection
//!   someone adds, so `[connections] remote = "deny"` refuses a server that
//!   is not on this machine;
//! - the address is this machine or a public host, never the private network,
//!   and every redirect must be public ([`apollia_core::net`]);
//! - the tools are unclassified whatever the server declares, and nobody
//!   watches the run: the first call governance would ask about ends the call
//!   on `approval_pending`. Nothing is left to decide later, since the
//!   connection is gone with the call; a fine rule set in developer mode is
//!   what lets a tool through.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_llm::LlmRouter;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::McpSessionError;
use apollia_mcp::transport::StreamableHttpTransport;
use apollia_tools::{AgentOrigin, PlanningMode, ResolvedAgent};
use axum::response::Response;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::agent_call::{Caller, ExposedCall};
use super::agent_reply::{answer, AnswerHeader};
use super::agent_reply_messages::MessagesAnswer;
use super::agent_reply_responses::ResponsesAnswer;
use super::agent_run::{agent_step, Progress, RunOutcome, StartedRun, StopOnDrop, Usage};
use super::agent_turn::read_turn;
use super::agents::{ExposedAgents, MODEL_PREFIX};
use super::auth::unauthorized_with;
use super::errors::{error_response, ErrorKind};
use super::journal::LaunchedRun;
use super::models::{HeldModel, LocalModels, Unheld};
use super::origin::{admit_agent_call, CallOrigin, AGENT_TOKEN_REQUIRED};
use super::relay::Endpoint;
use super::request_mcp::{read_servers, RequestedServer};
use super::tokens::ServerTokensHandle;
use crate::agents::task::{AgentRunOutcome, AgentTaskLauncher};
use crate::coordinator::ExecutionBackend;
use crate::governance::{AdmissionDecision, AdmissionRequest, ConnectionGovernance};
use crate::llama_server::{ModelKind, ModelLease};
use crate::workflows::BoxFuture;

/// The agent a request that brings an MCP server runs under. It is stored
/// nowhere: it exists for the length of one call, and a rule names it to let
/// one of its tools through.
pub const REQUEST_AGENT_ID: &str = "request-mcp";

/// Steps waiting to be read by the caller.
const PROGRESS_CAPACITY: usize = 64;

/// How long the server has to answer the handshake.
const INIT_TIMEOUT_SECS: u64 = 15;

/// How long one call of a tool may take.
const CALL_TIMEOUT_SECS: u64 = 60;

/// How long the connection waits for the TCP handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a run whose caller went away has to stop before its connections
/// are removed under it.
const STOP_GRACE: Duration = Duration::from_secs(30);

/// What turns the setting on, said in every refusal while it is off.
const TURNED_OFF: &str = "MCP servers brought by a request are turned off. Turn on \
                          `[server] allow_request_mcp` in Apollia to accept them.";

/// A run of the agent executor started for a request.
pub struct LaunchedRequest {
    /// The task of the run.
    pub task_id: String,
    /// The run the model turns report their tokens under, when recorded.
    pub run_id: Option<String>,
    /// Reports the end of the run.
    pub end: oneshot::Receiver<AgentRunOutcome>,
}

/// Runs the agent executor for a request, nobody watching, and stops a run.
pub trait RequestAgentRunner: Send + Sync {
    /// Run `agent` on `message`: no tool is authorized in advance, and the
    /// first one that would ask stops the run.
    fn launch(
        &self,
        agent: ResolvedAgent,
        message: String,
    ) -> BoxFuture<'_, Result<LaunchedRequest, String>>;

    /// Stop the run of `task_id` at its next checkpoint.
    fn cancel(&self, task_id: String) -> BoxFuture<'_, ()>;
}

/// [`RequestAgentRunner`] over the launcher every run outside a conversation
/// goes through.
pub struct LaunchedRequestRunner<B: ExecutionBackend> {
    launcher: AgentTaskLauncher<B>,
}

impl<B: ExecutionBackend> LaunchedRequestRunner<B> {
    /// A runner over `launcher`.
    pub fn new(launcher: AgentTaskLauncher<B>) -> Self {
        Self { launcher }
    }
}

impl<B: ExecutionBackend> RequestAgentRunner for LaunchedRequestRunner<B> {
    fn launch(
        &self,
        agent: ResolvedAgent,
        message: String,
    ) -> BoxFuture<'_, Result<LaunchedRequest, String>> {
        Box::pin(async move {
            let input = json!({ "request": message });
            let (task_id, end) = self
                .launcher
                .launch_unattended(&agent, &input, message, Vec::new())
                .await
                .map_err(|e| e.to_string())?;
            let task_id = task_id.to_string();
            let run_id = match &self.launcher.repository {
                Some(tasks) => tasks.get_run_id(&task_id).await.ok().flatten(),
                None => None,
            };
            Ok(LaunchedRequest {
                task_id,
                run_id,
                end,
            })
        })
    }

    fn cancel(&self, task_id: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if let Err(error) = self.launcher.router.cancel(&task_id).await {
                tracing::event!(
                    tracing::Level::WARN,
                    task_id = %task_id,
                    error = %error,
                    "compat_server.request_mcp.cancel_failed"
                );
            }
        })
    }
}

/// What the compatible server reaches to run a request that brings an MCP
/// server. Without it, such a request answers `run_unavailable`.
#[derive(Clone)]
pub struct RequestMcp {
    mcp: McpClientManagerHandle,
    runner: Arc<dyn RequestAgentRunner>,
    connections: ConnectionGovernance,
    router: Arc<LlmRouter>,
}

impl RequestMcp {
    /// Connections go through `mcp` and are admitted by `connections`; the
    /// run goes through `runner`, on a model `router` reaches.
    #[must_use]
    pub fn new(
        mcp: McpClientManagerHandle,
        runner: Arc<dyn RequestAgentRunner>,
        connections: ConnectionGovernance,
        router: Arc<LlmRouter>,
    ) -> Self {
        Self {
            mcp,
            runner,
            connections,
            router,
        }
    }

    /// The configuration that names and bounds the session of `server`. Its
    /// environment stays empty: what the request sends as headers goes to the
    /// transport directly, and is never resolved against the environment or
    /// the secret store.
    fn config(server: &RequestedServer) -> McpServerConfig {
        McpServerConfig {
            format_version: 1,
            name: server.name.clone(),
            command: String::new(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            transport: "streamable-http".to_owned(),
            url: Some(server.url.to_string()),
            requires_approval: false,
            init_timeout_secs: INIT_TIMEOUT_SECS,
            call_timeout_secs: CALL_TIMEOUT_SECS,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: Vec::new(),
            tool_families: Default::default(),
            origin: None,
        }
    }

    /// Whether the policy and the network rule let the request bring
    /// `server`, before anything is connected.
    fn admit(&self, server: &RequestedServer) -> Result<(), (ErrorKind, String)> {
        let request = AdmissionRequest::for_mcp_server(&Self::config(server));
        if let AdmissionDecision::Refused { reason } = self.connections.admit(&request) {
            return Err((
                ErrorKind::RefusedByPolicy,
                format!(
                    "The MCP server at '{}' is refused by the organisation policy: {reason}",
                    server.url
                ),
            ));
        }
        apollia_core::net::assert_public_or_loopback(&server.url).map_err(|_| {
            (
                ErrorKind::InvalidRequest,
                format!(
                    "The MCP server at '{}' is refused: a request may bring a server on this \
                     machine or on a public host, never one on the private network.",
                    server.url
                ),
            )
        })
    }

    /// Connects `server` and answers the tools the request keeps, by their
    /// registry name.
    async fn connect(&self, server: &RequestedServer) -> Result<Vec<String>, (ErrorKind, String)> {
        let unreachable = |cause: String| {
            (
                ErrorKind::McpServerUnavailable,
                format!("The MCP server at '{}' did not answer: {cause}", server.url),
            )
        };
        let client = apollia_core::net::request_supplied_client_builder(&server.url)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| unreachable(e.to_string()))?;
        let config = Self::config(server);
        let transport = StreamableHttpTransport::with_client(
            client,
            server.url.as_str(),
            server.headers.clone(),
            Duration::from_secs(CALL_TIMEOUT_SECS),
            config.max_response_bytes,
        );
        match self
            .mcp
            .add_request_server(config, Box::new(transport))
            .await
        {
            Ok(_) => {}
            Err(McpSessionError::InitializeFailed { cause, .. })
                if cause.contains("already exists") =>
            {
                return Err((
                    ErrorKind::RunUnavailable,
                    format!(
                        "Another request is using an MCP server on the host of '{}'. Send the \
                         request again once it has ended.",
                        server.url
                    ),
                ));
            }
            Err(error) => return Err(unreachable(error.to_string())),
        }
        let tools = match self.mcp.server_detail(&server.name).await {
            Some(detail) => detail.tools,
            None => Vec::new(),
        };
        Ok(tools
            .into_iter()
            .filter(|tool| {
                server
                    .allowed_tools
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(&tool.local_name))
            })
            .map(|tool| format!("mcp:{}/{}", server.name, tool.local_name))
            .collect())
    }

    /// Removes the connections named `names`; one already gone is left.
    async fn disconnect(&self, names: &[String]) {
        for name in names {
            match self.mcp.remove_server(name).await {
                Ok(()) => tracing::event!(
                    tracing::Level::INFO,
                    server = %name,
                    "compat_server.request_mcp.disconnected"
                ),
                Err(error) => tracing::event!(
                    tracing::Level::WARN,
                    server = %name,
                    error = %error,
                    "compat_server.request_mcp.disconnect_failed"
                ),
            }
        }
    }
}

/// The agent a request runs under: no mission of its own, the tools the
/// request brought and nothing else, the model the request names.
fn request_agent(access: Vec<String>, backend: &str) -> ResolvedAgent {
    ResolvedAgent {
        id: REQUEST_AGENT_ID.to_owned(),
        origin: AgentOrigin::Interface,
        package: None,
        version: "1".to_owned(),
        enabled: true,
        name: REQUEST_AGENT_ID.to_owned(),
        mission: String::new(),
        examples: Vec::new(),
        access: access.clone(),
        sources: Vec::new(),
        model: Some(apollia_core::model_preset::AgentModel {
            agent: REQUEST_AGENT_ID.to_owned(),
            model_source: apollia_core::model_preset::ModelSource::Local,
            model_id: backend.to_owned(),
            preset_id: None,
        }),
        autonomy: crate::agents::global_default_autonomy(),
        planning: PlanningMode::Never,
        profile: None,
        overridden: Vec::new(),
        // The access is written in the definition: an agent whose definition
        // names none would be handed the reading family of the machine.
        definition: json!({
            "id": REQUEST_AGENT_ID,
            "name": REQUEST_AGENT_ID,
            "mission": "",
            "defaults": { "access": access },
        }),
    }
}

/// The one message the agent executor reads: the conversation, then the
/// request.
fn message_of(conversation: Option<String>, request: String) -> String {
    match conversation {
        Some(conversation) if !conversation.is_empty() => {
            format!("conversation:\n{conversation}\n\nrequest: {request}")
        }
        _ => request,
    }
}

/// Follows the run of a request to its end, then removes its connections.
struct Follower {
    host: RequestMcp,
    servers: Vec<String>,
    task_id: String,
    run_id: Option<String>,
    tx: mpsc::Sender<Progress>,
    /// Keeps the model loaded while the run uses it.
    _lease: ModelLease,
}

impl Follower {
    async fn follow(
        self,
        mut events: apollia_core::events::ResilientReceiver,
        mut end: oneshot::Receiver<AgentRunOutcome>,
    ) {
        let mut usage = Usage::default();
        let outcome = loop {
            tokio::select! {
                biased;
                Some(event) = events.recv() => {
                    if let Some(step) = agent_step(&event, &self.task_id) {
                        if self.tx.send(Progress::Step(step)).await.is_err() {
                            break self.stopped(&mut end).await;
                        }
                    }
                    if let RuntimeEvent::LlmResponseCaptured {
                        run_id, prompt_tokens, completion_tokens, ..
                    } = event {
                        if self.run_id.as_deref() == Some(run_id.as_str()) {
                            usage.prompt += u64::from(prompt_tokens);
                            usage.completion += u64::from(completion_tokens);
                        }
                    }
                }
                ended = &mut end => break Some(self.outcome(ended.ok(), usage)),
                () = self.tx.closed() => break self.stopped(&mut end).await,
            }
        };
        // The connections go before the caller hears the end: once the answer
        // is out, nothing of the request is left.
        self.host.disconnect(&self.servers).await;
        if let Some(outcome) = outcome {
            tracing::event!(
                tracing::Level::INFO,
                task_id = %self.task_id,
                answered = matches!(outcome, RunOutcome::Answered { .. }),
                "compat_server.request_mcp.ended"
            );
            let _ = self.tx.send(Progress::Ended(outcome)).await;
        }
    }

    /// The caller went away: the run is stopped, and given [`STOP_GRACE`] to
    /// end before its connections are removed.
    async fn stopped(&self, end: &mut oneshot::Receiver<AgentRunOutcome>) -> Option<RunOutcome> {
        tracing::event!(
            tracing::Level::INFO,
            task_id = %self.task_id,
            "compat_server.request_mcp.client_gone"
        );
        self.host.runner.cancel(self.task_id.clone()).await;
        let _ = tokio::time::timeout(STOP_GRACE, end).await;
        None
    }

    fn outcome(&self, ended: Option<AgentRunOutcome>, usage: Usage) -> RunOutcome {
        match ended {
            Some(outcome) if outcome.status == "completed" => RunOutcome::Answered {
                content: outcome.output,
                usage,
            },
            Some(outcome) if outcome.status == "awaiting" => RunOutcome::Unapproved {
                access: outcome.awaiting.unwrap_or_default(),
                task_id: self.task_id.clone(),
            },
            Some(outcome) if outcome.status == "canceled" => RunOutcome::Failed {
                message: "the run was stopped before its end".to_owned(),
            },
            Some(outcome) => RunOutcome::Failed {
                message: outcome.output,
            },
            None => RunOutcome::Failed {
                message: "the run was dropped".to_owned(),
            },
        }
    }
}

/// The model `id` held for the run, when it is a local chat model.
async fn hold(models: &LocalModels, id: &str) -> Result<HeldModel, (ErrorKind, String)> {
    let held = models.hold(id).await.map_err(|unheld| match unheld {
        Unheld::NotLoaded => (
            ErrorKind::ModelNotLoaded,
            format!(
                "The model '{id}' is not loaded. Load it in Apollia first; \
                 GET /v1/models lists the models it serves."
            ),
        ),
        Unheld::LoadFailed(message) => (ErrorKind::EngineUnavailable, message),
    })?;
    if held.served.kind != ModelKind::Chat {
        return Err((
            ErrorKind::EndpointNotServed,
            format!("The model '{id}' does not answer a conversation, so it calls no tool."),
        ));
    }
    Ok(held)
}

/// Answers `call`, which brings an MCP server, for `caller`.
///
/// Every refusal comes before a connection is opened, in this order: the
/// setting, the target, the caller, the shape of the request, the policy and
/// the address of each server, then the model.
pub(crate) async fn call_with_request_mcp(
    models: &LocalModels,
    agents: &ExposedAgents,
    tokens: Option<&ServerTokensHandle>,
    caller: Caller<'_>,
    call: ExposedCall<'_>,
) -> Response {
    let ExposedCall {
        endpoint,
        body,
        model,
    } = call;
    let family = endpoint.family();
    let refuse = |(kind, message): (ErrorKind, String)| error_response(family, kind, message);
    let invalid = |message: String| error_response(family, ErrorKind::InvalidRequest, message);
    if !models.allows_request_mcp() {
        return invalid(TURNED_OFF.to_owned());
    }
    if model.starts_with(MODEL_PREFIX) {
        return invalid(format!(
            "'{model}' is an Apollia agent or workflow: it works with the connections it was \
             given in Apollia, so a request cannot bring it an MCP server. Name a model."
        ));
    }
    // The runtime acts for the caller, as it does when it runs an agent: from
    // the local network, the same token is asked for.
    let path = endpoint.path();
    let admission = match tokens {
        Some(tokens) => admit_agent_call(caller.origin, path, caller.headers, tokens).await,
        None if caller.origin == CallOrigin::Loopback => Ok(()),
        None => Err(unauthorized_with(path, AGENT_TOKEN_REQUIRED)),
    };
    if let Err(refusal) = admission {
        return refusal;
    }
    let servers = match read_servers(endpoint, body) {
        Ok(servers) => servers,
        Err(message) => return invalid(message),
    };
    let turn = match read_turn(endpoint, body) {
        Ok(turn) => turn,
        Err(message) => return invalid(message),
    };
    let (Some(host), Some(engines)) = (agents.request_mcp(), agents.engines()) else {
        return error_response(
            family,
            ErrorKind::RunUnavailable,
            "This runtime cannot run a request that brings an MCP server.".to_owned(),
        );
    };
    for server in &servers {
        if let Err(refusal) = host.admit(server) {
            return refuse(refusal);
        }
    }
    let held = match hold(models, model).await {
        Ok(held) => held,
        Err(refusal) => return refuse(refusal),
    };
    let backend = models.backend_row(model).await;
    if host.router.latest().get(Some(&backend)).is_none() {
        return error_response(
            family,
            ErrorKind::RunUnavailable,
            format!(
                "The model '{model}' is loaded, but Apollia does not run agents on it yet. \
                 Choose it once in Apollia, then send the request again."
            ),
        );
    }
    // Subscribed before the launch, so no step of the run is missed.
    let events = apollia_core::subscribe_resilient(engines.bus, "compat_server.request_mcp");
    let mut connected = Vec::new();
    let mut access = Vec::new();
    for server in &servers {
        match host.connect(server).await {
            Ok(tools) => {
                connected.push(server.name.clone());
                access.extend(tools);
            }
            Err(refusal) => {
                host.disconnect(&connected).await;
                return refuse(refusal);
            }
        }
    }
    let agent = request_agent(access, &backend);
    let message = message_of(turn.conversation, turn.request);
    let launched = match host.runner.launch(agent, message).await {
        Ok(launched) => launched,
        Err(message) => {
            host.disconnect(&connected).await;
            return error_response(family, ErrorKind::RunFailed, message);
        }
    };
    tracing::event!(
        tracing::Level::INFO,
        task_id = %launched.task_id,
        servers = connected.len(),
        "compat_server.request_mcp.started"
    );
    let (tx, progress) = mpsc::channel(PROGRESS_CAPACITY);
    let follower = Follower {
        host: host.clone(),
        servers: connected,
        task_id: launched.task_id.clone(),
        run_id: launched.run_id.clone(),
        tx,
        _lease: held.lease,
    };
    tokio::spawn(follower.follow(events, launched.end));
    let run = StartedRun {
        run_id: launched.task_id.clone(),
        task_id: Some(launched.task_id.clone()),
        journal_run: launched.run_id.clone(),
        progress,
        // The follower stops the run itself when the caller goes away, since
        // the connections have to go with it.
        guard: StopOnDrop::nothing(),
    };
    let header = AnswerHeader::new(&run.run_id, model);
    let journal = LaunchedRun {
        task_id: run.task_id.clone(),
        run_id: run.journal_run.clone(),
    };
    let streamed = body.get("stream") == Some(&Value::Bool(true));
    let mut response = match endpoint {
        Endpoint::Messages => answer(run, MessagesAnswer::new(header), streamed).await,
        _ => answer(run, ResponsesAnswer::new(header), streamed).await,
    };
    response.extensions_mut().insert(journal);
    response
}
