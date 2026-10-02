//! A run of an agent outside any conversation.
//!
//! A task names an agent and carries one message. It runs through the same
//! agent executor as the chat, with no history and no session: the task
//! identifier stands where the session identifier would. A tool approval of
//! the run is therefore raised by the same event, waits in the same registry
//! and is answered through the same command as one of the chat. It never
//! expires on its own: no one watches a task the way a conversation is
//! watched, and a refusal by timeout would fail the run for an absent human.

use std::collections::HashSet;
use std::time::Duration;

use super::*;

/// How often a cancelled run refuses the approvals it still waits on, until
/// it has stopped.
const CANCELLED_RUN_REFUSAL_PERIOD: Duration = Duration::from_millis(100);

/// One run of an agent on one message, outside any conversation.
pub struct AgentTaskRun {
    /// The task, named in the approvals of the run in place of a session.
    pub task_id: String,
    /// The run the audit journal records the calls under.
    pub run_id: RunId,
    /// The agent as resolved.
    pub spec: AgentRunSpec,
    /// The request.
    pub message: String,
    /// Stops the run at its next checkpoint, and refuses the approvals it
    /// waits on.
    pub cancel: CancellationToken,
    /// Receives the outcome once the run has ended.
    pub done: oneshot::Sender<Result<ChatAgentResponse, ChatError>>,
}

/// What a spawned run needs from the actor.
struct AgentTaskWiring {
    llm_router: Arc<LlmRouter>,
    tool_registry: ToolRegistryHandle,
    event_bus: EventBusSender,
    a2a_invoker: Option<Arc<A2AInvoker>>,
    pending_approvals: PendingChatApprovals,
    runtime_budget: StepBudgetConfig,
    pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs,
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    chat_tools_config: Option<Arc<ChatToolsConfig>>,
    mcp_loading: LoadingMode,
    tool_search_limit: usize,
    hook_executor: Option<Arc<HookExecutor>>,
    hitl: HitlInvokerParams,
    package_tools: Option<Arc<dyn crate::package_tools::PackageToolHost>>,
}

impl ChatSessionManager {
    /// Start `run` in the background. The outcome goes to `run.done`.
    pub(in crate::chat::manager) fn handle_run_agent_task(
        &mut self,
        run: AgentTaskRun,
    ) -> Result<(), ChatError> {
        let llm_router = self.llm_router.clone().ok_or(ChatError::NoLlmConfigured)?;
        let hitl = HitlInvokerParams {
            session_id: run.task_id.clone(),
            event_bus: self.event_bus.clone(),
            pending_fs: self.pending_fs_approvals.clone(),
            fs_allow_rules: Arc::new(std::sync::Mutex::new(HashSet::new())),
            risk_config: apollia_core::FilesystemRiskConfig::default(),
            trusted_paths: self
                .chat_tools_config
                .as_ref()
                .map(|c| c.trusted_paths.clone())
                .unwrap_or_default(),
        };
        let wiring = AgentTaskWiring {
            llm_router,
            tool_registry: self.tool_registry.clone(),
            event_bus: self.event_bus.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            pending_approvals: self.pending_chat_approvals.clone(),
            runtime_budget: self.runtime_budget.clone(),
            pending_user_inputs: self.pending_user_inputs.clone(),
            mcp_handle: self.mcp_handle.clone(),
            chat_tools_config: self.chat_tools_config.clone(),
            mcp_loading: self.mcp_loading,
            tool_search_limit: self.tool_search_limit,
            hook_executor: self.hook_executor.clone(),
            hitl,
            package_tools: self.package_tools.clone(),
        };
        info!(task_id = %run.task_id, agent = %run.spec.agent_id, "agent.task.started");
        tokio::spawn(run_agent_task(wiring, run));
        Ok(())
    }

    /// Answer a tool approval of a run outside any conversation.
    ///
    /// Such a run has no session to keep a grant in, so "always accept"
    /// accepts this call alone.
    pub(in crate::chat::manager) fn resolve_task_approval(
        &self,
        task_id: &str,
        message_id: &str,
        tool_call_id: &str,
        decision: ToolDecision,
    ) -> Result<(), ChatError> {
        let decision = match decision {
            ToolDecision::AlwaysAccept { .. } => ToolDecision::Accept,
            other => other,
        };
        let key = format!("{task_id}::{message_id}::{tool_call_id}");
        if self.pending_chat_approvals.resolve(&key, decision) {
            Ok(())
        } else {
            Err(ChatError::SessionNotFound(format!(
                "no pending approval for {key}"
            )))
        }
    }
}

