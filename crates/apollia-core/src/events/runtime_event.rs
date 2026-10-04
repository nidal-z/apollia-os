// REASON: single cohesive serialized event enum; splitting its variants would
// change the serde/wire representation (HTTP API, audit journal, replay), a
// behavior change. Kept whole intentionally; the <800-line module guideline
// does not apply to one indivisible item.

use serde::{Deserialize, Serialize};

use crate::mcp_health::McpHealth;

use super::{AgentId, ModelUnloadReason, RunId, TaskId, ToolCallRationale};

/// Complete catalogue of Apollia OS runtime events.
///
/// Defined in `apollia-core` to avoid circular dependencies: every actor
/// (`apollia-runtime`, `apollia-oria`, etc.) imports this type without
/// creating a cycle.
///
/// Carried over `tokio::sync::broadcast` by the `EventBus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    /// An agent was registered in the Registry (state: Initializing).
    AgentRegistered(AgentId),
    /// An agent finished initializing and is operational (state: Active).
    AgentReady(AgentId),
    /// An agent moved to a degraded state.
    AgentDegraded { agent_id: AgentId, reason: String },
    /// An agent is shutting down (state: Stopping, draining tasks).
    AgentStopping(AgentId),
    /// An agent stopped cleanly.
    AgentStopped(AgentId),
    /// A task started on an agent.
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    /// A task finished (success or failure).
    TaskCompleted {
        agent_id: AgentId,
        task_id: TaskId,
        success: bool,
        /// Text output produced by the agent on success; `None` on failure or when the
        /// backend does not carry output (legacy callers set this to `None`).
        #[serde(default)]
        output: Option<String>,
    },
    /// A task was canceled.
    TaskCanceled { task_id: TaskId },
    /// A tool circuit breaker opened.
    ToolCircuitBroken { tool_name: String },
    /// A tool circuit breaker closed again after recovery.
    ToolCircuitRestored { tool_name: String },
    /// All components are ready, runtime operational.
    AllReady,
    /// Shutdown requested (SIGTERM or CLI command).
    ShutdownRequested,

    /// Loading an installed agent failed.
    ///
    /// An agent is loaded by its first task, not at boot: this is emitted by
    /// the backend factory when the module of the agent cannot be loaded.
    /// The task fails, the runtime continues (graceful degradation).
    AgentLoadFailed {
        /// Name of the agent that failed to load.
        name: String,
        /// Error message detailing the cause of the failure.
        error: String,
    },

    /// An agent was installed permanently.
    AgentInstalled {
        /// Unique name of the installed agent.
        name: String,
        /// Semver version of the agent.
        version: String,
    },
    /// An installed agent was removed.
    AgentUninstalled {
        /// Name of the uninstalled agent.
        name: String,
    },

    /// A trigger fired, task submitted to the runtime.
    TriggerFired {
        /// Identifier of the trigger that produced the event.
        trigger_id: String,
        /// Name of the target agent.
        agent: String,
        /// Identifier of the task submitted to the TaskRouter.
        task_id: TaskId,
    },
    /// A trigger was skipped (OnBusyPolicy::Skip or agent busy).
    TriggerSkipped {
        /// Trigger identifier.
        trigger_id: String,
        /// Reason for the skip.
        reason: String,
    },
    /// An error occurred while processing a trigger.
    TriggerError {
        /// Trigger identifier.
        trigger_id: String,
        /// Error message.
        error: String,
    },
    /// A trigger's bounded queue is full, the trigger is dropped.
    ///
    /// Emitted by `TriggerEngine` when [`OnBusyPolicy::Queue`] is configured and
    /// `max_depth` is reached. The dropped trigger is lost (not persisted).
    TriggerQueueFull {
        /// Identifier of the dropped trigger.
        trigger_id: String,
    },
    /// A trigger was enabled via the CLI or the API.
    TriggerEnabled {
        /// Identifier of the enabled trigger.
        trigger_id: String,
    },
    /// A trigger was disabled via the CLI or the API.
    TriggerDisabled {
        /// Identifier of the disabled trigger.
        trigger_id: String,
    },
    /// The TriggerEngine reloaded its configuration (hot reload or initial start).
    TriggersReloaded {
        /// Number of active triggers after reload.
        count: usize,
    },

    /// The `apollia.toml` read at startup was refused: the embedded runtime
    /// runs on the default values instead, and says so here rather than in
    /// silence.
    ///
    /// Emitted once by the desktop's embedded runtime when it is up; the daemon
    /// refuses to start on the same file instead.
    ConfigInvalid {
        /// Path of the refused file.
        path: String,
        /// Every fault found, each at its line.
        diagnostics: Vec<crate::config::ConfigDiagnostic>,
    },

    /// A new `apollia.toml`, saved through the configuration routes or changed
    /// on disk, was applied by the running runtime.
    ///
    /// Emitted after each application, by the configuration reloader: the
    /// sections whose new values the runtime now uses, and the keys whose new
    /// value waits for a restart.
    ConfigReloaded {
        /// The sections applied without a restart, such as `tools` or `hitl`.
        sections: Vec<String>,
        /// The dotted keys changed since the start that wait for a restart.
        pending_restart: Vec<String>,
    },

    /// An MCP server was hot-reloaded successfully.
    ///
    /// Emitted by `McpClientManagerHandle::reload_server` after the new session
    /// has been established and its tools registered. Lets bus consumers detect
    /// changes to an MCP server's tool surface.
    McpServerReloaded {
        /// Name of the reloaded MCP server.
        name: String,
        /// Names of the tools exposed by the previous session.
        old_tools: Vec<String>,
        /// Names of the tools exposed by the new session.
        new_tools: Vec<String>,
    },

    /// An MCP server's operational health changed.
    ///
    /// Emitted by the `McpClientManager` actor on session start (success or
    /// failure), on a transport 401 turning into [`McpHealth::NeedsReauth`], and
    /// on tool-call outcome classification. Lets the desktop badge reflect
    /// reality without a manual refresh.
    McpServerHealthChanged {
        /// Name of the MCP server.
        name: String,
        /// New operational health.
        health: McpHealth,
    },

    /// An LLM backend is loading (before `load()` or HTTP initialization).
    LlmModelLoading {
        /// Logical backend name as configured in `apollia.toml`.
        backend: String,
        /// Path to the `.gguf` file (local backend) or API URL (cloud backend).
        model_path: String,
    },
    /// An LLM backend is ready: model loaded in memory or cloud connection verified.
    LlmModelReady {
        /// Logical backend name.
        backend: String,
        /// Model identifier: file name without extension (.gguf) or API model_id.
        model_id: String,
    },
    /// A resident local model was relaunched under its preset's new load settings.
    LlmModelReloaded {
        /// Logical backend name, the model the preset belongs to.
        backend: String,
        /// Path to the `.gguf` file the new process serves.
        model_path: String,
    },
    /// A resident local model was unloaded and its memory given back.
    LlmModelUnloaded {
        /// Wire name of the engine that served it (`llama-server`, `vllm-mlx`).
        engine: String,
        /// Path the model was loaded from, the key of the resident list.
        model_path: String,
        /// Why it was unloaded.
        reason: ModelUnloadReason,
    },
    /// Loading an LLM backend failed: backend skipped, runtime continues.
    LlmModelFailed {
        /// Logical backend name.
        backend: String,
        /// Reason for the failure (error message).
        reason: String,
    },
    /// An LLM call finished, emitted by `complete_with_observability()`.
    LlmCallCompleted {
        /// Logical name of the backend that handled the request.
        backend: String,
        /// Model identifier used (e.g. `"claude-sonnet-4-20250514"`).
        model: String,
        /// Identifier of the task that triggered the call (`None` outside a task context).
        task_id: Option<String>,
        /// Identifier of the ORIA step that triggered the call (`None` in direct mode).
        step_id: Option<String>,
        /// Number of tokens in the prompt.
        prompt_tokens: u32,
        /// Number of generated tokens.
        completion_tokens: u32,
        /// Total call latency in milliseconds.
        latency_ms: u64,
        /// Estimated cost in USD (cloud backends only; `None` for local inference).
        cost_usd: Option<f64>,
        /// Run this call belongs to, when emitted within a correlated run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
        /// Conversation the call was made in (`None` outside a conversation).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        /// Turn of that conversation: the identifier of the assistant message
        /// that answers it (`None` outside a conversation).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        /// Time to first token in milliseconds; `None` when not measured, as
        /// for a call answered in one piece.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttft_ms: Option<u64>,
        /// Why the call ended: `stop`, `tool_calls`, `length`, `error` or
        /// `cancelled`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        /// Time spent generating in milliseconds: the engine's decode time
        /// when it reports one, otherwise latency minus time to first token;
        /// `None` when neither is known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation_ms: Option<u64>,
    },

    /// A full LLM response captured for deterministic replay.
    ///
    /// Emitted once per LLM turn by the chat agent loop, where the run id and
    /// the complete (or partial, when interrupted) response are both available.
    /// The audit journal subscriber maps it to a chained `LlmCompletion` entry
    /// and assigns the per-run step ordinal. Unlike `LlmCallCompleted` (token
    /// metadata only), this carries the content and tool calls the replay
    /// harness needs to re-inject the response. The shared LLM router stays
    /// run-agnostic: capture happens in the agent loop, not the router.
    LlmResponseCaptured {
        /// Run this response belongs to.
        run_id: RunId,
        /// Best-effort name of the backend that produced the response.
        backend: String,
        /// Best-effort model identifier (empty when the streaming path omits it).
        model: String,
        /// Full response text, or the partial text received when the stream was cut.
        content: String,
        /// Tool calls returned by the model, serialized as JSON (empty when none
        /// or when the stream was truncated before any call).
        tool_calls: Vec<serde_json::Value>,
        /// Prompt token count when known, `0` otherwise.
        prompt_tokens: u32,
        /// Completion token count when known, `0` otherwise.
        completion_tokens: u32,
        /// Cost in USD when the backend reported it.
        cost_usd: Option<f64>,
        /// `true` when the stream was cut before a normal finish.
        stream_truncated: bool,
    },

    /// A tool output captured for deterministic replay.
    ///
    /// Emitted from the chat agent loop after a tool runs, carrying the full
    /// output (not the truncated preview). The journal subscriber maps it to a
    /// chained `ToolOutput` entry with a per-run step ordinal.
    ToolOutputCaptured {
        /// Run this tool call belongs to.
        run_id: RunId,
        /// Identifier of the originating tool call (matches the tool call id).
        tool_call_id: String,
        /// Name of the invoked tool.
        tool_name: String,
        /// Full tool output serialized as JSON.
        output: serde_json::Value,
        /// Outcome: `"success"`, `"error"`, or `"rejected"`.
        status: String,
    },

    // ── Plan / Step events ─────────────────────────────────────
    /// An `ExecutionPlan` was generated by the Reasoner and persisted to SQLite.
    PlanGenerated {
        /// Identifier of the task that triggered planning.
        task_id: TaskId,
        /// Name of the agent that owns the plan.
        agent_name: String,
        /// Unique plan identifier (UUID v4).
        plan_id: String,
        /// Number of steps in the plan.
        step_count: usize,
        /// Stable run identifier when this plan originates from a chat exchange.
        ///
        /// `None` on the orchestrated engine path, which correlates via `task_id`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },

    /// A step started executing, emitted by `ActorLoop` before each tool or LLM call.
    StepStarted {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Step identifier (e.g. `"s1"`).
        step_id: String,
        /// Sequential step number within the execution (1-based).
        step_num: usize,
        /// Total number of steps in the current plan.
        total: usize,
        /// Natural-language description of the step.
        desc: String,
    },

    /// A step completed successfully, emitted by `ActorLoop` after each successful call.
    StepCompleted {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Step identifier.
        step_id: String,
        /// Step execution duration in milliseconds.
        duration_ms: u64,
    },

    /// A step failed, emitted by `ActorLoop` after each failure.
    StepFailed {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Step identifier.
        step_id: String,
        /// Error message.
        error: String,
        /// `true` if the error can trigger a replan.
        retryable: bool,
    },

    /// A replan was triggered after a retryable step failed.
    PlanReplanning {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Replan attempt number (1-based).
        attempt: u32,
        /// Identifier of the step that failed and triggered the replan.
        failed_step: String,
        /// Reason the step failed.
        reason: String,
    },

    /// All steps completed successfully, plan finished.
    PlanCompleted {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Number of completed steps.
        step_count: usize,
        /// Total plan execution duration in milliseconds.
        duration_ms: u64,
    },

    /// The plan failed irrecoverably.
    PlanFailed {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// Plan identifier.
        plan_id: String,
        /// Reason for the failure.
        reason: String,
    },

    /// The post-run verification pass produced a verdict on an orchestrated run.
    ///
    /// Emitted by the ORIA engine after a completed orchestrated plan when the
    /// autonomy tier requests verification. Carries the aggregated verdict of the
    /// deterministic checks plus the optional LLM critic. Consumed by the audit
    /// journal so the verdict is traceable in the signed chain.
    VerificationCompleted {
        /// Identifier of the parent task.
        task_id: TaskId,
        /// True when every check passed and the critic proposed no correction.
        passed: bool,
        /// Number of failing check commands.
        check_failures: u32,
        /// Number of corrections proposed by the critic.
        corrections: u32,
        /// True when the critic pass was skipped (no backend, or routing error).
        skipped: bool,
        /// Number of verification-driven replans performed before this verdict.
        replans: u32,
    },

    /// A plan was generated and is awaiting an operator decision before execution.
    ///
    /// Emitted by the ORIA engine when the plan gate is active (plan-then-approve
    /// tiers). Consumers (CLI `run --plan`, Desktop plan review) display the plan
    /// and submit a decision via the task plan-decision API. If no decision arrives
    /// within `ttl_secs`, the gate closes and the run fails cleanly.
    PlanApprovalRequired {
        /// Run identifier correlating this gate to its originating run.
        run_id: String,
        /// The generated plan awaiting approval.
        plan_id: String,
        /// Task this plan belongs to.
        task_id: String,
        /// Number of steps in the plan.
        step_count: usize,
        /// The plan steps, so the reviewer can read them before deciding.
        #[serde(default)]
        steps: Vec<crate::plan::PlanStep>,
        /// Seconds before the gate closes when no decision is received.
        ttl_secs: u64,
    },

    /// A plan gate decision resolved to approval; the ActorLoop is starting.
    ///
    /// Emitted by the ORIA engine right after the gate unblocks on an approval,
    /// just before the `StepBudget` is created and execution begins.
    PlanApproved {
        /// Run identifier.
        run_id: String,
        /// Plan that was approved and will now execute.
        plan_id: String,
        /// Task this plan belongs to.
        task_id: String,
    },

    /// An operator rejected a plan; the engine will replan with the feedback.
    ///
    /// Emitted before the replanning attempt. Bounded by `plan_gate_max_replans`.
    PlanRejected {
        /// Run identifier.
        run_id: String,
        /// Identifier of the rejected plan.
        plan_id: String,
        /// Task this plan belongs to.
        task_id: String,
        /// Optional operator feedback injected into replanning.
        feedback: Option<String>,
        /// Number of replanifications already attempted for this run.
        replans_so_far: u32,
    },

    /// A run was abandoned after hitting the replan limit or a fatal replan error.
    PlanAbandoned {
        /// Run identifier.
        run_id: String,
        /// Task this plan belongs to.
        task_id: String,
        /// Machine-readable reason code.
        reason: String,
    },

    // ── HITL - Human-in-the-Loop events ────────────────────
    /// An `input_required` task expired, canceled automatically by the `TimeoutWatcher`.
    ///
    /// Emitted by `TimeoutWatcher::scan_and_cancel` for each task whose
    /// `input_required_at` exceeds `input_required_timeout`. Immediately
    /// followed by [`RuntimeEvent::TaskCanceled`] for the same task.
    TaskApprovalTimeout {
        /// Identifier of the expired task.
        task_id: TaskId,
        /// Configured timeout duration (in seconds).
        after_secs: u64,
    },

    /// A task is suspended awaiting human input.
    ///
    /// Emitted by ORIA once the suspension is detected.
    /// - **Direct mode**: emitted by `ORIAEngine::execute_direct()` when the
    ///   agent returns `AIPResult::input_required()`. `step_id` is `None`.
    /// - **Orchestrated mode**: emitted by `ActorLoop::suspend_for_approval()`
    ///   before executing a step whose tool is in `tools_requiring_approval`.
    ///   `step_id` is `Some(step.step_id)`.
    TaskInputRequired {
        /// Identifier of the suspended task.
        task_id: TaskId,
        /// Prompt shown to the user to make their decision.
        prompt: String,
        /// Identifier of the step awaiting approval (orchestrated mode only).
        ///
        /// `None` for direct-mode suspensions (the whole task is suspended).
        /// `Some(step_id)` for orchestrated-mode suspensions (a specific step).
        step_id: Option<String>,
    },

    /// An item of a batch waits for a decision on an access nobody granted.
    ///
    /// Its agent reached for that access during the run, which stopped.
    /// Emitted by the batch engine. The item waits for a decision rather than
    /// failing, and the batch goes on with its other items; once the access is
    /// granted, the item is run again from its input.
    BatchItemAwaitingApproval {
        /// The batch.
        batch_id: String,
        /// Position of the item in its source, from 0.
        rank: u64,
        /// Agent the batch runs.
        agent_id: AgentId,
        /// Access the agent reached for.
        access: String,
    },

    /// A batch ended, with its successes and failures.
    ///
    /// Every item ran, the batch was interrupted past its failure threshold,
    /// or it was stopped. Emitted by the batch engine once no item of the batch is running any
    /// more, with the counters as they stand then.
    BatchEnded {
        /// The batch.
        batch_id: String,
        /// Agent the batch runs.
        agent_id: AgentId,
        /// `completed`, `interrupted` or `stopped`.
        state: String,
        /// Items of the batch.
        total: u64,
        /// Items that succeeded.
        completed: u64,
        /// Items that failed.
        failed: u64,
        /// Items canceled, or never run because the batch was stopped.
        canceled: u64,
        /// Items waiting for an access to be granted.
        awaiting: u64,
    },

    /// A task resumed after a HITL suspension.
    ///
    /// Emitted by the `ResumeHandler` after persisting the human decision to
    /// SQLite and before relaunching ORIA, which subscribes to this event to
    /// relaunch `run()` on the agent.
    TaskResumed {
        /// Identifier of the resumed task.
        task_id: TaskId,
        /// `true` if the operator approved, `false` if rejected.
        approved: bool,
    },

    // ── Chat events ────────────────────────────────
    /// A chat session was created.
    ChatSessionCreated {
        /// Unique session identifier.
        session_id: String,
        /// Session mode (`"libre"` or `"agent"`).
        mode: String,
        /// Name of the associated agent (agent mode only).
        agent_name: Option<String>,
    },
    /// A chat session was closed.
    ChatSessionClosed {
        /// Identifier of the closed session.
        session_id: String,
    },
    /// A user message was sent in a session.
    ChatMessageSent {
        /// Session identifier.
        session_id: String,
        /// Unique message identifier.
        message_id: String,
    },
    /// The runtime began generating a response.
    ChatResponseStarted {
        /// Session identifier.
        session_id: String,
        /// Identifier of the response message.
        message_id: String,
        /// Stable run identifier for this exchange (one user turn, one response).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    /// A streaming token was produced by the LLM.
    ChatToken {
        /// Session identifier.
        session_id: String,
        /// Identifier of the in-progress response message.
        message_id: String,
        /// Text token produced.
        token: String,
    },
    /// The full response was generated.
    ChatResponseCompleted {
        /// Session identifier.
        session_id: String,
        /// Identifier of the response message.
        message_id: String,
        /// Full content of the response.
        content: String,
        /// Stable run identifier for this exchange (one user turn, one response).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    /// An error occurred in a chat session.
    ChatError {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message that caused the error (if applicable).
        message_id: Option<String>,
        /// Error description.
        error: String,
    },
    /// A tool call started in a chat session.
    ChatToolCallStarted {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message containing the tool call.
        message_id: String,
        /// Name of the invoked tool.
        tool_name: String,
        /// Truncated preview of the input arguments.
        input_preview: String,
        /// Meta-LLM narration explaining the call's intent (opt-in).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<ToolCallRationale>,
    },
    /// A tool call finished in a chat session.
    ChatToolCallCompleted {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message containing the tool call.
        message_id: String,
        /// Name of the invoked tool.
        tool_name: String,
        /// `true` if execution succeeded.
        success: bool,
        /// Truncated preview of the output (if available).
        output_preview: Option<String>,
        /// Structured error analysis: present only when `success = false` or
        /// when the detector flagged a hallucination despite apparent success.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        analysis: Option<crate::error_analysis::ErrorAnalysis>,
    },
    /// A tool call is being retried after a transient failure.
    ///
    /// Emitted by [`apollia_oria::resilience::ResilienceLayer::execute_with_observability`]
    /// before each new attempt. Lets the UI show a "Retry Nx" badge in real time
    /// on the relevant tool-call card.
    ToolCallRetrying {
        /// Logical identifier of the tool call (correlates successive attempts).
        tool_call_id: String,
        /// Name of the invoked tool.
        tool_name: String,
        /// New attempt number (1-based, `2` for the first retry).
        attempt: u32,
        /// Structured analysis of the error that triggered this retry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<crate::error_analysis::ErrorAnalysis>,
    },

    /// The LLM router switched to a secondary backend.
    ///
    /// Emitted by `LlmRouter::complete_with_fallback` when the primary backend
    /// fails non-recoverably and a secondary target is available. The UI shows a
    /// discreet banner.
    LlmFallbackTriggered {
        /// Name of the backend that failed.
        from_provider: String,
        /// Name of the backend that took over.
        to_provider: String,
        /// Human-readable reason (category label or short message).
        reason: String,
    },

    /// The model assigned to an agent is not available, so the call runs on the default model.
    ///
    /// Emitted by `LlmRouter::resolve_agent_model` at call time: the agent keeps
    /// working, and its sheet can say which model it asked for and which one
    /// answered.
    LlmAgentModelFallback {
        /// Agent whose call fell back.
        agent: String,
        /// Model the agent asked for: the user's assignment, or the backend its
        /// manifest declares.
        requested: String,
        /// Model that served the call instead.
        used: String,
    },

    /// A human approval is required for a tool call in the chat.
    ChatApprovalRequired {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message containing the tool call.
        message_id: String,
        /// Unique id of the tool call requiring approval. Correlates this
        /// request with its resolution/timeout and with the frontend card, so
        /// the same tool invoked twice in one turn never collides on
        /// `(message_id, tool_name)`.
        tool_call_id: String,
        /// Name of the tool requiring approval.
        tool_name: String,
        /// Prompt shown to the user: a short preview of the call, or the
        /// whole command or source for a tool that runs code.
        prompt: String,
        /// The answers offered, among `"once"`, `"always"` and `"refuse"`.
        /// Empty in a request that predates them.
        #[serde(default)]
        answers: Vec<String>,
        /// The full arguments of the call, as JSON, for the detail a short
        /// preview leaves out.
        #[serde(default)]
        detail: String,
        /// For a tool that runs code, the whole command or source, never
        /// truncated.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        /// For a tool that runs code, the folder it runs in.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },
    /// A tool-call approval was resolved by the user.
    ChatApprovalResolved {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message containing the tool call.
        message_id: String,
        /// Unique id of the resolved tool call (see
        /// [`Self::ChatApprovalRequired`]).
        tool_call_id: String,
        /// Name of the tool concerned.
        tool_name: String,
        /// Decision taken (`"accept"`, `"refuse"`, `"always_accept"`).
        decision: String,
    },
    /// A tool-call approval expired (timeout).
    ChatApprovalTimeout {
        /// Session identifier.
        session_id: String,
        /// Identifier of the message containing the tool call.
        message_id: String,
        /// Unique id of the timed-out tool call (see
        /// [`Self::ChatApprovalRequired`]).
        tool_call_id: String,
        /// Name of the tool concerned.
        tool_name: String,
    },

    // ── User Input events (ask_user tool) ─────────
    /// The agent requests information from the user via the `ask_user` tool.
    ChatUserInputRequired {
        /// Unique request identifier (to correlate the response).
        request_id: String,
        /// Chat session identifier.
        session_id: String,
        /// Identifier of the current message.
        message_id: String,
        /// Questions serialized as JSON (Vec<UserQuestion>).
        questions_json: String,
        /// Optional context explaining why the agent asks these questions.
        context: Option<String>,
    },
    /// The user answered the `ask_user` tool's questions.
    ChatUserInputResolved {
        /// Request identifier.
        request_id: String,
        /// Session identifier.
        session_id: String,
    },

    // ── Plan Cache events ────────────────────────
    /// A plan was retrieved from the cache instead of being generated by the Reasoner.
    PlanCacheHit {
        /// Identifier of the task that triggered the cache lookup.
        task_id: TaskId,
        /// SHA-256 cache key that produced the hit.
        cache_key: String,
    },

    // ── Agent Messaging events ────────────────────
    /// A message was sent between two agents via the AgentMailbox.
    ///
    /// `run_id` carries the run of the sending agent (or a synthetic host-scoped
    /// run for a host-injected message) so the audit subscriber can journal the
    /// event: entries without a `run_id` are skipped. `payload_hash` is the
    /// SHA-256 of the message payload, never the payload itself.
    AgentMessageSent {
        /// Name of the sending agent (or `host:<id>` for a host injection).
        from: String,
        /// Name of the receiving agent.
        to: String,
        /// Unique identifier of the message.
        message_id: String,
        /// Run that originated the send, when known (required to be journaled).
        run_id: Option<RunId>,
        /// SHA-256 hash of the payload (hex), for auditability without content.
        payload_hash: String,
        /// Full payload, present only when the runtime is configured to record
        /// message contents in the audit journal (regulated / high assurance).
        /// `None` otherwise, so the default path never carries content.
        full_payload: Option<serde_json::Value>,
    },
    /// A pending message was leased to its recipient (delivered on receive).
    AgentMessageDelivered {
        /// Name of the receiving agent.
        to: String,
        /// Unique identifier of the delivered message.
        message_id: String,
        /// Run of the receiving agent, when known.
        run_id: Option<RunId>,
    },
    /// A delivered message was acknowledged and removed from the store.
    AgentMessageAcked {
        /// Name of the receiving agent that acknowledged.
        to: String,
        /// Unique identifier of the acknowledged message.
        message_id: String,
        /// Run of the receiving agent, when known.
        run_id: Option<RunId>,
    },
    /// A message was dropped without being processed.
    ///
    /// `reason` is `"expired"` (past its TTL) or `"queue_full"` (rejected at
    /// send because the recipient queue was at capacity).
    AgentMessageDropped {
        /// Name of the intended recipient.
        to: String,
        /// Unique identifier of the dropped message, when one was assigned.
        message_id: String,
        /// Drop cause: `"expired"` or `"queue_full"`.
        reason: String,
        /// Run of the sender for a `queue_full` drop; `None` for a TTL eviction
        /// (which happens outside any run and is not journaled).
        run_id: Option<RunId>,
    },
    /// A mailbox safeguard blocked a send.
    ///
    /// Mirrors [`RuntimeEvent::A2AGuardTriggered`]. Emitted as soon as an
    /// automatic protection (per-run send quota, oversized payload) prevents a
    /// send from proceeding, just before the error is returned.
    MailboxGuardTriggered {
        /// Safeguard category: `"send_quota"` or `"payload_too_large"`.
        guard_type: String,
        /// Name of the agent (or `host:<id>`) whose send was blocked.
        caller: String,
        /// Explanatory message for logs and observability.
        detail: String,
    },

    // ── A2A Invocation events ─────────────────────
    /// An A2A invocation started, emitted by `A2AInvoker` before task submission.
    ///
    /// Emitted fire-and-forget before the call to the TaskRouter. Followed by
    /// [`RuntimeEvent::A2AInvocationCompleted`] after execution.
    A2AInvocationStarted {
        /// Name of the initiating agent (Director).
        caller: String,
        /// Name of the target agent (Worker).
        target: String,
        /// Identifier of the invoked skill.
        skill_id: String,
    },
    /// An A2A invocation finished, emitted after the result or a failure is received.
    ///
    /// `status` is `"completed"` on success or `"failed"` on error.
    A2AInvocationCompleted {
        /// Name of the initiating agent (Director).
        caller: String,
        /// Name of the target agent (Worker).
        target: String,
        /// Identifier of the invoked skill.
        skill_id: String,
        /// Final status: `"completed"` or `"failed"`.
        status: String,
        /// Total invocation duration in milliseconds.
        duration_ms: u64,
    },

    // ── A2A Guard events ─────────────────────────
    /// An A2A safeguard blocked an inter-agent invocation.
    ///
    /// Emitted by `A2AInvoker::invoke()` as soon as an automatic protection
    /// (max depth, self-invocation, cumulative chain timeout) prevents the
    /// invocation from continuing. The emission precedes returning the error.
    A2AGuardTriggered {
        /// Safeguard category: `"max_depth"`, `"self_invocation"` or `"chain_timeout"`.
        guard_type: String,
        /// Name of the agent that initiated the blocked invocation.
        caller: String,
        /// Identifier of the skill whose invocation was blocked.
        skill_id: String,
        /// Explanatory message for logs and observability.
        detail: String,
    },

    // ── A2A Skill telemetry ─────────
    /// An A2A skill was just invoked, emitted before the effective submission.
    ///
    /// Intended for per-skill telemetry aggregation and for feeding
    /// [`A2AStepProvenance`] in the global timeline.
    A2ASkillInvoked {
        /// Unique identifier of the step in the A2A chain.
        step_id: String,
        /// Identifier of the invoked skill.
        skill_id: String,
        /// Name of the target Worker agent.
        agent_name: String,
        /// Advertised version of the Worker.
        version: String,
        /// Input excerpt truncated to 240 characters.
        input_excerpt: String,
        /// Name of the initiating agent (caller).
        caller: String,
        /// Parent step in the A2A chain, `None` for the root.
        parent_step: Option<String>,
    },
    /// An A2A skill just finished, emitted after the result is received.
    A2ASkillCompleted {
        /// Step identifier correlated with [`RuntimeEvent::A2ASkillInvoked`].
        step_id: String,
        /// Identifier of the invoked skill.
        skill_id: String,
        /// Name of the target Worker agent.
        agent_name: String,
        /// Duration in milliseconds.
        duration_ms: u64,
        /// `true` if the invocation succeeded.
        success: bool,
        /// Tokens consumed by this invocation (0 if unknown).
        tokens_delta: u64,
        /// Output excerpt truncated to 240 characters. `None` on failure without output.
        output_excerpt: Option<String>,
    },
    /// Semver compatibility mismatch between the required and advertised version.
    A2ACompatibilityWarning {
        /// Identifier of the skill concerned.
        skill_id: String,
        /// Name of the target Worker.
        agent_name: String,
        /// Version required by the Director.
        required_version: String,
        /// Version advertised by the Worker.
        advertised_version: String,
        /// `"warning"` for a minor mismatch, `"incompatible"` for a major mismatch.
        severity: String,
        /// Human-readable message.
        message: String,
        /// Name of a compatible alternative Worker if detected.
        alternative_agent: Option<String>,
    },

    // ── Onboarding events ────────────────────────
    /// Emitted on first launch when the UserMemory is empty.
    ///
    /// The frontend intercepts this event via SSE to show the onboarding
    /// welcome screen. The runtime keeps running normally: this event is purely
    /// informational and blocks nothing.
    OnboardingRequired,

    /// Emitted when an onboarding session is triggered (full or partial).
    ///
    /// The frontend uses this event to navigate to the onboarding chat screen.
    /// `mode` is `"full"` for a complete onboarding or `"partial"` for a
    /// specific topic.
    OnboardingStarted {
        /// Identifier of the chat session created for onboarding.
        session_id: String,
        /// `"full"` or `"partial"`.
        mode: String,
        /// Targeted topic in partial mode; `None` in full mode.
        topic: Option<String>,
    },

    /// Emitted when the onboarding state machine reaches the `Done` phase.
    ///
    /// The frontend uses this event to permanently hide the resume banner and
    /// enable all application features.
    OnboardingCompleted {
        /// Total onboarding duration in seconds.
        duration_sec: u64,
        /// Total number of actions completed during the flow.
        actions_count: u32,
    },

    // ── STT events ───────────────────────────────────
    /// STT audio recording started (hotkey activated).
    ///
    /// Emitted by `SttFlow` when the user activates the recording hotkey. The
    /// frontend uses this event to show the recording overlay.
    SttRecordingStarted,

    /// STT audio recording stopped (hotkey released or silence detected).
    ///
    /// Emitted by `SttFlow` when recording ends, before transcription starts.
    /// `audio_duration_ms` is the duration of the captured audio.
    SttRecordingStopped {
        /// Duration of the recorded audio in milliseconds.
        audio_duration_ms: u64,
    },

    /// The STT model was loaded successfully, engine operational.
    ///
    /// Emitted by `SttEngine` after loading the GGML model in `spawn_blocking`.
    /// The frontend can use this event to indicate that STT is ready.
    SttModelLoaded {
        /// Name of the backend used (e.g. `"whisper-cpp"`).
        backend: String,
        /// Path of the loaded model file.
        model_path: String,
        /// Short model name (derived from the file name without extension).
        model_name: String,
    },

    /// An STT transcription completed successfully.
    ///
    /// Emitted by `SttEngine` after persisting to `SttRepository` and before
    /// responding to the caller. Lets the frontend refresh the transcription
    /// list and show a confirmation toast.
    SttTranscribed {
        /// Full transcribed text.
        text: String,
        /// Detected or used language (ISO 639-1 code).
        language: Option<String>,
        /// Source of the transcription (`"hotkey"`, `"file"`, `"api"`).
        source: String,
        /// Duration of the source audio in milliseconds.
        duration_ms: u64,
        /// Processing time in milliseconds.
        processing_time_ms: u64,
    },

    /// An error occurred during an STT transcription.
    ///
    /// Emitted by `SttEngine` when `SttBackend::transcribe()` fails. The
    /// frontend can show an error toast or notification.
    SttTranscriptionFailed {
        /// Error description.
        reason: String,
    },

    // ── Token Budget events ──────────────────────
    /// Session budget update, emitted after each LLM call.
    ///
    /// Emitted by `LlmRouter::complete_with_observability` after each backend
    /// call. The desktop widget listens to this event to show cost in real
    /// time. The emission is non-blocking (broadcast channel).
    TokenBudgetUpdated {
        /// Total session cost in USD since the last reset.
        session_cost_usd: f64,
        /// Input tokens accumulated since the last reset.
        total_input_tokens: u64,
        /// Output tokens accumulated since the last reset.
        total_output_tokens: u64,
        /// Cache-read tokens accumulated since the last reset.
        total_cache_read_tokens: u64,
        /// Cost threshold configured by the operator in USD. `f64::MAX` if unset.
        threshold_usd: f64,
        /// `true` if `session_cost_usd > threshold_usd`.
        threshold_exceeded: bool,
    },

    // ── Thinking / Reasoning transparency events ───
    /// Emitted at the start of the Reasoner phase, the agent begins "thinking".
    ///
    /// Lets the frontend show a streaming thinking indicator. `turn_id`
    /// correlates this event with `ThinkingEnded` and any summaries produced by
    /// `MetaRoutine::GenerateThinkingSummary`.
    ThinkingStarted {
        /// Conversation turn identifier (often the `task_id`).
        turn_id: String,
        /// Unix timestamp in milliseconds of the thinking phase start.
        ts_ms: u64,
    },
    /// Emitted at the end of the Reasoner phase, reasoning is complete.
    ///
    /// Carries the raw content produced by the LLM, the duration, and an
    /// estimate of the tokens consumed. The frontend can use it to show a
    /// "Thinking raw" panel and trigger the meta narration.
    ThinkingEnded {
        /// Conversation turn identifier (often the `task_id`).
        turn_id: String,
        /// Unix timestamp in milliseconds of the thinking phase end.
        ts_ms: u64,
        /// Total duration of the thinking phase in milliseconds.
        duration_ms: u64,
        /// Raw content produced by the LLM during the thinking phase.
        raw_content: String,
        /// Tokens consumed during the thinking phase (estimate).
        tokens: u32,
    },

    /// An LLM call failed, emitted by `complete_with_observability()` when the
    /// backend request returns an error (timeout, auth, quota, etc.).
    /// Carries an [`crate::error_analysis::ErrorAnalysis`] to humanize the error
    /// on the UI side.
    LlmCallFailed {
        /// Logical name of the backend that failed.
        backend: String,
        /// Identifier of the targeted model.
        model: String,
        /// Identifier of the task that triggered the call (`None` outside a task).
        task_id: Option<String>,
        /// Identifier of the ORIA step that triggered the call.
        step_id: Option<String>,
        /// Raw error message (for technical details).
        error: String,
        /// Structured analysis, always present.
        analysis: crate::error_analysis::ErrorAnalysis,
    },

    // ── Meta LLM Orchestrator events ───
    /// Emitted by `MetaLlmOrchestrator` when the tokens/session budget is exceeded.
    ///
    /// Lets the frontend hide the remaining transparency artifacts for this
    /// session and show a "budget exhausted" indicator.
    MetaLlmBudgetExceeded {
        /// Session identifier associated with the budget.
        session_id: String,
        /// Tokens consumed since the start of the session.
        tokens_used: u64,
        /// Configured budget (tokens/session, default 10_000).
        budget: u64,
    },

    // ── Context Manager events ───────────────────
    /// Emitted by `ContextManager` when the conversation history was compacted.
    ///
    /// Triggered in the ReAct loop of `BuiltInChatAgent` when accumulated
    /// messages exceed `context_compact_threshold` x the model window. The
    /// original system prompt (messages[0]) is always preserved.
    ContextCompacted {
        /// Number of characters in the generated summary.
        summary_chars: usize,
        /// Number of original messages replaced by the summary.
        original_messages: usize,
    },

    // ── Todo events ──────────────────────────────
    /// The session todo list changed after a successful `todo_write`.
    ///
    /// Emitted by the todo actor once items are persisted. Consumers (the
    /// desktop panel, future CLI views) refresh their todo display from this
    /// snapshot instead of polling the read route.
    TodoUpdated {
        /// Session whose todo list changed.
        session_id: String,
        /// Full snapshot of the todo list after the update.
        items: Vec<crate::todo::TodoItem>,
    },

    // ── Conversational plan-mode events ──────────
    /// A session plan was mutated (proposed, edited, reordered, or a step status changed).
    ///
    /// Emitted by the per-session plan actor after every successful plan
    /// mutation. Carries the full resulting plan plus the mutation that produced
    /// it, so the desktop can render the new state without re-fetching. Distinct
    /// from the run-keyed [`RuntimeEvent::PlanApproved`] / [`RuntimeEvent::PlanRejected`]
    /// orchestration-gate events: these are session-keyed and chat-native.
    PlanUpdated {
        /// Chat session the plan belongs to.
        session_id: String,
        /// Full plan after the mutation.
        ///
        /// Boxed: a [`crate::plan::Plan`] with its steps is the largest payload
        /// in this enum, so it is kept behind a pointer to keep every other
        /// [`RuntimeEvent`] variant cheap to move. Serde (de)serializes a
        /// `Box<T>` exactly like `T`, so the wire format is unchanged.
        plan: Box<crate::plan::Plan>,
        /// Mutation that produced this revision (boxed for the same reason).
        mutation: Box<crate::plan::PlanMutation>,
    },

    /// A session plan was submitted for approval.
    ///
    /// Emitted when a submit mutation is applied, in addition to the
    /// accompanying [`RuntimeEvent::PlanUpdated`]. Moves the conversational gate
    /// to the awaiting-approval phase.
    PlanSubmitted {
        /// Chat session the plan belongs to.
        session_id: String,
        /// Full submitted plan (boxed like [`RuntimeEvent::PlanUpdated`]).
        plan: Box<crate::plan::Plan>,
    },

    /// A submitted session plan was approved by the operator.
    ///
    /// Emitted by the chat-native approve path. The agent resumes execution.
    ChatPlanApproved {
        /// Chat session whose plan was approved.
        session_id: String,
    },

    /// A submitted session plan was rejected by the operator.
    ///
    /// Emitted by the chat-native reject path. The agent revises the plan.
    ChatPlanRejected {
        /// Chat session whose plan was rejected.
        session_id: String,
        /// Optional reason supplied by the operator.
        reason: Option<String>,
    },

    /// The plan-mode lifecycle phase of a session changed.
    ///
    /// Emitted by the chat ReAct loop as a turn moves through the conversational
    /// gate: into discovery on a substantive plan-mode turn, into drafting on the
    /// first plan mutation, and back to a safe state when discovery is cancelled.
    /// Carries the phase as a stable lowercase string (the runtime `PlanPhase`
    /// type is not visible from this crate). Distinct from the plan-content
    /// [`RuntimeEvent::PlanUpdated`] event: a phase can change before any plan
    /// exists (discovery precedes drafting), so the desktop tracks it separately.
    ChatPlanPhaseChanged {
        /// Chat session whose plan phase changed.
        session_id: String,
        /// New phase: `"discovery"`, `"drafting"`, `"awaiting_approval"`,
        /// `"executing"`, or `"done"`.
        phase: String,
    },

    // ── Hook decision events ─────────────────────
    /// A `PreToolUse` hook resolved a decision for a tool call.
    ///
    /// Emitted once per call after the registered `PreToolUse` handlers run,
    /// carrying the aggregate decision (`allow`, `deny`, or `rewrite`). The
    /// decision is applied best effort: a handler that times out, fails to
    /// deliver, or answers with something unparseable falls back to `allow`,
    /// and the tool call proceeds. The desktop accumulates these live to build
    /// the decision log; decisions are not persisted, so the log is scoped to
    /// the running session.
    HookDecisionRecorded {
        /// Run that issued the tool call.
        run_id: RunId,
        /// Session that issued the tool call.
        session_id: String,
        /// Tool the decision applies to.
        tool_name: String,
        /// Aggregate decision: `"allow"`, `"deny"`, or `"rewrite"`.
        decision: String,
        /// Replacement arguments as a JSON string, `Some` only for `"rewrite"`.
        rewritten_args: Option<String>,
    },

    // ── Cost ceiling events ──────────────────────
    /// The hybrid routing cost ceiling was reached while `HardStop` is active.
    ///
    /// The run is stopped cleanly before this event fires. The CLI and the
    /// desktop subscribe to surface the stop and the budget figures.
    CostCeilingReached {
        /// Session or run identifier.
        session_id: String,
        /// Accumulated session cost at the stop, in USD.
        cost_usd: f64,
        /// Configured ceiling, in USD.
        ceiling_usd: f64,
    },

    // ── File Path Extraction events ──────────────
    /// File paths extracted from a bash command's output.
    ///
    /// Emitted non-blocking by `FilePathExtractor::extract_detached` after each
    /// successful bash execution. Lets ORIA invalidate plan-cache entries for
    /// the affected files (one actor, one responsibility).
    BashFilePathsExtracted {
        /// Paths extracted from the bash command output.
        paths: Vec<std::path::PathBuf>,
    },

    // ── Permission events ────────────────────────
    /// A tool invocation requires a human approval.
    ///
    /// Emitted by the agent mailbox guard (`MailContext::emit_approval_required`)
    /// when an agent-to-agent send is gated. The frontend intercepts this event
    /// via SSE to show the appropriate HITL dialog.
    PermissionRequired {
        /// Name of the tool whose invocation is suspended.
        tool_name: String,
        /// Serialized JSON input of the invocation.
        input: serde_json::Value,
        /// Unique identifier of this approval request (UUID v4).
        request_id: String,
    },

    /// Governance started or stopped failing closed.
    ///
    /// Emitted by the governance actor when what it decides from can no longer
    /// be read (`governance.db` unreadable, or a deposited policy refused) and
    /// again when it can. While it fails closed, every sensitive tool call and
    /// every new connection is refused; reading continues.
    GovernanceFailClosed {
        /// `true` while governance fails closed, `false` once it recovered.
        active: bool,
        /// `store_unreadable` or `policy_invalid` while active, `None` once
        /// recovered.
        cause: Option<String>,
        /// What could not be read, `None` once recovered.
        detail: Option<String>,
    },

    // ── File Timestamp Cache events ──────────────────
    /// A previously read file was modified between two accesses.
    ///
    /// Emitted by `FileTimestampCache::record_read()` when the file's `mtime` on
    /// disk differs from the `mtime` recorded at the last access. ORIA
    /// invalidates plan-cache entries for this file.
    FileModifiedSinceRead {
        /// Absolute path of the modified file.
        path: std::path::PathBuf,
        /// `mtime` timestamp at the last access (Unix milliseconds).
        old_mtime_ms: i64,
        /// Current `mtime` timestamp (Unix milliseconds).
        new_mtime_ms: i64,
    },

    // ── Binary Feedback / Plan Alternatives events ──
    /// Two alternative plans were generated in parallel by the Reasoner.
    ///
    /// Emitted by `ORIAEngine::run_task_with_alternatives()` after the two plans
    /// (conservative and exploratory) have been produced via `tokio::join!`. The
    /// CLI and Desktop intercept this event to display both plans and ask the
    /// operator which one to execute.
    PlanAlternativesGenerated {
        /// The two alternative plans produced in parallel.
        alternatives: crate::plan_alternatives::PlanAlternatives,
    },

    // ── Decision branches ─────────────────────
    /// A significant decision point was captured with its alternatives.
    ///
    /// Emitted after the thinking phase when the meta routine
    /// `GenerateAlternativeBranches` extracted the chosen option plus the
    /// rejected paths. Opt-in: only if `routines.decision_branches` is enabled
    /// in [`crate::decision_point::DecisionPoint`].
    DecisionPointRecorded {
        /// The decision point, chosen plus alternatives (<= 3).
        point: crate::decision_point::DecisionPoint,
    },

    // ── Memory namespaces ─────────────────────
    /// An agent gained access to a shared memory namespace.
    ///
    /// Emitted by `apollia-aip::memory` when an agent is authorized to
    /// read/write in a shared namespace. Lets bus consumers (audit,
    /// observability) trace memory scope expansions without inspecting the
    /// configuration agent by agent.
    SharedNamespaceAdded {
        /// Identifier of the agent that gained access.
        agent_id: AgentId,
        /// Name of the shared namespace now accessible.
        namespace: String,
    },

    // ── Session metrics ───
    /// Aggregated session metrics updated.
    ///
    /// Emitted by `SessionMetricsActor` on each notable change: new LLM call,
    /// tool completion, summarization event, or crossing a budget threshold. The
    /// full payload is carried so the frontend can refresh the entire panel
    /// without re-querying the backend.
    SessionMetricsUpdated {
        /// Session identifier (chat session or task id).
        session_id: String,
        /// Full snapshot of the current metrics.
        metrics: crate::session_metrics::SessionMetrics,
        /// Current alert level based on the configured thresholds.
        alert: crate::session_metrics::BudgetAlertLevel,
    },

    // ── Observability - event-sourced runtime trace ─────────
    /// An agent emitted a message via `ctx.log(level, msg, **fields)`.
    ///
    /// What previously went only to `tracing::*` (stderr) is now persisted to
    /// `runtime_events.db` via `EventPersistor` and exposed to the UI via
    /// `GET /api/v1/tasks/{id}/trace`.
    ///
    /// This variant is *fire-and-forget*: do not block the agent thread if the
    /// bus is saturated.
    AgentLog {
        /// Task concerned. Lets the trace be filtered by task_id.
        task_id: TaskId,
        /// Agent that emitted the log.
        agent_id: AgentId,
        /// Standard level: `"debug" | "info" | "warn" | "error"`. Validated by
        /// `apollia-aip::context` before emission.
        level: String,
        /// Free-form message provided by the agent.
        message: String,
        /// Extra structured fields (Python kwargs serialized to JSON); `None` if
        /// the agent provided no structured fields.
        #[serde(default)]
        extra_fields_json: Option<String>,
    },

    /// An agent read the user profile through `ctx.profile`.
    ///
    /// Emitted by `apollia-aip::profile` on each read (`get`, `has`, `all`),
    /// only reachable when the profile source is checked for the agent.
    /// Persisted to `runtime_events.db` when the read belongs to a task, so it
    /// shows in that execution's trace.
    ProfileRead {
        /// Task the read belongs to; `None` outside a task.
        #[serde(default)]
        task_id: Option<TaskId>,
        /// Agent that read the profile.
        agent_id: AgentId,
        /// Identifier of the profile served.
        profile_id: String,
        /// When the read happened, RFC 3339 UTC.
        read_at: String,
    },

    // ── Observability - ReAct loop & enriched tools ───────────
    /// The LLM emitted a ReAct `thought` (reasoning chain).
    ///
    /// Captured in the Python SDK (`react.py`) on each turn, after the JSON
    /// parsing of the action. Shown in builder mode as a reasoning bubble,
    /// hidden in operator mode (unless the agent flags the `thought` as
    /// noteworthy).
    Thought {
        /// Current task.
        task_id: TaskId,
        /// Agent that produced the thought.
        agent_id: AgentId,
        /// ReAct turn number (1-based).
        step_num: u32,
        /// Raw text of the `thought` extracted from the LLM action JSON.
        text: String,
    },
    /// An LLM call is about to be sent (before `LlmProxy::complete`).
    ///
    /// `LlmCallCompleted` (existing) is still emitted afterward. Lets the UI
    /// detect LLM hangs (started without completed) and open a timer.
    ///
    /// The event carries no prompt text, only its size in characters. Prompt
    /// content is never persisted; the only way to see it is
    /// `[llm.observability] debug_log_prompt`, which logs it at `TRACE`.
    LlmCallStarted {
        /// Current task.
        task_id: TaskId,
        /// Calling agent.
        agent_id: AgentId,
        /// ORIA step if in plan mode, `None` in direct mode.
        step_id: Option<String>,
        /// Resolved backend: `"anthropic" | "openai" | "ollama" | ...`.
        backend: String,
        /// Resolved model (e.g. `claude-opus-4-7`).
        model: String,
        /// Total number of messages in the context (system+user+history).
        messages_count: u32,
        /// Cumulative prompt size in characters (token proxy).
        prompt_chars: u64,
        /// Run this call belongs to, when emitted within a correlated run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
        /// Fingerprint of the JSON Schema the answer was constrained to, when
        /// the call asked for structured output.
        ///
        /// `Some` means the output was constrained and says by which schema;
        /// `None` means free-form generation. The schema itself is never
        /// carried: it is caller data, it can be large, and the journal is an
        /// audit trail rather than a store. Two calls constrained by the same
        /// schema carry the same fingerprint, which is what makes the trail
        /// answer "was this run constrained the way that one was".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_schema_fingerprint: Option<String>,
    },
    /// A tool is about to be invoked (before the dispatcher).
    ///
    /// Forms a sandwich with `ToolCallCompleted` or `ToolCallDenied`; the
    /// `event_id` is exposed via the bus so the persistor can chain
    /// `parent_event_id`.
    ///
    /// The `args_json` payload is `None` if
    /// `[observability] capture_tool_args = false`.
    ToolCallStarted {
        /// Unique identifier of this call (UUID v7): becomes the
        /// `parent_event_id` of the matching `ToolCallCompleted`.
        event_id: String,
        /// Task.
        task_id: TaskId,
        /// Calling agent.
        agent_id: AgentId,
        /// Name of the dispatched tool (`web_search`, `file_write`, `a2a:*`...).
        tool_name: String,
        /// Call arguments serialized as JSON.
        args_json: Option<String>,
        /// Run this call belongs to, when emitted within a correlated run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    /// The tool finished its execution.
    ///
    /// `parent_event_id` ties this record to the matching `ToolCallStarted`.
    /// `output_json` is `None` if `capture_tool_outputs = false` or if the tool
    /// returned nothing.
    ToolCallCompleted {
        /// `event_id` of the `ToolCallStarted` this record closes.
        parent_event_id: String,
        /// Task.
        task_id: TaskId,
        /// Agent.
        agent_id: AgentId,
        /// Tool name (redundant with the started event, simplifies joins and UI
        /// renderers).
        tool_name: String,
        /// JSON-serialized output.
        output_json: Option<String>,
        /// Return code (bash/python). `None` for pure JSON tools.
        exit_code: Option<i32>,
        /// Total duration in milliseconds (dispatch + execution).
        duration_ms: u64,
        /// `true` if the tool returned a logical success.
        success: bool,
        /// Run this call belongs to, when emitted within a correlated run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    /// The tool was denied (manifest, permission rule, HITL).
    ///
    /// Emitted instead of `ToolCallCompleted` when the manifest allowlist, a
    /// permission rule or a human decision blocks the invocation. Valuable for explaining to the
    /// operator why an agent could not act.
    ToolCallDenied {
        /// `event_id` of the `ToolCallStarted` this record closes.
        parent_event_id: String,
        /// Task.
        task_id: TaskId,
        /// Agent.
        agent_id: AgentId,
        /// Attempted tool.
        tool_name: String,
        /// Normalized reason: `"not_in_manifest" | "permission_denied"
        /// | "hitl_rejected" | "circuit_open" | "other"`.
        reason: String,
        /// Readable message.
        detail: Option<String>,
    },
    /// An A2A (agent-to-agent) invocation starts.
    ///
    /// Emitted alongside a `ToolCallStarted` for `a2a:*` tools; opens the
    /// callee's sub-trace. The `correlation_id` shared across the full A2A chain
    /// lets the UI rebuild the tree.
    A2AInvokeStarted {
        /// `event_id` (UUID v7): becomes the `parent_event_id` of the records
        /// produced by the callee in its own trace.
        event_id: String,
        /// `correlation_id` shared across the whole A2A chain. Inherited from the
        /// parent invocation if one exists, otherwise newly emitted.
        correlation_id: String,
        /// Root task.
        task_id: TaskId,
        /// Calling agent.
        caller_agent_id: AgentId,
        /// Requested A2A skill (without the `a2a:` prefix).
        skill_id: String,
        /// `task_id` of the new task created for the callee.
        child_task_id: Option<TaskId>,
    },
    /// An A2A invocation finished.
    A2AInvokeCompleted {
        /// `event_id` of the `A2AInvokeStarted` this record closes.
        parent_event_id: String,
        /// Root task.
        task_id: TaskId,
        /// A2A skill.
        skill_id: String,
        /// `true` if the called agent succeeded.
        success: bool,
        /// Short output summary for list-view rendering (the detail is in the
        /// child_task_id sub-trace).
        output_summary: Option<String>,
        /// Total duration.
        duration_ms: u64,
    },
    /// The ReAct loop retries a step (parse error, tool failure, etc.).
    ///
    /// Emitted by the Python SDK (`react.py`) to signal that a turn was retried.
    /// Distinct from `PlanReplanning`, which applies to plan mode.
    Retry {
        /// Task.
        task_id: TaskId,
        /// Agent.
        agent_id: AgentId,
        /// ReAct turn number concerned.
        step_num: u32,
        /// Normalized cause: `"action_parse_error" | "tool_error"
        /// | "llm_error" | "other"`.
        cause: String,
        /// Attempt number (1 = first retry).
        attempt: u32,
    },
    /// The LLM returned an invalid action JSON that could not be repaired.
    ///
    /// Emitted by the Python SDK before the next attempt. Lets the builder see
    /// exactly what the LLM produced and adjust its system prompt.
    ActionParseError {
        /// Task.
        task_id: TaskId,
        /// Agent.
        agent_id: AgentId,
        /// ReAct turn number.
        step_num: u32,
        /// Raw content returned by the LLM.
        raw_content: String,
        /// `true` if a heuristic repair was attempted.
        repair_attempted: bool,
    },

    /// A declared step of a workflow run opened.
    WorkflowStepStarted {
        /// The run.
        run_id: String,
        /// The workflow, `<package>/<workflow>`.
        workflow_id: String,
        /// The step, as the workflow declares it.
        step: String,
    },
    /// A step of a workflow run ended without error.
    WorkflowStepCompleted {
        /// The run.
        run_id: String,
        /// The workflow, `<package>/<workflow>`.
        workflow_id: String,
        /// The step.
        step: String,
    },
    /// A step of a workflow run raised, or was stopped with its run.
    WorkflowStepFailed {
        /// The run.
        run_id: String,
        /// The workflow, `<package>/<workflow>`.
        workflow_id: String,
        /// The step.
        step: String,
        /// What went wrong.
        error: String,
    },
}

impl RuntimeEvent {
    /// Returns `true` if this event rearms the inactivity timer.
    ///
    /// Significant events cover task transitions, step and tool execution, LLM
    /// responses, and human approval requests.
    pub fn is_significant_for_inactivity(&self) -> bool {
        matches!(
            self,
            RuntimeEvent::TaskStarted { .. }
                | RuntimeEvent::StepCompleted { .. }
                | RuntimeEvent::LlmCallCompleted { .. }
                | RuntimeEvent::PermissionRequired { .. }
        )
    }
}
