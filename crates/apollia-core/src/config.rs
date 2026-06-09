//! Apollia OS runtime configuration.
//!
//! Defines the configuration sections read from `apollia.toml`:
//! - [`RuntimeConfig`]: `[runtime]` section for EventBus and mailbox capacity.
//! - [`A2AConfig`]: `[a2a]` section for inter-agent routing.
//! - [`HitlConfig`]: `[hitl]` section for the Human-in-the-Loop watcher.
//! - [`ORIAConfig`]: `[oria]` section for the Observer-Reasoner-Actor engine.
//! - [`ApiConfig`]: `[api]` section for the TCP listener and the Unix socket.
//! - [`HooksConfig`]: `[hooks]` section for lifecycle hook handlers.
//!
//! Every field has a sane default via [`Default`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::budget::StepBudgetConfig;

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Configuration validation error raised at startup.
///
/// Produced by the `validate()` methods of the section configs. The runtime
/// must treat these as fatal errors (fail-fast principle).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A configuration value lies outside the acceptable range.
    #[error("invalid configuration value for '{field}': {reason}")]
    InvalidValue {
        /// Field path in dotted notation, for example `"oria.max_replans"`.
        field: String,
        /// Human-readable description of the violated constraint.
        reason: String,
    },

    /// A numeric value is outside the expected `[min, max]` range.
    #[error("configuration field '{key}' = {actual} is out of bounds (expected [{min}, {max}])")]
    OutOfBounds {
        /// Field path in dotted notation.
        key: String,
        /// Inclusive lower bound.
        min: String,
        /// Inclusive upper bound.
        max: String,
        /// Value actually supplied.
        actual: String,
    },

    /// The parent directory of the Unix socket does not exist.
    #[error("unix_socket parent directory does not exist: '{path}'")]
    SocketParentMissing {
        /// Configured Unix socket path.
        path: String,
    },

    /// `[llm.routing.hybrid] frontier` is empty or absent.
    #[error("hybrid routing requires a non-empty frontier backend name")]
    HybridFrontierMissing,

    /// `[llm.routing.hybrid] cost_ceiling_usd` is not strictly positive.
    #[error("hybrid routing cost_ceiling_usd must be > 0.0, got {value}")]
    HybridCeilingInvalid {
        /// The invalid value supplied.
        value: f64,
    },
}

/// Validates that a numeric value lies in the inclusive interval `[min, max]`.
///
/// Returns [`ConfigError::OutOfBounds`] if `value < min || value > max`.
///
/// # Parameters
///
/// - `key`: field name in dotted notation (e.g. `"runtime.eventbus_capacity"`).
/// - `value`: value to validate.
/// - `min`: inclusive lower bound.
/// - `max`: inclusive upper bound.
pub fn validate_bounds<T>(key: &str, value: T, min: T, max: T) -> Result<(), ConfigError>
where
    T: PartialOrd + std::fmt::Display,
{
    if value < min || value > max {
        return Err(ConfigError::OutOfBounds {
            key: key.to_string(),
            min: min.to_string(),
            max: max.to_string(),
            actual: value.to_string(),
        });
    }
    Ok(())
}

// ─────────────────────────────────────────────
// Autonomy tiers
// ─────────────────────────────────────────────

/// Execution autonomy tier: opt-in, explicit, auditable.
///
/// Each variant maps to an effective [`StepBudgetConfig`] and two behavioral
/// flags (`inject_memory`, `run_verification`). The effective budget is always
/// capped by the runtime ceiling via `StepBudget::from_capped`, so a tier can
/// never raise the budget above the runtime bound (principle #7).
/// Gate policy for plan review: whether the engine pauses after plan generation
/// and waits for human approval before starting the `ActorLoop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePolicy {
    /// The plan must be explicitly approved before execution starts.
    Active,
    /// The plan executes immediately without waiting for approval.
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// Default conversational mode. Low budget, no memory injection, no verification.
    Assisted,
    /// Supervised mode. Slightly raised budget, verification on, memory off.
    Supervised,
    /// Bounded autonomous mode. Raised budget, verification on, memory off by default.
    BoundedAutonomous,
    /// Long-horizon autonomous mode. High budget, verification on, memory injection on.
    LongAutonomous,
}

impl AutonomyLevel {
    /// All variants in canonical order. Used by validation and round-trip tests.
    pub const ALL: [AutonomyLevel; 4] = [
        AutonomyLevel::Assisted,
        AutonomyLevel::Supervised,
        AutonomyLevel::BoundedAutonomous,
        AutonomyLevel::LongAutonomous,
    ];

    /// Canonical snake_case identifier (round-trips with the [`std::str::FromStr`] impl).
    pub fn as_str(self) -> &'static str {
        match self {
            AutonomyLevel::Assisted => "assisted",
            AutonomyLevel::Supervised => "supervised",
            AutonomyLevel::BoundedAutonomous => "bounded_autonomous",
            AutonomyLevel::LongAutonomous => "long_autonomous",
        }
    }

    /// Plan gate policy for this autonomy tier.
    ///
    /// | Tier               | Gate   |
    /// |--------------------|--------|
    /// | Assisted           | Active |
    /// | Supervised         | Active |
    /// | BoundedAutonomous  | Bypass |
    /// | LongAutonomous     | Bypass |
    ///
    /// The safe default is `Active`: only the explicitly autonomous tiers bypass
    /// the gate. The match is exhaustive, so an unrepresentable tier cannot slip
    /// through to `Bypass`.
    pub fn gate_policy(self) -> GatePolicy {
        match self {
            AutonomyLevel::Assisted | AutonomyLevel::Supervised => GatePolicy::Active,
            AutonomyLevel::BoundedAutonomous | AutonomyLevel::LongAutonomous => GatePolicy::Bypass,
        }
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = AutonomyLevelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "assisted" => Ok(AutonomyLevel::Assisted),
            "supervised" => Ok(AutonomyLevel::Supervised),
            "bounded_autonomous" => Ok(AutonomyLevel::BoundedAutonomous),
            "long_autonomous" => Ok(AutonomyLevel::LongAutonomous),
            other => Err(AutonomyLevelParseError {
                given: other.to_string(),
            }),
        }
    }
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when an unknown autonomy level string is supplied.
#[derive(Debug, thiserror::Error)]
#[error(
    "unknown autonomy level '{given}'; accepted values: assisted, supervised, bounded_autonomous, long_autonomous"
)]
pub struct AutonomyLevelParseError {
    /// The string that failed to parse.
    pub given: String,
}

/// Per-level configuration: effective budget plus behavioral flags.
///
/// When absent from `apollia.toml`, each level uses the values defined by
/// [`AutonomyLevelConfig::default_for`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyLevelConfig {
    /// Effective budget for this tier. Always capped by the runtime ceiling.
    pub budget: StepBudgetConfig,
    /// Inject user memory context into the system prompt for this tier.
    /// Default: false for all tiers except `long_autonomous`.
    #[serde(default)]
    pub inject_memory: bool,
    /// Run the verification loop before declaring the task done for this tier.
    /// Default: false for `assisted`; true for the other tiers.
    #[serde(default)]
    pub run_verification: bool,
}

impl AutonomyLevelConfig {
    /// Canonical default configuration for a given tier.
    ///
    /// Default tier matrix (suggested budgets, capped by the runtime ceiling at
    /// execution time):
    ///
    /// | tier               | max_steps | max_tool_calls | wall_clock_secs | inject_memory | run_verification |
    /// |--------------------|-----------|----------------|-----------------|---------------|------------------|
    /// | assisted           | 100       | 200            | 1200            | false         | false            |
    /// | supervised         | 200       | 400            | 2400            | false         | true             |
    /// | bounded_autonomous | 300       | 600            | 3600            | false         | true             |
    /// | long_autonomous    | 500       | 1000           | 7200            | true          | true             |
    pub fn default_for(level: AutonomyLevel) -> Self {
        match level {
            AutonomyLevel::Assisted => Self {
                budget: StepBudgetConfig::chat_default(),
                inject_memory: false,
                run_verification: false,
            },
            AutonomyLevel::Supervised => Self {
                budget: StepBudgetConfig {
                    max_steps: 200,
                    max_tool_calls: 400,
                    wall_clock_secs: 2400,
                },
                inject_memory: false,
                run_verification: true,
            },
            AutonomyLevel::BoundedAutonomous => Self {
                budget: StepBudgetConfig {
                    max_steps: 300,
                    max_tool_calls: 600,
                    wall_clock_secs: 3600,
                },
                inject_memory: false,
                run_verification: true,
            },
            AutonomyLevel::LongAutonomous => Self {
                budget: StepBudgetConfig {
                    max_steps: 500,
                    max_tool_calls: 1000,
                    wall_clock_secs: 7200,
                },
                inject_memory: true,
                run_verification: true,
            },
        }
    }
}

/// Default tier applied when `--autonomy` is not specified.
fn default_autonomy_level() -> AutonomyLevel {
    AutonomyLevel::Assisted
}

/// Autonomy tiers configuration (`[autonomy]` section in `apollia.toml`).
///
/// Absent per-tier fields fall back to [`AutonomyLevelConfig::default_for`].
/// Each tier is validated against the runtime ceiling in [`AutonomyConfig::validate`]:
/// no tier may declare a budget above the runtime bound, which keeps the
/// `StepBudget` a non-bypassable ceiling at every tier (principle #7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    /// Default tier applied when `--autonomy` is not specified. Default: `assisted`.
    #[serde(default = "default_autonomy_level")]
    pub default_level: AutonomyLevel,
    /// Optional override for the `assisted` tier.
    #[serde(default)]
    pub assisted: Option<AutonomyLevelConfig>,
    /// Optional override for the `supervised` tier.
    #[serde(default)]
    pub supervised: Option<AutonomyLevelConfig>,
    /// Optional override for the `bounded_autonomous` tier.
    #[serde(default)]
    pub bounded_autonomous: Option<AutonomyLevelConfig>,
    /// Optional override for the `long_autonomous` tier.
    #[serde(default)]
    pub long_autonomous: Option<AutonomyLevelConfig>,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            default_level: default_autonomy_level(),
            assisted: None,
            supervised: None,
            bounded_autonomous: None,
            long_autonomous: None,
        }
    }
}

impl AutonomyConfig {
    /// Effective config for the requested level (TOML override, or the default).
    pub fn level_config(&self, level: AutonomyLevel) -> AutonomyLevelConfig {
        let override_config = match level {
            AutonomyLevel::Assisted => &self.assisted,
            AutonomyLevel::Supervised => &self.supervised,
            AutonomyLevel::BoundedAutonomous => &self.bounded_autonomous,
            AutonomyLevel::LongAutonomous => &self.long_autonomous,
        };
        override_config
            .clone()
            .unwrap_or_else(|| AutonomyLevelConfig::default_for(level))
    }

