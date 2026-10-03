//! Load guardrails: a model that would saturate the machine is not launched
//! unless the user confirms.
//!
//! Before an engine launches a process, the footprint of the model is sized by
//! the one estimator the Hub, the ranking and the `-ngl` plan already use
//! (`apollia_llm::recommend`), with the window and the slot count the process
//! would really be launched with, and graded against the machine. The level
//! read from `[llm.guardrails]` in `apollia.toml` then decides:
//!
//! | Level | Launches |
//! |---|---|
//! | `disabled` | everything, no estimate is made |
//! | `lenient` | everything, with a warning past "might fit" |
//! | `balanced` (default) | up to "might fit" |
//! | `strict` | "fits" only |
//! | `ceiling` | an estimate of at most `ceiling_gb` |
//!
//! ```toml
//! [llm.guardrails]
//! level = "ceiling"
//! ceiling_gb = 12.0
//! ```
//!
//! The section is read at every launch, so a change applies to the next load
//! without a restart. A level or a ceiling the organisation policy fixes in
//! its `[settings]` (`llm.guardrails.level`, `llm.guardrails.ceiling_gb`) wins
//! over the file, see [`GuardrailPolicy::read_under`]. A refusal is a [`GuardrailRefusal`] carrying the estimate
//! and the limit; a launch asked with [`LoadConsent::Confirmed`] goes past it
//! and is logged as `llm.guardrail.bypassed`. A model whose footprint cannot be
//! estimated (an unreadable file, an MLX folder without its `config.json`) is
//! launched: the guardrail judges estimates, it does not guess them.

use std::path::Path;

use apollia_core::config::PolicySettings;
use apollia_llm::hardware::{CompatibilityBadge, HardwareProfile};
use apollia_llm::recommend::{MemoryArchitecture, MemoryEstimate, OffloadPlan, RuntimeShape};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// The setting that fixes the level, as `[settings]` of the organisation
/// policy names it.
pub const LEVEL_SETTING: &str = "llm.guardrails.level";

/// The setting that fixes the ceiling, in gigabytes; `0` sets none.
pub const CEILING_SETTING: &str = "llm.guardrails.ceiling_gb";

/// Machine-readable code of a refusal on the wire.
pub const GUARDRAIL_REFUSED_CODE: &str = "guardrail_refused";

/// How strictly a launch is held to the machine's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailLevel {
    /// No check at all.
    Disabled,
    /// Launch past "might fit", with a warning.
    Lenient,
    /// Launch up to "might fit". The default.
    #[default]
    Balanced,
    /// Launch "fits" only.
    Strict,
    /// Launch an estimate of at most the configured ceiling.
    Ceiling,
}

impl GuardrailLevel {
    /// Wire name, as written in `apollia.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Lenient => "lenient",
            Self::Balanced => "balanced",
            Self::Strict => "strict",
            Self::Ceiling => "ceiling",
        }
    }

    /// The level a wire name designates, `None` for an unknown one.
    #[must_use]
    pub(crate) fn from_wire(name: &str) -> Option<Self> {
        [
            Self::Disabled,
            Self::Lenient,
            Self::Balanced,
            Self::Strict,
            Self::Ceiling,
        ]
        .into_iter()
        .find(|level| level.as_str() == name)
    }
}

