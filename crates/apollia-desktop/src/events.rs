//! EventBus → Tauri event bridge.
//!
//! Subscribes to the `broadcast::Sender<RuntimeEvent>` and re-emits each
//! event as a Tauri application event (`"runtime-event"`).  The frontend
//! listens via `@tauri-apps/api/event::listen("runtime-event", …)`.

use apollia_core::events::RuntimeEvent;
use apollia_core::EventBusSender;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Payload emitted to the Svelte frontend via `app.emit("chat-token", …)`.
///
/// Dedicated fast-path for token streaming — avoids the generic `"runtime-event"`
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
/// Dedicated fast-path for filesystem HITL — allows the UI to display the diff
/// modal immediately without going through the generic `"runtime-event"` envelope.
#[derive(Debug, Clone, Serialize)]
pub struct HitlFsRequiredPayload {
    /// Unique request identifier — must be passed back to `respond_hitl_filesystem`.
    pub request_id: String,
    /// Chat session that triggered this request.
    pub session_id: String,
    /// Risk level string (e.g. `"medium"`, `"high"`, `"critical"`).
    pub level: String,
    /// Filesystem operation string (e.g. `"write"`, `"delete"`, `"chmod"`).
    pub op: String,
    /// Absolute path of the file being operated on.
    pub path: String,
    /// Preview payload — JSON value matching `FilesystemPreview` variants.
    pub preview: serde_json::Value,
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
/// - `pipeline-changed`
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
/// bridge was perfectly alive — there was simply no `runtime-event` traffic.
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

/// Spawns a background Tokio task that bridges `EventBus` → Tauri events.
///
/// The task runs for the lifetime of the application.  It terminates when the
/// broadcast channel is closed (runtime shutdown).
pub fn spawn_event_bridge(app: AppHandle, event_bus: EventBusSender) {
    let mut rx = event_bus.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // ChatToken uses a dedicated "chat-token" Tauri event for
                    // real-time streaming without triggering a full IPC refresh.
                    if let RuntimeEvent::ChatToken {
                        ref session_id,
                        ref message_id,
                        ref token,
                    } = event
                    {
                        let payload = ChatTokenPayload {
                            session_id: session_id.clone(),
                            message_id: message_id.clone(),
                            token: token.clone(),
                        };
                        if let Err(e) = app.emit("chat-token", &payload) {
                            tracing::warn!(error = %e, "failed to emit chat-token event");
                        }
                        continue;
                    }

                    // Dedicated fast-path for filesystem HITL — emits "hitl-fs-required"
                    // so the chat UI can open the diff modal without polling.
                    if let RuntimeEvent::HitlFilesystemRequired {
                        ref request_id,
                        ref session_id,
                        ref level,
                        ref op,
                        ref path,
                        ref preview,
                    } = event
                    {
                        let preview_value =
                            serde_json::to_value(preview).unwrap_or(serde_json::Value::Null);
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
                        // fall through — also emit via generic runtime-event
                    }

                    // Dedicated fast-path events for STT overlay and
                    // latency-critical transcription delivery.
                    match &event {
                        RuntimeEvent::SttTranscribed { ref text, .. } => {
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

                    let tauri_event = map_runtime_event(&event);
                    if let Err(e) = app.emit("runtime-event", &tauri_event) {
                        tracing::warn!(error = %e, "failed to emit Tauri event");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "event bridge lagged, events dropped");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("EventBus closed, stopping event bridge");
                    break;
                }
            }
        }
    });
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
        | RuntimeEvent::StepExecuted { .. } => "task-changed",

        // ── HITL / approvals ─────────────────────────────────────────────
        RuntimeEvent::TaskInputRequired { .. }
        | RuntimeEvent::TaskResumed { .. }
        | RuntimeEvent::TaskApprovalTimeout { .. }
        | RuntimeEvent::HitlFilesystemRequired { .. }
        | RuntimeEvent::HitlRejected { .. } => "approval-changed",

        // ── LLM ──────────────────────────────────────────────────────────
        RuntimeEvent::LlmModelLoading { .. }
        | RuntimeEvent::LlmModelReady { .. }
        | RuntimeEvent::LlmModelFailed { .. }
        | RuntimeEvent::LlmCallCompleted { .. }
        | RuntimeEvent::LlmCallFailed { .. }
        | RuntimeEvent::LlmFallbackTriggered { .. }
        | RuntimeEvent::TokenBudgetUpdated { .. } => "llm-changed",

        // ── Triggers ─────────────────────────────────────────────────────
        RuntimeEvent::TriggerFired { .. }
        | RuntimeEvent::TriggerSkipped { .. }
        | RuntimeEvent::TriggerError { .. }
        | RuntimeEvent::TriggerEnabled { .. }
        | RuntimeEvent::TriggerDisabled { .. }
        | RuntimeEvent::TriggersReloaded { .. } => "trigger-fired",

        // ── Pipelines ────────────────────────────────────────────────────
        RuntimeEvent::PipelineStarted { .. }
        | RuntimeEvent::PipelineStepStarted { .. }
        | RuntimeEvent::PipelineStepCompleted { .. }
        | RuntimeEvent::PipelineStepFailed { .. }
        | RuntimeEvent::PipelineStepSkipped { .. }
        | RuntimeEvent::PipelineSuspended { .. }
        | RuntimeEvent::PipelineResumed { .. }
        | RuntimeEvent::PipelineCompleted { .. }
        | RuntimeEvent::PipelineFailed { .. } => "pipeline-changed",

        // ── Plan / orchestration steps ───────────────────────────────────
        RuntimeEvent::PlanGenerated { .. }
        | RuntimeEvent::StepStarted { .. }
        | RuntimeEvent::StepCompleted { .. }
        | RuntimeEvent::StepFailed { .. }
        | RuntimeEvent::PlanReplanning { .. }
        | RuntimeEvent::PlanCompleted { .. }
        | RuntimeEvent::PlanFailed { .. }
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

        // ChatToken uses a dedicated fast path — not "chat-changed" to avoid
        // triggering a full IPC refresh on every streamed token.
        RuntimeEvent::ChatToken { .. } => "chat-token",

        // ── Agent messaging ────────────────────────────────────────────
        RuntimeEvent::AgentMessageSent { .. } => "agent-changed",

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
        RuntimeEvent::PlanAlternativesGenerated { .. } | RuntimeEvent::PlanChosen { .. } => {
            "task-changed"
        }

        // ── Context manager ──────────────────────────────────────────────
        RuntimeEvent::ContextCompacted { .. } => "system",

        // ── System-level ─────────────────────────────────────────────────
        RuntimeEvent::AllReady | RuntimeEvent::ShutdownRequested | RuntimeEvent::FatalError(_) => {
            "system"
        }

        // ── Triggers (extended) ───────────────────────────────────────────
        RuntimeEvent::TriggerQueueFull { .. } => "trigger-fired",

        // ── MCP ──────────────────────────────────────────────────────────
        RuntimeEvent::McpServerReloaded { .. } => "system",

        // ── Workspace / file read ─────────────────────────────────────────
        RuntimeEvent::FileModifiedSinceRead { .. } => "system",

        // ── Thinking / Reasoning transparency ───────────────
        RuntimeEvent::ThinkingStarted { .. } | RuntimeEvent::ThinkingEnded { .. } => "chat-changed",

        // ── Decision branches ───────────────────────────────
        RuntimeEvent::DecisionPointRecorded { .. } => "chat-changed",

        // ── Meta LLM Orchestrator (ADR-073) ───────────────────────────────
        RuntimeEvent::MetaLlmBudgetExceeded { .. } => "llm-changed",

        // ── Session metrics ─────────────────────────────────
        RuntimeEvent::SessionMetricsUpdated { .. } => "session-metrics",

        // ── Memory namespaces ───────────────────────────────
        RuntimeEvent::SharedNamespaceAdded { .. } => "memory-changed",
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
    fn test_map_runtime_event_pipeline_category() {
        // GIVEN a PipelineStarted event
        let event = RuntimeEvent::PipelineStarted {
            run_id: "r-1".into(),
            pipeline_id: "p-1".into(),
            trigger_id: None,
            step_count: 3,
        };
        // WHEN mapped
        let tauri_event = map_runtime_event(&event);
        // THEN category is "pipeline-changed"
        assert_eq!(tauri_event.category, "pipeline-changed");
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
            RuntimeEvent::StepExecuted {
                task_id: "t".into(),
                step: 1,
                tool: None,
            },
            RuntimeEvent::ToolCircuitBroken {
                tool_name: "x".into(),
            },
            RuntimeEvent::ToolCircuitRestored {
                tool_name: "x".into(),
            },
            RuntimeEvent::AllReady,
            RuntimeEvent::ShutdownRequested,
            RuntimeEvent::FatalError("err".into()),
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
            RuntimeEvent::PipelineStarted {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                trigger_id: None,
                step_count: 1,
            },
            RuntimeEvent::PipelineStepStarted {
                run_id: "r".into(),
                step_id: "s".into(),
                task_id: "t".into(),
                agent: "a".into(),
            },
            RuntimeEvent::PipelineStepCompleted {
                run_id: "r".into(),
                step_id: "s".into(),
            },
            RuntimeEvent::PipelineStepFailed {
                run_id: "r".into(),
                step_id: "s".into(),
                reason: "r".into(),
                on_failure: "fail".into(),
            },
            RuntimeEvent::PipelineStepSkipped {
                run_id: "r".into(),
                step_id: "s".into(),
                reason: "r".into(),
            },
            RuntimeEvent::PipelineSuspended {
                run_id: "r".into(),
                step_id: "s".into(),
                task_id: "t".into(),
            },
            RuntimeEvent::PipelineResumed {
                run_id: "r".into(),
                step_id: "s".into(),
            },
            RuntimeEvent::PipelineCompleted {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                duration_ms: 0,
            },
            RuntimeEvent::PipelineFailed {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                step_id: "s".into(),
                reason: "r".into(),
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
                tool_name: "t".into(),
                prompt: "p".into(),
            },
            RuntimeEvent::ChatApprovalResolved {
                session_id: "s".into(),
                message_id: "m".into(),
                tool_name: "t".into(),
                decision: "accept".into(),
            },
            RuntimeEvent::ChatApprovalTimeout {
                session_id: "s".into(),
                message_id: "m".into(),
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
        ];

        let valid_categories = [
            "agent-changed",
            "task-changed",
            "approval-changed",
            "llm-changed",
            "trigger-fired",
            "pipeline-changed",
            "chat-changed",
            "chat-token",
            "onboarding-required",
            "stt-changed",
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
