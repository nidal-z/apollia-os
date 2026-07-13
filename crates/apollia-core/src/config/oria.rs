use serde::Deserialize;

use super::{validate_bounds, AutonomyLevel, ConfigError};

// ─────────────────────────────────────────────
// ORIAConfig
// ─────────────────────────────────────────────

/// ORIA engine configuration (Observer-Reasoner-Actor).
///
/// Maps to the `[oria]` section in `apollia.toml`. Every field has a sane
/// default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct ORIAConfig {
    /// Maximum number of replans allowed per orchestrated run.
    ///
    /// Controls how many times the agent may re-plan after a failure or a
    /// context change. Validated at startup: must be between 0 and 10 inclusive.
    ///
    /// - `0`: no replan allowed, the task fails on the first failed plan.
    /// - `2`: default value.
    /// - `10`: accepted upper bound.
    #[serde(default = "default_max_replans")]
    pub max_replans: u32,

    /// Confidence threshold above which the Observer classifies a task as Orchestrated.
    ///
    /// The Observer computes a weighted complexity score. If that score is at or
    /// above the threshold, the task runs in Orchestrated mode (LLM planning).
    /// Default: 0.40. Bounds: [0.0, 1.0].
    #[serde(default = "default_orchestrated_threshold")]
    pub orchestrated_threshold: f64,

    /// Maximum length of a step output stored in episodic memory.
    ///
    /// Truncates over-long outputs before writing them to episodic memory, to
    /// limit LLM context consumption on future recalls.
    /// Default: 200. Bounds: [50, 10000].
    #[serde(default = "default_step_memory_max_chars")]
    pub step_memory_max_chars: usize,

    /// Polling interval for the remaining StepBudget, in milliseconds.
    ///
    /// The runtime queries the budget at this frequency to detect exhaustion
    /// during direct execution. Too short wastes CPU; too long delays detection.
    /// Default: 100. Bounds: [10, 5000].
    #[serde(default = "default_budget_poll_ms")]
    pub budget_poll_ms: u64,

    /// Automatic compaction trigger threshold (0.0 to 1.0).
    ///
    /// Fraction of the LLM context window at which `ContextManager` compacts the
    /// conversation history before each Reasoner call.
    /// `0.80` leaves 20% headroom for at least one more full turn.
    /// Default: 0.80. Bounds: [0.0, 1.0].
    #[serde(default = "default_compact_threshold")]
    pub context_compact_threshold: f32,

    /// Maximum length of the summary produced during compaction, in characters.
    ///
    /// Upper bound applied to the LLM output when synthesizing the history.
    /// `4000` is about 1000 tokens, enough to capture the state of a complex
    /// task with modified files and next steps.
    /// Default: 4000. Bounds: [500, 32000].
    #[serde(default = "default_summary_max_chars")]
    pub context_summary_max_chars: usize,

    /// Character length above which a tool result is offloaded to disk.
    ///
    /// Tier 1 of the compaction pipeline. A `ToolResult` whose content exceeds
    /// this length is replaced by a compact stub and written to the workspace
    /// offload directory, so a single large result never floods the window.
    /// Default: 8000. Bounds: [500, 200000].
    ///
    /// ```toml
    /// [oria]
    /// tool_offload_threshold_chars = 8000
    /// ```
    #[serde(default = "default_tool_offload_threshold_chars")]
    pub tool_offload_threshold_chars: usize,

    /// Number of recent messages kept verbatim during graduated compaction.
    ///
    /// Tier 2 of the compaction pipeline. The last `recent_verbatim_count`
    /// messages are always preserved as-is; only older messages are summarized.
    /// The system prompt (the first message) is always preserved regardless of
    /// this setting.
    /// Default: 8. Bounds: [1, 64].
    ///
    /// ```toml
    /// [oria]
    /// recent_verbatim_count = 8
    /// ```
    #[serde(default = "default_recent_verbatim_count")]
    pub recent_verbatim_count: usize,

    /// LLM temperature for Plan A (conservative) during binary feedback.
    ///
    /// Low temperature yields deterministic, conservative output.
    /// Default: 0.3. Bounds: [0.0, 2.0].
    #[serde(default = "default_plan_alternatives_temp_a")]
    pub plan_alternatives_temp_a: f32,

    /// LLM temperature for Plan B (exploratory) during binary feedback.
    ///
    /// High temperature yields creative, exploratory output.
    /// Default: 0.8. Bounds: [0.0, 2.0].
    #[serde(default = "default_plan_alternatives_temp_b")]
    pub plan_alternatives_temp_b: f32,

    /// Seconds the plan gate waits for an operator decision before closing.
    ///
    /// When the gate is active, the engine pauses after plan generation and
    /// awaits an approve/reject decision. If none arrives within this delay the
    /// gate closes and the run fails cleanly (no step is executed).
    /// Default: 300 (5 minutes). Bounds: [1, 3600].
    #[serde(default = "default_plan_gate_ttl_secs")]
    pub plan_gate_ttl_secs: u64,

    /// Maximum replanning cycles allowed per run after plan-gate rejections.
    ///
    /// Bounds repeated rejections so the engine cannot replan forever. Once the
    /// limit is reached, a further rejection abandons the run. `0` makes a
    /// rejection immediately fatal (no replanning).
    /// Default: 3. Bounds: [0, 10].
    #[serde(default = "default_plan_gate_max_replans")]
    pub plan_gate_max_replans: u32,

    /// Autonomy tier governing the plan gate policy for the run.
    ///
    /// Resolves the plan gate: `Assisted` / `Supervised` activate it,
    /// `BoundedAutonomous` / `LongAutonomous` bypass it. Absent (`None`) means the
    /// runtime default, currently `Assisted` (gate active).
    #[serde(default)]
    pub autonomy_level: Option<AutonomyLevel>,

    /// Maximum verification-driven replans allowed per orchestrated run.
    ///
    /// After a completed run, when the autonomy tier requests verification and the
    /// verdict fails, the engine may replan and re-execute up to this many times,
    /// each replan still bounded by the shared `StepBudget`. `0` disables replan:
    /// the failing verdict is emitted and the last result is returned as-is.
    /// Default: 2. Bounds: [0, 10].
    #[serde(default = "default_verification_max_replans")]
    pub verification_max_replans: u32,
}

