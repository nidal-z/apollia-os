//! Cross-crate integration: load a full `apollia.toml` with a `[hooks]` section,
//! validate it, build the registry, and assert the handler indexing. No daemon
//! is required.

use apollia_core::HookEventKind;
use apollia_runtime::{EmbeddedConfig, HookRegistry};

#[test]
fn hooks_section_loads_validates_and_builds_registry() {
    // GIVEN an apollia.toml with a command PreToolUse handler and an http
    // PostToolUse handler
    let toml = r#"
        [[hooks.handlers]]
        events = ["pre_tool_use"]
        type = "command"
        command = ["/usr/bin/my-hook", "--event", "pre_tool_use"]

        [[hooks.handlers]]
        events = ["post_tool_use"]
        type = "http"
        url = "http://127.0.0.1:9000/hook"
        timeout_ms = 2000
    "#;

    // WHEN the embedded config applies the TOML and validates the hooks section
    let cfg = EmbeddedConfig::default().apply_toml(toml);
    cfg.hooks_config
        .validate()
        .expect("valid hooks section must pass validation");

    // THEN the registry indexes both handlers by their subscribed events
    let registry = HookRegistry::from_config(&cfg.hooks_config);
    assert_eq!(registry.handlers_for(HookEventKind::PreToolUse).len(), 1);
    assert_eq!(registry.handlers_for(HookEventKind::PostToolUse).len(), 1);
    assert!(registry.handlers_for(HookEventKind::PreCompact).is_empty());

    let summaries = registry.list_all();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].r#type, "command");
    assert_eq!(summaries[1].r#type, "http");
    assert_eq!(summaries[1].timeout_ms, 2000);
}

#[test]
fn absent_hooks_section_yields_empty_registry() {
    // GIVEN a config file without any [hooks] section
    let cfg = EmbeddedConfig::default().apply_toml("[runtime]\n");

    // WHEN the registry is built from the (default) hooks config
    let registry = HookRegistry::from_config(&cfg.hooks_config);

    // THEN it is empty and validation passes
    assert!(registry.is_empty());
    assert!(cfg.hooks_config.validate().is_ok());
}
