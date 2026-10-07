//! Unloading the resident models before another one is loaded.
//!
//! Several models stay loaded together by default, as many as the load
//! guardrail admits (see [`room`](super::room)). `[llm.residency]
//! unload_before_load` turns that around: a load first unloads every resident
//! model that is not pinned and answers no request, so one model is loaded at
//! a time. A pinned model stays, and so does a model in the middle of an
//! answer; their footprint still counts against the guardrail.
//!
//! The rule is read at every load, under the organization policy, and applies
//! to whatever asks for the load: the load window, a request that loads its
//! model lazily, the compatible server. Each engine applies it to its own
//! resident models.

use std::path::Path;

use apollia_core::config::{
    resolve_setting, LlmResidencyConfig, PolicySettings, UNLOAD_BEFORE_LOAD_KEY,
};

use crate::local_engine::guardrails::LoadEstimate;

use super::room::with_residents;

/// Whether a load first unloads the resident models, read now from
/// `config_file` under `imposed`.
///
/// Without a file, or when the file or the value cannot be read, the default
/// applies, with a warning in the second case: the models stay loaded.
#[must_use]
pub fn unload_before_load(config_file: Option<&Path>, imposed: &PolicySettings) -> bool {
    let default = LlmResidencyConfig::default().unload_before_load;
    let Some(path) = config_file else {
        return imposed
            .imposed(UNLOAD_BEFORE_LOAD_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default);
    };
    match resolve_setting(path, UNLOAD_BEFORE_LOAD_KEY, imposed) {
        Ok(resolved) => resolved.value.as_bool().unwrap_or(default),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                key = UNLOAD_BEFORE_LOAD_KEY,
                error = %error,
                "llm.residency.setting_unreadable"
            );
            default
        }
    }
}

/// A model resident in an engine, as the rule sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Replaceable {
    /// Path it was loaded from.
    pub model_path: String,
    /// Its estimated footprint, in gigabytes; `0` when it was not estimated.
    pub footprint_gb: f64,
    /// Whether the rule leaves it loaded: pinned, or answering a request.
    pub stays: bool,
}

/// The resident models a load unloads first, in the order given.
#[must_use]
pub fn replaced(residents: &[Replaceable]) -> Vec<String> {
    residents
        .iter()
        .filter(|r| !r.stays)
        .map(|r| r.model_path.clone())
        .collect()
}

/// `estimate` judged beside the resident models the rule leaves loaded: the
/// others are gone by the time the model loads.
#[must_use]
pub fn beside_the_ones_that_stay(
    estimate: &LoadEstimate,
    residents: &[Replaceable],
) -> LoadEstimate {
    let staying_gb: f64 = residents
        .iter()
        .filter(|r| r.stays)
        .map(|r| r.footprint_gb)
        .sum();
    with_residents(estimate, staying_gb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resident(path: &str, stays: bool) -> Replaceable {
        Replaceable {
            model_path: path.to_owned(),
            footprint_gb: 4.0,
            stays,
        }
    }

    #[test]
    fn test_the_rule_is_off_without_a_file_and_read_from_the_file() {
        // GIVEN no file, a file that says nothing, and a file that turns it on
        let dir = tempfile::tempdir().unwrap();
        let silent = dir.path().join("silent.toml");
        let on = dir.path().join("on.toml");
        std::fs::write(&silent, "[desktop]\ndeveloper_mode = true\n").unwrap();
        std::fs::write(&on, "[llm.residency]\nunload_before_load = true\n").unwrap();
        let none = PolicySettings::none();

        // WHEN the rule is read from each
        let without_file = unload_before_load(None, &none);
        let from_silent = unload_before_load(Some(&silent), &none);
        let from_on = unload_before_load(Some(&on), &none);

        // THEN it is off by default, and on where the file says so
        assert!(!without_file);
        assert!(!from_silent);
        assert!(from_on);
    }

    #[test]
    fn test_a_rule_the_policy_imposes_wins_over_the_file() {
        // GIVEN a file that turns the rule on, and a policy that imposes off
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[llm.residency]\nunload_before_load = true\n").unwrap();
        let policy =
            PolicySettings::none().impose(UNLOAD_BEFORE_LOAD_KEY, serde_json::json!(false));

        // WHEN the rule is read
        let rule = unload_before_load(Some(&path), &policy);

        // THEN the imposed value is the one in force
        assert!(!rule);
    }

    #[test]
    fn test_an_unreadable_file_keeps_the_models_loaded() {
        // GIVEN a file that is not valid TOML
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[llm.residency\nunload_before_load = ").unwrap();

        // WHEN the rule is read
        let rule = unload_before_load(Some(&path), &PolicySettings::none());

        // THEN the default applies: nothing is unloaded first
        assert!(!rule);
    }

    #[test]
    fn test_a_load_replaces_every_resident_model_that_does_not_stay() {
        // GIVEN three resident models, the second pinned or answering
        let residents = [
            resident("/m/a.gguf", false),
            resident("/m/b.gguf", true),
            resident("/m/c.gguf", false),
        ];

        // WHEN the models a load unloads first are listed
        let gone = replaced(&residents);

        // THEN they are the two that do not stay, in order
        assert_eq!(gone, ["/m/a.gguf", "/m/c.gguf"]);
    }

    #[test]
    fn test_the_new_model_is_judged_beside_the_ones_that_stay_only() {
        // GIVEN a model of 6 GB on a machine of 16, beside one model that
        //       stays and one that goes, 4 GB each
        let estimate = LoadEstimate {
            total_gb: 6.0,
            capacity_gb: 16.0,
            badge: apollia_llm::hardware::CompatibilityBadge::Fits,
        };
        let residents = [resident("/m/a.gguf", false), resident("/m/b.gguf", true)];

        // WHEN the estimate the guardrail judges is built
        let judged = beside_the_ones_that_stay(&estimate, &residents);

        // THEN it counts the model and the one that stays, not the one that
        //      goes
        assert!((judged.total_gb - 10.0).abs() < f64::EPSILON, "{judged:?}");
    }
}
