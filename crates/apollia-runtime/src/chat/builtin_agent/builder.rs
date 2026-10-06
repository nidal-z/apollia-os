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
            conversation_profile: None,
            composer: None,
            context_manager: ContextManager::from_config(&ORIAConfig::default()),
            session: SessionOverrides::default(),
            meta_handle: None,
            workspace_path: None,
            mcp_index: None,
            // Overwritten by `with_mcp_index` in deferred mode; unused otherwise.
            tool_search_limit: 20,
            todo: deps.todo,
            plan: deps.plan,
            agent_draft: None,
            session_plan_mode: false,
            session_plan_phase: PlanPhase::Done,
            hook_executor: None,
            pending_injection: None,
            turn_images: crate::chat::images::TurnImages::default(),
            tool_turn_temperature: DEFAULT_TOOL_TURN_TEMPERATURE,
            governance: crate::governance::GovernanceHandle::detached(
                crate::governance::GovernanceState::default(),
                None,
            ),
            unattended: false,
            preferred_backend: None,
            agent_id: crate::chat::manager::APOLLIA_CHAT_AGENT_ID.to_string(),
            approval_timeout: Some(CHAT_APPROVAL_TIMEOUT),
            tool_output_dir: None,
            resilience: apollia_oria::ResilienceLayer::default(),
            turn_id: None,
        }
    }

    /// Apply what the conversation changes from its agent: its instructions,
    /// its request settings, its reasoning, its accesses proposed, and what
    /// its context does at the limit, which the context manager reads here.
    pub fn with_session_overrides(mut self, session: SessionOverrides) -> Self {
        self.context_manager =
            ContextManager::from_config(&ORIAConfig::default()).with_overflow(session.overflow);
        self.session = session;
        self
    }

    /// Offer the `propose_agent` tool, its drafts held to `offer`. `None`,
    /// the default, leaves the tool out.
    pub fn with_agent_draft(mut self, offer: Option<DraftOffer>) -> Self {
        self.agent_draft = offer;
        self
    }

    /// Keep a tool output too long for the context whole in `dir`, and give
    /// the model its beginning and the path. `None` cuts it instead.
    pub fn with_tool_output_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.tool_output_dir = dir;
        self
    }

    /// Name the conversation turn the run's model calls are made for, the
    /// identifier of the answer. `None` outside a conversation.
    pub fn with_turn_id(mut self, turn_id: Option<String>) -> Self {
        self.turn_id = turn_id;
        self
    }

    /// Set the delay after which a pending tool approval is refused. `None`
    /// waits for the human however long it takes.
    pub fn with_approval_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.approval_timeout = timeout;
        self
    }

    /// The delay after which a pending tool approval is refused.
    #[cfg(test)]
    pub(crate) fn approval_timeout(&self) -> Option<Duration> {
        self.approval_timeout
    }

    /// Name the agent this loop runs for. Its model is resolved, and a
    /// fallback said, under this identifier.
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = agent_id.into();
        self
    }

    /// Offer the agents and workflows the agent was granted, read through
    /// `composer`. `None` offers none of them.
    pub fn with_composer(mut self, composer: Option<Arc<dyn Composer>>) -> Self {
        self.composer = composer;
        self
    }

    /// Route the turn's calls to the backend the session prefers. `None`, or
    /// a backend the router cannot reach, keeps the default.
    pub fn with_preferred_backend(mut self, backend: Option<String>) -> Self {
        self.preferred_backend = backend;
        self
    }

    /// Decide every tool call with `governance`. `None` keeps the empty
    /// state the agent starts with: reading runs, everything sensitive asks.
    pub fn with_governance(
        mut self,
        governance: Option<crate::governance::GovernanceHandle>,
    ) -> Self {
        if let Some(governance) = governance {
            self.governance = governance;
        }
        self
    }

    /// Mark the run as watched by nobody: a call that would ask waits for an
    /// answer given later instead.
    pub fn with_unattended(mut self, unattended: bool) -> Self {
        self.unattended = unattended;
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

    /// The request settings every call of a turn on `backend` starts from.
    ///
    /// Four levels, field by field: the conversation, over the preset (the
    /// one of the conversation, else the agent's), over the agent's package,
    /// over the model. Read once per turn: the levels below the conversation
    /// come from files.
    pub(in crate::chat::builtin_agent) fn turn_request_settings(
        &self,
        backend: Option<&str>,
    ) -> TurnRequestSettings {
        let backend = backend.unwrap_or_else(|| self.llm_router.default_name());
        let levels = self
            .session
            .request_levels
            .as_ref()
            .map(|binding| binding.request_levels(&self.llm_router, Some(backend)))
            .unwrap_or_default();
        TurnRequestSettings {
            chosen_temperature: self
                .session
                .request
                .clone()
                .over(&levels.preset)
                .temperature,
            resolved: levels.resolved_in_session(&self.session.request),
        }
    }

    /// Apply the request settings of the turn to one of its calls.
    ///
    /// A temperature the user chose, for the conversation or in a preset,
    /// is sent as chosen. Without one, a call that offers tools keeps the
    /// tool-turn temperature, and the package and the model fill the rest.
    pub(in crate::chat::builtin_agent) fn apply_request_settings(
        &self,
        settings: &TurnRequestSettings,
        request: &mut CompletionRequest,
    ) {
        request.temperature = settings
            .chosen_temperature
            .or_else(|| self.turn_temperature(!request.tools.is_empty()));
        request.thinking = self.session.thinking;
        request.reasoning_effort = self.session.reasoning_effort;
        settings.resolved.apply_to(request);
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

    /// Attaches the images of the turn: those of the message answered, sent
    /// after its text, and those of earlier user messages, replayed with them.
    pub fn with_turn_images(mut self, images: crate::chat::images::TurnImages) -> Self {
        self.turn_images = images;
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

    /// Name the profile the conversation designates, the one the profile
    /// block is read from. `None` reads the default profile.
    pub fn with_conversation_profile(mut self, profile_id: Option<String>) -> Self {
        self.conversation_profile = profile_id;
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
