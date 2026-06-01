//! Apollia OS runtime configuration.
//!
//! Defines the configuration sections read from `apollia.toml`:
//! - [`RuntimeConfig`]: `[runtime]` section for EventBus and mailbox capacity.
//! - [`A2AConfig`]: `[a2a]` section for inter-agent routing.
//! - [`HitlConfig`]: `[hitl]` section for the Human-in-the-Loop watcher.
//! - [`ORIAConfig`]: `[oria]` section for the Observer-Reasoner-Actor engine.
//! - [`ApiConfig`]: `[api]` section for the TCP listener and the Unix socket.
//!
//! Every field has a sane default via [`Default`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
            plan_alternatives_temp_a: default_plan_alternatives_temp_a(),
            plan_alternatives_temp_b: default_plan_alternatives_temp_b(),
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
    /// - `plan_alternatives_temp_a`: must be in [0.0, 2.0].
    /// - `plan_alternatives_temp_b`: must be in [0.0, 2.0].
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
        Ok(())
    }
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

/// MCP module configuration (`[mcp]` section in `apollia.toml`).
///
/// Controls the MCP-layer behaviors exposed by the runtime: TTL of the HITL
/// approvals persisted in SQLite. Every field has a sane default via
/// [`Default`].
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
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            approval_ttl_hours: default_approval_ttl_hours(),
        }
    }
}

impl McpConfig {
    /// Validates the MCP configuration bounds at startup (fail-fast).
    ///
    /// - `approval_ttl_hours`: must be in [0, 8760].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "mcp.approval_ttl_hours",
            self.approval_ttl_hours,
            0_u64,
            8760_u64,
        )?;
        Ok(())
    }
}

fn default_approval_ttl_hours() -> u64 {
    24
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