impl ChatSessionManagerHandle {
    /// Run an agent on one message, outside any conversation.
    ///
    /// Returns once the run has started; its outcome reaches `run.done`.
    ///
    /// # Errors
    ///
    /// [`ChatError::NoLlmConfigured`] when no model router is set, or
    /// [`ChatError::InternalError`] when the actor is gone.
    pub async fn run_agent_task(&self, run: AgentTaskRun) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ChatCommand::RunAgentTask {
                run,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::InternalError("actor channel closed".into()))?;
        reply_rx
            .await
            .map_err(|_| ChatError::InternalError("actor reply dropped".into()))?
    }
}

/// Run `run` to its end and send the outcome.
async fn run_agent_task(wiring: AgentTaskWiring, run: AgentTaskRun) {
    let AgentTaskRun {
        task_id,
        run_id,
        mut spec,
        message,
        cancel,
        done,
    } = run;
    let mcp_index: Vec<ToolIndexSnapshot> = match (wiring.mcp_loading, &wiring.mcp_handle) {
        (LoadingMode::Deferred, Some(handle)) => handle.get_tool_index().await,
        _ => Vec::new(),
    };
    let disabled: Vec<String> = wiring
        .chat_tools_config
        .as_deref()
        .map(super::dispatcher::effective_disabled_tools)
        .unwrap_or_default();
    let mcp_index = super::disabled::only_granted(mcp_index, &spec.granted_tools);
    let (granted, mcp_index) = super::disabled::without_disabled(
        std::mem::take(&mut spec.granted_tools),
        mcp_index,
        &disabled,
    );
    spec.granted_tools = granted;
    let tool_turn_temperature = wiring
        .chat_tools_config
        .as_ref()
        .and_then(|c| c.tool_turn_temperature);

    let resolved = super::libre::build_session_invoker(
        WorkspaceResolutionParams {
            project_id: None,
            project_repo: None,
            hitl: Some(wiring.hitl),
            pending_user_inputs: Some(wiring.pending_user_inputs),
            mcp_handle: wiring.mcp_handle,
            chat_tools_config: wiring.chat_tools_config,
            session_id: task_id.clone(),
            mcp_loading: wiring.mcp_loading,
            mcp_index: mcp_index.clone(),
            tool_search_limit: wiring.tool_search_limit,
            package_tools: super::package_executors(&wiring.package_tools, &spec.agent_id, &cancel),
        },
        &wiring.a2a_invoker,
        wiring.hook_executor.clone(),
    )
    .await;
    let invoker = match resolved {
        Ok(invoker) => invoker,
        Err(e) => {
            let _ = done.send(Err(e));
            return;
        }
    };
    let executor = AgentExecutor::new(
        spec,
        AgentExecutorDeps {
            llm_router: wiring.llm_router,
            tool_registry: wiring.tool_registry,
            tool_invoker: invoker.invoker,
            event_bus: wiring.event_bus,
            a2a_invoker: wiring.a2a_invoker,
            todo: None,
            plan: None,
        },
        ApolliaContext::default(),
        SessionWiring {
            workspace_path: invoker.workspace,
            mcp_index: match wiring.mcp_loading {
                LoadingMode::Deferred => Some(mcp_index),
                LoadingMode::Eager => None,
            },
            tool_search_limit: wiring.tool_search_limit,
            hook_executor: wiring.hook_executor,
            tool_turn_temperature,
            ..SessionWiring::default()
        },
        &wiring.runtime_budget,
        cancel.clone(),
    );

    // A cancelled run may be waiting on a human. Refusing its approvals until
    // it has stopped lets it reach its next checkpoint, including an approval
    // raised between that refusal and the checkpoint.
    let refusals = {
        let pending = wiring.pending_approvals.clone();
        let task_id = task_id.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            cancel.cancelled().await;
            loop {
                pending.refuse_session(&task_id);
                tokio::time::sleep(CANCELLED_RUN_REFUSAL_PERIOD).await;
            }
        })
    };
    // The search over the deferred tools reads an in-memory index, as in the
    // chat; every other tool asks.
    let authorized: HashSet<String> =
        HashSet::from([apollia_mcp::tool_search::TOOL_SEARCH_NAME.to_string()]);
    let result = executor
        .run(RunInput {
            session_id: &task_id,
            message_id: &task_id,
            run_id: &run_id,
            user_message: &message,
            history: &[],
            authorized_tools: &authorized,
            pending_approvals: &wiring.pending_approvals,
            summary: None,
            context_window_size: DEFAULT_CONTEXT_WINDOW_SIZE,
        })
        .await;
    refusals.abort();
    info!(task_id = %task_id, ok = result.is_ok(), "agent.task.ended");
    let _ = done.send(result);
}
