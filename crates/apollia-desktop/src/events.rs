//! EventBus → Tauri event bridge.
//!
//! Subscribes to the `broadcast::Sender<RuntimeEvent>` and re-emits each
//! event as a Tauri application event (`"runtime-event"`).  The frontend
//! listens via `@tauri-apps/api/event::listen("runtime-event", …)`.

use apollia_core::events::RuntimeEvent;
use apollia_core::{subscribe_resilient, EventBusSender};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::token_coalescer::TokenCoalescer;

/// How long streamed tokens are allowed to accumulate before reaching the
/// webview.
///
/// One flush per interval instead of one per token, so the IPC and re-render
/// cost of a turn follows its wall-clock duration rather than its token count.
/// Roughly thirty frames a second, which is above the threshold where the eye
/// reads text as arriving continuously.
const TOKEN_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Payload emitted to the Svelte frontend via `app.emit("chat-token", …)`.
///
/// Dedicated fast-path for token streaming, avoids the generic `"runtime-event"`
/// envelope so the frontend can append tokens without a full IPC refresh.
#[derive(Debug, Clone, Serialize)]
pub struct ChatTokenPayload {
    /// Chat session that owns this token.
    pub session_id: String,
    /// Assistant message being streamed.
    pub message_id: String,
    /// Streamed token text.
    pub token: String,
}

/// Payload emitted to the Svelte frontend via `app.emit("hitl-fs-required", …)`.
///
/// Dedicated fast-path for filesystem HITL, allows the UI to display the diff
/// modal immediately without going through the generic `"runtime-event"` envelope.
#[derive(Debug, Clone, Serialize)]
pub struct HitlFsRequiredPayload {
    /// Unique request identifier, must be passed back to `respond_hitl_filesystem`.
    pub request_id: String,
    /// Chat session that triggered this request.
    pub session_id: String,
    /// Risk level string (e.g. `"medium"`, `"high"`, `"critical"`).
    pub level: String,
    /// Filesystem operation string (e.g. `"write"`, `"delete"`, `"chmod"`).
    pub op: String,
    /// Absolute path of the file being operated on.
    pub path: String,
    /// Preview payload: JSON value matching `FilesystemPreview` variants.
    pub preview: serde_json::Value,
}

/// Payload emitted to the Svelte frontend via `app.emit("hook-decision", …)`.
///
/// Dedicated fast-path for the PreToolUse decision log: the Builder hooks view
/// accumulates these live without persisting them or parsing the generic
/// `"runtime-event"` envelope.
#[derive(Debug, Clone, Serialize)]
pub struct HookDecisionPayload {
    /// Run that issued the tool call.
    pub run_id: String,
    /// Session that issued the tool call.
    pub session_id: String,
    /// Tool the decision applies to.
    pub tool_name: String,
    /// Aggregate decision: `"allow"`, `"deny"`, or `"rewrite"`.
    pub decision: String,
    /// Replacement arguments as a JSON string, present only for `"rewrite"`.
    pub rewritten_args: Option<String>,
}

/// Payload emitted to the Svelte frontend via `app.emit("runtime-event", …)`.
///
/// The `category` groups events by domain so the frontend can dispatch to the
/// correct store without parsing every variant:
/// - `agent-changed`
/// - `task-changed`
/// - `approval-changed`
/// - `llm-changed`
/// - `trigger-fired`
/// - `system`
#[derive(Debug, Clone, Serialize)]
pub struct TauriRuntimeEvent {
    /// Domain category for frontend dispatch.
    pub category: String,
    /// Discriminant name of the `RuntimeEvent` variant (e.g. `"AgentReady"`).
    pub event_type: String,
    /// Full event serialized as JSON value for type-safe consumption.
    pub payload: serde_json::Value,
}

/// Spawns a background Tokio task that emits `"runtime:heartbeat"` Tauri events
/// at a fixed cadence so the frontend `runtimeHealth` watchdog never marks the
/// runtime as disconnected during idle periods.
///
/// Without this, entering or switching chat sessions could trigger the
/// "Reconnexion au runtime" banner after 15s of inactivity even though the
/// bridge was perfectly alive; there was simply no `runtime-event` traffic.
pub fn spawn_heartbeat(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(e) = app.emit("runtime:heartbeat", ()) {
                tracing::debug!(error = %e, "failed to emit runtime:heartbeat");
            }
        }
    });
}

/// What the bridge has to do with one event received from the `EventBus`.
pub(crate) struct BridgeStep {
    /// Coalesced token chunks that must reach the webview first.
    pub tokens: Vec<ChatTokenPayload>,
    /// Whether the event itself is then forwarded. False for a streamed token,
    /// which the coalescer has absorbed.
    pub forward: bool,
}

/// Decide what one incoming event produces, given the tokens already buffered.
///
/// A token is absorbed and nothing is emitted. Anything else drains the buffer
/// first: the frontend derives state from the accumulated answer at the instant
/// a non-token event lands (the reasoning cursor attached to a starting tool
/// call, for one), so delivering a token after the event it preceded would
/// misplace it.
pub(crate) fn coalesce_step(coalescer: &mut TokenCoalescer, event: &RuntimeEvent) -> BridgeStep {
    if let RuntimeEvent::ChatToken {
        session_id,
        message_id,
        token,
    } = event
    {
        coalescer.push(session_id, message_id, token);
        return BridgeStep {
            tokens: Vec::new(),
            forward: false,
        };
    }

    BridgeStep {
        tokens: coalescer.drain(),
        forward: true,
    }
}

