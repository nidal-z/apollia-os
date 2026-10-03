//! The registry of keys held to the structures it describes.

use std::collections::BTreeSet;

use super::keys::{is_live_path, lookup, withdrawn_covering, ReloadClass, KEYS};
use super::{check_imposed_setting, ApolliaConfig, LlmConfig, SettingError};

/// Every leaf path of a JSON value: a scalar, a null, an array, or an empty
/// object ends a path; a non-empty object is walked.
fn leaves(prefix: &str, value: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                leaves(&path, v, out);
            }
        }
        _ => out.push((prefix.to_owned(), value.clone())),
    }
}

/// The defaults of every section, `[llm]` at its minimal shape since an absent
/// `[llm]` has no keys to show.
fn default_leaves() -> Vec<(String, serde_json::Value)> {
    let config = ApolliaConfig {
        llm: Some(LlmConfig::minimal()),
        ..ApolliaConfig::default()
    };
    let json = serde_json::to_value(&config).unwrap();
    let mut out = Vec::new();
    leaves("", &json, &mut out);
    out
}

#[test]
fn the_registry_lists_exactly_the_keys_of_the_structures() {
    // GIVEN the serialized defaults of every section, and the live keys of the
    //       registry
    let from_structures: BTreeSet<String> = default_leaves().into_iter().map(|(p, _)| p).collect();
    let from_registry: BTreeSet<String> = KEYS
        .iter()
        .filter(|k| k.withdrawn.is_none())
        .map(|k| k.path.to_owned())
        .collect();

    // WHEN the two sets are compared
    let missing: Vec<_> = from_structures.difference(&from_registry).collect();
    let extra: Vec<_> = from_registry.difference(&from_structures).collect();

    // THEN they are equal; a key added to a section without its registry line
    //      is named, and so is a registry line left behind
    assert!(
        missing.is_empty(),
        "keys of the structures missing from config::keys::KEYS: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "keys of config::keys::KEYS that no structure declares: {extra:?}"
    );
}

#[test]
fn every_registered_default_is_the_default_of_the_structure() {
    // GIVEN the serialized defaults
    let defaults = default_leaves();

    // WHEN each registry default is read as TOML and compared
    let mut wrong = Vec::new();
    for (path, value) in &defaults {
        // The socket path depends on the home folder; the two keys [llm]
        // requires have no default and show at their minimal value.
        if ["api.unix_socket", "llm.default", "llm.backends"].contains(&path.as_str()) {
            continue;
        }
        let Some(entry) = lookup(path) else { continue };
        let registered = match entry.default {
            None => serde_json::Value::Null,
            Some(literal) => {
                let doc: toml::Table = toml::from_str(&format!("v = {literal}")).unwrap();
                serde_json::to_value(&doc["v"]).unwrap()
            }
        };
        if &registered != value {
            wrong.push(format!("{path}: registry {registered}, structure {value}"));
        }
    }

    // THEN they agree
    assert!(
        wrong.is_empty(),
        "registry defaults that differ: {wrong:#?}"
    );
}

#[test]
fn every_path_is_registered_once_with_an_english_documentation() {
    // GIVEN the registry
    // WHEN its paths and documentation are read
    let mut seen = BTreeSet::new();
    let duplicates: Vec<_> = KEYS.iter().filter(|k| !seen.insert(k.path)).collect();
    let undocumented: Vec<_> = KEYS.iter().filter(|k| k.doc.trim().is_empty()).collect();

    // THEN no path appears twice and none is undocumented
    assert!(duplicates.is_empty(), "registered twice: {duplicates:?}");
    assert!(
        undocumented.is_empty(),
        "no documentation: {undocumented:?}"
    );
}

