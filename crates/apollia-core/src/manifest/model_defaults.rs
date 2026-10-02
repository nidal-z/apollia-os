//! The model defaults an agent package declares.
//!
//! One shape, read from two places: `[agents.defaults.model]` in a package's
//! `apollia.toml`, and the `model` key of the manifest the SDK's `@agent`
//! decorator builds. Both are strict: an unknown key is refused, so a
//! misspelt setting never vanishes in silence.

use serde::{Deserialize, Serialize};

/// The model an agent package recommends, and how to query it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentModelDefaults {
    /// Model identifier, as the model catalogue names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Name of a model preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Request settings the package recommends for this agent: the package
    /// level of the request settings resolution, above the model's own
    /// defaults and below the preset the user chose for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<PackageRequestSettings>,
}

/// Request settings an agent package recommends.
///
/// The keys are those of the per-model sampling defaults (`temperature`,
/// `top_p`, `top_k`, `repetition_penalty`) and of the completion request
/// (`max_tokens`, `seed`), with their types. Every field is optional: an
/// absent one leaves the level below in force.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRequestSettings {
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Repetition penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repetition_penalty: Option<f32>,
    /// Maximum number of tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}