    /// Validate all tiers against the runtime ceiling (fail-fast at startup).
    ///
    /// Returns [`ConfigError::OutOfBounds`] if any tier declares a budget that
    /// exceeds the runtime ceiling along any dimension. The reported `key`
    /// identifies the offending tier and dimension, e.g.
    /// `"autonomy.supervised.budget.max_steps"`.
    pub fn validate(&self, runtime_ceiling: &StepBudgetConfig) -> Result<(), ConfigError> {
        for level in AutonomyLevel::ALL {
            let config = self.level_config(level);
            validate_bounds(
                &format!("autonomy.{level}.budget.max_steps"),
                config.budget.max_steps,
                0,
                runtime_ceiling.max_steps,
            )?;
            validate_bounds(
                &format!("autonomy.{level}.budget.max_tool_calls"),
                config.budget.max_tool_calls,
                0,
                runtime_ceiling.max_tool_calls,
            )?;
            validate_bounds(
                &format!("autonomy.{level}.budget.wall_clock_secs"),
                config.budget.wall_clock_secs,
                0,
                runtime_ceiling.wall_clock_secs,
            )?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────
// RuntimeConfig
// ─────────────────────────────────────────────

/// Core runtime configuration (`[runtime]` section in `apollia.toml`).
///
/// Controls the capacity of the internal communication infrastructure: the
/// EventBus broadcast channel and the actor mailboxes. Every field has a sane
/// default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// EventBus broadcast channel capacity.
    ///
    /// Maximum number of buffered events before slow receivers get
    /// [`tokio::sync::broadcast::error::RecvError::Lagged`].
    /// Default: 1024. Bounds: [64, 65536].
    #[serde(default = "default_eventbus_capacity")]
    pub eventbus_capacity: usize,

    /// Maximum capacity of an actor mailbox.
    ///
    /// Maximum number of pending messages per agent in the [`AgentMailbox`].
    /// Beyond it, `send()` returns `MailboxError::QueueFull`.
    /// Default: 100. Bounds: [10, 10000].
    #[serde(default = "default_mailbox_capacity")]
    pub mailbox_capacity: usize,

    /// Runtime startup timeout in seconds.
    ///
    /// Maximum time allotted to load every component at startup, including
    /// local LLM models. Large models (e.g. 70B to 400B) can take several
    /// minutes.
    /// Default: 300. No upper bound (0 disables the timeout).
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [runtime]
    /// startup_timeout_secs = 600
    /// ```
    #[serde(default = "default_startup_timeout_secs")]
    pub startup_timeout_secs: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            eventbus_capacity: default_eventbus_capacity(),
            mailbox_capacity: default_mailbox_capacity(),
            startup_timeout_secs: default_startup_timeout_secs(),
        }
    }
}

impl RuntimeConfig {
    /// Validates the runtime configuration bounds at startup (fail-fast).
    ///
    /// - `eventbus_capacity`: must be in [64, 65536].
    /// - `mailbox_capacity`: must be in [10, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "runtime.eventbus_capacity",
            self.eventbus_capacity,
            64,
            65536,
        )?;
        validate_bounds("runtime.mailbox_capacity", self.mailbox_capacity, 10, 10000)?;
        Ok(())
    }
}

fn default_startup_timeout_secs() -> u64 {
    300
}

fn default_eventbus_capacity() -> usize {
    1024
}

fn default_mailbox_capacity() -> usize {
    100
}

// ─────────────────────────────────────────────
// A2AConfig
// ─────────────────────────────────────────────

/// A2A routing configuration enforced by the runtime.
///
/// Controls the three automatic safeguards triggered during inter-agent
/// invocations: recursion depth, per-invocation timeout, and cumulative chain
/// timeout.
///
/// Defaults are tuned for the majority of use cases: `max_depth = 3`,
/// `invocation_timeout_secs = 120`, `chain_timeout_secs = 300`. Every field can
/// be overridden in `apollia.toml` under `[a2a]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AConfig {
    /// Maximum allowed A2A recursion depth.
    ///
    /// A value of `3` means a chain can reach three nesting levels before being
    /// blocked. The check is enforced by the runtime before each invocation and
    /// cannot be bypassed from the agent side.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Timeout for a single A2A invocation, in seconds.
    ///
    /// Applied to each `invoke()` call independently of the overall chain. An
    /// invocation exceeding this delay is cancelled.
    #[serde(default = "default_invocation_timeout")]
    pub invocation_timeout_secs: u64,

    /// Cumulative timeout for the whole A2A chain, in seconds.
    ///
    /// Initialized on the first invocation of a chain (`chain_deadline = None`).
    /// The remaining budget is used as the upper bound for every subsequent
    /// invocation in the same chain, preventing long chains from monopolizing
    /// resources beyond this total budget.
    /// Default: 300. Bounds: [10, 3600].
    #[serde(default = "default_chain_timeout")]
    pub chain_timeout_secs: u64,
}

impl Default for A2AConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            invocation_timeout_secs: default_invocation_timeout(),
            chain_timeout_secs: default_chain_timeout(),
        }
    }
}

impl A2AConfig {
    /// Validates the A2A configuration bounds at startup (fail-fast).
    ///
    /// - `chain_timeout_secs`: must be in [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("a2a.chain_timeout_secs", self.chain_timeout_secs, 10, 3600)?;
        Ok(())
    }
}

fn default_max_depth() -> u32 {
    3
}

fn default_invocation_timeout() -> u64 {
    120
}

fn default_chain_timeout() -> u64 {
    300
}

// ─────────────────────────────────────────────
// HitlConfig
// ─────────────────────────────────────────────

/// Human-in-the-Loop configuration (`[hitl]` section in `apollia.toml`).
///
/// Controls the `TimeoutWatcher` behavior: maximum wait for human approval and
/// the scan frequency for expired tasks. Every field has a sane default via
/// [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct HitlConfig {
    /// Maximum wait for human approval, in hours.
    ///
    /// `None` (default): the task stays paused indefinitely until the operator
    /// responds. `Some(n)`: automatic cancellation after `n` hours.
    /// Bounds when `Some`: [1, 168] (1 hour to 7 days).
    ///
    /// Do not set a global timeout unless an agent explicitly requests one.
    #[serde(default)]
    pub timeout_hours: Option<u64>,

    /// Scan interval for expired HITL tasks, in seconds.
    ///
    /// How often the `TimeoutWatcher` checks suspended tasks.
    /// Default: 60. Bounds: [10, 3600].
    /// Ignored when `timeout_hours` is `None`.
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u64,
}

impl Default for HitlConfig {
    fn default() -> Self {
        Self {
            timeout_hours: None,
            scan_interval_secs: default_scan_interval_secs(),
        }
    }
}

impl HitlConfig {
    /// Validates the HITL configuration bounds at startup (fail-fast).
    ///
    /// - `timeout_hours`: if `Some`, must be in [1, 168].
    /// - `scan_interval_secs`: must be in [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        if let Some(h) = self.timeout_hours {
            validate_bounds("hitl.timeout_hours", h, 1, 168)?;
        }
        validate_bounds("hitl.scan_interval_secs", self.scan_interval_secs, 10, 3600)?;
        Ok(())
    }
}

fn default_scan_interval_secs() -> u64 {
    60
}

// ─────────────────────────────────────────────
// HooksConfig
// ─────────────────────────────────────────────

/// Identifies the lifecycle point at which a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    /// Fires before each tool call. Blocking: the handler can allow, deny, or
    /// rewrite the invocation.
    PreToolUse,
    /// Fires after each tool call. Non-blocking: the handler can inject
    /// additional context but cannot veto the result.
    PostToolUse,
    /// Fires before context compaction. Non-blocking.
    PreCompact,
    /// Fires after context compaction. Non-blocking.
    PostCompact,
    /// Fires when a sub-agent or A2A worker is started. Non-blocking.
    SubagentStart,
    /// Fires when a sub-agent or A2A worker stops, on success or error.
    /// Non-blocking.
    SubagentStop,
}

impl HookEventKind {
    /// Returns the snake_case wire name of this event.
    ///
    /// Stable identifier shared by the JSON payload `event` field, tracing
    /// output, and the CLI summary.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::PreCompact => "pre_compact",
            Self::PostCompact => "post_compact",
            Self::SubagentStart => "subagent_start",
            Self::SubagentStop => "subagent_stop",
        }
    }
}

/// How the runtime delivers a hook event to the handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookHandlerKind {
    /// Spawn an external process. The payload is written to stdin as JSON and
    /// the decision is read back from stdout. The process must exit within the
    /// handler timeout.
    Command {
        /// Argv. Must be non-empty. Index 0 is the executable.
        command: Vec<String>,
    },
    /// POST the payload as JSON to an HTTP endpoint. The endpoint must respond
    /// within the handler timeout.
    Http {
        /// Full URL, for example `"http://127.0.0.1:9000/hook"`.
        url: String,
    },
}

impl HookHandlerKind {
    /// Returns the snake_case wire name of this delivery mechanism
    /// (`"command"` or `"http"`).
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Http { .. } => "http",
        }
    }

    /// Returns a human-readable target: the argv joined by spaces for a command
    /// handler, or the URL for an HTTP handler.
    pub fn target(&self) -> String {
        match self {
            Self::Command { command } => command.join(" "),
            Self::Http { url } => url.clone(),
        }
    }
}

/// A single registered lifecycle hook handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHandlerConfig {
    /// Lifecycle events this handler subscribes to.
    ///
    /// A handler can subscribe to multiple events; the runtime invokes it once
    /// per matching event. Must contain at least one event.
    pub events: Vec<HookEventKind>,

    /// Delivery mechanism (command or http).
    #[serde(flatten)]
    pub kind: HookHandlerKind,

    /// Maximum time the runtime waits for a handler response, in milliseconds.
    ///
    /// Default: 5000. Bounds: [100, 30000]. For the blocking `PreToolUse` hook,
    /// timeout expiry falls back to `allow` and emits a warn-level trace event.
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
}

/// Hooks configuration (`[hooks]` section in `apollia.toml`).
///
/// Declares the set of lifecycle hook handlers the runtime invokes at defined
/// points in the agent execution loop. See [`HookEventKind`] for the available
/// lifecycle events and [`HookHandlerKind`] for the delivery mechanisms.
///
/// An absent `[hooks]` section is equivalent to an empty handler list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Registered hook handlers.
    ///
    /// Each entry subscribes to one or more [`HookEventKind`] events. The
    /// runtime invokes handlers in declaration order for a given event.
    #[serde(default)]
    pub handlers: Vec<HookHandlerConfig>,
}

