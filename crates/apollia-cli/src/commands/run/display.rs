//! The parsed SSE event and the display state it feeds.

// ─── SSE types ───────────────────────────────────────────────────────────────

/// A parsed Server-Sent Event received on the task stream.
pub struct SseEvent {
    /// The `event` field extracted from the JSON payload (e.g. `"plan_generated"`).
    pub event_type: String,
    /// The full parsed JSON data payload.
    pub data: serde_json::Value,
    /// The original raw JSON string as received on the wire.
    pub raw_json: String,
}

// ─── Display state ────────────────────────────────────────────────────────────

/// Display state maintained across SSE events for an orchestrated run.
pub struct RunDisplayState {
    /// ID of the current execution plan, if one was received.
    pub plan_id: Option<String>,
    /// Total number of steps in the current plan.
    pub step_count: usize,
    /// Sequential number of the step currently in progress.
    pub current_num: usize,
    /// Whether `--json` raw mode is active.
    pub json_mode: bool,
    /// When `true`, suppress all intermediate events: only terminal events produce output.
    ///
    /// Used by the default (non-`--stream`) `run` invocation to display only the final
    /// result while still using SSE internally to receive the agent output.
    pub terminal_only: bool,
    /// When `true`, the stream pauses on `plan_approval_required` to collect the
    /// operator's decision before execution proceeds.
    pub plan_mode: bool,
    /// Run id extracted from the last `plan_approval_required` event, used to
    /// submit the decision to the runtime API.
    pub pending_plan_run_id: Option<String>,
}

impl RunDisplayState {
    /// Create a new display state.
    pub fn new(json_mode: bool, terminal_only: bool) -> Self {
        Self {
            plan_id: None,
            step_count: 0,
            current_num: 0,
            json_mode,
            terminal_only,
            plan_mode: false,
            pending_plan_run_id: None,
        }
    }

    /// Create a display state with plan-approval mode enabled.
    pub fn with_plan(json_mode: bool) -> Self {
        Self {
            plan_mode: true,
            ..Self::new(json_mode, false)
        }
    }
}
