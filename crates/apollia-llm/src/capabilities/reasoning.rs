//! How a model lets its caller steer its reasoning: not at all, by a switch, or
//! by a level.
//!
//! The answer is read in the chat template, the only place where the engine
//! honours such a setting. A template reads one of two variables, which the
//! OpenAI-compatible engines fill from the `chat_template_kwargs` of a request:
//!
//! - `enable_thinking`, a switch (Qwen3 and the families that copied it);
//! - `reasoning_effort`, a level among `low`, `medium` and `high` (gpt-oss and
//!   the families that copied it).
//!
//! A template that reads neither variable offers no setting, even when its
//! model reasons: sending one would change nothing, and no text is ever
//! injected into a message to make up for it. [`FAMILY_PROFILES`] settles the
//! cases the template alone leaves open: a template that reads both variables,
//! and a model whose template is not at hand.

use serde::{Deserialize, Serialize};

/// The reasoning setting a model exposes to its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningProfile {
    /// No setting: the model reasons or not on its own.
    None,
    /// Reasoning on or off, through the `enable_thinking` template variable.
    Toggle,
    /// A level of reasoning (low, medium, high), through the
    /// `reasoning_effort` template variable.
    Levels,
}

/// The template variable a [`ReasoningProfile::Toggle`] model reads.
pub const THINKING_VARIABLE: &str = "enable_thinking";

/// The template variable a [`ReasoningProfile::Levels`] model reads.
pub const REASONING_EFFORT_VARIABLE: &str = "reasoning_effort";

/// Families whose profile the template alone does not settle, keyed by the
/// architecture a GGUF header names (`general.architecture`) or the
/// `model_type` of an MLX `config.json`, compared without case, `_` read as
/// `-`.
///
/// gpt-oss is the family that defined `reasoning_effort`: its template always
/// reads it, and a template that also mentions `enable_thinking` stays a level
/// template.
pub const FAMILY_PROFILES: &[(&str, ReasoningProfile)] = &[("gpt-oss", ReasoningProfile::Levels)];

/// The reasoning variables a chat template reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateReasoningVariables {
    /// The template reads `enable_thinking`.
    pub thinking: bool,
    /// The template reads `reasoning_effort`.
    pub effort: bool,
}

impl TemplateReasoningVariables {
    /// The variables `template` reads.
    #[must_use]
    pub fn of(template: &str) -> Self {
        Self {
            thinking: template.contains(THINKING_VARIABLE),
            effort: template.contains(REASONING_EFFORT_VARIABLE),
        }
    }
}

/// The profile [`FAMILY_PROFILES`] gives `family`, if any.
#[must_use]
pub fn family_profile(family: &str) -> Option<ReasoningProfile> {
    let key = family.trim().to_ascii_lowercase().replace('_', "-");
    FAMILY_PROFILES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, profile)| *profile)
}

/// The profile of a model whose chat template reads `variables`, `None` when
/// it has no template, and whose family is `family` when known.
#[must_use]
pub fn reasoning_profile(
    variables: Option<TemplateReasoningVariables>,
    family: Option<&str>,
) -> ReasoningProfile {
    let from_family = || family.and_then(family_profile);
    match variables {
        None => from_family().unwrap_or(ReasoningProfile::None),
        Some(v) => match (v.thinking, v.effort) {
            (false, false) => ReasoningProfile::None,
            (true, false) => ReasoningProfile::Toggle,
            (false, true) => ReasoningProfile::Levels,
            (true, true) => from_family().unwrap_or(ReasoningProfile::Levels),
        },
    }
}

/// The `model_type` of an MLX `config.json`, the family the MLX conversion
/// keeps from its upstream model. Unreadable JSON names none.
#[must_use]
pub fn mlx_config_model_type(config_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(config_json)
        .ok()?
        .get("model_type")?
        .as_str()
        .map(str::to_owned)
}