impl HooksConfig {
    /// Validates the hooks configuration at startup (fail-fast).
    ///
    /// - Every handler must subscribe to at least one event.
    /// - `command` handlers must have a non-empty argv.
    /// - `http` handlers must have a non-empty URL.
    /// - `timeout_ms` must be in [100, 30000].
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidValue`] for an empty event list, an empty
    /// command argv, or an empty HTTP URL, and [`ConfigError::OutOfBounds`]
    /// when `timeout_ms` falls outside `[100, 30000]`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (i, handler) in self.handlers.iter().enumerate() {
            if handler.events.is_empty() {
                return Err(ConfigError::InvalidValue {
                    field: format!("hooks.handlers[{i}].events"),
                    reason: "a handler must subscribe to at least one event".to_string(),
                });
            }
            match &handler.kind {
                HookHandlerKind::Command { command } => {
                    if command.iter().all(|arg| arg.trim().is_empty()) {
                        return Err(ConfigError::InvalidValue {
                            field: format!("hooks.handlers[{i}].command"),
                            reason: "command argv must be non-empty".to_string(),
                        });
                    }
                }
                HookHandlerKind::Http { url } => {
                    if url.trim().is_empty() {
                        return Err(ConfigError::InvalidValue {
                            field: format!("hooks.handlers[{i}].url"),
                            reason: "http url must be non-empty".to_string(),
                        });
                    }
                }
            }
            validate_bounds(
                &format!("hooks.handlers[{i}].timeout_ms"),
                handler.timeout_ms,
                100,
                30_000,
            )?;
        }
        Ok(())
    }
}

fn default_hook_timeout_ms() -> u64 {
    5_000
}

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
        Ok(())
    }
}

fn default_plan_gate_ttl_secs() -> u64 {
    300
}

fn default_plan_gate_max_replans() -> u32 {
    3
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

// ─────────────────────────────────────────────
// TriggersConfig
// ─────────────────────────────────────────────

/// Trigger engine configuration (`[triggers]` section in `apollia.toml`).
///
/// Controls the bounded queue used by [`OnBusyPolicy::Queue`] when an agent is
/// busy at fire time. Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct TriggersConfig {
    /// Maximum capacity of the per-agent bounded FIFO queue.
    ///
    /// Used by `OnBusyPolicy::Queue { max_depth }` to cap the number of pending
    /// triggers per agent. When the queue is full, the trigger is dropped and
    /// `RuntimeEvent::TriggerQueueFull` is emitted.
    /// `0` disables the bound (not recommended in production).
    /// Default: 10. Bounds: [0, 10000].
    #[serde(default = "default_trigger_queue_max_depth")]
    pub queue_max_depth: usize,
}

impl Default for TriggersConfig {
    fn default() -> Self {
        Self {
            queue_max_depth: default_trigger_queue_max_depth(),
        }
    }
}

impl TriggersConfig {
    /// Validates the trigger configuration bounds at startup (fail-fast).
    ///
    /// - `queue_max_depth`: must be in [0, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("triggers.queue_max_depth", self.queue_max_depth, 0, 10_000)?;
        Ok(())
    }
}

fn default_trigger_queue_max_depth() -> usize {
    10
}

// ─────────────────────────────────────────────
// ApiConfig
// ─────────────────────────────────────────────

/// Local REST API configuration (`[api]` section in `apollia.toml`).
///
/// Controls TCP binding, static token authentication, and the local Unix
/// socket path. The Unix socket stays unauthenticated: only the owner of the
/// socket file can access it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    /// IP address to bind the TCP listener to.
    ///
    /// Default: `"127.0.0.1"`, loopback only, unreachable from the network.
    #[serde(default = "default_api_bind")]
    pub bind: String,

    /// TCP port of the REST server.
    ///
    /// Default: `7771`.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Require a Bearer token on every inbound TCP connection.
    ///
    /// When `true` (default), each TCP request must carry a valid
    /// `Authorization: Bearer <token>` header. Requests without a header or with
    /// an invalid token get `401 Unauthorized`.
    /// The Unix socket is never subject to this check.
    #[serde(default = "default_require_token")]
    pub require_token: bool,

    /// Local Unix socket path.
    ///
    /// Used by the CLI and the desktop app to talk to the runtime without
    /// authentication (local access only).
    /// Default: `/tmp/apollia.sock`. The parent directory must exist.
    #[serde(default = "default_unix_socket")]
    pub unix_socket: PathBuf,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            port: default_api_port(),
            require_token: default_require_token(),
            unix_socket: default_unix_socket(),
        }
    }
}

impl ApiConfig {
    /// Validates the API configuration at startup (fail-fast).
    ///
    /// Checks that the parent directory of the Unix socket exists. A Unix socket
    /// whose parent directory is missing cannot be bound.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let parent = self.unix_socket.parent().unwrap_or_else(|| {
            // Fallback to root, which is always accessible.
            std::path::Path::new("/")
        });
        if !parent.exists() {
            return Err(ConfigError::SocketParentMissing {
                path: self.unix_socket.display().to_string(),
            });
        }
        Ok(())
    }
}

fn default_api_bind() -> String {
    "127.0.0.1".to_owned()
}

fn default_api_port() -> u16 {
    7771
}

fn default_require_token() -> bool {
    true
}

fn default_unix_socket() -> PathBuf {
    PathBuf::from("/tmp/apollia.sock")
}

// ─────────────────────────────────────────────
// ToolsConfig
// ─────────────────────────────────────────────

/// Native tool output configuration (`[tools]` section in `apollia.toml`).
///
/// Controls the limits the runtime applies to native tool outputs before
/// forwarding them to the LLM. Protects the LLM context window against large
/// outputs (one of the non-negotiable safeguards).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    /// Maximum size of a tool output forwarded to the LLM, in UTF-8 bytes.
    ///
    /// Outputs exceeding this limit are truncated with a "middle-trim" strategy:
    /// the start and end are preserved, the middle is removed and replaced by a
    /// message stating how many lines were lost.
    /// Default: 30 000. Bounds: [10, 1 000 000].
    #[serde(default = "default_max_output_chars")]
    pub max_output_chars: usize,

    /// Regex pattern for extracting paths from bash output.
    ///
    /// Used by `FilePathExtractor` to parse the lightweight LLM response.
    /// Default: `None`, meaning the built-in pattern (Unix paths per
    /// POSIX.1-2017 and Windows UNC per RFC 8089) is applied.
    /// Configurable for environments with custom naming conventions.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [tools]
    /// # file_path_extraction_pattern = "(?:/[^\\s]+)"
    /// ```
    #[serde(default)]
    pub file_path_extraction_pattern: Option<String>,

    /// Native tools statically disabled by the operator in `apollia.toml`.
    ///
    /// The names listed here are removed from the dispatcher at boot, so any
    /// invocation results in `UnknownTool`. This list complements the `tools`
    /// table in `governance.db`: a tool disabled in either one is inactive.
    /// Default: `[]`.
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Configuration of the native `web_search` tool.
    #[serde(default)]
    pub web_search: WebSearchConfig,

    /// Configuration of the native `web_read` tool.
    #[serde(default)]
    pub web_read: WebReadConfig,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_chars: default_max_output_chars(),
            file_path_extraction_pattern: None,
            disabled: Vec::new(),
            web_search: WebSearchConfig::default(),
            web_read: WebReadConfig::default(),
        }
    }
}

impl ToolsConfig {
    /// Validates the tools configuration bounds at startup (fail-fast).
    ///
    /// - `max_output_chars`: must be in [10, 1 000 000].
    /// - `web_search.brave.max_results`: must be in [1, 20].
    /// - `web_search.brave.timeout_secs` and `web_search.duckduckgo.timeout_secs`: `[1, 120]`.
    /// - `web_search.duckduckgo.max_response_kb`: `[16, 16 384]`.
    /// - `web_read.timeout_secs`: `[1, 120]`. `web_read.max_response_kb`: `[64, 32 768]`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.max_output_chars",
            self.max_output_chars,
            10_usize,
            1_000_000_usize,
        )?;
        self.web_search.validate()?;
        self.web_read.validate()?;
        Ok(())
    }
}

fn default_max_output_chars() -> usize {
    30_000
}

// ─────────────────────────────────────────────
// WebSearchConfig
// ─────────────────────────────────────────────

/// Configuration of the `web_search` tool (`[tools.web_search]` section).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebSearchConfig {
    /// Preferred backend: `auto`, `duckduckgo`, or `brave`. Default: `auto`.
    #[serde(default)]
    pub backend: WebSearchBackend,

    /// If `true`, boot fails when the selected backend is not operational
    /// (e.g. `backend = "brave"` without an API key). Default: `false`.
    #[serde(default)]
    pub require_configured: bool,

    /// Brave Search backend configuration.
    #[serde(default)]
    pub brave: BraveBackendConfig,

    /// DuckDuckGo backend configuration.
    #[serde(default)]
    pub duckduckgo: DuckDuckGoBackendConfig,
}

impl WebSearchConfig {
    /// Validates the bounds of the backend sub-configurations.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.brave.validate()?;
        self.duckduckgo.validate()?;
        Ok(())
    }
}

/// `web_search` backend choice exposed by `apollia.toml`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchBackend {
    /// Automatic selection: DuckDuckGo first, Brave if configured.
    #[default]
    Auto,
    /// Force DuckDuckGo (zero-config, always available).
    DuckDuckGo,
    /// Force Brave Search, requires a valid API key.
    Brave,
}

/// Brave backend configuration (`[tools.web_search.brave]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct BraveBackendConfig {
    /// Environment variable holding the Brave API key.
    /// Default: `"BRAVE_SEARCH_API_KEY"`.
    #[serde(default = "default_brave_env_var")]
    pub api_key_env_var: String,

    /// HTTP request timeout in seconds. Default: 15. Bounds: [1, 120].
    #[serde(default = "default_web_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum number of results requested from Brave per query.
    /// Default: 10. Bounds: [1, 20].
    #[serde(default = "default_brave_max_results")]
    pub max_results: u8,
}

impl Default for BraveBackendConfig {
    fn default() -> Self {
        Self {
            api_key_env_var: default_brave_env_var(),
            timeout_secs: default_web_timeout_secs(),
            max_results: default_brave_max_results(),
        }
    }
}

impl BraveBackendConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_search.brave.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_search.brave.max_results",
            self.max_results,
            1_u8,
            20_u8,
        )?;
        Ok(())
    }
}

/// DuckDuckGo backend configuration (`[tools.web_search.duckduckgo]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct DuckDuckGoBackendConfig {
    /// HTTP request timeout in seconds. Default: 15. Bounds: [1, 120].
    #[serde(default = "default_web_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum HTTP response size in kilobytes before giving up.
    /// Default: 1024. Bounds: [16, 16 384].
    #[serde(default = "default_ddg_max_response_kb")]
    pub max_response_kb: u32,
}

impl Default for DuckDuckGoBackendConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_web_timeout_secs(),
            max_response_kb: default_ddg_max_response_kb(),
        }
    }
}

impl DuckDuckGoBackendConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_search.duckduckgo.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_search.duckduckgo.max_response_kb",
            self.max_response_kb,
            16_u32,
            16_384_u32,
        )?;
        Ok(())
    }
}

/// Configuration of the `web_read` tool (`[tools.web_read]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct WebReadConfig {
    /// HTTP request timeout in seconds. Default: 20. Bounds: [1, 120].
    #[serde(default = "default_webread_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum HTTP response size in kilobytes before giving up.
    /// Default: 2048 (2 MB). Bounds: [64, 32 768].
    #[serde(default = "default_webread_max_response_kb")]
    pub max_response_kb: u32,

    /// Enables the anti-SSRF guard (rejects private and loopback hosts). Default: `true`.
    #[serde(default = "default_true")]
    pub ssrf_guard: bool,
}

impl Default for WebReadConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_webread_timeout_secs(),
            max_response_kb: default_webread_max_response_kb(),
            ssrf_guard: true,
        }
    }
}

impl WebReadConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_read.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_read.max_response_kb",
            self.max_response_kb,
            64_u32,
            32_768_u32,
        )?;
        Ok(())
    }
}

