use super::*;

impl BuiltInChatAgent {
    /// Create a new agent with the given dependencies.
    pub fn new(deps: BuiltInChatAgentDeps) -> Self {
        Self {
            llm_router: deps.llm_router,
            tool_registry: deps.tool_registry,
            tool_invoker: deps.tool_invoker,
            event_bus: deps.event_bus,
            user_memory: deps.user_memory,
            a2a_invoker: deps.a2a_invoker,
            context_manager: ContextManager::from_config(&ORIAConfig::default()),
            meta_handle: None,
            workspace_path: None,
            mcp_index: None,
            // Overwritten by `with_mcp_index` in deferred mode; unused otherwise.
            tool_search_limit: 20,
            todo: deps.todo,
            plan: deps.plan,
            session_plan_mode: false,
            session_plan_phase: PlanPhase::Done,
            hook_executor: None,
            pending_injection: None,
            tool_turn_temperature: DEFAULT_TOOL_TURN_TEMPERATURE,
            prefix_checker: None,
        }
    }

    /// Attach the per-invocation prefix-rule checker. `None` keeps the
    /// previous behavior: a tool call outside the name-only authorization set
    /// goes straight to human approval.
    pub fn with_prefix_checker(mut self, checker: Option<Arc<PrefixChecker>>) -> Self {
        self.prefix_checker = checker;
        self
    }

    /// Sets the temperature applied to turns that advertise tools.
    ///
    /// Threaded from `[chat] tool_turn_temperature`. `None` resolves to
    /// [`DEFAULT_TOOL_TURN_TEMPERATURE`], so an unset config keeps the tuned
    /// default rather than the backend's conversational temperature.
    pub fn with_tool_turn_temperature(mut self, temperature: Option<f32>) -> Self {
        self.tool_turn_temperature = temperature.unwrap_or(DEFAULT_TOOL_TURN_TEMPERATURE);
        self
    }

    /// Temperature to send for one ReAct turn.
    ///
    /// Lowering the temperature whenever tools are advertised makes structured
    /// tool-call output far more reliable on small local models. A turn with no
    /// tools returns `None`, leaving the backend's conversational default intact
    /// so plain chat is byte-for-byte unchanged.
    pub(in crate::chat::builtin_agent) fn turn_temperature(&self, has_tools: bool) -> Option<f32> {
        has_tools.then_some(self.tool_turn_temperature)
    }

    /// Attaches an operator instruction to a resume turn.
    ///
    /// Set by the manager when resuming a paused session with a queued injection.
    /// The instruction is prepended as a user message on the turn and any plan
    /// step the agent creates or modifies is stamped with
    /// [`StepOrigin::UserInject`] provenance carrying the operator text as reason.
    /// Absent for every ordinary turn.
    pub fn with_pending_injection(mut self, injection: Option<InjectedInstruction>) -> Self {
        self.pending_injection = injection;
        self
    }

    /// Sets the plan phase the owning session is in at the start of the turn.
    ///
    /// Threaded from `ChatSession::plan_phase`. A turn opened in
    /// [`PlanPhase::AwaitingApproval`] is a revision turn and is handled without
    /// reopening discovery (see [`session_plan_phase`](Self::session_plan_phase)).
    pub fn with_plan_phase_start(mut self, phase: PlanPhase) -> Self {
        self.session_plan_phase = phase;
        self
    }

    /// Sets whether the owning session has plan mode enabled.
    ///
    /// This is the gate the runtime threads from `ChatSession::plan_mode`. The
    /// `plan_*` tools require both a plan store and this flag, so a session with
    /// plan mode off behaves exactly as before.
    pub fn with_plan_mode(mut self, enabled: bool) -> Self {
        self.session_plan_mode = enabled;
        self
    }

    /// Configure the deferred MCP tool index for this agent.
    ///
    /// `Some(index)` switches `build_tool_specs` to the deferred path: it injects
    /// the synthetic `tool_search` spec (capped at `tool_search_limit`) and omits
    /// the individual MCP schemas. `None` keeps the eager path unchanged.
    pub fn with_mcp_index(
        mut self,
        mcp_index: Option<Vec<ToolIndexSnapshot>>,
        tool_search_limit: usize,
    ) -> Self {
        self.mcp_index = mcp_index;
        self.tool_search_limit = tool_search_limit;
        self
    }

    /// Set the workspace path for this agent (used in system prompt and bash CWD).
    pub fn with_workspace_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.workspace_path = path;
        self
    }

    /// Attach the shared lifecycle hook executor. No-op when `None`: the loop
    /// runs without any hook interception.
    pub fn with_hook_executor(mut self, executor: Option<Arc<HookExecutor>>) -> Self {
        self.hook_executor = executor;
        self
    }
}
