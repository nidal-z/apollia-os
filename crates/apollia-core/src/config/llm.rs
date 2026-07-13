use serde::{Deserialize, Serialize};

use super::ConfigError;

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