fn default_brave_env_var() -> String {
    "BRAVE_SEARCH_API_KEY".to_string()
}

fn default_web_timeout_secs() -> u64 {
    15
}

fn default_brave_max_results() -> u8 {
    10
}

fn default_ddg_max_response_kb() -> u32 {
    1024
}

fn default_webread_timeout_secs() -> u64 {
    20
}

fn default_webread_max_response_kb() -> u32 {
    2048
}

fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────
// McpConfig
// ─────────────────────────────────────────────

/// MCP tool loading strategy.
///
/// Controls whether tool schemas are loaded eagerly at session start or
/// deferred until the first use of each tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpToolLoading {
    /// Load every advertised tool schema up front, during the session handshake.
    ///
    /// Preserves the legacy behavior. Suitable for deployments with a small,
    /// fixed set of MCP servers where upfront loading is cheap.
    Eager,
    /// Load only a lightweight index at boot; fetch full schemas on demand.
    ///
    /// Default. Near-zero context cost for large MCP ecosystems. Relies on the
    /// synthetic `tool_search` tool, injected by the runtime, to let an agent
    /// discover tools by intent before any schema is fetched.
    #[default]
    Deferred,
}

/// MCP module configuration (`[mcp]` section in `apollia.toml`).
///
/// Controls the MCP-layer behaviors exposed by the runtime: the TTL of the HITL
/// approvals persisted in SQLite, the tool loading strategy, and the
/// `tool_search` result cap. Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    /// Validity duration of MCP HITL approvals, in hours.
    ///
    /// When an operator runs `apollia mcp set-approval`, the `mcp_approvals`
    /// entry is created with `expires_at = now + approval_ttl_hours`. A value of
    /// `0` disables expiration (permanent approval).
    /// Default: 24. Bounds: [0, 8760] (0 h to 1 year).
    #[serde(default = "default_approval_ttl_hours")]
    pub approval_ttl_hours: u64,

    /// Tool schema loading strategy for all MCP servers.
    ///
    /// `"deferred"` (default): only tool names and descriptions are loaded at
    /// boot; full schemas are fetched on demand. Recommended for large
    /// ecosystems and local models with narrow context windows.
    ///
    /// `"eager"`: all schemas are loaded at session start. Suitable for small,
    /// fixed server sets where upfront loading is cheap.
    #[serde(default)]
    pub tool_loading: McpToolLoading,

    /// Maximum number of results returned by the `tool_search` synthetic tool.
    ///
    /// Default: 20. Bounds: [1, 500]. Passed to the `tool_search` executor at
    /// construction time.
    #[serde(default = "default_tool_search_limit")]
    pub tool_search_limit: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            approval_ttl_hours: default_approval_ttl_hours(),
            tool_loading: McpToolLoading::default(),
            tool_search_limit: default_tool_search_limit(),
        }
    }
}

impl McpConfig {
    /// Validates the MCP configuration bounds at startup (fail-fast).
    ///
    /// - `approval_ttl_hours`: must be in [0, 8760].
    /// - `tool_search_limit`: must be in [1, 500].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "mcp.approval_ttl_hours",
            self.approval_ttl_hours,
            0_u64,
            8760_u64,
        )?;
        validate_bounds(
            "mcp.tool_search_limit",
            self.tool_search_limit,
            1_usize,
            500_usize,
        )?;
        Ok(())
    }
}

fn default_approval_ttl_hours() -> u64 {
    24
}

fn default_tool_search_limit() -> usize {
    20
}

// ─────────────────────────────────────────────
// PermissionsConfig
// ─────────────────────────────────────────────

/// Permission engine configuration (`[permissions]` section in `apollia.toml`).
///
/// Controls the three layers of the permission engine:
/// - SafeList (layer 1): commands auto-approved without HITL.
/// - PrefixRuleEngine (layer 2): prefix rules persisted in SQLite.
/// - InjectionDetector (layer 3): detection of dangerous shell patterns.
///
/// The SafeList is **empty by default**: the operator explicitly defines what
/// is safe (least-privilege principle, OWASP ASVS V1.4, CWE-272).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PermissionsConfig {
    /// Commands auto-approved without HITL, configured by the operator.
    ///
    /// Format: `"tool_name(arg_text)"` or `"tool_name"`.
    /// Examples: `"bash_executor(git status)"`, `"bash_executor(git log)"`.
    /// **Empty by default**: no command is auto-approved without explicit
    /// configuration.
    #[serde(default)]
    pub safe_commands: Vec<String>,

    /// Enables shell-injection detection (layer 3, absolute priority).
    ///
    /// Default: `true`. Disable only for controlled test environments.
    #[serde(default = "default_injection_detection")]
    pub injection_detection: bool,

    /// Lifetime of SQLite prefix rules, in hours.
    ///
    /// Default: 168 (7 days). Older rules may be purged by maintenance.
    #[serde(default = "default_prefix_rule_ttl_hours")]
    pub prefix_rule_ttl_hours: u64,

    /// Path to the consolidated SQLite database (governance.db).
    ///
    /// This single database holds the `permission_rules`, `permission_audit`,
    /// `tools` and `tool_credentials` tables. It replaces the former
    /// `permissions.db`: on the first start with an existing `permissions.db`,
    /// the runtime migrates it automatically to `governance.db` and keeps a
    /// `permissions.db.bak` backup.
    ///
    /// Default: `~/.apollia/governance.db`.
    #[serde(default = "default_permissions_db_path")]
    pub db_path: std::path::PathBuf,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            safe_commands: vec![],
            injection_detection: default_injection_detection(),
            prefix_rule_ttl_hours: default_prefix_rule_ttl_hours(),
            db_path: default_permissions_db_path(),
        }
    }
}

fn default_injection_detection() -> bool {
    true
}

fn default_prefix_rule_ttl_hours() -> u64 {
    168
}

fn default_permissions_db_path() -> std::path::PathBuf {
    std::path::PathBuf::from("~/.apollia/governance.db")
}

// ─────────────────────────────────────────────
// BashValidatorConfig
// ─────────────────────────────────────────────

/// Pre-execution bash validator configuration (`[tools.bash]` section in `apollia.toml`).
///
/// Controls two protection mechanisms `BashValidator` applies before each
/// `BashExecutor` invocation:
/// - Per-category risk classification (`RiskClassifier`), synchronous and fast.
/// - Syntax validation via `bash -n -c`, asynchronous with a timeout.
///
/// All categories are **enabled** (`block_* = true`) but the pattern lists are
/// **empty by default**: no blocking happens without explicit configuration
/// (opt-in: the operator defines what to block).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BashValidatorConfig {
    /// Enables blocking of outbound network-access commands.
    ///
    /// Reference: OWASP A10:2021 (SSRF) and the Apollia local-first principle.
    /// Default: `true`. No effective blocking without `network_egress_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_network_egress: bool,

    /// Enables blocking of irreversible destructive operations.
    ///
    /// Reference: NIST SP 800-190 §4.4.
    /// Default: `true`. No effective blocking without `destructive_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_destructive: bool,

    /// Enables blocking of privilege escalations.
    ///
    /// Reference: CWE-269 (Improper Privilege Management).
    /// Default: `true`. No effective blocking without `privilege_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_privilege_escalation: bool,

    /// Enables blocking of resource-exhaustion commands.
    ///
    /// Reference: CWE-400 (Uncontrolled Resource Consumption).
    /// Default: `true`. No effective blocking without `exhaustion_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_resource_exhaustion: bool,

    /// Patterns triggering the `NetworkEgress` category.
    ///
    /// Each entry is a substring searched in the command (e.g. `"curl"`, `"wget"`).
    /// Empty by default: the operator defines patterns based on installed tools.
    #[serde(default)]
    pub network_egress_patterns: Vec<String>,

    /// Patterns triggering the `DestructiveOp` category.
    ///
    /// Examples: `"rm -rf /"`, `"dd if="`, `"mkfs"`.
    /// Empty by default.
    #[serde(default)]
    pub destructive_patterns: Vec<String>,

    /// Patterns triggering the `PrivilegeEscalation` category.
    ///
    /// Examples: `"sudo"`, `"su "`, `"chmod 777 /"`.
    /// Empty by default.
    #[serde(default)]
    pub privilege_patterns: Vec<String>,

    /// Patterns triggering the `ResourceExhaustion` category.
    ///
    /// Examples: `":(){ :|:& };:"` (fork bomb).
    /// Empty by default.
    #[serde(default)]
    pub exhaustion_patterns: Vec<String>,

    /// Timeout for `bash -n -c` syntax validation, in milliseconds.
    ///
    /// Beyond this delay, `BashValidator::validate_syntax()` returns
    /// `SyntaxValidationTimeout`. Default: 1000ms. Bounds: [100, 10000].
    #[serde(default = "default_syntax_check_timeout_ms")]
    pub syntax_check_timeout_ms: u64,
}

impl Default for BashValidatorConfig {
    fn default() -> Self {
        Self {
            block_network_egress: default_block_flag(),
            block_destructive: default_block_flag(),
            block_privilege_escalation: default_block_flag(),
            block_resource_exhaustion: default_block_flag(),
            network_egress_patterns: vec![],
            destructive_patterns: vec![],
            privilege_patterns: vec![],
            exhaustion_patterns: vec![],
            syntax_check_timeout_ms: default_syntax_check_timeout_ms(),
        }
    }
}

impl BashValidatorConfig {
    /// Validates the bash validator configuration bounds at startup (fail-fast).
    ///
    /// - `syntax_check_timeout_ms`: must be in [100, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.bash.syntax_check_timeout_ms",
            self.syntax_check_timeout_ms,
            100_u64,
            10_000_u64,
        )?;
        Ok(())
    }
}

fn default_block_flag() -> bool {
    true
}

fn default_syntax_check_timeout_ms() -> u64 {
    1000
}

// ─────────────────────────────────────────────
// RegistryConfig
// ─────────────────────────────────────────────

/// Community pipeline registry configuration (`[registry]` section in `apollia.toml`).
///
/// Holds the URL of the public Git repository from which `apollia pipeline
/// install` downloads templates. GitHub URLs (`https://github.com/org/repo`)
/// are converted automatically to raw-content URLs by the `PipelineRegistry`.
/// Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Git repository URL of the community pipeline registry.
    ///
    /// GitHub format: `https://github.com/org/repo`.
    /// The `PipelineRegistry` converts this URL automatically to a raw-content
    /// URL (`raw.githubusercontent.com`).
    /// Default: `"https://github.com/apollia-os/pipelines"`.
    #[serde(default = "default_pipeline_registry_url")]
    pub pipeline_registry_url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            pipeline_registry_url: default_pipeline_registry_url(),
        }
    }
}

fn default_pipeline_registry_url() -> String {
    "https://github.com/apollia-os/pipelines".to_owned()
}

// ─────────────────────────────────────────────
// FilesystemRiskConfig
// ─────────────────────────────────────────────

