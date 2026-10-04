use super::*;

/// Commands sent to the [`ChatSessionManager`] actor.
pub enum ChatCommand {
    /// Create a new chat session.
    CreateSession {
        /// Session mode (Libre or Agent).
        mode: ChatMode,
        /// Agent name (required for Agent mode).
        agent_name: Option<String>,
        /// Custom system prompt override.
        system_prompt: Option<String>,
        /// Tool names available in this session.
        tools: Vec<String>,
        /// The model chosen for the session, `None` for the agent's.
        model: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionInfo, ChatError>>,
    },
    /// Send a user message in a session.
    SendMessage {
        /// Target session.
        session_id: SessionId,
        /// Text content.
        content: String,
        /// Images attached after the text, already decoded and checked for
        /// count and size.
        images: Vec<apollia_llm::message_image::MessageImage>,
        /// Documents attached to the message, already copied and read: their
        /// text is part of `content`, this is what the metadata lists.
        documents: Option<crate::chat::documents::MessageDocuments>,
        /// Response channel.
        reply: oneshot::Sender<Result<MessageId, ChatError>>,
    },
    /// Resolve a pending tool approval.
    ResolveTool {
        /// Target session.
        session_id: SessionId,
        /// Message that triggered the tool call.
        message_id: MessageId,
        /// Unique id of the tool call being resolved (correlates with the
        /// pending-approval key and the frontend card).
        tool_call_id: String,
        /// Name of the tool.
        tool_name: String,
        /// User decision.
        decision: ToolDecision,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List all sessions (optionally filtered by status).
    ListSessions {
        /// Optional status filter.
        status_filter: Option<SessionStatus>,
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    /// Get detailed session info.
    GetSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Option<SessionDetail>>,
    },
    /// The model a session runs on, as the next message would route it.
    SessionModel {
        /// Target session.
        session_id: SessionId,
        /// Response channel: the backend or model name, `None` for the
        /// runtime default.
        reply: oneshot::Sender<Result<Option<String>, ChatError>>,
    },
    /// Resolve the effective working directory for a session.
    ///
    /// Mirrors the sandbox-root resolution applied per message when the agent
    /// runs its file tools, so a caller can reveal a relative path against the
    /// exact directory the agent used. Returns `None` for an unknown session,
    /// or a free chat with neither a configured default workspace nor an
    /// existing `~/.apollia`.
    ResolveSessionWorkspace {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Option<std::path::PathBuf>>,
    },
    /// Read the todo list for a session.
    GetSessionTodo {
        /// Target session.
        session_id: SessionId,
        /// Response channel: `SessionNotFound` for an unknown session, an empty
        /// list for a known session with no items.
        reply: oneshot::Sender<Result<Vec<TodoItem>, ChatError>>,
    },
    /// Close a session.
    CloseSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Update session configuration (system_prompt, tools, llm_backend, profile).
    UpdateSession {
        /// Target session.
        session_id: SessionId,
        /// The fields to change.
        update: SessionConfigUpdate,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Empty the profile of every session designating `profile_id`, the
    /// profile having been deleted.
    ReleaseProfile {
        /// The deleted profile.
        profile_id: String,
        /// Response channel: how many sessions designated it.
        reply: oneshot::Sender<Result<usize, ChatError>>,
    },
    /// Delete a session and all its data.
    DeleteSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Rename a session (set a user-defined title).
    RenameSession {
        /// Target session.
        session_id: SessionId,
        /// New display title.
        title: String,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Enable or disable plan mode for a session.
    SetPlanMode {
        /// Target session.
        session_id: SessionId,
        /// Desired plan-mode state.
        enabled: bool,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Approve the plan awaiting approval for a session (soft gate).
    ApprovePlan {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Reject the plan awaiting approval for a session, with an optional reason.
    RejectPlan {
        /// Target session.
        session_id: SessionId,
        /// Optional operator reason forwarded to the revising agent.
        reason: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Read the ordered plan mutation history for a session.
    ReadPlanMutations {
        /// Target session.
        session_id: SessionId,
        /// Response channel: `SessionNotFound` for an unknown session, an empty
        /// list for a known session whose plan recorded no mutations.
        reply: oneshot::Sender<Result<Vec<PlanMutation>, ChatError>>,
    },
    /// Read the current plan snapshot for a session.
    GetPlan {
        /// Target session.
        session_id: SessionId,
        /// Response channel: `SessionNotFound` for an unknown session, a
        /// snapshot with `plan: None` for a known session that has no plan yet.
        reply: oneshot::Sender<Result<ChatPlanSnapshot, ChatError>>,
    },
    /// Run an agent on one message, outside any conversation.
    RunAgentTask {
        /// The run.
        run: super::agent_task::AgentTaskRun,
        /// Answered once the run has started.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Cooperatively pause the active ReAct turn for a session.
    PauseSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), PauseError>>,
    },
    /// Resume a paused session, restarting the ReAct loop from persisted state.
    ResumePausedSession {
        /// Target session.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), PauseError>>,
    },
    /// Inject a natural-language instruction into a paused session and resume it.
    InjectInstruction {
        /// Target session.
        session_id: SessionId,
        /// Operator instruction text.
        text: String,
        /// Response channel.
        reply: oneshot::Sender<Result<(), InjectError>>,
    },
    /// Read the cooperative pause state of a session.
    GetPauseState {
        /// Target session.
        session_id: SessionId,
        /// Response channel: `None` for an unknown session.
        reply: oneshot::Sender<Option<PauseState>>,
    },
    /// Regenerate the assistant reply to the last user turn (truncate-in-place).
    RegenerateResponse {
        /// Target session.
        session_id: SessionId,
        /// Assistant message to regenerate. This message and every later message
        /// are truncated, then the preceding user turn is re-run.
        message_id: MessageId,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Replace a user message and re-run from it (truncate-in-place).
    EditAndResend {
        /// Target session.
        session_id: SessionId,
        /// User message to edit. This message and everything after it are
        /// truncated, then a fresh user message with `content` is sent.
        message_id: MessageId,
        /// New user message content.
        content: String,
        /// Response channel: the id of the newly appended user message.
        reply: oneshot::Sender<Result<MessageId, ChatError>>,
    },
    /// Internal: ReAct exchange completed successfully (sent by spawned task).
    ExchangeComplete {
        /// Target session.
        session_id: SessionId,
        /// User message that triggered the exchange.
        message_id: MessageId,
        /// Agent response.
        response: ChatAgentResponse,
    },
    /// Internal: ReAct exchange failed (sent by spawned task).
    ExchangeError {
        /// Target session.
        session_id: SessionId,
        /// User message that triggered the exchange.
        message_id: MessageId,
        /// Error description.
        error: String,
    },
    /// Internal: persist a conversation summary computed by the spawned task.
    PersistSummary {
        /// Target session.
        session_id: SessionId,
        /// Summary text to store.
        summary: String,
    },
    /// List the N most recent sessions with their first message content.
    GetRecentSummaries {
        /// Maximum number of sessions to return.
        limit: usize,
        /// Response channel.
        reply: oneshot::Sender<Vec<RecentSessionSummary>>,
    },
    /// Load a session from SQLite into memory (if not already loaded) and reset
    /// Processing status to Active.
    ResumeSession {
        /// Target session identifier.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionDetail, ChatError>>,
    },
    /// Hot-reload the LLM router (e.g. after onboarding setup).
    ReloadLlm {
        /// New router to use for subsequent requests.
        router: Option<Arc<LlmRouter>>,
    },
    /// Hand the source of the package tool executors to the actor.
    SetPackageTools {
        /// Used by every run that starts afterwards.
        host: Arc<dyn crate::package_tools::PackageToolHost>,
    },
    /// Hand the live sections of the configuration to the actor.
    FollowConfig {
        /// Read between two commands: a changed section applies to the next
        /// conversation and the next exchange.
        feed: crate::config_reload::ConfigFeed,
    },
    /// Hand the composer of the granted agent and workflow calls to the actor.
    SetComposer {
        /// Used by every run that starts afterwards.
        composer: Arc<dyn crate::composition::Composer>,
    },
    /// Hand the governance every tool call is decided by to the actor.
    SetGovernance {
        /// Used by every run that starts afterwards.
        governance: crate::governance::GovernanceHandle,
    },
    /// Fork a session, creating a child with a copy of the history.
    ForkSession {
        /// Parent session to fork from.
        session_id: SessionId,
        /// Number of messages to copy (None = all).
        up_to_index: Option<usize>,
        /// Response channel.
        reply: oneshot::Sender<Result<SessionInfo, ChatError>>,
    },
    /// List all child sessions of a parent (forks).
    ListChildren {
        /// Parent session identifier.
        session_id: SessionId,
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    /// List in-memory tool authorizations across all active sessions
    /// (used by the desktop Settings > Permissions page to surface
    /// session-only auths that don't live in `governance.db`).
    ListSessionAuthorizations {
        /// Response channel.
        reply: oneshot::Sender<Vec<SessionAuthorizationView>>,
    },
    /// Remove an in-memory tool authorization from a specific session.
    RevokeSessionAuthorization {
        /// Target session.
        session_id: SessionId,
        /// Tool name to remove from `authorized_tools`.
        tool_name: String,
        /// Response channel, `true` if the entry existed and was removed.
        reply: oneshot::Sender<Result<bool, ChatError>>,
    },
    /// List all A2A skills available from active worker agents.
    ListA2ASkills {
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::SkillListing>>,
    },
    /// Snapshot A2A skill telemetry.
    ListA2ASkillTelemetry {
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::A2ASkillTelemetry>>,
    },
    /// Retrieve A2A step provenance entries, optionally filtered by skill id.
    ListA2AStepProvenance {
        /// Optional skill id filter.
        skill_id: Option<String>,
        /// Response channel.
        reply: oneshot::Sender<Vec<crate::a2a::A2AStepProvenance>>,
    },
    /// Check compatibility of a skill against a required semver version.
    CheckA2ACompatibility {
        /// Skill id to check.
        skill_id: String,
        /// Required semver version (e.g. `"1.5.0"`).
        required_version: String,
        /// Response channel.
        reply: oneshot::Sender<Option<crate::a2a::A2ACompatibilityWarning>>,
    },
    /// List recently resolved chat tool approvals.
    ListApprovalHistory {
        /// Maximum number of entries to return.
        limit: i64,
        /// Number of days to look back.
        days: i64,
        /// Response channel.
        reply:
            oneshot::Sender<Result<Vec<super::super::repository::ChatApprovalLogRow>, ChatError>>,
    },
    /// Internal: register a pending `ask_user` reply channel from the background drain task.
    RegisterUserInputReply {
        /// Unique request identifier.
        request_id: String,
        /// Chat session that triggered the ask_user call.
        session_id: String,
        /// Questions JSON for event emission.
        questions_json: String,
        /// Agent context for the questions.
        context: Option<String>,
        /// Oneshot sender to deliver answers back to the executor.
        reply_tx: tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
    },
    /// Resolve a pending `ask_user` request with user answers.
    ResolveUserInput {
        /// Unique request identifier emitted in `ChatUserInputRequired`.
        request_id: String,
        /// User answers to the questions.
        answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// Reject a pending `ask_user` request with a mandatory reason.
    RejectUserInput {
        /// Unique request identifier.
        request_id: String,
        /// Operator-provided reason (non-empty, enforced by the frontend).
        reason: String,
        /// Response channel.
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    /// List all currently pending `ask_user` requests (for inbox / reconnection).
    ListPendingUserInputs {
        /// Response channel.
        reply: oneshot::Sender<Vec<PendingUserInputView>>,
    },
    /// Fetch aggregated metrics for a session (tokens, cost, tool stats, context window).
    GetSessionMetrics {
        /// Target session.
        session_id: SessionId,
        /// Response channel (returns `None` when session is unknown).
        reply: oneshot::Sender<Option<SessionMetrics>>,
    },
    /// Shut down the actor.
    Shutdown,
}
