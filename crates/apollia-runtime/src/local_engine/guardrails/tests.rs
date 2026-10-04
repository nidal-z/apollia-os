use std::sync::{Arc, Mutex};

use super::*;

fn estimate(total_gb: f64, badge: CompatibilityBadge) -> LoadEstimate {
    LoadEstimate {
        total_gb,
        capacity_gb: 24.0,
        badge,
    }
}

fn policy(level: GuardrailLevel) -> GuardrailPolicy {
    GuardrailPolicy {
        level,
        ceiling_gb: None,
    }
}

const GRADES: [CompatibilityBadge; 3] = [
    CompatibilityBadge::Fits,
    CompatibilityBadge::MightFit,
    CompatibilityBadge::TooLarge,
];

fn verdicts(policy: &GuardrailPolicy) -> Vec<GuardrailVerdict> {
    GRADES
        .iter()
        .map(|badge| decide(policy, &estimate(30.0, *badge)))
        .collect()
}

/// Records the message of every tracing event, so a test can assert that a
/// named event was published. No subscriber crate is a dependency here, and
/// this needs none.
#[derive(Default, Clone)]
struct EventNames(Arc<Mutex<Vec<String>>>);

struct MessageVisitor<'a>(&'a mut Option<String>);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = Some(format!("{value:?}"));
        }
    }
}