/// System path lists used by `RiskClassifier::classify_filesystem`.
///
/// Configurable via `apollia.toml` under `[tools.filesystem]`.
///
/// `credential_paths` are expanded relative to `$HOME` at runtime. Writing to a
/// system or credential path always produces `RiskLevel::High`. Reading
/// credential paths stays `RiskLevel::Low`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilesystemRiskConfig {
    /// System paths: writing = High.
    ///
    /// Default: `["/etc", "/usr", "/bin", "/sbin", "/boot", "/var/log"]`.
    #[serde(default = "default_system_paths")]
    pub system_paths: Vec<std::path::PathBuf>,

    /// Credential paths: writing = High, reading stays Low.
    ///
    /// Default: `["$HOME/.ssh", "$HOME/.aws/credentials", "$HOME/.gnupg"]`
    /// (resolved relative to `$HOME` when the config is loaded).
    #[serde(default = "default_credential_paths")]
    pub credential_paths: Vec<std::path::PathBuf>,
}

fn default_system_paths() -> Vec<std::path::PathBuf> {
    ["/etc", "/usr", "/bin", "/sbin", "/boot", "/var/log"]
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
}

fn default_credential_paths() -> Vec<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    [".ssh", ".aws/credentials", ".gnupg", ".config/gh/hosts.yml"]
        .iter()
        .map(|rel| std::path::PathBuf::from(&home).join(rel))
        .collect()
}

impl Default for FilesystemRiskConfig {
    fn default() -> Self {
        Self {
            system_paths: default_system_paths(),
            credential_paths: default_credential_paths(),
        }
    }
}

// ─────────────────────────────────────────────
// LlmRoutingConfig
// ─────────────────────────────────────────────

/// Per-precision LLM routing configuration (`[llm.routing]` section in `apollia.toml`).
///
/// Splits LLM calls along two natural axes from the scaling laws (Kaplan et al., 2020):
/// - deep-reasoning tasks: precise but expensive backend
/// - lightweight extraction tasks: fast and cheap backend
///
/// The `[llm.routing]` section is **mandatory**: its absence is a fatal error
/// at startup (fail-fast). Set both fields explicitly in `apollia.toml`.
///
/// `apollia.toml` example:
/// ```toml
/// [llm.routing]
/// precise = "claude-opus-4-6"
/// fast    = "claude-haiku-4-5-20251001"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRoutingConfig {
    /// Backend for deep-reasoning tasks (ORIA planning, analysis, judgment).
    ///
    /// Criterion: a task where an error has high impact or needs nuance.
    /// Must match the name of a backend declared in `[[llm.backends]]`.
    pub precise: String,

    /// Backend for lightweight extraction tasks (metadata, summaries, classification, paths).
    ///
    /// Criterion: a deterministic task with verifiable output and low error cost.
    /// Must match the name of a backend declared in `[[llm.backends]]`.
    pub fast: String,

    /// Optional hybrid routing section for frontier escalation.
    ///
    /// Declared under `[llm.routing.hybrid]`. `None` (the default) disables
    /// escalation: behavior is local-only and unchanged. When present, the
    /// router may escalate to a frontier backend under a per-session cost
    /// ceiling.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [llm.routing.hybrid]
    /// frontier         = "claude-opus-4-6"
    /// cost_ceiling_usd = 1.00
    /// ```
    #[serde(default)]
    pub hybrid: Option<HybridRoutingConfig>,
}

/// Action taken by the runtime when the per-session cost ceiling is reached.
///
/// Configured under `[llm.routing.hybrid]` via `ceiling_action`. When absent
/// from the TOML file, defaults to [`CeilingAction::StayLocal`], preserving the
/// behavior introduced before this field existed (local fallback, run continues).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CeilingAction {
    /// Fall back to the local backend when the ceiling is reached.
    ///
    /// The run continues, silently degraded to local inference. This is the
    /// default and preserves the prior behavior.
    #[default]
    StayLocal,
    /// Stop the current run cleanly with a structured error when the ceiling is
    /// reached. No panic, no data loss. Enforced in the chat runtime.
    HardStop,
}

/// Hybrid routing configuration for optional frontier escalation.
///
/// Declared under `[llm.routing.hybrid]` in `apollia.toml`. When this section is
/// absent, routing behaves as before (local-only, no escalation). When present,
/// both `frontier` and `cost_ceiling_usd` are required and validated at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridRoutingConfig {
    /// Name of the frontier backend to use when an escalation signal fires.
    ///
    /// Must match the name of a backend declared in `[[llm.backends]]`.
    /// Cannot be empty.
    pub frontier: String,

    /// Hard per-session cost ceiling in USD.
    ///
    /// Escalation is skipped when `session_cost_usd >= cost_ceiling_usd`.
    /// Must be strictly positive (`> 0.0`). Typical value: `1.00`.
    pub cost_ceiling_usd: f64,

    /// Action taken when `session_cost_usd >= cost_ceiling_usd`.
    ///
    /// Defaults to [`CeilingAction::StayLocal`] for backward compatibility, so a
    /// TOML file without this key keeps deserializing and behaving as before.
    /// Set to `"hard_stop"` to stop the run cleanly instead.
    #[serde(default)]
    pub ceiling_action: CeilingAction,
}

impl HybridRoutingConfig {
    /// Validate the hybrid routing config.
    ///
    /// Called by the Supervisor at startup, after deserialization and before the
    /// router is built, so a misconfiguration is caught before the first request
    /// (fail fast, Principle #4).
    ///
    /// # Errors
    ///
    /// - [`ConfigError::HybridFrontierMissing`] if `frontier` is empty.
    /// - [`ConfigError::HybridCeilingInvalid`] if `cost_ceiling_usd <= 0.0`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.frontier.is_empty() {
            return Err(ConfigError::HybridFrontierMissing);
        }
        if self.cost_ceiling_usd <= 0.0 {
            return Err(ConfigError::HybridCeilingInvalid {
                value: self.cost_ceiling_usd,
            });
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────
// LlmRunnerConfig
// ─────────────────────────────────────────────

/// Local LLM sidecar runner configuration (`[llm.runner]` section in `apollia.toml`).
///
/// Lets the user force a specific backend (`cuda`, `rocm`, `vulkan`, `metal`,
/// `cpu`) or let automatic detection choose (`auto`).
///
/// See the `apollia_runtime::runner_supervisor::gpu_detection` module for the
/// decision hierarchy and its implementation.
///
/// `apollia.toml` example:
/// ```toml
/// [llm.runner]
/// backend = "vulkan"   # auto (default) | cuda | rocm | vulkan | metal | cpu
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRunnerConfig {
    /// Backend forced by the operator, or `"auto"` to let detection decide.
    ///
    /// Accepted values: `"auto"`, `"cuda"`, `"rocm"`, `"vulkan"`, `"metal"`, `"cpu"`.
    /// Any other value is treated as `"auto"` with a warning at startup.
    #[serde(default = "default_runner_backend")]
    pub backend: String,
}

impl Default for LlmRunnerConfig {
    fn default() -> Self {
        Self {
            backend: default_runner_backend(),
        }
    }
}

fn default_runner_backend() -> String {
    "auto".to_string()
}

// ─────────────────────────────────────────────
// VertexConfig
// ─────────────────────────────────────────────

/// Google Vertex AI backend configuration.
///
/// Auth via Application Default Credentials (ADC): the
/// `~/.config/gcloud/application_default_credentials.json` file or the
/// `GOOGLE_APPLICATION_CREDENTIALS` environment variable.
///
/// The `[llm.vertex]` section in `apollia.toml` is optional. When absent,
/// `enabled` is `false` and the backend is not loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexConfig {
    /// Enable this backend (false by default).
    #[serde(default)]
    pub enabled: bool,
    /// GCP project ID (e.g. `"my-gcp-project"`).
    pub project_id: String,
    /// Vertex AI region (e.g. `"us-east5"`, `"europe-west1"`).
    pub location: String,
    /// ID of the Anthropic model published on Vertex (e.g. `"claude-sonnet-4-6@20251001"`).
    pub model_id: String,
}

// ─────────────────────────────────────────────
// FilesystemConfig / JournalConfig
// ─────────────────────────────────────────────

/// Reversible filesystem journal configuration (`[filesystem.journal]` section in `apollia.toml`).
///
/// Controls the journal that persists the prior state of each native mutation
/// before it is applied. Lets `apollia rollback` restore the disk after an
/// agent performs unwanted operations.
///
/// Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct JournalConfig {
    /// Enables the reversible journal. Default: `true`.
    ///
    /// When `false`, `FileWrite` and `FileEdit` mutate without recording.
    /// Disable only for controlled test environments.
    #[serde(default = "default_journal_enabled")]
    pub enabled: bool,

    /// Maximum number of sessions kept on disk before the oldest is purged.
    ///
    /// Default: 50. Bounds: [1, 10 000].
    #[serde(default = "default_journal_max_sessions")]
    pub max_sessions: usize,

    /// Journal root directory. `~` is resolved at startup.
    ///
    /// Default: `~/.apollia/journal`.
    #[serde(default = "default_journal_root")]
    pub root: PathBuf,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            enabled: default_journal_enabled(),
            max_sessions: default_journal_max_sessions(),
            root: default_journal_root(),
        }
    }
}

impl JournalConfig {
    /// Validates the journal configuration bounds at startup (fail-fast).
    ///
    /// - `max_sessions`: must be in [1, 10 000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "filesystem.journal.max_sessions",
            self.max_sessions,
            1_usize,
            10_000_usize,
        )?;
        Ok(())
    }

    /// Resolves `~` in `root` to the effective home directory.
    ///
    /// Returns the resolved path without modifying `self`.
    pub fn resolved_root(&self) -> PathBuf {
        let s = self.root.to_string_lossy();
        if s.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(s.trim_start_matches("~/"))
        } else if s == "~" {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
        } else {
            self.root.clone()
        }
    }
}

fn default_journal_enabled() -> bool {
    true
}

fn default_journal_max_sessions() -> usize {
    50
}

fn default_journal_root() -> PathBuf {
    PathBuf::from("~/.apollia/journal")
}

/// Agent filesystem configuration (`[filesystem]` section in `apollia.toml`).
///
/// Groups every sub-configuration related to filesystem operations: currently
/// the reversible journal.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilesystemConfig {
    /// Sub-section dedicated to the reversible journal.
    #[serde(default)]
    pub journal: JournalConfig,
}

impl FilesystemConfig {
    /// Validates the filesystem configuration at startup (fail-fast).
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.journal.validate()
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── HybridRoutingConfig: ceiling_action ───────────────────────────────