impl std::fmt::Display for GuardrailLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `[llm.guardrails]` section of `apollia.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
pub struct GuardrailPolicy {
    /// The level, `balanced` when the key is absent.
    #[serde(default)]
    pub level: GuardrailLevel,
    /// Largest estimate, in gigabytes, the `ceiling` level launches.
    #[serde(default)]
    pub ceiling_gb: Option<f64>,
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    llm: LlmSection,
}

#[derive(Default, Deserialize)]
struct LlmSection {
    #[serde(default)]
    guardrails: GuardrailPolicy,
}

impl GuardrailPolicy {
    /// Read `[llm.guardrails]` from the text of `apollia.toml`.
    ///
    /// # Errors
    ///
    /// The parse error when the text is not valid TOML or the section holds a
    /// value of the wrong type or an unknown level.
    pub fn from_toml_str(text: &str) -> Result<Self, toml::de::Error> {
        let file: ConfigFile = toml::from_str(text)?;
        Ok(file.llm.guardrails)
    }

    /// The policy in force now, read from `path`.
    ///
    /// A missing file or a missing section is the default, `balanced`. An
    /// unreadable section is the default too, with a warning: a typo in the
    /// file must not turn the guardrail off.
    #[must_use]
    pub fn read(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match Self::from_toml_str(&text) {
            Ok(policy) => policy,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %path.display(),
                    error = %error,
                    detail = "the balanced level applies",
                    "llm.guardrail.config_unreadable"
                );
                Self::default()
            }
        }
    }

    /// The policy in force now: [`read`](Self::read) from `path`, then the
    /// level and the ceiling the organisation policy fixes in `imposed`, which
    /// win over the file.
    #[must_use]
    pub fn read_under(path: Option<&Path>, imposed: &PolicySettings) -> Self {
        let mut policy = Self::read(path);
        if let Some(level) = imposed
            .imposed(LEVEL_SETTING)
            .and_then(serde_json::Value::as_str)
            .and_then(GuardrailLevel::from_wire)
        {
            policy.level = level;
        }
        if let Some(gb) = imposed
            .imposed(CEILING_SETTING)
            .and_then(serde_json::Value::as_f64)
        {
            policy.ceiling_gb = (gb > 0.0).then_some(gb);
        }
        policy
    }

    /// The ceiling the `ceiling` level holds to: a positive, finite value, or
    /// `None` when the key is absent or unusable.
    fn usable_ceiling(&self) -> Option<f64> {
        self.ceiling_gb.filter(|gb| gb.is_finite() && *gb > 0.0)
    }
}

/// Whether the user explicitly confirmed a launch the guardrail may refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadConsent {
    /// An ordinary request: a refusal stops the launch.
    #[default]
    Unconfirmed,
    /// The user confirmed: a refusal is logged and the launch goes ahead.
    Confirmed,
}

/// A model's footprint, graded against the machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadEstimate {
    /// Weights, cache and engine overhead, in gigabytes.
    pub total_gb: f64,
    /// What the machine can hold, in gigabytes: the budget the grade is
    /// measured against.
    pub capacity_gb: f64,
    /// The grade `fit::grade` gives the estimate on this machine.
    pub badge: CompatibilityBadge,
}

impl LoadEstimate {
    /// Size a GGUF file of `file_size_bytes` launched with `shape`.
    ///
    /// `path` is probed for its header; a header that cannot be read leaves
    /// the cache to the estimator's proportional fallback, as in the Hub.
    #[must_use]
    pub fn gguf(
        path: &Path,
        file_size_bytes: u64,
        shape: &RuntimeShape,
        profile: &HardwareProfile,
    ) -> Self {
        let facts =
            apollia_llm::gguf_probe::probe_file(path, apollia_llm::gguf_probe::SCREEN_BYTES)
                .unwrap_or_default();
        let fit = apollia_llm::recommend::assess_fit(file_size_bytes, &facts, profile, shape);
        Self {
            total_gb: fit.estimate.total_gb,
            capacity_gb: MemoryArchitecture::of(profile).total_capacity_gb(profile),
            badge: fit.badge,
        }
    }

    /// Size the MLX folder `dir` served with `shape`, or `None` when its
    /// `config.json` does not carry what the cache formula needs.
    #[must_use]
    pub fn mlx(dir: &Path, shape: &RuntimeShape, profile: &HardwareProfile) -> Option<Self> {
        let estimate = apollia_llm::recommend::estimate_mlx_memory(dir, shape).ok()?;
        Some(Self::graded(&estimate, profile))
    }

    /// Grade an estimate of a model that runs whole on unified memory, as MLX
    /// does: it is on the accelerator exactly when the budget holds it.
    #[must_use]
    pub fn graded(estimate: &MemoryEstimate, profile: &HardwareProfile) -> Self {
        let capacity_gb = MemoryArchitecture::of(profile).total_capacity_gb(profile);
        let plan = OffloadPlan {
            n_gpu_layers: 0,
            total_layers: 0,
            device_gb: estimate.total_gb,
            host_gb: 0.0,
            fully_offloaded: estimate.total_gb < profile.memory_budget_gb,
            tokens_per_second: 0.0,
        };
        Self {
            total_gb: estimate.total_gb,
            capacity_gb,
            badge: apollia_llm::recommend::grade(estimate, &plan, profile),
        }
    }
}

/// What a level makes of an estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailVerdict {
    /// The launch goes ahead.
    Allowed,
    /// The launch goes ahead, past what the machine holds comfortably.
    Warned,
    /// The launch is refused unless the user confirms.
    Refused,
}

/// The verdict of `policy` on `estimate`.
///
/// A `ceiling` level without a usable `ceiling_gb` holds to `balanced`.
#[must_use]
pub fn decide(policy: &GuardrailPolicy, estimate: &LoadEstimate) -> GuardrailVerdict {
    let too_large = estimate.badge == CompatibilityBadge::TooLarge;
    match policy.level {
        GuardrailLevel::Disabled => GuardrailVerdict::Allowed,
        GuardrailLevel::Lenient if too_large => GuardrailVerdict::Warned,
        GuardrailLevel::Lenient => GuardrailVerdict::Allowed,
        GuardrailLevel::Strict if estimate.badge == CompatibilityBadge::Fits => {
            GuardrailVerdict::Allowed
        }
        GuardrailLevel::Strict => GuardrailVerdict::Refused,
        GuardrailLevel::Ceiling => match policy.usable_ceiling() {
            Some(ceiling) if estimate.total_gb <= ceiling => GuardrailVerdict::Allowed,
            Some(_) => GuardrailVerdict::Refused,
            None if too_large => GuardrailVerdict::Refused,
            None => GuardrailVerdict::Allowed,
        },
        GuardrailLevel::Balanced if too_large => GuardrailVerdict::Refused,
        GuardrailLevel::Balanced => GuardrailVerdict::Allowed,
    }
}

/// A launch the guardrail refused.
#[derive(Debug, Clone, PartialEq, Error)]
#[error(
    "loading {model} was refused by the {level} load guardrail: it needs an estimated \
     {estimated_gb:.1} GB and this machine allows {budget_gb:.1} GB; confirm the load to go ahead"
)]
pub struct GuardrailRefusal {
    /// The model whose launch was refused, as the engine was asked for it.
    pub model: String,
    /// The level that refused it.
    pub level: GuardrailLevel,
    /// The estimated footprint, in gigabytes.
    pub estimated_gb: f64,
    /// The limit the level holds to, in gigabytes: the ceiling, or what the
    /// machine can hold.
    pub budget_gb: f64,
}

/// Body of an API answer to a launch the guardrail refused.
///
/// `error` is the English sentence; `code` and the figures let an interface
/// write its own, in its own language.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailRefusedBody {
    /// Human-readable description, in English.
    pub error: String,
    /// Always `guardrail_refused`.
    pub code: String,
    /// The model whose launch was refused.
    pub model: String,
    /// The level that refused it.
    pub level: GuardrailLevel,
    /// The estimated footprint, in gigabytes.
    pub estimated_gb: f64,
    /// The limit the level holds to, in gigabytes.
    pub budget_gb: f64,
}

impl GuardrailRefusedBody {
    /// The body of an LLM call a local engine refused to load, `None` for any
    /// other failure.
    #[must_use]
    pub(crate) fn from_llm_error(error: &apollia_llm::LlmError) -> Option<Self> {
        let apollia_llm::LlmError::LoadRefused {
            model,
            level,
            estimated_gb,
            budget_gb,
            ..
        } = error
        else {
            return None;
        };
        let refusal = GuardrailRefusal {
            model: model.clone(),
            level: GuardrailLevel::from_wire(level)?,
            estimated_gb: *estimated_gb,
            budget_gb: *budget_gb,
        };
        Some(Self::from(&refusal))
    }
}

impl From<&GuardrailRefusal> for GuardrailRefusedBody {
    fn from(refusal: &GuardrailRefusal) -> Self {
        Self {
            error: refusal.to_string(),
            code: GUARDRAIL_REFUSED_CODE.to_owned(),
            model: refusal.model.clone(),
            level: refusal.level,
            estimated_gb: refusal.estimated_gb,
            budget_gb: refusal.budget_gb,
        }
    }
}

/// Hold the launch of `model` to `policy`.
///
/// `estimate` is `None` when the footprint could not be sized; the launch then
/// goes ahead. A refusal confirmed by the user goes ahead too, and is logged as
/// `llm.guardrail.bypassed`.
///
/// # Errors
///
/// [`GuardrailRefusal`] when the level refuses the estimate and the user did
/// not confirm.
pub fn enforce(
    policy: &GuardrailPolicy,
    estimate: Option<&LoadEstimate>,
    consent: LoadConsent,
    model: &str,
) -> Result<(), GuardrailRefusal> {
    if policy.level == GuardrailLevel::Disabled {
        return Ok(());
    }
    let Some(estimate) = estimate else {
        tracing::event!(
            tracing::Level::WARN,
            model = %model,
            level = %policy.level,
            detail = "the footprint could not be estimated, the model is launched",
            "llm.guardrail.unestimated"
        );
        return Ok(());
    };
    let budget_gb = match (policy.level, policy.usable_ceiling()) {
        (GuardrailLevel::Ceiling, Some(ceiling)) => ceiling,
        _ => estimate.capacity_gb,
    };
    match decide(policy, estimate) {
        GuardrailVerdict::Allowed => Ok(()),
        GuardrailVerdict::Warned => {
            tracing::event!(
                tracing::Level::WARN,
                model = %model,
                level = %policy.level,
                estimated_gb = estimate.total_gb,
                budget_gb,
                "llm.guardrail.warned"
            );
            Ok(())
        }
        GuardrailVerdict::Refused if consent == LoadConsent::Confirmed => {
            tracing::event!(
                tracing::Level::WARN,
                model = %model,
                level = %policy.level,
                estimated_gb = estimate.total_gb,
                budget_gb,
                "llm.guardrail.bypassed"
            );
            Ok(())
        }
        GuardrailVerdict::Refused => {
            tracing::event!(
                tracing::Level::INFO,
                model = %model,
                level = %policy.level,
                estimated_gb = estimate.total_gb,
                budget_gb,
                "llm.guardrail.refused"
            );
            Err(GuardrailRefusal {
                model: model.to_owned(),
                level: policy.level,
                estimated_gb: estimate.total_gb,
                budget_gb,
            })
        }
    }
}

/// Serialises the tests that reach the `llm.guardrail.bypassed` event site.
/// Tracing caches the interest of a call site on first use, so a test that
/// hits the site on another thread with no subscriber can leave it disabled
/// while a recording test runs.
#[cfg(test)]
pub(crate) static BYPASS_EVENT_SITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests;