/// Spawns a background Tokio task that bridges `EventBus` → Tauri events.
///
/// The task runs for the lifetime of the application.  It terminates when the
/// broadcast channel is closed (runtime shutdown).
///
/// Streamed tokens are coalesced over [`TOKEN_FLUSH_INTERVAL`] instead of
/// crossing the IPC boundary one by one.
pub fn spawn_event_bridge(app: AppHandle, event_bus: EventBusSender) {
    let mut rx = subscribe_resilient(&event_bus, "desktop.event_bridge");
    tauri::async_runtime::spawn(async move {
        let mut coalescer = TokenCoalescer::new();
        let mut ticker = tokio::time::interval(TOKEN_FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                received = rx.recv() => match received {
                    Some(event) => {
                        let step = coalesce_step(&mut coalescer, &event);
                        emit_tokens(&app, step.tokens);
                        if step.forward {
                            bridge_one_event(&app, &event);
                        }
                    }
                    None => {
                        emit_tokens(&app, coalescer.drain());
                        break;
                    }
                },
                _ = ticker.tick() => {
                    if !coalescer.is_empty() {
                        emit_tokens(&app, coalescer.drain());
                    }
                }
            }
        }
    });
}

/// Deliver coalesced token chunks on the dedicated `"chat-token"` channel.
fn emit_tokens(app: &AppHandle, payloads: Vec<ChatTokenPayload>) {
    for payload in payloads {
        if let Err(e) = app.emit("chat-token", &payload) {
            tracing::warn!(error = %e, "failed to emit chat-token event");
        }
    }
}

/// Re-emit a single `RuntimeEvent` to the frontend.
///
/// Some events take a dedicated fast-path Tauri channel, and every channel
/// opened here has a listener in `ui/src`; all events are also emitted via the
/// generic `"runtime-event"` envelope. Streamed tokens never
/// reach this function: [`coalesce_step`] absorbs them into the dedicated
/// `"chat-token"` channel.
fn bridge_one_event(app: &AppHandle, event: &RuntimeEvent) {
    emit_hitl_fs_fastpath(app, event);
    emit_stt_fastpath(app, event);
    emit_hook_decision_fastpath(app, event);

    let tauri_event = map_runtime_event(event);
    if let Err(e) = app.emit("runtime-event", &tauri_event) {
        tracing::warn!(error = %e, "failed to emit Tauri event");
    }
}

/// Dedicated fast-path for filesystem HITL, emits "hitl-fs-required" so the
/// chat UI can open the diff modal without polling. Falls through afterwards so
/// the generic `"runtime-event"` is still emitted.
fn emit_hitl_fs_fastpath(app: &AppHandle, event: &RuntimeEvent) {
    if let RuntimeEvent::HitlFilesystemRequired {
        request_id,
        session_id,
        level,
        op,
        path,
        preview,
    } = event
    {
        let preview_value = serde_json::to_value(preview).unwrap_or(serde_json::Value::Null);
        let payload = HitlFsRequiredPayload {
            request_id: request_id.clone(),
            session_id: session_id.clone(),
            level: level.clone(),
            op: op.clone(),
            path: path.clone(),
            preview: preview_value,
        };
        if let Err(e) = app.emit("hitl-fs-required", &payload) {
            tracing::warn!(error = %e, "failed to emit hitl-fs-required event");
        }
    }
}

/// Dedicated fast-path events for STT overlay and latency-critical
/// transcription delivery.
fn emit_stt_fastpath(app: &AppHandle, event: &RuntimeEvent) {
    match event {
        RuntimeEvent::SttTranscribed { text, .. } => {
            if let Err(e) = app.emit("stt-transcribed", text) {
                tracing::warn!(error = %e, "failed to emit stt-transcribed event");
            }
        }
        RuntimeEvent::SttRecordingStarted => {
            if let Err(e) = app.emit("stt-recording-started", ()) {
                tracing::warn!(error = %e, "failed to emit stt-recording-started event");
            }
        }
        RuntimeEvent::SttRecordingStopped { .. } => {
            if let Err(e) = app.emit("stt-recording-stopped", ()) {
                tracing::warn!(error = %e, "failed to emit stt-recording-stopped event");
            }
        }
        _ => {}
    }
}

/// Dedicated fast-path for the PreToolUse decision log, emits "hook-decision"
/// so the Builder hooks view accumulates decisions live. Falls through so the
/// generic `"runtime-event"` is still emitted.
fn emit_hook_decision_fastpath(app: &AppHandle, event: &RuntimeEvent) {
    if let RuntimeEvent::HookDecisionRecorded {
        run_id,
        session_id,
        tool_name,
        decision,
        rewritten_args,
    } = event
    {
        let payload = HookDecisionPayload {
            run_id: run_id.as_str().to_string(),
            session_id: session_id.clone(),
            tool_name: tool_name.clone(),
            decision: decision.clone(),
            rewritten_args: rewritten_args.clone(),
        };
        if let Err(e) = app.emit("hook-decision", &payload) {
            tracing::warn!(error = %e, "failed to emit hook-decision event");
        }
    }
}

/// Maps a [`RuntimeEvent`] to a [`TauriRuntimeEvent`] with the correct category.
fn map_runtime_event(event: &RuntimeEvent) -> TauriRuntimeEvent {
    let category = categorize(event);
    let event_type = extract_variant_name(event);
    let payload = serde_json::to_value(event).unwrap_or(serde_json::Value::Null);
    TauriRuntimeEvent {
        category: category.to_string(),
        event_type: event_type.to_string(),
        payload,
    }
}

