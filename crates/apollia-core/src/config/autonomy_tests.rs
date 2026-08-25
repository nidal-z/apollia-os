use super::hooks::default_hook_timeout_ms;
use super::*;
use crate::budget::StepBudgetConfig;
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
        format_version: 1,
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
        format_version: 1,
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
        format_version: 1,
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
        format_version: 1,
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
            format_version: 1,
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
            format_version: 1,
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
            format_version: 1,
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

#[test]
fn test_chat_config_default_plan_mode_off() {
    // GIVEN the default chat config
    let cfg = ChatConfig::default();
    // THEN plan mode is off by default
    assert!(!cfg.plan_mode_default);
}

#[test]
fn test_chat_config_absent_field_defaults_off() {
    // GIVEN an empty [chat] section
    let toml = "";
    // WHEN deserialized
    let cfg: ChatConfig = toml::from_str(toml).expect("valid toml");
    // THEN the missing field falls back to false
    assert!(!cfg.plan_mode_default);
}

#[test]
fn test_chat_config_plan_mode_default_parses_true() {
    // GIVEN a [chat] section enabling plan mode by default
    let toml = "plan_mode_default = true";
    // WHEN deserialized
    let cfg: ChatConfig = toml::from_str(toml).expect("valid toml");
    // THEN the flag is on
    assert!(cfg.plan_mode_default);
}