impl tracing::Subscriber for EventNames {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = None;
        event.record(&mut MessageVisitor(&mut message));
        if let Some(m) = message {
            self.0.lock().expect("event lock").push(m);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[test]
fn test_the_disabled_level_launches_everything() {
    // GIVEN the disabled level
    let disabled = policy(GuardrailLevel::Disabled);

    // WHEN it judges a model of each grade
    let seen = verdicts(&disabled);

    // THEN every one is launched, too large included
    assert_eq!(seen, vec![GuardrailVerdict::Allowed; 3]);
}

#[test]
fn test_the_lenient_level_warns_past_might_fit() {
    // GIVEN the lenient level
    let lenient = policy(GuardrailLevel::Lenient);

    // WHEN it judges a model of each grade
    let seen = verdicts(&lenient);

    // THEN nothing is refused, and a model too large is launched with a warning
    assert_eq!(
        seen,
        vec![
            GuardrailVerdict::Allowed,
            GuardrailVerdict::Allowed,
            GuardrailVerdict::Warned
        ]
    );
}

#[test]
fn test_the_balanced_level_launches_up_to_might_fit() {
    // GIVEN the balanced level, which is also the default
    let balanced = policy(GuardrailLevel::Balanced);
    assert_eq!(GuardrailPolicy::default().level, GuardrailLevel::Balanced);

    // WHEN it judges a model of each grade
    let seen = verdicts(&balanced);

    // THEN fits and might fit are launched, too large is refused
    assert_eq!(
        seen,
        vec![
            GuardrailVerdict::Allowed,
            GuardrailVerdict::Allowed,
            GuardrailVerdict::Refused
        ]
    );
}

#[test]
fn test_the_strict_level_launches_fits_only() {
    // GIVEN the strict level
    let strict = policy(GuardrailLevel::Strict);

    // WHEN it judges a model of each grade
    let seen = verdicts(&strict);

    // THEN only a model that fits is launched
    assert_eq!(
        seen,
        vec![
            GuardrailVerdict::Allowed,
            GuardrailVerdict::Refused,
            GuardrailVerdict::Refused
        ]
    );
}

#[test]
fn test_the_ceiling_level_launches_an_estimate_at_most_the_ceiling() {
    // GIVEN a ceiling of 12 GB
    let ceiling = GuardrailPolicy {
        level: GuardrailLevel::Ceiling,
        ceiling_gb: Some(12.0),
    };

    // WHEN it judges models that fit the machine, at 11, 12 and 13 GB
    let at = |gb: f64| decide(&ceiling, &estimate(gb, CompatibilityBadge::Fits));

    // THEN the ceiling itself is launched and one gigabyte above is refused,
    // whatever the grade says
    assert_eq!(at(11.0), GuardrailVerdict::Allowed);
    assert_eq!(at(12.0), GuardrailVerdict::Allowed);
    assert_eq!(at(13.0), GuardrailVerdict::Refused);
}

#[test]
fn test_a_ceiling_level_without_a_usable_ceiling_holds_to_balanced() {
    // GIVEN the ceiling level with no ceiling, and with a negative one
    let absent = policy(GuardrailLevel::Ceiling);
    let negative = GuardrailPolicy {
        level: GuardrailLevel::Ceiling,
        ceiling_gb: Some(-4.0),
    };

    // WHEN each judges a model of each grade
    // THEN both decide as the balanced level does, never as disabled
    let balanced = verdicts(&policy(GuardrailLevel::Balanced));
    assert_eq!(verdicts(&absent), balanced);
    assert_eq!(verdicts(&negative), balanced);
}

#[test]
fn test_a_refusal_carries_the_estimate_and_the_budget_for_the_api() {
    // GIVEN the strict level and a model estimated at 18.25 GB that might fit
    // a machine holding 16 GB
    let strict = policy(GuardrailLevel::Strict);
    let big = LoadEstimate {
        total_gb: 18.25,
        capacity_gb: 16.0,
        badge: CompatibilityBadge::MightFit,
    };

    // WHEN an unconfirmed launch is held to it
    let refusal = enforce(&strict, Some(&big), LoadConsent::Unconfirmed, "qwen.gguf")
        .expect_err("strict refuses what only might fit");
    let body = serde_json::to_value(GuardrailRefusedBody::from(&refusal)).expect("serialises");

    // THEN the error is typed and carries the figures, and the API body holds
    // a stable code and the figures an interface words in French or English,
    // beside the English sentence
    assert_eq!(
        refusal,
        GuardrailRefusal {
            model: "qwen.gguf".to_owned(),
            level: GuardrailLevel::Strict,
            estimated_gb: 18.25,
            budget_gb: 16.0,
        }
    );
    assert_eq!(body["code"], "guardrail_refused");
    assert_eq!(body["level"], "strict");
    assert_eq!(body["model"], "qwen.gguf");
    assert_eq!(body["estimatedGb"], 18.25);
    assert_eq!(body["budgetGb"], 16.0);
    assert_eq!(
        body["error"],
        "loading qwen.gguf was refused by the strict load guardrail: it needs an estimated \
         18.2 GB and this machine allows 16.0 GB; confirm the load to go ahead"
    );
}

#[test]
fn test_a_refusal_under_a_ceiling_reports_the_ceiling_as_its_budget() {
    // GIVEN a ceiling of 8 GB on a machine holding 24
    let ceiling = GuardrailPolicy {
        level: GuardrailLevel::Ceiling,
        ceiling_gb: Some(8.0),
    };

    // WHEN a 9 GB model is held to it
    let refusal = enforce(
        &ceiling,
        Some(&estimate(9.0, CompatibilityBadge::Fits)),
        LoadConsent::Unconfirmed,
        "m",
    )
    .expect_err("above the ceiling");

    // THEN the budget named is the ceiling, not the machine
    assert_eq!(refusal.budget_gb, 8.0);
}

#[test]
fn test_a_confirmed_launch_goes_past_a_refusal_and_is_logged_as_bypassed() {
    let _site = BYPASS_EVENT_SITE.blocking_lock();
    // GIVEN the balanced level, a model too large for the machine, and a
    // recorder of the events published
    let balanced = policy(GuardrailLevel::Balanced);
    let big = estimate(40.0, CompatibilityBadge::TooLarge);
    let names = EventNames::default();

    // WHEN the same launch is asked unconfirmed, then confirmed
    let (refused, confirmed) = tracing::subscriber::with_default(names.clone(), || {
        (
            enforce(&balanced, Some(&big), LoadConsent::Unconfirmed, "m"),
            enforce(&balanced, Some(&big), LoadConsent::Confirmed, "m"),
        )
    });

    // THEN the first is refused, the second goes ahead, and only the second
    // publishes `llm.guardrail.bypassed`
    assert!(refused.is_err());
    assert_eq!(confirmed, Ok(()));
    let seen = names.0.lock().expect("event lock").clone();
    assert_eq!(
        seen.iter()
            .filter(|m| *m == "llm.guardrail.bypassed")
            .count(),
        1,
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|m| m == "llm.guardrail.refused"),
        "{seen:?}"
    );
}

#[test]
fn test_a_footprint_that_cannot_be_estimated_is_launched() {
    // GIVEN the strict level and no estimate
    let strict = policy(GuardrailLevel::Strict);

    // WHEN an unconfirmed launch is held to it
    let outcome = enforce(&strict, None, LoadConsent::Unconfirmed, "m");

    // THEN it goes ahead: the guardrail judges estimates, it does not guess
    assert_eq!(outcome, Ok(()));
}

#[test]
fn test_the_section_is_read_under_llm_guardrails() {
    // GIVEN an apollia.toml with other sections and a guardrail section
    let text = r#"
        [runtime]
        log_level = "info"

        [llm.routing]
        precise = "a"
        fast = "b"

        [llm.guardrails]
        level = "ceiling"
        ceiling_gb = 12.5
    "#;

    // WHEN the policy is read from it
    let read = GuardrailPolicy::from_toml_str(text).expect("valid");

    // THEN the level and the ceiling are those of the section
    assert_eq!(
        read,
        GuardrailPolicy {
            level: GuardrailLevel::Ceiling,
            ceiling_gb: Some(12.5),
        }
    );
}

#[test]
fn test_a_missing_or_unreadable_section_is_the_balanced_default() {
    // GIVEN a file without the section, a file with an unknown level, and no
    // file at all
    let dir = tempfile::tempdir().expect("tempdir");
    let without = dir.path().join("without.toml");
    std::fs::write(&without, "[runtime]\nlog_level = \"info\"\n").expect("write");
    let typo = dir.path().join("typo.toml");
    std::fs::write(&typo, "[llm.guardrails]\nlevel = \"off\"\n").expect("write");

    // WHEN the policy is read from each
    // THEN each is balanced: a typo never turns the guardrail off
    let balanced = GuardrailPolicy::default();
    assert_eq!(GuardrailPolicy::read(Some(&without)), balanced);
    assert_eq!(GuardrailPolicy::read(Some(&typo)), balanced);
    assert_eq!(
        GuardrailPolicy::read(Some(&dir.path().join("absent.toml"))),
        balanced
    );
    assert_eq!(GuardrailPolicy::read(None), balanced);
}

#[test]
fn test_an_estimate_of_an_mlx_folder_is_graded_on_the_budget() {
    // GIVEN a unified-memory machine with a 12 GB budget and two estimates,
    // one under it and one over it
    let profile = HardwareProfile {
        total_ram_gb: 16.0,
        available_ram_gb: 8.0,
        cpu_model: "Apple M2".to_owned(),
        cpu_cores: 8,
        accelerator: apollia_llm::hardware::AcceleratorProfile::AppleSilicon {
            chip: "M2".to_owned(),
            generation: 2,
            vram_gb: 16.0,
        },
        memory_budget_gb: 12.0,
    };
    let footprint = |total_gb: f64| MemoryEstimate {
        weights_gb: total_gb,
        kv_cache_gb: 0.0,
        overhead_gb: 0.0,
        total_gb,
        kv_cache_measured: true,
        layout: None,
    };

    // WHEN each is graded
    let small = LoadEstimate::graded(&footprint(6.0), &profile);
    let large = LoadEstimate::graded(&footprint(14.0), &profile);

    // THEN the one within the budget fits and the other is too large
    assert_eq!(small.badge, CompatibilityBadge::Fits);
    assert_eq!(large.badge, CompatibilityBadge::TooLarge);
    assert_eq!(small.capacity_gb, 12.0);
}

#[test]
fn test_the_levels_are_the_ones_the_settings_contract_accepts() {
    // GIVEN every level of the guardrail
    let levels = [
        GuardrailLevel::Disabled,
        GuardrailLevel::Lenient,
        GuardrailLevel::Balanced,
        GuardrailLevel::Strict,
        GuardrailLevel::Ceiling,
    ];

    // WHEN their wire names are listed
    let names: Vec<&str> = levels.iter().map(|level| level.as_str()).collect();

    // THEN they are exactly the values a policy may fix the level at
    assert_eq!(names, apollia_core::config::GUARDRAIL_LEVELS);
}

#[test]
fn test_a_level_the_policy_fixes_wins_over_the_file_and_refuses_a_write() {
    // GIVEN an apollia.toml turning the guardrail off, and a policy fixing the
    // level at `ceiling` with a ceiling of 0.5 GB
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("apollia.toml");
    std::fs::write(&config, "[llm.guardrails]\nlevel = \"disabled\"\n").expect("write");
    let imposed = apollia_core::config::PolicySettings::none()
        .impose(LEVEL_SETTING, serde_json::json!("ceiling"))
        .impose(CEILING_SETTING, serde_json::json!(0.5));

    // WHEN the guardrail is read for a launch, then the user writes the level
    let read = GuardrailPolicy::read_under(Some(&config), &imposed);
    let written = apollia_core::config::write_setting(
        &config,
        LEVEL_SETTING,
        serde_json::json!("disabled"),
        &imposed,
    );

    // THEN the policy's level and ceiling apply, the write is refused, and the
    // file is left as it was
    assert_eq!(
        read,
        GuardrailPolicy {
            level: GuardrailLevel::Ceiling,
            ceiling_gb: Some(0.5),
        }
    );
    assert!(matches!(
        written,
        Err(apollia_core::config::SettingError::LockedByPolicy { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        "[llm.guardrails]\nlevel = \"disabled\"\n"
    );
}