impl Default for ORIAConfig {
    fn default() -> Self {
        Self {
            max_replans: default_max_replans(),
            orchestrated_threshold: default_orchestrated_threshold(),
            step_memory_max_chars: default_step_memory_max_chars(),
            budget_poll_ms: default_budget_poll_ms(),
            context_compact_threshold: default_compact_threshold(),
            context_summary_max_chars: default_summary_max_chars(),
            tool_offload_threshold_chars: default_tool_offload_threshold_chars(),
            recent_verbatim_count: default_recent_verbatim_count(),
            plan_alternatives_temp_a: default_plan_alternatives_temp_a(),
            plan_alternatives_temp_b: default_plan_alternatives_temp_b(),
            plan_gate_ttl_secs: default_plan_gate_ttl_secs(),
            plan_gate_max_replans: default_plan_gate_max_replans(),
            autonomy_level: None,
            verification_max_replans: default_verification_max_replans(),
        }
    }
}

impl ORIAConfig {
    /// Validates the ORIA configuration at startup (fail-fast).
    ///
    /// - `max_replans`: must be between 0 and 10 inclusive.
    /// - `orchestrated_threshold`: must be in [0.0, 1.0].
    /// - `step_memory_max_chars`: must be in [50, 10000].
    /// - `budget_poll_ms`: must be in [10, 5000].
    /// - `context_compact_threshold`: must be in [0.0, 1.0].
    /// - `context_summary_max_chars`: must be in [500, 32000].
    /// - `tool_offload_threshold_chars`: must be in [500, 200000].
    /// - `recent_verbatim_count`: must be in [1, 64].
    /// - `plan_alternatives_temp_a`: must be in [0.0, 2.0].
    /// - `plan_alternatives_temp_b`: must be in [0.0, 2.0].
    /// - `plan_gate_ttl_secs`: must be in [1, 3600].
    /// - `plan_gate_max_replans`: must be between 0 and 10 inclusive.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_replans > 10 {
            return Err(ConfigError::InvalidValue {
                field: "oria.max_replans".into(),
                reason: "must be between 0 and 10".into(),
            });
        }
        validate_bounds(
            "oria.orchestrated_threshold",
            self.orchestrated_threshold,
            0.0_f64,
            1.0_f64,
        )?;
        validate_bounds(
            "oria.step_memory_max_chars",
            self.step_memory_max_chars,
            50_usize,
            10_000_usize,
        )?;
        validate_bounds(
            "oria.budget_poll_ms",
            self.budget_poll_ms,
            10_u64,
            5_000_u64,
        )?;
        validate_bounds(
            "oria.context_compact_threshold",
            self.context_compact_threshold,
            0.0_f32,
            1.0_f32,
        )?;
        validate_bounds(
            "oria.context_summary_max_chars",
            self.context_summary_max_chars,
            500_usize,
            32_000_usize,
        )?;
        validate_bounds(
            "oria.tool_offload_threshold_chars",
            self.tool_offload_threshold_chars,
            500_usize,
            200_000_usize,
        )?;
        validate_bounds(
            "oria.recent_verbatim_count",
            self.recent_verbatim_count,
            1_usize,
            64_usize,
        )?;
        validate_bounds(
            "oria.plan_alternatives_temp_a",
            self.plan_alternatives_temp_a,
            0.0_f32,
            2.0_f32,
        )?;
        validate_bounds(
            "oria.plan_alternatives_temp_b",
            self.plan_alternatives_temp_b,
            0.0_f32,
            2.0_f32,
        )?;
        validate_bounds(
            "oria.plan_gate_ttl_secs",
            self.plan_gate_ttl_secs,
            1_u64,
            3_600_u64,
        )?;
        if self.plan_gate_max_replans > 10 {
            return Err(ConfigError::InvalidValue {
                field: "oria.plan_gate_max_replans".into(),
                reason: "must be between 0 and 10".into(),
            });
        }
        if self.verification_max_replans > 10 {
            return Err(ConfigError::InvalidValue {
                field: "oria.verification_max_replans".into(),
                reason: "must be between 0 and 10".into(),
            });
        }
        Ok(())
    }
}

fn default_plan_gate_ttl_secs() -> u64 {
    300
}

fn default_plan_gate_max_replans() -> u32 {
    3
}

fn default_verification_max_replans() -> u32 {
    2
}

fn default_plan_alternatives_temp_a() -> f32 {
    0.3
}

fn default_plan_alternatives_temp_b() -> f32 {
    0.8
}

fn default_max_replans() -> u32 {
    2
}

fn default_orchestrated_threshold() -> f64 {
    0.40
}

fn default_step_memory_max_chars() -> usize {
    200
}

fn default_budget_poll_ms() -> u64 {
    100
}

fn default_compact_threshold() -> f32 {
    0.80
}

fn default_summary_max_chars() -> usize {
    4000
}

fn default_tool_offload_threshold_chars() -> usize {
    8_000
}

fn default_recent_verbatim_count() -> usize {
    8
}
