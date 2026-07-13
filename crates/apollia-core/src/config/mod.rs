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

mod a2a;
mod api;
mod autonomy;
mod chat;
mod filesystem;
mod filesystem_risk;
mod hitl;
mod hooks;
mod llm;
mod mcp;
mod oria;
mod permissions;
mod registry;
mod runtime;
mod tools;
mod triggers;
mod web;

#[cfg(test)]
mod autonomy_tests;
#[cfg(test)]
mod tests;

pub use a2a::A2AConfig;
pub use api::ApiConfig;
pub use autonomy::{
    AutonomyConfig, AutonomyLevel, AutonomyLevelConfig, AutonomyLevelParseError, GatePolicy,
};
pub use chat::ChatConfig;
pub use filesystem::{FilesystemConfig, JournalConfig};
pub use filesystem_risk::FilesystemRiskConfig;
pub use hitl::HitlConfig;
pub use hooks::{HookEventKind, HookHandlerConfig, HookHandlerKind, HooksConfig};
pub use llm::{
    CeilingAction, HybridRoutingConfig, LlmRoutingConfig, LlmRunnerConfig, VertexConfig,
};
pub use mcp::{McpConfig, McpToolLoading};
pub use oria::ORIAConfig;
pub use permissions::{BashValidatorConfig, PermissionsConfig};
pub use registry::RegistryConfig;
pub use runtime::RuntimeConfig;
pub use tools::ToolsConfig;
pub use triggers::TriggersConfig;
pub use web::{
    BraveBackendConfig, DuckDuckGoBackendConfig, WebReadConfig, WebSearchBackend, WebSearchConfig,
};

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