    #[test]
    fn test_hybrid_config_default_ceiling_action() {
        // GIVEN a hybrid config payload without ceiling_action
        let raw = r#"{"frontier":"claude-opus-4","cost_ceiling_usd":2.0}"#;

        // WHEN deserializing
        let cfg: HybridRoutingConfig = serde_json::from_str(raw).expect("valid payload");

        // THEN ceiling_action defaults to StayLocal and validate passes
        assert_eq!(cfg.ceiling_action, CeilingAction::StayLocal);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_hybrid_config_hard_stop_parsed() {
        // GIVEN a payload with ceiling_action = "hard_stop"
        let raw =
            r#"{"frontier":"claude-opus-4","cost_ceiling_usd":2.0,"ceiling_action":"hard_stop"}"#;

        // WHEN deserializing
        let cfg: HybridRoutingConfig = serde_json::from_str(raw).expect("valid payload");

        // THEN the action is HardStop
        assert_eq!(cfg.ceiling_action, CeilingAction::HardStop);
    }

    #[test]
    fn test_hybrid_config_negative_ceiling_rejected() {
        // GIVEN a config with a negative ceiling
        let cfg = HybridRoutingConfig {
            frontier: "claude-opus-4".into(),
            cost_ceiling_usd: -1.0,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validating
        let result = cfg.validate();

        // THEN the ceiling is rejected
        assert!(matches!(
            result,
            Err(ConfigError::HybridCeilingInvalid { value }) if value == -1.0
        ));
    }

    #[test]
    fn test_hybrid_config_empty_frontier_rejected() {
        // GIVEN a config with an empty frontier
        let cfg = HybridRoutingConfig {
            frontier: String::new(),
            cost_ceiling_usd: 1.0,
            ceiling_action: CeilingAction::HardStop,
        };

        // WHEN validating
        // THEN the frontier is rejected
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::HybridFrontierMissing)
        ));
    }

    #[test]
    fn test_hybrid_config_zero_ceiling_rejected() {
        // GIVEN a config with a zero ceiling (exact limit)
        let cfg = HybridRoutingConfig {
            frontier: "claude-opus-4".into(),
            cost_ceiling_usd: 0.0,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validating
        // THEN zero is rejected (must be strictly positive)
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::HybridCeilingInvalid { value }) if value == 0.0
        ));
    }

    #[test]
    fn test_ceiling_action_serde_round_trip() {
        // GIVEN the HardStop action
        let action = CeilingAction::HardStop;

        // WHEN serializing then deserializing
        let json = serde_json::to_string(&action).expect("serialize");
        let back: CeilingAction = serde_json::from_str(&json).expect("deserialize");

        // THEN the value is preserved and renders snake_case
        assert_eq!(json, "\"hard_stop\"");
        assert_eq!(back, action);
    }

    // ── McpConfig: tool_loading + tool_search_limit ────────────────────────

    #[test]
    fn test_mcp_config_default_is_deferred_limit_20() {
        // GIVEN a default McpConfig
        let cfg = McpConfig::default();
        // WHEN its fields are read
        // THEN the loading mode is deferred and the search cap is 20
        assert_eq!(cfg.tool_loading, McpToolLoading::Deferred);
        assert_eq!(cfg.tool_search_limit, 20);
        assert_eq!(cfg.approval_ttl_hours, 24);
    }

    #[test]
    fn test_mcp_config_deserializes_eager_mode() {
        // GIVEN a config selecting eager mode with an explicit limit
        let json = serde_json::json!({
            "tool_loading": "eager",
            "tool_search_limit": 10
        });
        // WHEN it is deserialized
        let cfg: McpConfig = serde_json::from_value(json).unwrap();
        // THEN both values are taken from the input
        assert_eq!(cfg.tool_loading, McpToolLoading::Eager);
        assert_eq!(cfg.tool_search_limit, 10);
    }

    #[test]
    fn test_mcp_config_deserializes_deferred_mode() {
        // GIVEN a config selecting deferred mode only
        let json = serde_json::json!({ "tool_loading": "deferred" });
        // WHEN it is deserialized
        let cfg: McpConfig = serde_json::from_value(json).unwrap();
        // THEN the loading mode is deferred and the limit falls back to its default
        assert_eq!(cfg.tool_loading, McpToolLoading::Deferred);
        assert_eq!(cfg.tool_search_limit, 20);
    }

    #[test]
    fn test_mcp_tool_loading_unknown_value_fails() {
        // GIVEN a config with an unknown loading strategy
        let json = serde_json::json!({ "tool_loading": "stream" });
        // WHEN it is deserialized
        let result = serde_json::from_value::<McpConfig>(json);
        // THEN deserialization fails rather than panicking
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_tool_search_limit_zero_fails() {
        // GIVEN a config with a zero search cap
        let cfg = McpConfig {
            tool_search_limit: 0,
            ..McpConfig::default()
        };
        // WHEN it is validated
        let result = cfg.validate();
        // THEN an out-of-bounds error is reported for the right field
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "mcp.tool_search_limit"),
            "expected OutOfBounds for mcp.tool_search_limit, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_tool_search_limit_exceeds_max_fails() {
        // GIVEN a config above the upper bound
        let cfg = McpConfig {
            tool_search_limit: 501,
            ..McpConfig::default()
        };
        // WHEN it is validated
        let result = cfg.validate();
        // THEN an out-of-bounds error is reported
        assert!(matches!(result, Err(ConfigError::OutOfBounds { .. })));
    }

    #[test]
    fn test_validate_tool_search_limit_at_max_passes() {
        // GIVEN a config exactly at the upper bound
        let cfg = McpConfig {
            tool_search_limit: 500,
            ..McpConfig::default()
        };
        // WHEN / THEN validation accepts the boundary value
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_mcp_config_default_validates_ok() {
        // GIVEN / WHEN / THEN the default config passes validation
        assert!(McpConfig::default().validate().is_ok());
    }

    #[test]
    fn test_mcp_tool_loading_copy_and_eq() {
        // GIVEN a loading mode value
        let m = McpToolLoading::Eager;
        // WHEN it is copied
        let m2 = m;
        // THEN both equal each other and differ from the other variant
        assert_eq!(m, m2);
        assert_ne!(McpToolLoading::Eager, McpToolLoading::Deferred);
    }

    #[test]
    fn test_mcp_tool_loading_serialize_round_trip() {
        // GIVEN the deferred variant
        let deferred = McpToolLoading::Deferred;
        // WHEN it is serialized and read back
        let s = serde_json::to_string(&deferred).unwrap();
        let back: McpToolLoading = serde_json::from_str(&s).unwrap();
        // THEN the wire form is lowercase and the round-trip is lossless
        assert_eq!(s, "\"deferred\"");
        assert_eq!(back, McpToolLoading::Deferred);
    }

    // ── Absent config preserves every default ──────────────────────────────

    #[test]
    fn test_default_config_preserves_all_defaults() {
        // GIVEN default configs (no TOML)
        let runtime = RuntimeConfig::default();
        let a2a = A2AConfig::default();
        let hitl = HitlConfig::default();
        let api = ApiConfig::default();

        // THEN all defaults are the expected values
        assert_eq!(runtime.eventbus_capacity, 1024);
        assert_eq!(runtime.mailbox_capacity, 100);
        assert_eq!(a2a.chain_timeout_secs, 300);
        assert_eq!(hitl.timeout_hours, None);
        assert_eq!(hitl.scan_interval_secs, 60);
        assert_eq!(api.unix_socket, PathBuf::from("/tmp/apollia.sock"));

        // AND all defaults pass validation
        runtime
            .validate()
            .expect("default RuntimeConfig must be valid");
        a2a.validate().expect("default A2AConfig must be valid");
        hitl.validate().expect("default HitlConfig must be valid");
    }

    // ── Custom values are honored ──────────────────────────────────────────

    #[test]
    fn test_custom_eventbus_capacity_used() {
        // GIVEN
        let toml = r#"eventbus_capacity = 2048"#;
        let cfg: RuntimeConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.eventbus_capacity, 2048);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_mailbox_capacity_used() {
        // GIVEN
        let toml = r#"mailbox_capacity = 200"#;
        let cfg: RuntimeConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.mailbox_capacity, 200);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_chain_timeout_used() {
        // GIVEN
        let toml = r#"chain_timeout_secs = 600"#;
        let cfg: A2AConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.chain_timeout_secs, 600);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_hitl_timeout_used() {
        // GIVEN
        let toml = r#"timeout_hours = 48"#;
        let cfg: HitlConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.timeout_hours, Some(48));
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_hitl_no_timeout_by_default() {
        // GIVEN default config (no TOML)
        let cfg = HitlConfig::default();

        // THEN timeout is None - tasks pause indefinitely
        assert_eq!(cfg.timeout_hours, None);
        cfg.validate().expect("default must be valid");
    }

    #[test]
    fn test_hitl_explicit_none_timeout_valid() {
        // GIVEN TOML without timeout_hours field
        let toml = r#"scan_interval_secs = 120"#;
        let cfg: HitlConfig = toml::from_str(toml).expect("valid toml");

        // THEN timeout is None, scan interval is set
        assert_eq!(cfg.timeout_hours, None);
        assert_eq!(cfg.scan_interval_secs, 120);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_scan_interval_used() {
        // GIVEN
        let toml = r#"scan_interval_secs = 120"#;
        let cfg: HitlConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.scan_interval_secs, 120);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_unix_socket_used() {
        // GIVEN - /tmp always exists
        let toml = r#"unix_socket = "/tmp/custom-apollia.sock""#;
        let cfg: ApiConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.unix_socket, PathBuf::from("/tmp/custom-apollia.sock"));
        cfg.validate().expect("/tmp parent must exist");
    }

    // ── Out-of-bounds value fails at startup ───────────────────────────────

    #[test]
    fn test_eventbus_capacity_below_min_fails() {
        // GIVEN capacity = 10, below min 64
        let cfg = RuntimeConfig {
            eventbus_capacity: 10,
            mailbox_capacity: 100,
            startup_timeout_secs: 30,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.eventbus_capacity"),
            "expected OutOfBounds for runtime.eventbus_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_eventbus_capacity_above_max_fails() {
        // GIVEN capacity = 100000, above max 65536
        let cfg = RuntimeConfig {
            eventbus_capacity: 100_000,
            mailbox_capacity: 100,
            startup_timeout_secs: 30,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.eventbus_capacity"),
            "expected OutOfBounds for runtime.eventbus_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_mailbox_capacity_out_of_bounds_fails() {
        // GIVEN capacity = 5, below min 10
        let cfg = RuntimeConfig {
            eventbus_capacity: 1024,
            mailbox_capacity: 5,
            startup_timeout_secs: 30,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.mailbox_capacity"),
            "expected OutOfBounds for runtime.mailbox_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_chain_timeout_out_of_bounds_fails() {
        // GIVEN chain_timeout_secs = 5, below min 10
        let cfg = A2AConfig {
            chain_timeout_secs: 5,
            ..A2AConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "a2a.chain_timeout_secs"),
            "expected OutOfBounds for a2a.chain_timeout_secs, got: {result:?}"
        );
    }

    #[test]
    fn test_hitl_timeout_out_of_bounds_fails() {
        // GIVEN timeout_hours = Some(0), below min 1
        let cfg = HitlConfig {
            timeout_hours: Some(0),
            scan_interval_secs: 60,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "hitl.timeout_hours"),
            "expected OutOfBounds for hitl.timeout_hours, got: {result:?}"
        );
    }

    #[test]
    fn test_scan_interval_out_of_bounds_fails() {
        // GIVEN scan_interval_secs = 5, below min 10
        let cfg = HitlConfig {
            timeout_hours: None,
            scan_interval_secs: 5,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "hitl.scan_interval_secs"),
            "expected OutOfBounds for hitl.scan_interval_secs, got: {result:?}"
        );
    }

    #[test]
    fn test_boundary_values_accepted() {
        // GIVEN min and max exact values for all fields

        let runtime_min = RuntimeConfig {
            eventbus_capacity: 64,
            mailbox_capacity: 10,
            startup_timeout_secs: 30,
        };
        let runtime_max = RuntimeConfig {
            eventbus_capacity: 65536,
            mailbox_capacity: 10000,
            startup_timeout_secs: 600,
        };
        let a2a_min = A2AConfig {
            chain_timeout_secs: 10,
            ..A2AConfig::default()
        };
        let a2a_max = A2AConfig {
            chain_timeout_secs: 3600,
            ..A2AConfig::default()
        };
        let hitl_min = HitlConfig {
            timeout_hours: Some(1),
            scan_interval_secs: 10,
        };
        let hitl_max = HitlConfig {
            timeout_hours: Some(168),
            scan_interval_secs: 3600,
        };

        // THEN all boundary values are accepted
        runtime_min.validate().expect("min RuntimeConfig valid");
        runtime_max.validate().expect("max RuntimeConfig valid");
        a2a_min
            .validate()
            .expect("min A2AConfig chain_timeout valid");
        a2a_max
            .validate()
            .expect("max A2AConfig chain_timeout valid");
        hitl_min.validate().expect("min HitlConfig valid");
        hitl_max.validate().expect("max HitlConfig valid");
    }

    // ── ORIAConfig defaults ─────────────────────────────────────────────────

    #[test]
    fn test_default_oria_config_preserves_defaults() {
        // GIVEN no TOML for [oria]
        let cfg = ORIAConfig::default();

        // THEN all defaults are the expected values
        assert_eq!(cfg.max_replans, 2);
        assert!((cfg.orchestrated_threshold - 0.40).abs() < f64::EPSILON);
        assert_eq!(cfg.step_memory_max_chars, 200);
        assert_eq!(cfg.budget_poll_ms, 100);
        assert_eq!(cfg.tool_offload_threshold_chars, 8000);
        assert_eq!(cfg.recent_verbatim_count, 8);

        // AND defaults pass validation
        cfg.validate().expect("default ORIAConfig must be valid");
    }

    // ── ORIAConfig custom values ────────────────────────────────────────────

    #[test]
    fn test_custom_orchestrated_threshold_used() {
        // GIVEN
        let toml = r#"orchestrated_threshold = 0.65"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert!((cfg.orchestrated_threshold - 0.65).abs() < f64::EPSILON);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_step_memory_max_chars_used() {
        // GIVEN
        let toml = r#"step_memory_max_chars = 500"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.step_memory_max_chars, 500);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_budget_poll_ms_used() {
        // GIVEN
        let toml = r#"budget_poll_ms = 200"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.budget_poll_ms, 200);
        cfg.validate().expect("valid bounds");
    }

    // ── orchestrated_threshold out of bounds ────────────────────────────────

    #[test]
    fn test_orchestrated_threshold_above_1_fails() {
        // GIVEN orchestrated_threshold = 1.5, above max 1.0
        let cfg = ORIAConfig {
            orchestrated_threshold: 1.5,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.orchestrated_threshold"),
            "expected OutOfBounds for oria.orchestrated_threshold, got: {result:?}"
        );
    }

    #[test]
    fn test_orchestrated_threshold_negative_fails() {
        // GIVEN orchestrated_threshold = -0.1, below min 0.0
        let cfg = ORIAConfig {
            orchestrated_threshold: -0.1,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.orchestrated_threshold"),
            "expected OutOfBounds for oria.orchestrated_threshold, got: {result:?}"
        );
    }

    // ── step_memory_max_chars out of bounds ─────────────────────────────────

    #[test]
    fn test_step_memory_max_chars_below_50_fails() {
        // GIVEN step_memory_max_chars = 10, below min 50
        let cfg = ORIAConfig {
            step_memory_max_chars: 10,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.step_memory_max_chars"),
            "expected OutOfBounds for oria.step_memory_max_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_step_memory_max_chars_above_10000_fails() {
        // GIVEN step_memory_max_chars = 20000, above max 10000
        let cfg = ORIAConfig {
            step_memory_max_chars: 20_000,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.step_memory_max_chars"),
            "expected OutOfBounds for oria.step_memory_max_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_budget_poll_ms_out_of_bounds_fails() {
        // GIVEN budget_poll_ms = 5, below min 10
        let cfg = ORIAConfig {
            budget_poll_ms: 5,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.budget_poll_ms"),
            "expected OutOfBounds for oria.budget_poll_ms, got: {result:?}"
        );
    }

    #[test]
    fn test_oria_boundary_values_accepted() {
        // GIVEN min and max exact values for ORIAConfig fields
        let oria_min = ORIAConfig {
            orchestrated_threshold: 0.0,
            step_memory_max_chars: 50,
            budget_poll_ms: 10,
            ..ORIAConfig::default()
        };
        let oria_max = ORIAConfig {
            orchestrated_threshold: 1.0,
            step_memory_max_chars: 10_000,
            budget_poll_ms: 5_000,
            ..ORIAConfig::default()
        };

        // THEN all boundary values are accepted
        oria_min.validate().expect("min ORIAConfig valid");
        oria_max.validate().expect("max ORIAConfig valid");
    }

    // ── compaction tier fields ──────────────────────────────────────────────

    #[test]
    fn test_oria_compaction_tiers_toml_round_trip() {
        // GIVEN a [oria] TOML overriding both compaction tier fields
        let toml = r#"
            tool_offload_threshold_chars = 4000
            recent_verbatim_count = 12
        "#;

        // WHEN deserialized
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN the exact values are read back and validation passes
        assert_eq!(cfg.tool_offload_threshold_chars, 4000);
        assert_eq!(cfg.recent_verbatim_count, 12);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_tool_offload_threshold_below_min_fails() {
        // GIVEN tool_offload_threshold_chars = 200, below min 500
        let cfg = ORIAConfig {
            tool_offload_threshold_chars: 200,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.tool_offload_threshold_chars"),
            "expected OutOfBounds for oria.tool_offload_threshold_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_recent_verbatim_count_below_min_fails() {
        // GIVEN recent_verbatim_count = 0, below min 1
        let cfg = ORIAConfig {
            recent_verbatim_count: 0,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.recent_verbatim_count"),
            "expected OutOfBounds for oria.recent_verbatim_count, got: {result:?}"
        );
    }

    #[test]
    fn test_recent_verbatim_count_above_max_fails() {
        // GIVEN recent_verbatim_count = 65, above max 64
        let cfg = ORIAConfig {
            recent_verbatim_count: 65,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.recent_verbatim_count"),
            "expected OutOfBounds for oria.recent_verbatim_count, got: {result:?}"
        );
    }

    // ── ToolsConfig ────────────────────────────────────────────────────────

    #[test]
    fn test_tools_config_default_values() {
        // GIVEN the default config
        let cfg = ToolsConfig::default();
        // THEN
        assert_eq!(cfg.max_output_chars, 30_000);
        cfg.validate().expect("default ToolsConfig must be valid");
    }

    #[test]
    fn test_tools_config_serde_override() {
        // GIVEN a TOML with a custom value
        let toml = r#"max_output_chars = 100"#;
        // WHEN
        let cfg: ToolsConfig = toml::from_str(toml).expect("valid toml");
        // THEN
        assert_eq!(cfg.max_output_chars, 100);
        cfg.validate().expect("100 is within bounds");
    }

    #[test]
    fn test_tools_config_below_min_fails() {
        // GIVEN max_output_chars below minimum (min = 10)
        let cfg = ToolsConfig {
            max_output_chars: 5,
            file_path_extraction_pattern: None,
            disabled: Vec::new(),
            web_search: WebSearchConfig::default(),
            web_read: WebReadConfig::default(),
        };
        // WHEN
        let result = cfg.validate();
        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "tools.max_output_chars"),
            "expected OutOfBounds for tools.max_output_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_tools_config_above_max_fails() {
        // GIVEN max_output_chars above maximum
        let cfg = ToolsConfig {
            max_output_chars: 2_000_000,
            file_path_extraction_pattern: None,
            disabled: Vec::new(),
            web_search: WebSearchConfig::default(),
            web_read: WebReadConfig::default(),
        };
        // WHEN
        let result = cfg.validate();
        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "tools.max_output_chars"),
            "expected OutOfBounds for tools.max_output_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_default_config_deserialization() {
        // GIVEN apollia.toml without [tools.web_search] / [tools.web_read]
        let toml = "";
        let cfg: ToolsConfig = toml::from_str(toml).expect("empty toml parses");

        assert_eq!(cfg.web_search.backend, WebSearchBackend::Auto);
        assert!(!cfg.web_search.require_configured);
        assert_eq!(cfg.web_search.brave.timeout_secs, 15);
        assert_eq!(cfg.web_search.brave.max_results, 10);
        assert_eq!(cfg.web_search.brave.api_key_env_var, "BRAVE_SEARCH_API_KEY");
        assert_eq!(cfg.web_search.duckduckgo.timeout_secs, 15);
        assert_eq!(cfg.web_search.duckduckgo.max_response_kb, 1024);
        assert_eq!(cfg.web_read.timeout_secs, 20);
        assert_eq!(cfg.web_read.max_response_kb, 2048);
        assert!(cfg.web_read.ssrf_guard);
        assert!(cfg.disabled.is_empty());
        cfg.validate().expect("default tools config valid");
    }

    #[test]
    fn test_disabled_tools_from_toml() {
        // GIVEN [tools] disabled = ["bash_executor"]
        let toml = r#"
            disabled = ["bash_executor", "python_executor"]
        "#;
        let cfg: ToolsConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(cfg.disabled, vec!["bash_executor", "python_executor"]);
    }

    #[test]
    fn test_backend_brave_only_config() {
        // GIVEN [tools.web_search] backend = "brave"
        let toml = r#"
            [web_search]
            backend = "brave"
            require_configured = true

            [web_search.brave]
            timeout_secs = 30
            max_results = 5
        "#;
        let cfg: ToolsConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(cfg.web_search.backend, WebSearchBackend::Brave);
        assert!(cfg.web_search.require_configured);
        assert_eq!(cfg.web_search.brave.timeout_secs, 30);
        assert_eq!(cfg.web_search.brave.max_results, 5);
        cfg.validate().expect("config valid");
    }

    #[test]
    fn test_brave_max_results_out_of_bounds_fails() {
        let toml = r#"
            [web_search.brave]
            max_results = 50
        "#;
        let cfg: ToolsConfig = toml::from_str(toml).expect("toml parses");
        let err = cfg.validate().expect_err("max_results=50 must fail");
        assert!(
            matches!(err, ConfigError::OutOfBounds { ref key, .. }
                if key == "tools.web_search.brave.max_results"),
            "got: {err:?}"
        );
    }
}

#[cfg(test)]
mod autonomy_tests {
    use super::*;
    use std::str::FromStr;

    // assisted tier matches chat_default with flags off.
    #[test]
    fn test_assisted_level_matches_chat_default() {
        // GIVEN the default autonomy config
        let config = AutonomyConfig::default();

        // WHEN reading the assisted tier
        let lc = config.level_config(AutonomyLevel::Assisted);

        // THEN it matches the chat default budget with both flags off
        assert_eq!(lc.budget.max_steps, 100);
        assert_eq!(lc.budget.max_tool_calls, 200);
        assert_eq!(lc.budget.wall_clock_secs, 1200);
        assert!(!lc.inject_memory);
        assert!(!lc.run_verification);
    }

    // long_autonomous tier enables the autonomy flags.
    #[test]
    fn test_long_autonomous_level_flags() {
        // GIVEN the default autonomy config
        let config = AutonomyConfig::default();

        // WHEN reading the long_autonomous tier
        let lc = config.level_config(AutonomyLevel::LongAutonomous);

        // THEN the budget is raised and both flags are on
        assert!(lc.budget.max_steps >= 500);
        assert!(lc.budget.max_tool_calls >= 1000);
        assert!(lc.budget.wall_clock_secs >= 7200);
        assert!(lc.inject_memory);
        assert!(lc.run_verification);
    }

    // (error case): validate rejects a tier above the runtime ceiling.
    #[test]
    fn test_validate_rejects_budget_above_ceiling() {
        // GIVEN a low runtime ceiling and the default tiers (all above it)
        let ceiling = StepBudgetConfig {
            max_steps: 50,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let config = AutonomyConfig::default();

        // WHEN validating against the ceiling
        let result = config.validate(&ceiling);

        // THEN validation fails and names the autonomy dimension
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("autonomy"));
        assert!(msg.contains("out of bounds"));
    }

    // validate passes when every tier fits under the ceiling.
    #[test]
    fn test_validate_accepts_tiers_within_ceiling() {
        // GIVEN a ceiling at least as high as the most demanding tier
        let ceiling = StepBudgetConfig {
            max_steps: 1000,
            max_tool_calls: 2000,
            wall_clock_secs: 10_000,
        };
        let config = AutonomyConfig::default();

        // WHEN validating against the ceiling
        let result = config.validate(&ceiling);

        // THEN validation succeeds
        assert!(result.is_ok());
    }

    // (error case): an unknown level string is rejected with a typed error.
    #[test]
    fn test_from_str_unknown_level_fails() {
        // GIVEN / WHEN parsing an unknown level
        let result = AutonomyLevel::from_str("turbo");

        // THEN it errors and lists the accepted values
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("turbo"));
        assert!(msg.contains("assisted"));
    }

    // Round-trip as_str / from_str for the four tiers.
    #[test]
    fn test_autonomy_level_roundtrip_str() {
        // GIVEN the four tiers
        // WHEN / THEN as_str and from_str round-trip
        for level in AutonomyLevel::ALL {
            let s = level.as_str();
            let parsed = AutonomyLevel::from_str(s).expect("round-trip must succeed");
            assert_eq!(parsed, level);
        }
    }

    // gate_policy is stable and matches the documented routing table.
    #[test]
    fn test_gate_policy_all_variants() {
        // GIVEN the four tiers
        // WHEN gate_policy is read
        // THEN Assisted/Supervised gate, Bounded/Long bypass
        assert_eq!(AutonomyLevel::Assisted.gate_policy(), GatePolicy::Active);
        assert_eq!(AutonomyLevel::Supervised.gate_policy(), GatePolicy::Active);
        assert_eq!(
            AutonomyLevel::BoundedAutonomous.gate_policy(),
            GatePolicy::Bypass
        );
        assert_eq!(
            AutonomyLevel::LongAutonomous.gate_policy(),
            GatePolicy::Bypass
        );
    }

    // The default ORIA tier (absent) resolves to Assisted: gate active.
    #[test]
    fn test_default_autonomy_level_is_assisted_gate_active() {
        // GIVEN the default ORIAConfig (no autonomy_level)
        let config = ORIAConfig::default();
        // WHEN resolved with the safe default
        let level = config.autonomy_level.unwrap_or(AutonomyLevel::Assisted);
        // THEN the gate is active
        assert_eq!(level.gate_policy(), GatePolicy::Active);
    }

    // effective_budget mirrors the default tier budget.
    #[test]
    fn test_effective_budget_matches_default_for() {
        // GIVEN a tier
        let level = AutonomyLevel::BoundedAutonomous;

        // WHEN reading its effective budget
        let budget = level.effective_budget();

        // THEN it matches the default tier budget
        let expected = AutonomyLevelConfig::default_for(level).budget;
        assert_eq!(budget.max_steps, expected.max_steps);
        assert_eq!(budget.max_tool_calls, expected.max_tool_calls);
        assert_eq!(budget.wall_clock_secs, expected.wall_clock_secs);
    }

    // ── HybridRoutingConfig ────────────────────────────────────

    #[test]
    fn test_hybrid_absent_deserializes_to_none() {
        // GIVEN a routing TOML without a hybrid section
        let toml_str = r#"
            precise = "local"
            fast    = "local"
        "#;

        // WHEN it is deserialized
        let routing: LlmRoutingConfig = toml::from_str(toml_str).expect("valid toml");

        // THEN hybrid is None
        assert!(routing.hybrid.is_none());
    }

    #[test]
    fn test_hybrid_complete_deserializes_correctly() {
        // GIVEN a routing TOML with a complete hybrid section
        let toml_str = r#"
            precise = "local"
            fast    = "local"
            [hybrid]
            frontier         = "claude-opus-4-6"
            cost_ceiling_usd = 2.00
        "#;

        // WHEN it is deserialized
        let routing: LlmRoutingConfig = toml::from_str(toml_str).expect("valid toml");

        // THEN hybrid is Some with the supplied values
        let h = routing.hybrid.expect("hybrid should be Some");
        assert_eq!(h.frontier, "claude-opus-4-6");
        assert!((h.cost_ceiling_usd - 2.00).abs() < 1e-9);
    }

    #[test]
    fn test_validate_rejects_zero_ceiling() {
        // GIVEN a hybrid config with a zero ceiling
        let cfg = HybridRoutingConfig {
            frontier: "claude-opus-4-6".to_owned(),
            cost_ceiling_usd: 0.0,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validate is called
        let result = cfg.validate();

        // THEN it is rejected as an invalid ceiling
        assert!(matches!(
            result,
            Err(ConfigError::HybridCeilingInvalid { .. })
        ));
    }

    #[test]
    fn test_validate_rejects_empty_frontier() {
        // GIVEN a hybrid config with an empty frontier
        let cfg = HybridRoutingConfig {
            frontier: String::new(),
            cost_ceiling_usd: 1.00,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validate is called
        let result = cfg.validate();

        // THEN it is rejected as a missing frontier
        assert!(matches!(result, Err(ConfigError::HybridFrontierMissing)));
    }

    #[test]
    fn test_validate_rejects_negative_ceiling() {
        // GIVEN a hybrid config with a negative ceiling
        let cfg = HybridRoutingConfig {
            frontier: "claude-opus-4-6".to_owned(),
            cost_ceiling_usd: -0.5,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validate is called
        let result = cfg.validate();

        // THEN it is rejected as an invalid ceiling carrying the negative value
        assert!(matches!(
            result,
            Err(ConfigError::HybridCeilingInvalid { value }) if value < 0.0
        ));
    }

    #[test]
    fn test_validate_accepts_complete_hybrid() {
        // GIVEN a valid hybrid config
        let cfg = HybridRoutingConfig {
            frontier: "claude-opus-4-6".to_owned(),
            cost_ceiling_usd: 1.00,
            ceiling_action: CeilingAction::StayLocal,
        };

        // WHEN validate is called
        // THEN it succeeds
        assert!(cfg.validate().is_ok());
    }

    // ── HooksConfig ───────────────────────────────────────────────────────

    #[test]
    fn test_hooks_ac1_valid_command_and_http_handlers() {
        // GIVEN a HooksConfig with one valid command handler and one valid http handler
        let toml = r#"
            [[handlers]]
            events = ["pre_tool_use"]
            type = "command"
            command = ["/usr/bin/my-hook", "--event", "pre_tool_use"]

            [[handlers]]
            events = ["post_tool_use"]
            type = "http"
            url = "http://127.0.0.1:9000/hook"
        "#;
        let cfg: HooksConfig = toml::from_str(toml).expect("valid hooks toml");

        // WHEN validate is called
        // THEN it succeeds and both handlers are present
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.handlers.len(), 2);
    }

    #[test]
    fn test_hooks_ac2_missing_command_argv_rejected() {
        // GIVEN a command handler with an empty argv
        let cfg = HooksConfig {
            handlers: vec![HookHandlerConfig {
                events: vec![HookEventKind::PreToolUse],
                kind: HookHandlerKind::Command { command: vec![] },
                timeout_ms: default_hook_timeout_ms(),
            }],
        };

        // WHEN validate is called
        let result = cfg.validate();

        // THEN it is rejected with a field naming the offending command
        assert!(matches!(
            result,
            Err(ConfigError::InvalidValue { field, .. }) if field.contains("command")
        ));
    }

    #[test]
    fn test_hooks_ac3_unknown_type_deserialization_error() {
        // GIVEN a TOML handler with an unknown delivery type
        let toml = r#"
            [[handlers]]
            events = ["pre_tool_use"]
            type = "grpc"
            url = "http://127.0.0.1:9000/hook"
        "#;

        // WHEN deserialization runs
        let result = toml::from_str::<HooksConfig>(toml);

        // THEN it fails at the serde layer, before validate, without panicking
        assert!(result.is_err());
    }

    #[test]
    fn test_hooks_ac4_default_timeout_applied() {
        // GIVEN a valid handler without an explicit timeout_ms
        let toml = r#"
            [[handlers]]
            events = ["pre_tool_use"]
            type = "http"
            url = "http://127.0.0.1:9000/hook"
        "#;
        let cfg: HooksConfig = toml::from_str(toml).expect("valid hooks toml");

        // WHEN validate is called
        // THEN the handler carries the default 5000 ms timeout and validation passes
        assert_eq!(cfg.handlers[0].timeout_ms, 5_000);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_hooks_ac5_empty_hooks_section_valid() {
        // GIVEN the default (absent) hooks config
        let cfg = HooksConfig::default();

        // WHEN validate is called
        // THEN it succeeds and the handler list is empty
        assert!(cfg.validate().is_ok());
        assert!(cfg.handlers.is_empty());
    }

    #[test]
    fn test_hooks_empty_events_rejected() {
        // GIVEN a handler subscribing to no event
        let cfg = HooksConfig {
            handlers: vec![HookHandlerConfig {
                events: vec![],
                kind: HookHandlerKind::Http {
                    url: "http://127.0.0.1:9000/hook".to_string(),
                },
                timeout_ms: default_hook_timeout_ms(),
            }],
        };

        // WHEN validate is called
        // THEN it is rejected with a field naming the events list
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidValue { field, .. }) if field.contains("events")
        ));
    }

    #[test]
    fn test_hooks_timeout_out_of_bounds_rejected() {
        // GIVEN a handler with a timeout below the lower bound
        let cfg = HooksConfig {
            handlers: vec![HookHandlerConfig {
                events: vec![HookEventKind::PreToolUse],
                kind: HookHandlerKind::Http {
                    url: "http://127.0.0.1:9000/hook".to_string(),
                },
                timeout_ms: 10,
            }],
        };

        // WHEN validate is called
        // THEN it is rejected as out of bounds
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::OutOfBounds { key, .. }) if key.contains("timeout_ms")
        ));
    }
}
