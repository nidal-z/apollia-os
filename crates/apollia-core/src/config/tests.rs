use super::*;
use std::path::PathBuf;

// ── HybridRoutingConfig: ceiling_action ───────────────────────────────

#[test]
fn test_hybrid_config_default_ceiling_action() {
    // GIVEN a hybrid config payload without ceiling_action
    let raw = r#"{"frontier":"claude-opus-4","cost_ceiling_usd":2.0}"#;

    // WHEN deserializing
    let cfg: HybridRoutingConfig = serde_json::from_str(raw).expect("valid payload");

    // THEN ceiling_action defaults to StayLocal and validate passes
    assert_eq!(cfg.ceiling_action, CeilingAction::StayLocal);
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_hybrid_config_hard_stop_parsed() {
    // GIVEN a payload with ceiling_action = "hard_stop"
    let raw = r#"{"frontier":"claude-opus-4","cost_ceiling_usd":2.0,"ceiling_action":"hard_stop"}"#;

    // WHEN deserializing
    let cfg: HybridRoutingConfig = serde_json::from_str(raw).expect("valid payload");

    // THEN the action is HardStop
    assert_eq!(cfg.ceiling_action, CeilingAction::HardStop);
}

#[test]
fn test_hybrid_config_negative_ceiling_rejected() {
    // GIVEN a config with a negative ceiling
    let cfg = HybridRoutingConfig {
        frontier: "claude-opus-4".into(),
        cost_ceiling_usd: -1.0,
        ceiling_action: CeilingAction::StayLocal,
    };

    // WHEN validating
    let result = cfg.validate();

    // THEN the ceiling is rejected
    assert!(matches!(
        result,
        Err(ConfigError::HybridCeilingInvalid { value }) if value == -1.0
    ));
}

#[test]
fn test_hybrid_config_empty_frontier_rejected() {
    // GIVEN a config with an empty frontier
    let cfg = HybridRoutingConfig {
        frontier: String::new(),
        cost_ceiling_usd: 1.0,
        ceiling_action: CeilingAction::HardStop,
    };

    // WHEN validating
    // THEN the frontier is rejected
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::HybridFrontierMissing)
    ));
}

#[test]
fn test_hybrid_config_zero_ceiling_rejected() {
    // GIVEN a config with a zero ceiling (exact limit)
    let cfg = HybridRoutingConfig {
        frontier: "claude-opus-4".into(),
        cost_ceiling_usd: 0.0,
        ceiling_action: CeilingAction::StayLocal,
    };

    // WHEN validating
    // THEN zero is rejected (must be strictly positive)
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::HybridCeilingInvalid { value }) if value == 0.0
    ));
}

#[test]
fn test_ceiling_action_serde_round_trip() {
    // GIVEN the HardStop action
    let action = CeilingAction::HardStop;

    // WHEN serializing then deserializing
    let json = serde_json::to_string(&action).expect("serialize");
    let back: CeilingAction = serde_json::from_str(&json).expect("deserialize");

    // THEN the value is preserved and renders snake_case
    assert_eq!(json, "\"hard_stop\"");
    assert_eq!(back, action);
}

// ── McpConfig: tool_loading + tool_search_limit ────────────────────────

#[test]
fn test_mcp_config_default_is_deferred_limit_20() {
    // GIVEN a default McpConfig
    let cfg = McpConfig::default();
    // WHEN its fields are read
    // THEN the loading mode is deferred and the search cap is 20
    assert_eq!(cfg.tool_loading, McpToolLoading::Deferred);
    assert_eq!(cfg.tool_search_limit, 20);
    assert_eq!(cfg.approval_ttl_hours, 24);
}

#[test]
fn test_mcp_config_deserializes_eager_mode() {
    // GIVEN a config selecting eager mode with an explicit limit
    let json = serde_json::json!({
        "tool_loading": "eager",
        "tool_search_limit": 10
    });
    // WHEN it is deserialized
    let cfg: McpConfig = serde_json::from_value(json).unwrap();
    // THEN both values are taken from the input
    assert_eq!(cfg.tool_loading, McpToolLoading::Eager);
    assert_eq!(cfg.tool_search_limit, 10);
}

#[test]
fn test_mcp_config_deserializes_deferred_mode() {
    // GIVEN a config selecting deferred mode only
    let json = serde_json::json!({ "tool_loading": "deferred" });
    // WHEN it is deserialized
    let cfg: McpConfig = serde_json::from_value(json).unwrap();
    // THEN the loading mode is deferred and the limit falls back to its default
    assert_eq!(cfg.tool_loading, McpToolLoading::Deferred);
    assert_eq!(cfg.tool_search_limit, 20);
}