/// Returns the category string for a given [`RuntimeEvent`].
fn categorize(event: &RuntimeEvent) -> &'static str {
    match event {
        // ── Agent lifecycle ──────────────────────────────────────────────
        RuntimeEvent::AgentRegistered(_)
        | RuntimeEvent::AgentReady(_)
        | RuntimeEvent::AgentDegraded { .. }
        | RuntimeEvent::AgentStopping(_)
        | RuntimeEvent::AgentStopped(_)
        | RuntimeEvent::AgentLoadFailed { .. }
        | RuntimeEvent::AgentInstalled { .. }
        | RuntimeEvent::AgentUninstalled { .. }
        | RuntimeEvent::AgentEnabled { .. }
        | RuntimeEvent::AgentDisabled { .. } => "agent-changed",

        // ── Task lifecycle ───────────────────────────────────────────────
        RuntimeEvent::TaskStarted { .. }
        | RuntimeEvent::TaskCompleted { .. }
        | RuntimeEvent::TaskCanceled { .. }
        | RuntimeEvent::TodoUpdated { .. } => "task-changed",

        // ── Hook decisions ───────────────────────────────────────────────
        RuntimeEvent::HookDecisionRecorded { .. } => "hook-decision",

        // ── HITL / approvals ─────────────────────────────────────────────
        RuntimeEvent::TaskInputRequired { .. }
        | RuntimeEvent::TaskResumed { .. }
        | RuntimeEvent::TaskApprovalTimeout { .. }
        | RuntimeEvent::HitlFilesystemRequired { .. } => "approval-changed",

        // ── LLM ──────────────────────────────────────────────────────────
        RuntimeEvent::LlmModelLoading { .. }
        | RuntimeEvent::LlmModelReady { .. }
        | RuntimeEvent::LlmModelFailed { .. }
        | RuntimeEvent::LlmCallCompleted { .. }
        | RuntimeEvent::LlmResponseCaptured { .. }
        | RuntimeEvent::LlmCallFailed { .. }
        | RuntimeEvent::LlmFallbackTriggered { .. }
        | RuntimeEvent::TokenBudgetUpdated { .. }
        | RuntimeEvent::CostCeilingReached { .. } => "llm-changed",

        // ── Triggers ─────────────────────────────────────────────────────
        RuntimeEvent::TriggerFired { .. }
        | RuntimeEvent::TriggerSkipped { .. }
        | RuntimeEvent::TriggerError { .. }
        | RuntimeEvent::TriggerEnabled { .. }
        | RuntimeEvent::TriggerDisabled { .. }
        | RuntimeEvent::TriggersReloaded { .. } => "trigger-fired",

        // ── Plan-mode approval gate ──────────────────────────────────────
        RuntimeEvent::PlanApprovalRequired { .. }
        | RuntimeEvent::PlanApproved { .. }
        | RuntimeEvent::PlanRejected { .. }
        | RuntimeEvent::PlanAbandoned { .. } => "plan-approval",

        // ── Conversational plan-mode (session-keyed) ─────────────────────
        RuntimeEvent::PlanUpdated { .. }
        | RuntimeEvent::PlanSubmitted { .. }
        | RuntimeEvent::ChatPlanApproved { .. }
        | RuntimeEvent::ChatPlanRejected { .. }
        | RuntimeEvent::ChatPlanPhaseChanged { .. } => "plan-mode",

        // ── Plan / orchestration steps ───────────────────────────────────
        RuntimeEvent::PlanGenerated { .. }
        | RuntimeEvent::StepStarted { .. }
        | RuntimeEvent::StepCompleted { .. }
        | RuntimeEvent::StepFailed { .. }
        | RuntimeEvent::PlanReplanning { .. }
        | RuntimeEvent::PlanCompleted { .. }
        | RuntimeEvent::PlanFailed { .. }
        | RuntimeEvent::VerificationCompleted { .. }
        | RuntimeEvent::PlanCacheHit { .. } => "task-changed",

        // ── Circuit breaker ──────────────────────────────────────────────
        RuntimeEvent::ToolCircuitBroken { .. } | RuntimeEvent::ToolCircuitRestored { .. } => {
            "system"
        }

        // ── Chat ────────────────────────────────────────────────────────
        RuntimeEvent::ChatSessionCreated { .. }
        | RuntimeEvent::ChatSessionClosed { .. }
        | RuntimeEvent::ChatMessageSent { .. }
        | RuntimeEvent::ChatResponseStarted { .. }
        | RuntimeEvent::ChatResponseCompleted { .. }
        | RuntimeEvent::ChatError { .. }
        | RuntimeEvent::ChatToolCallStarted { .. }
        | RuntimeEvent::ChatToolCallCompleted { .. }
        | RuntimeEvent::ToolCallRetrying { .. }
        | RuntimeEvent::ChatApprovalRequired { .. }
        | RuntimeEvent::ChatApprovalResolved { .. }
        | RuntimeEvent::ChatApprovalTimeout { .. }
        | RuntimeEvent::ChatUserInputRequired { .. }
        | RuntimeEvent::ChatUserInputResolved { .. } => "chat-changed",

        // ChatToken uses a dedicated fast path, not "chat-changed", to avoid
        // triggering a full IPC refresh on every streamed token.
        RuntimeEvent::ChatToken { .. } => "chat-token",

        // ── Agent messaging ────────────────────────────────────────────
        RuntimeEvent::AgentMessageSent { .. }
        | RuntimeEvent::AgentMessageDelivered { .. }
        | RuntimeEvent::AgentMessageAcked { .. }
        | RuntimeEvent::AgentMessageDropped { .. }
        | RuntimeEvent::MailboxGuardTriggered { .. } => "agent-changed",

        // ── Onboarding ─────────────────────────────────────────────────────
        RuntimeEvent::OnboardingRequired
        | RuntimeEvent::OnboardingStarted { .. }
        | RuntimeEvent::OnboardingCompleted { .. } => "onboarding-changed",

        // ── STT ──────────────────────────────────────────────────────────
        RuntimeEvent::SttRecordingStarted
        | RuntimeEvent::SttRecordingStopped { .. }
        | RuntimeEvent::SttModelLoaded { .. }
        | RuntimeEvent::SttTranscribed { .. }
        | RuntimeEvent::SttTranscriptionFailed { .. } => "stt-changed",

        // ── A2A invocations ──────────────────────────────────────────────
        RuntimeEvent::A2AInvocationStarted { .. }
        | RuntimeEvent::A2AInvocationCompleted { .. }
        | RuntimeEvent::A2AGuardTriggered { .. }
        | RuntimeEvent::A2ASkillInvoked { .. }
        | RuntimeEvent::A2ASkillCompleted { .. }
        | RuntimeEvent::A2ACompatibilityWarning { .. } => "a2a",

        // ── File path extraction ─────────────────────────────────────────
        RuntimeEvent::BashFilePathsExtracted { .. } => "task-changed",

        // ── Permissions ──────────────────────────────────────────────────
        RuntimeEvent::PermissionRequired { .. } => "approval-changed",

        // ── Binary feedback / plan alternatives ───────────────────────────
        RuntimeEvent::PlanAlternativesGenerated { .. } => "task-changed",

        // ── Context manager ──────────────────────────────────────────────
        RuntimeEvent::ContextCompacted { .. } => "system",

        // ── Replay capture (internal, not surfaced in the UI) ─────────────
        RuntimeEvent::ToolOutputCaptured { .. } => "system",

        // ── System-level ─────────────────────────────────────────────────
        RuntimeEvent::AllReady | RuntimeEvent::ShutdownRequested => "system",

        // ── Triggers (extended) ───────────────────────────────────────────
        RuntimeEvent::TriggerQueueFull { .. } => "trigger-fired",

        // ── MCP ──────────────────────────────────────────────────────────
        RuntimeEvent::McpServerReloaded { .. } | RuntimeEvent::McpServerHealthChanged { .. } => {
            "system"
        }

        // ── Workspace / file read ─────────────────────────────────────────
        RuntimeEvent::FileModifiedSinceRead { .. } => "system",

        // ── Thinking / Reasoning transparency ───────────────
        RuntimeEvent::ThinkingStarted { .. } | RuntimeEvent::ThinkingEnded { .. } => "chat-changed",

        // ── Decision branches ───────────────────────────────
        RuntimeEvent::DecisionPointRecorded { .. } => "chat-changed",

        // ── Meta LLM Orchestrator ─────────────────────────────────────────
        RuntimeEvent::MetaLlmBudgetExceeded { .. } => "llm-changed",

        // ── Session metrics ─────────────────────────────────
        RuntimeEvent::SessionMetricsUpdated { .. } => "session-metrics",

        // ── Memory namespaces ───────────────────────────────
        RuntimeEvent::SharedNamespaceAdded { .. } => "memory-changed",

        // ── Observability: event-sourced runtime trace ────
        // Routed onto the front-side `trace-event` bus.
        RuntimeEvent::AgentLog { .. }
        | RuntimeEvent::Thought { .. }
        | RuntimeEvent::LlmCallStarted { .. }
        | RuntimeEvent::ToolCallStarted { .. }
        | RuntimeEvent::ToolCallCompleted { .. }
        | RuntimeEvent::ToolCallDenied { .. }
        | RuntimeEvent::A2AInvokeStarted { .. }
        | RuntimeEvent::A2AInvokeCompleted { .. }
        | RuntimeEvent::Retry { .. }
        | RuntimeEvent::ActionParseError { .. } => "trace-event",
    }
}

