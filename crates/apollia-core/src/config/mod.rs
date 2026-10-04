//! Apollia OS runtime configuration.
//!
//! [`ApolliaConfig`] is the single root of `apollia.toml`: every section the
//! runtime, the CLI and the desktop application read, strict at every level.
//! [`validate`] turns the text of the file into that root or into diagnostics
//! placed at their line, and [`keys`] is the registry of every key, held to the
//! structures by its test. The sections defined here include:
//! - [`RuntimeConfig`]: `[runtime]`, EventBus and mailbox capacity.
//! - [`ApiConfig`]: `[api]`, the TCP listener, authentication, TLS, and the
//!   Unix socket.
//! - [`HitlConfig`]: `[hitl]`, the Human-in-the-Loop watcher.
//! - [`ToolsConfig`]: `[tools]`, native tools and their per-tool settings.
//! - [`McpConfig`]: `[mcp]`, the MCP client.
//! - [`HooksConfig`]: `[hooks]`, lifecycle hook handlers.
//! - [`ChatConfig`]: `[chat]`, chat session defaults.
//! - [`FilesystemConfig`]: `[filesystem]`, the trusted paths.
//! - [`ModelsConfig`]: `[models]`, the signed index of model cards the
//!   catalogue reads, its mirror, the switch that turns it off, and the rule
//!   applied to an imported file.
//! - [`DesktopConfig`]: `[desktop]`, the settings of the desktop application
//!   that Rust reads, each a named setting resolved with its source by
//!   [`resolve_setting`] and written by [`write_setting`], which refuses a key
//!   the organization policy locks.
//! - [`ServerConfig`]: `[server]`, the compatible server, a second listener
//!   of the runtime on its own port, switched off by default; its two keys
//!   are named settings too.
//! - [`EnginesConfig`]: `[engines]`, the settings of the local inference
//!   engines, the engine log level first; named settings, read when a model
//!   loads.
//! - [`AuthorConfig`]: `[author]`, the publisher name of the agents exported
//!   from this machine.
//! - [`LlmConfig`]: `[llm]`, the backends declared in the file, routing and
//!   the prompt debug log; `apollia-llm` re-exports its shapes. Its
//!   `[llm.residency]` subsection, [`LlmResidencyConfig`], is a named setting
//!   resolved by [`resolve_setting`].
//!
//! [`default_file_text`] renders the registry as the `apollia.toml` written on a
//! first start, every key commented at its default under its documentation,
//! and [`write_default_file_if_absent`] writes it without ever replacing a file.
//!
//! The other types in this module are runtime parameters, not `apollia.toml`
//! sections. No loader deserializes an `[a2a]` or an `[oria]` table, both are
//! withdrawn keys of the registry: [`A2AConfig`] reaches the runtime from its
//! [`Default`] alone, and [`ORIAConfig`] from its [`Default`] plus whatever the
//! caller overrides in code, which today is the autonomy tier passed on the
//! command line.
//!
//! Every field has a sane default via [`Default`].

mod a2a;
mod api;
mod author;
mod autonomy;
mod chat;
mod default_file;
mod desktop;
mod engines;
mod filesystem;
mod filesystem_risk;
mod hitl;
mod hooks;
pub mod keys;
mod llm;
mod llm_residency;
mod llm_section;
mod mcp;
mod models;
mod oria;
mod permissions;
mod registry;
mod retired;
mod root;
mod runtime;
mod scan;
mod server;
mod settings;
mod tools;
mod triggers;
mod validate;
mod web;
mod workflows;

#[cfg(test)]
mod autonomy_tests;
#[cfg(test)]
mod default_file_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validate_tests;

pub use a2a::A2AConfig;
pub use api::{ApiConfig, DEFAULT_API_PORT};
pub use author::{AuthorConfig, AUTHOR_NAME_KEY};
pub use autonomy::{
    AutonomyConfig, AutonomyLevel, AutonomyLevelConfig, AutonomyLevelParseError, GatePolicy,
};
pub use chat::ChatConfig;
pub use default_file::{default_file_text, write_default_file_if_absent};
pub use desktop::{DesktopConfig, UpdateChannel};
pub use engines::{EngineLogLevel, EnginesConfig, ENGINE_LOG_LEVELS};
pub use filesystem::FilesystemConfig;
pub use filesystem_risk::FilesystemRiskConfig;
pub use hitl::HitlConfig;
pub use hooks::{HookEventKind, HookHandlerConfig, HookHandlerKind, HooksConfig};
pub use llm::{CeilingAction, HybridRoutingConfig, LlmRoutingConfig};
pub use llm_residency::{LlmResidencyConfig, DEFAULT_IDLE_UNLOAD_MINUTES, IDLE_UNLOAD_KEY};
pub use llm_section::{
    infer_api_provider_from_url, LlmApiBackendEntry, LlmBackendEntry, LlmBackendKind, LlmConfig,
    LlmGuardrailsConfig, LlmObservabilityConfig,
};
pub use mcp::{McpConfig, McpToolLoading};
pub use models::{
    default_models_dir, models_dir, resolve_models_dir, ModelsConfig, ModelsImportConfig,
    DEFAULT_MODEL_REGISTRY_MIRROR_URL, DEFAULT_MODEL_REGISTRY_URL, MODELS_DIR_KEY, MODELS_DIR_NAME,
};
pub use oria::ORIAConfig;
pub use permissions::BashValidatorConfig;
pub use registry::RegistryConfig;
pub(crate) use retired::warn_retired_keys;
pub use root::ApolliaConfig;
pub use runtime::RuntimeConfig;
pub use server::{ServerConfig, DEFAULT_COMPAT_SERVER_PORT};
pub use settings::{
    check_imposed_setting, resolve_setting, resolve_setting_in, write_setting, PolicySettings,
    ResolvedSetting, SettingError, SettingSource, GUARDRAIL_LEVELS,
};
pub use tools::ToolsConfig;
pub use triggers::TriggersConfig;
pub use validate::{validate, ConfigDiagnostic};
pub use web::{
    BraveBackendConfig, DuckDuckGoBackendConfig, WebReadConfig, WebSearchBackend, WebSearchConfig,
};
pub use workflows::WorkflowsConfig;

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Configuration validation error raised at startup.
///
/// Produced by the `validate()` methods of the section configs. The runtime
/// must treat these as fatal errors (fail-fast principle).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