#[test]
fn test_mcp_tool_loading_unknown_value_fails() {
    // GIVEN a config with an unknown loading strategy
    let json = serde_json::json!({ "tool_loading": "stream" });
    // WHEN it is deserialized
    let result = serde_json::from_value::<McpConfig>(json);
    // THEN deserialization fails rather than panicking
    assert!(result.is_err());
}

#[test]
fn test_validate_tool_search_limit_zero_fails() {
    // GIVEN a config with a zero search cap
    let cfg = McpConfig {
        tool_search_limit: 0,
        ..McpConfig::default()
    };
    // WHEN it is validated
    let result = cfg.validate();
    // THEN an out-of-bounds error is reported for the right field
    assert!(
        matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "mcp.tool_search_limit"),
        "expected OutOfBounds for mcp.tool_search_limit, got: {result:?}"
    );
}

#[test]
fn test_validate_tool_search_limit_exceeds_max_fails() {
    // GIVEN a config above the upper bound
    let cfg = McpConfig {
        tool_search_limit: 501,
        ..McpConfig::default()
    };
    // WHEN it is validated
    let result = cfg.validate();
    // THEN an out-of-bounds error is reported
    assert!(matches!(result, Err(ConfigError::OutOfBounds { .. })));
}

#[test]
fn test_validate_tool_search_limit_at_max_passes() {
    // GIVEN a config exactly at the upper bound
    let cfg = McpConfig {
        tool_search_limit: 500,
        ..McpConfig::default()
    };
    // WHEN / THEN validation accepts the boundary value
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_mcp_config_default_validates_ok() {
    // GIVEN / WHEN / THEN the default config passes validation
    assert!(McpConfig::default().validate().is_ok());
}

#[test]
fn test_mcp_tool_loading_copy_and_eq() {
    // GIVEN a loading mode value
    let m = McpToolLoading::Eager;
    // WHEN it is copied
    let m2 = m;
    // THEN both equal each other and differ from the other variant
    assert_eq!(m, m2);
    assert_ne!(McpToolLoading::Eager, McpToolLoading::Deferred);
}

#[test]
fn test_mcp_tool_loading_serialize_round_trip() {
    // GIVEN the deferred variant
    let deferred = McpToolLoading::Deferred;
    // WHEN it is serialized and read back
    let s = serde_json::to_string(&deferred).unwrap();
    let back: McpToolLoading = serde_json::from_str(&s).unwrap();
    // THEN the wire form is lowercase and the round-trip is lossless
    assert_eq!(s, "\"deferred\"");
    assert_eq!(back, McpToolLoading::Deferred);
}

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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
    };
    let runtime_max = RuntimeConfig {
        eventbus_capacity: 65536,
        mailbox_capacity: 10000,
        startup_timeout_secs: 600,
        ..Default::default()
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
    assert_eq!(cfg.tool_offload_threshold_chars, 8000);
    assert_eq!(cfg.recent_verbatim_count, 8);

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

// ── compaction tier fields ──────────────────────────────────────────────

#[test]
fn test_oria_compaction_tiers_toml_round_trip() {
    // GIVEN a [oria] TOML overriding both compaction tier fields
    let toml = r#"
            tool_offload_threshold_chars = 4000
            recent_verbatim_count = 12
        "#;

    // WHEN deserialized
    let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

    // THEN the exact values are read back and validation passes
    assert_eq!(cfg.tool_offload_threshold_chars, 4000);
    assert_eq!(cfg.recent_verbatim_count, 12);
    cfg.validate().expect("valid bounds");
}

#[test]
fn test_tool_offload_threshold_below_min_fails() {
    // GIVEN tool_offload_threshold_chars = 200, below min 500
    let cfg = ORIAConfig {
        tool_offload_threshold_chars: 200,
        ..ORIAConfig::default()
    };

    // WHEN
    let result = cfg.validate();

    // THEN
    assert!(
        matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.tool_offload_threshold_chars"),
        "expected OutOfBounds for oria.tool_offload_threshold_chars, got: {result:?}"
    );
}

#[test]
fn test_recent_verbatim_count_below_min_fails() {
    // GIVEN recent_verbatim_count = 0, below min 1
    let cfg = ORIAConfig {
        recent_verbatim_count: 0,
        ..ORIAConfig::default()
    };

    // WHEN
    let result = cfg.validate();

    // THEN
    assert!(
        matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.recent_verbatim_count"),
        "expected OutOfBounds for oria.recent_verbatim_count, got: {result:?}"
    );
}

#[test]
fn test_recent_verbatim_count_above_max_fails() {
    // GIVEN recent_verbatim_count = 65, above max 64
    let cfg = ORIAConfig {
        recent_verbatim_count: 65,
        ..ORIAConfig::default()
    };

    // WHEN
    let result = cfg.validate();

    // THEN
    assert!(
        matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.recent_verbatim_count"),
        "expected OutOfBounds for oria.recent_verbatim_count, got: {result:?}"
    );
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