#[test]
fn every_section_carries_the_reload_class_of_the_measured_table() {
    // GIVEN the reload class of each section, as measured (table 2.7.2)
    let expected = [
        ("api", ReloadClass::Restart),
        ("runtime", ReloadClass::Restart),
        ("llm", ReloadClass::Restart),
        ("hitl", ReloadClass::Reloadable),
        ("tools", ReloadClass::Reloadable),
        ("mcp", ReloadClass::Reloadable),
        ("hooks", ReloadClass::Reloadable),
        ("chat", ReloadClass::Reloadable),
        ("filesystem", ReloadClass::Reloadable),
        ("observability", ReloadClass::Reloadable),
        ("ui", ReloadClass::Reloadable),
        ("desktop", ReloadClass::Reloadable),
        ("server", ReloadClass::Restart),
        // Read again at every model launch, unlike the rest of `[llm]`.
        ("llm.guardrails", ReloadClass::Reloadable),
    ];

    // WHEN every live key of those sections is read, against its longest
    // listed prefix
    let wrong: Vec<_> = KEYS
        .iter()
        .filter(|k| k.withdrawn.is_none())
        .filter(|k| {
            expected
                .iter()
                .filter(|(s, _)| k.path.starts_with(&format!("{s}.")))
                .max_by_key(|(s, _)| s.len())
                .is_some_and(|(_, class)| *class != k.reload)
        })
        .map(|k| k.path)
        .collect();

    // THEN each has its section's class
    assert!(
        wrong.is_empty(),
        "keys with the wrong reload class: {wrong:?}"
    );
}

#[test]
fn the_formerly_inert_and_deprecated_sections_are_withdrawn() {
    // GIVEN the sections the CLI used to warn about, and the retired [llm.vertex]
    let sections = [
        "memory",
        "budget",
        "a2a",
        "oria",
        "registry",
        "permissions",
        "triggers",
        "notifications",
        "stt",
        "llm.vertex",
    ];

    // WHEN a key under each is looked up
    let not_withdrawn: Vec<_> = sections
        .iter()
        .filter(|s| withdrawn_covering(&format!("{s}.anything")).is_none())
        .collect();

    // THEN every one is covered by a withdrawn entry with a replacement
    assert!(not_withdrawn.is_empty(), "not withdrawn: {not_withdrawn:?}");
}

#[test]
fn a_key_is_lockable_exactly_when_the_policy_can_impose_it() {
    // GIVEN the live keys of the registry
    let live = KEYS.iter().filter(|k| k.withdrawn.is_none());

    // WHEN each is offered to the policy check, which knows the named settings
    let wrong: Vec<_> = live
        .filter(|k| {
            let named = !matches!(
                check_imposed_setting(k.path, &serde_json::Value::Null),
                Err(SettingError::UnknownKey { .. })
            );
            named != k.lockable
        })
        .map(|k| k.path)
        .collect();

    // THEN the registry marks lockable the named settings, and only them
    assert!(
        wrong.is_empty(),
        "lockable flag disagrees with the policy: {wrong:?}"
    );
}

#[test]
fn the_named_settings_are_lockable() {
    // GIVEN the registry
    // WHEN the lockable keys are listed
    let lockable: Vec<_> = KEYS.iter().filter(|k| k.lockable).map(|k| k.path).collect();

    // THEN they are the named settings the policy can impose today
    assert_eq!(
        lockable,
        vec![
            "llm.guardrails.level",
            "llm.guardrails.ceiling_gb",
            "mcp.approval_ttl_hours",
            "mcp.tool_loading",
            "mcp.tool_search_limit",
            "mcp.server_enabled",
            "models.registry_url",
            "models.registry_mirror_url",
            "models.registry_enabled",
            "models.import.require_verified",
            "desktop.developer_mode",
            "server.enabled",
            "server.port",
            "server.require_auth",
        ]
    );
}

#[test]
fn a_live_path_is_a_registered_key_or_lies_under_a_free_table() {
    // GIVEN registered keys, keys under free tables, and typos
    // WHEN each is checked
    // THEN only the first two are live
    assert!(is_live_path("api.port"));
    assert!(is_live_path("llm.pricing_overrides.m.input_per_mtok"));
    assert!(is_live_path("llm.routing.precise"));
    assert!(!is_live_path("api.typo"));
    assert!(!is_live_path("api"));
    assert!(!is_live_path("oria.max_replans"));
}