/// Extracts the variant name from a `RuntimeEvent` via its `Debug` representation.
///
/// Returns the first word before any `(` or `{` or space, e.g. `"AgentReady"`.
fn extract_variant_name(event: &RuntimeEvent) -> String {
    let debug = format!("{event:?}");
    debug
        .split(['(', '{', ' '])
        .next()
        .unwrap_or("Unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal session-scoped plan (boxed to match the [`RuntimeEvent`] shape).
    fn plan_mode_test_plan() -> Box<apollia_core::plan::Plan> {
        Box::new(apollia_core::plan::Plan {
            plan_id: "p-1".into(),
            scope: apollia_core::plan::PlanScope::Session("s".into()),
            revision: 0,
            status: apollia_core::plan::PlanStatus::Draft,
            steps: vec![apollia_core::plan::PlanStep::new("s1", "do the thing")],
        })
    }

    /// One streamed token for the coalescing tests.
    fn chat_token(session_id: &str, message_id: &str, token: &str) -> RuntimeEvent {
        RuntimeEvent::ChatToken {
            session_id: session_id.into(),
            message_id: message_id.into(),
            token: token.into(),
        }
    }

    /// A non-token chat event, the kind that has to observe every token that
    /// preceded it.
    fn tool_call_started(session_id: &str) -> RuntimeEvent {
        RuntimeEvent::ChatToolCallStarted {
            session_id: session_id.into(),
            message_id: "m1".into(),
            tool_name: "web_search".into(),
            input_preview: String::new(),
            rationale: None,
        }
    }

    #[test]
    fn test_a_streamed_token_is_absorbed_and_not_forwarded() {
        // GIVEN a fresh coalescer
        let mut coalescer = TokenCoalescer::new();

        // WHEN a streamed token arrives
        let step = coalesce_step(&mut coalescer, &chat_token("s1", "m1", "Bon"));

        // THEN nothing is emitted yet and the event is not bridged
        assert!(step.tokens.is_empty());
        assert!(!step.forward);
        assert!(!coalescer.is_empty());
    }

    #[test]
    fn test_pending_tokens_are_delivered_before_the_event_that_follows_them() {
        // GIVEN two tokens buffered but not yet flushed
        let mut coalescer = TokenCoalescer::new();
        let _ = coalesce_step(&mut coalescer, &chat_token("s1", "m1", "Bon"));
        let _ = coalesce_step(&mut coalescer, &chat_token("s1", "m1", "jour"));

        // WHEN a tool call starts
        let step = coalesce_step(&mut coalescer, &tool_call_started("s1"));

        // THEN the buffered text is emitted first, then the event is bridged,
        // so the frontend reads the tool call against the complete answer
        assert_eq!(step.tokens.len(), 1);
        assert_eq!(step.tokens[0].token, "Bonjour");
        assert!(step.forward);
        assert!(coalescer.is_empty());
    }

    #[test]
    fn test_a_non_token_event_with_nothing_buffered_emits_no_token() {
        // GIVEN a coalescer holding nothing
        let mut coalescer = TokenCoalescer::new();

        // WHEN a non-token event arrives
        let step = coalesce_step(&mut coalescer, &tool_call_started("s1"));

        // THEN it is bridged without a spurious empty token payload
        assert!(step.tokens.is_empty());
        assert!(step.forward);
    }

    /// A minimal Propose mutation (boxed to match the [`RuntimeEvent`] shape).
    fn plan_mode_test_mutation() -> Box<apollia_core::plan::PlanMutation> {
        Box::new(apollia_core::plan::PlanMutation {
            kind: apollia_core::plan::PlanMutationKind::Propose,
            step_id: None,
            reason: None,
            before: None,
            after: None,
            at: 0,
        })
    }

    #[test]
    fn test_map_runtime_event_agent_category() {
        // GIVEN an AgentReady event
        let event = RuntimeEvent::AgentReady("agent-1".into());
        // WHEN mapped to TauriRuntimeEvent
        let tauri_event = map_runtime_event(&event);
        // THEN category is "agent-changed"
        assert_eq!(tauri_event.category, "agent-changed");
        assert_eq!(tauri_event.event_type, "AgentReady");
    }

    #[test]
    fn test_map_runtime_event_task_category() {
        // GIVEN a TaskStarted event
        let event = RuntimeEvent::TaskStarted {
            agent_id: "a".into(),
            task_id: "t".into(),
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "task-changed"
        assert_eq!(tauri_event.category, "task-changed");
        assert_eq!(tauri_event.event_type, "TaskStarted");
    }

    #[test]
    fn test_map_runtime_event_approval_category() {
        // GIVEN a TaskInputRequired event
        let event = RuntimeEvent::TaskInputRequired {
            task_id: "t".into(),
            prompt: "confirm?".into(),
            step_id: None,
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "approval-changed"
        assert_eq!(tauri_event.category, "approval-changed");
    }

    #[test]
    fn test_map_runtime_event_llm_category() {
        // GIVEN a LlmModelReady event
        let event = RuntimeEvent::LlmModelReady {
            backend: "local".into(),
            model_id: "llama3".into(),
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "llm-changed"
        assert_eq!(tauri_event.category, "llm-changed");
    }

    #[test]
    fn test_map_runtime_event_trigger_category() {
        // GIVEN a TriggerFired event
        let event = RuntimeEvent::TriggerFired {
            trigger_id: "cron-1".into(),
            agent: "agent-1".into(),
            task_id: "t-1".into(),
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "trigger-fired"
        assert_eq!(tauri_event.category, "trigger-fired");
    }

    #[test]
    fn test_map_runtime_event_system_category() {
        // GIVEN an AllReady event
        let event = RuntimeEvent::AllReady;
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "system"
        assert_eq!(tauri_event.category, "system");
        assert_eq!(tauri_event.event_type, "AllReady");
    }

    #[test]
    fn test_map_runtime_event_plan_events_are_task_changed() {
        // GIVEN orchestration plan events
        let events = vec![
            RuntimeEvent::PlanGenerated {
                task_id: "t".into(),
                agent_name: "a".into(),
                plan_id: "p".into(),
                step_count: 2,
                run_id: None,
            },
            RuntimeEvent::StepStarted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_id: "s1".into(),
                step_num: 1,
                total: 2,
                desc: "do stuff".into(),
            },
            RuntimeEvent::StepCompleted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_id: "s1".into(),
                duration_ms: 100,
            },
            RuntimeEvent::PlanCompleted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_count: 2,
                duration_ms: 500,
            },
        ];
        // WHEN / THEN all are categorized as "task-changed"
        for event in &events {
            let mapped = map_runtime_event(event);
            assert_eq!(mapped.category, "task-changed", "failed for {:?}", event);
        }
    }

    #[test]
    fn test_map_all_event_categories_covered() {
        // GIVEN all RuntimeEvent variants (same list as apollia-core tests)
        let all_events: Vec<RuntimeEvent> = vec![
            RuntimeEvent::AgentRegistered("a".into()),
            RuntimeEvent::AgentReady("a".into()),
            RuntimeEvent::AgentDegraded {
                agent_id: "a".into(),
                reason: "r".into(),
            },
            RuntimeEvent::AgentStopping("a".into()),
            RuntimeEvent::AgentStopped("a".into()),
            RuntimeEvent::AgentLoadFailed {
                name: "broken".into(),
                error: "invalid".into(),
            },
            RuntimeEvent::AgentInstalled {
                name: "a".into(),
                version: "1.0.0".into(),
            },
            RuntimeEvent::AgentUninstalled { name: "a".into() },
            RuntimeEvent::AgentEnabled { name: "a".into() },
            RuntimeEvent::AgentDisabled { name: "a".into() },
            RuntimeEvent::TaskStarted {
                agent_id: "a".into(),
                task_id: "t".into(),
            },
            RuntimeEvent::TaskCompleted {
                agent_id: "a".into(),
                task_id: "t".into(),
                success: true,
                output: None,
            },
            RuntimeEvent::TaskCanceled {
                task_id: "t".into(),
            },
            RuntimeEvent::ToolCircuitBroken {
                tool_name: "x".into(),
            },
            RuntimeEvent::ToolCircuitRestored {
                tool_name: "x".into(),
            },
            RuntimeEvent::AllReady,
            RuntimeEvent::ShutdownRequested,
            RuntimeEvent::LlmModelLoading {
                backend: "b".into(),
                model_path: "p".into(),
            },
            RuntimeEvent::LlmModelReady {
                backend: "b".into(),
                model_id: "m".into(),
            },
            RuntimeEvent::LlmModelFailed {
                backend: "b".into(),
                reason: "r".into(),
            },
            RuntimeEvent::LlmCallCompleted {
                backend: "b".into(),
                model: "m".into(),
                task_id: None,
                step_id: None,
                prompt_tokens: 0,
                completion_tokens: 0,
                latency_ms: 0,
                cost_usd: None,
                run_id: None,
            },
            RuntimeEvent::TriggerFired {
                trigger_id: "t".into(),
                agent: "a".into(),
                task_id: "t".into(),
            },
            RuntimeEvent::TriggerSkipped {
                trigger_id: "t".into(),
                reason: "r".into(),
            },
            RuntimeEvent::TriggerError {
                trigger_id: "t".into(),
                error: "e".into(),
            },
            RuntimeEvent::TriggerEnabled {
                trigger_id: "t".into(),
            },
            RuntimeEvent::TriggerDisabled {
                trigger_id: "t".into(),
            },
            RuntimeEvent::TriggersReloaded { count: 0 },
            RuntimeEvent::PlanGenerated {
                task_id: "t".into(),
                agent_name: "a".into(),
                plan_id: "p".into(),
                step_count: 1,
                run_id: None,
            },
            RuntimeEvent::StepStarted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_id: "s".into(),
                step_num: 1,
                total: 1,
                desc: "d".into(),
            },
            RuntimeEvent::StepCompleted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_id: "s".into(),
                duration_ms: 0,
            },
            RuntimeEvent::StepFailed {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_id: "s".into(),
                error: "e".into(),
                retryable: false,
            },
            RuntimeEvent::PlanReplanning {
                task_id: "t".into(),
                plan_id: "p".into(),
                attempt: 1,
                failed_step: "s".into(),
                reason: "r".into(),
            },
            RuntimeEvent::PlanCompleted {
                task_id: "t".into(),
                plan_id: "p".into(),
                step_count: 1,
                duration_ms: 0,
            },
            RuntimeEvent::PlanFailed {
                task_id: "t".into(),
                plan_id: "p".into(),
                reason: "r".into(),
            },
            RuntimeEvent::TaskApprovalTimeout {
                task_id: "t".into(),
                after_secs: 60,
            },
            RuntimeEvent::TaskInputRequired {
                task_id: "t".into(),
                prompt: "p".into(),
                step_id: None,
            },
            RuntimeEvent::TaskResumed {
                task_id: "t".into(),
                approved: true,
            },
            // ── Chat events ────────────────────────────────────────────
            RuntimeEvent::ChatSessionCreated {
                session_id: "s".into(),
                mode: "libre".into(),
                agent_name: None,
            },
            RuntimeEvent::ChatSessionClosed {
                session_id: "s".into(),
            },
            RuntimeEvent::ChatMessageSent {
                session_id: "s".into(),
                message_id: "m".into(),
            },
            RuntimeEvent::ChatResponseStarted {
                session_id: "s".into(),
                message_id: "m".into(),
                run_id: None,
            },
            RuntimeEvent::ChatToken {
                session_id: "s".into(),
                message_id: "m".into(),
                token: "t".into(),
            },
            RuntimeEvent::ChatResponseCompleted {
                session_id: "s".into(),
                message_id: "m".into(),
                content: "c".into(),
                run_id: None,
            },
            RuntimeEvent::ChatError {
                session_id: "s".into(),
                message_id: None,
                error: "e".into(),
            },
            RuntimeEvent::ChatToolCallStarted {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_name: "t".into(),
                input_preview: "i".into(),
                rationale: None,
            },
            RuntimeEvent::ChatToolCallCompleted {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_name: "t".into(),
                success: true,
                output_preview: None,
                analysis: None,
            },
            RuntimeEvent::ChatApprovalRequired {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_call_id: "c".into(),
                tool_name: "t".into(),
                prompt: "p".into(),
            },
            RuntimeEvent::ChatApprovalResolved {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_call_id: "c".into(),
                tool_name: "t".into(),
                decision: "accept".into(),
            },
            RuntimeEvent::ChatApprovalTimeout {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_call_id: "c".into(),
                tool_name: "t".into(),
            },
            RuntimeEvent::PlanCacheHit {
                task_id: "t".into(),
                cache_key: "k".into(),
            },
            // ── STT events ─────────────────────────────────────────
            RuntimeEvent::SttRecordingStarted,
            RuntimeEvent::SttRecordingStopped {
                audio_duration_ms: 1500,
            },
            RuntimeEvent::SttModelLoaded {
                backend: "whisper-cpp".into(),
                model_path: "/tmp/model.bin".into(),
                model_name: "model".into(),
            },
            RuntimeEvent::SttTranscribed {
                text: "hello".into(),
                language: Some("en".into()),
                source: "hotkey".into(),
                duration_ms: 1000,
                processing_time_ms: 200,
            },
            RuntimeEvent::SttTranscriptionFailed {
                reason: "error".into(),
            },
            RuntimeEvent::HookDecisionRecorded {
                run_id: apollia_core::events::RunId::new(),
                session_id: "s".into(),
                tool_name: "bash_executor".into(),
                decision: "deny".into(),
                rewritten_args: None,
            },
            RuntimeEvent::PlanUpdated {
                session_id: "s".into(),
                plan: plan_mode_test_plan(),
                mutation: plan_mode_test_mutation(),
            },
            RuntimeEvent::PlanSubmitted {
                session_id: "s".into(),
                plan: plan_mode_test_plan(),
            },
            RuntimeEvent::ChatPlanApproved {
                session_id: "s".into(),
            },
            RuntimeEvent::ChatPlanRejected {
                session_id: "s".into(),
                reason: None,
            },
        ];

        let valid_categories = [
            "agent-changed",
            "task-changed",
            "approval-changed",
            "llm-changed",
            "trigger-fired",
            "chat-changed",
            "chat-token",
            "onboarding-required",
            "stt-changed",
            "hook-decision",
            "plan-mode",
            "system",
        ];

        // WHEN / THEN every variant maps to a known category
        for event in &all_events {
            let mapped = map_runtime_event(event);
            assert!(
                valid_categories.contains(&mapped.category.as_str()),
                "unknown category '{}' for event {:?}",
                mapped.category,
                event,
            );
            assert!(
                !mapped.event_type.is_empty(),
                "empty event_type for {:?}",
                event
            );
            assert!(!mapped.payload.is_null(), "null payload for {:?}", event);
        }
    }

    #[test]
    fn test_categorize_chat_events_are_chat_changed() {
        // GIVEN chat lifecycle events (not ChatToken)
        let events = vec![
            RuntimeEvent::ChatSessionCreated {
                session_id: "s".into(),
                mode: "libre".into(),
                agent_name: None,
            },
            RuntimeEvent::ChatSessionClosed {
                session_id: "s".into(),
            },
            RuntimeEvent::ChatMessageSent {
                session_id: "s".into(),
                message_id: "m".into(),
            },
            RuntimeEvent::ChatResponseCompleted {
                session_id: "s".into(),
                message_id: "m".into(),
                content: "c".into(),
                run_id: None,
            },
            RuntimeEvent::ChatError {
                session_id: "s".into(),
                message_id: None,
                error: "e".into(),
            },
        ];
        // WHEN / THEN all are categorized as "chat-changed"
        for event in &events {
            let mapped = map_runtime_event(event);
            assert_eq!(
                mapped.category, "chat-changed",
                "expected chat-changed for {:?}",
                event
            );
        }
    }

    #[test]
    fn test_categorize_chat_token_is_chat_token() {
        // GIVEN a ChatToken event
        let event = RuntimeEvent::ChatToken {
            session_id: "sess-1".into(),
            message_id: "msg-1".into(),
            token: "Hello".into(),
        };
        // WHEN categorized
        let mapped = map_runtime_event(&event);
        // THEN category is "chat-token" (not "chat-changed")
        assert_eq!(mapped.category, "chat-token");
        assert_eq!(mapped.event_type, "ChatToken");
    }

    #[test]
    fn test_chat_token_payload_serialization() {
        // GIVEN a ChatTokenPayload
        let payload = ChatTokenPayload {
            session_id: "sess-42".into(),
            message_id: "msg-7".into(),
            token: "world".into(),
        };
        // WHEN serialized
        let json = serde_json::to_value(&payload).expect("serialize");
        // THEN all fields are present
        assert_eq!(json["session_id"], "sess-42");
        assert_eq!(json["message_id"], "msg-7");
        assert_eq!(json["token"], "world");
    }

    #[test]
    fn test_hook_decision_payload_serialization() {
        // GIVEN a HookDecisionPayload for a rewrite decision
        let payload = HookDecisionPayload {
            run_id: "run-3".into(),
            session_id: "sess-3".into(),
            tool_name: "bash_executor".into(),
            decision: "rewrite".into(),
            rewritten_args: Some("{\"cmd\":\"ls\"}".into()),
        };

        // WHEN serialized for the "hook-decision" Tauri event
        let json = serde_json::to_value(&payload).expect("serialize");

        // THEN the decision, tool and rewritten args are present
        assert_eq!(json["decision"], "rewrite");
        assert_eq!(json["tool_name"], "bash_executor");
        assert_eq!(json["rewritten_args"], "{\"cmd\":\"ls\"}");
    }

    #[test]
    fn test_hook_decision_recorded_is_hook_decision_category() {
        // GIVEN a HookDecisionRecorded event
        let event = RuntimeEvent::HookDecisionRecorded {
            run_id: apollia_core::events::RunId::new(),
            session_id: "s".into(),
            tool_name: "bash_executor".into(),
            decision: "allow".into(),
            rewritten_args: None,
        };

        // WHEN categorized
        let mapped = map_runtime_event(&event);

        // THEN it lands in the dedicated hook-decision category
        assert_eq!(mapped.category, "hook-decision");
        assert_eq!(mapped.event_type, "HookDecisionRecorded");
    }

    #[test]
    fn test_todo_updated_travels_under_task_changed() {
        // GIVEN a todo snapshot for a session
        let event = RuntimeEvent::TodoUpdated {
            session_id: "sess-9".into(),
            items: vec![apollia_core::todo::TodoItem {
                id: "t1".into(),
                content: "analyse".into(),
                status: apollia_core::todo::TodoStatus::InProgress,
                depends_on: vec![],
            }],
        };

        // WHEN mapped for the generic envelope
        let mapped = map_runtime_event(&event);

        // THEN it reaches the webview under task-changed, with its snapshot
        assert_eq!(mapped.category, "task-changed");
        assert_eq!(mapped.event_type, "TodoUpdated");
        assert_eq!(mapped.payload["TodoUpdated"]["items"][0]["id"], "t1");
    }

    #[test]
    fn test_payload_contains_event_data() {
        // GIVEN an event with identifiable data
        let event = RuntimeEvent::AgentDegraded {
            agent_id: "my-agent-42".into(),
            reason: "tool missing".into(),
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN the payload contains the original data
        let json = serde_json::to_string(&tauri_event.payload).unwrap_or_default();
        assert!(json.contains("my-agent-42"));
        assert!(json.contains("tool missing"));
    }

    #[test]
    fn test_stt_events_are_stt_changed() {
        // GIVEN STT lifecycle events
        let events = vec![
            RuntimeEvent::SttRecordingStarted,
            RuntimeEvent::SttRecordingStopped {
                audio_duration_ms: 2000,
            },
            RuntimeEvent::SttModelLoaded {
                backend: "whisper-cpp".into(),
                model_path: "/tmp/m.bin".into(),
                model_name: "m".into(),
            },
            RuntimeEvent::SttTranscribed {
                text: "bonjour".into(),
                language: Some("fr".into()),
                source: "hotkey".into(),
                duration_ms: 1500,
                processing_time_ms: 300,
            },
            RuntimeEvent::SttTranscriptionFailed {
                reason: "oops".into(),
            },
        ];
        // WHEN / THEN all map to "stt-changed"
        for event in &events {
            let mapped = map_runtime_event(event);
            assert_eq!(
                mapped.category, "stt-changed",
                "expected stt-changed for {:?}",
                event
            );
        }
    }

    #[test]
    fn test_plan_updated_categorized_as_plan_mode() {
        // GIVEN a session-keyed PlanUpdated carrying a plan and its mutation
        let event = RuntimeEvent::PlanUpdated {
            session_id: "s-1".into(),
            plan: plan_mode_test_plan(),
            mutation: plan_mode_test_mutation(),
        };
        // WHEN categorized
        let mapped = map_runtime_event(&event);
        // THEN it lands in the plan-mode category
        assert_eq!(mapped.category, "plan-mode");
        assert_eq!(mapped.event_type, "PlanUpdated");
    }

    #[test]
    fn test_all_four_plan_mode_events_map_to_plan_mode() {
        // GIVEN the four session-keyed plan-mode variants
        let events = vec![
            RuntimeEvent::PlanUpdated {
                session_id: "s-1".into(),
                plan: plan_mode_test_plan(),
                mutation: plan_mode_test_mutation(),
            },
            RuntimeEvent::PlanSubmitted {
                session_id: "s-1".into(),
                plan: plan_mode_test_plan(),
            },
            RuntimeEvent::ChatPlanApproved {
                session_id: "s-1".into(),
            },
            RuntimeEvent::ChatPlanRejected {
                session_id: "s-1".into(),
                reason: Some("too risky".into()),
            },
        ];
        // WHEN / THEN every variant maps to "plan-mode"
        for event in &events {
            assert_eq!(
                categorize(event),
                "plan-mode",
                "expected plan-mode for {event:?}"
            );
        }
    }

    #[test]
    fn test_plan_mode_event_for_closed_session_still_categorized() {
        // GIVEN a PlanUpdated for a session id no longer open on the frontend
        let event = RuntimeEvent::PlanUpdated {
            session_id: "ghost-session".into(),
            plan: plan_mode_test_plan(),
            mutation: plan_mode_test_mutation(),
        };
        // WHEN the desktop bridge categorizes it
        let mapped = map_runtime_event(&event);
        // THEN it is classed plan-mode without panic and carries a payload
        // (dropping it is the frontend consumer's responsibility)
        assert_eq!(mapped.category, "plan-mode");
        assert!(!mapped.payload.is_null());
    }

    #[test]
    fn test_stt_transcribed_payload_contains_text() {
        // GIVEN a SttTranscribed event
        let event = RuntimeEvent::SttTranscribed {
            text: "Bonjour le monde".into(),
            language: Some("fr".into()),
            source: "hotkey".into(),
            duration_ms: 2000,
            processing_time_ms: 300,
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN the payload contains the transcription text
        let json = serde_json::to_string(&tauri_event.payload).unwrap_or_default();
        assert!(json.contains("Bonjour le monde"));
    }
}
