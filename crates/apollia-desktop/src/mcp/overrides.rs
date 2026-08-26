//! User-side overrides for the MCP catalog (`~/.apollia/mcp-overrides.json`).
//!
//! The v0.1.0 catalog is static, embedded in the binary. To let
//! power users patch the catalog without waiting for an Apollia release, an
//! optional `~/.apollia/mcp-overrides.json` file can:
//!
//! - **`add`**: append new entries (e.g. self-hosted internal MCP servers).
//! - **`disable`**: hide entries by `package_identifier`.
//! - **`override`**: deep-merge fields on existing entries (e.g. flip
//!   `default_requires_approval`).
//!
//! Errors parsing the override file are logged but never fatal; the catalog
//! falls back to the embedded set.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::mcp::enrichments::ConnectorEnrichment;

/// Override document loaded from `~/.apollia/mcp-overrides.json`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct McpOverrides {
    /// Entries to append to the catalog.
    pub add: Vec<ConnectorEnrichment>,
    /// `package_identifier`s to hide from the catalog.
    pub disable: Vec<String>,
    /// Per-`package_identifier` patches (JSON merge style).
    #[serde(rename = "override")]
    pub override_: HashMap<String, serde_json::Value>,
}

/// Default location: `~/.apollia/mcp-overrides.json`.
pub fn default_overrides_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| apollia_core::paths::data_dir_under(h).join("mcp-overrides.json"))
}

/// Apply `overrides` to `base`, returning the merged catalog.
///
/// Order of operations:
/// 1. Disable: drop any entry whose `package_identifier` is in `disable`.
/// 2. Override: deep-merge each `override` patch into the matching entry.
/// 3. Add: append `add` entries (no de-duplication; the power user owns the file).
pub fn apply_overrides(
    base: Vec<ConnectorEnrichment>,
    overrides: &McpOverrides,
) -> Vec<ConnectorEnrichment> {
    let mut catalog: Vec<ConnectorEnrichment> = base
        .into_iter()
        .filter(|e| !overrides.disable.iter().any(|d| d == &e.package_identifier))
        .collect();

    for entry in catalog.iter_mut() {
        if let Some(patch) = overrides.override_.get(&entry.package_identifier) {
            if let Ok(mut current) = serde_json::to_value(&entry) {
                merge_json(&mut current, patch);
                if let Ok(merged) = serde_json::from_value::<ConnectorEnrichment>(current) {
                    *entry = merged;
                }
            }
        }
    }

    catalog.extend(overrides.add.iter().cloned());
    catalog
}

/// Load the override file from `path` (defaults to [`default_overrides_path`]).
///
/// Returns `Ok(McpOverrides::default())` when the file does not exist or is
/// unreadable; overrides are best-effort.
pub fn load_overrides_from(path: Option<&std::path::Path>) -> McpOverrides {
    let resolved: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => match default_overrides_path() {
            Some(p) => p,
            None => return McpOverrides::default(),
        },
    };
    if !resolved.exists() {
        return McpOverrides::default();
    }
    match std::fs::read(&resolved) {
        Ok(bytes) => match serde_json::from_slice::<McpOverrides>(&bytes) {
            Ok(o) => {
                tracing::info!(
                    add = o.add.len(),
                    disable = o.disable.len(),
                    overrides = o.override_.len(),
                    path = %resolved.display(),
                    "mcp.catalog.overrides.applied"
                );
                o
            }
            Err(e) => {
                tracing::warn!(
                    path = %resolved.display(),
                    err = %e,
                    "mcp.catalog.overrides.parse_failed"
                );
                McpOverrides::default()
            }
        },
        Err(e) => {
            tracing::warn!(
                path = %resolved.display(),
                err = %e,
                "mcp.catalog.overrides.read_failed"
            );
            McpOverrides::default()
        }
    }
}

/// Recursive JSON merge: `patch` overwrites scalar / null values in `target`,
/// recurses on objects, and replaces arrays wholesale.
fn merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                merge_json(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (t, p) => {
            *t = p.clone();
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::enrichments::load_builtin_enrichments;

    #[test]
    fn test_disable_removes_matching_entries() {
        // GIVEN the builtin catalog plus a disable for Notion
        let base = load_builtin_enrichments();
        let initial_len = base.len();
        let mut overrides = McpOverrides::default();
        overrides.disable.push("@notionhq/notion-mcp-server".into());

        // WHEN overrides are applied
        let after = apply_overrides(base, &overrides);

        // THEN Notion is gone and the catalog is one shorter
        assert_eq!(after.len(), initial_len - 1);
        assert!(!after
            .iter()
            .any(|e| e.package_identifier == "@notionhq/notion-mcp-server"));
    }

    #[test]
    fn test_add_appends_entries_to_catalog() {
        // GIVEN the built-in catalogue and one entry added by the operator
        let base = load_builtin_enrichments();
        let initial_len = base.len();
        let custom: ConnectorEnrichment = serde_json::from_value(serde_json::json!({
            "package_identifier": "internal-acme-mcp",
            "operator_label": { "en": "ACME Internal", "fr": "ACME Interne" },
            "category": "internal",
            "icon_name": "building",
            "trust_level": "custom",
            "default_requires_approval": true,
            "cost_model": { "kind": "free" }
        }))
        .expect("custom entry");
        let overrides = McpOverrides {
            add: vec![custom],
            ..McpOverrides::default()
        };

        // WHEN the overrides are applied
        let after = apply_overrides(base, &overrides);

        // THEN the catalogue grew by exactly that entry
        assert_eq!(after.len(), initial_len + 1);
        assert!(after
            .iter()
            .any(|e| e.package_identifier == "internal-acme-mcp"));
    }

    #[test]
    fn test_override_flips_default_requires_approval() {
        // GIVEN an override turning approval off for one catalogue entry
        let base = load_builtin_enrichments();
        let mut overrides = McpOverrides::default();
        overrides.override_.insert(
            "io.github.github/github-mcp-server".into(),
            serde_json::json!({ "default_requires_approval": false }),
        );

        // WHEN the overrides are applied
        let after = apply_overrides(base, &overrides);

        // THEN that entry no longer requires approval by default
        let github = after
            .iter()
            .find(|e| e.package_identifier == "io.github.github/github-mcp-server")
            .expect("github");
        assert!(!github.default_requires_approval);
    }

    #[test]
    fn test_empty_overrides_returns_base_unchanged() {
        // GIVEN the built-in catalogue and no override at all
        let base = load_builtin_enrichments();
        let len = base.len();
        // WHEN the overrides are applied
        let after = apply_overrides(base, &McpOverrides::default());
        // THEN the catalogue is the one it started with
        assert_eq!(after.len(), len);
    }

    #[test]
    fn test_load_overrides_missing_file_returns_default() {
        // GIVEN a path that does not exist
        let path = std::path::PathBuf::from("/nonexistent/apollia/overrides.json");
        // WHEN we try to load
        let overrides = load_overrides_from(Some(&path));
        // THEN we get the default (empty)
        assert!(overrides.add.is_empty());
        assert!(overrides.disable.is_empty());
        assert!(overrides.override_.is_empty());
    }

    #[test]
    fn test_load_overrides_parses_valid_file() {
        // GIVEN a temp file with a valid overrides document
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("overrides.json");
        std::fs::write(
            &path,
            r#"{
                "disable": ["@anthropic/mcp-server-puppeteer"],
                "override": {
                    "io.github.github/github-mcp-server": { "default_requires_approval": false }
                }
            }"#,
        )
        .expect("write");

        // WHEN we load
        let overrides = load_overrides_from(Some(&path));

        // THEN the disable + override are parsed
        assert_eq!(overrides.disable, vec!["@anthropic/mcp-server-puppeteer"]);
        assert!(overrides
            .override_
            .contains_key("io.github.github/github-mcp-server"));
    }

    #[test]
    fn test_load_overrides_malformed_returns_default_without_panic() {
        // GIVEN a temp file with invalid JSON
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("overrides.json");
        std::fs::write(&path, "this is not json").expect("write");

        // WHEN we load
        let overrides = load_overrides_from(Some(&path));

        // THEN we get the default (no panic, just a logged warning)
        assert!(overrides.add.is_empty());
        assert!(overrides.disable.is_empty());
    }

    #[test]
    fn test_merge_json_replaces_scalars() {
        // GIVEN a target object and a patch touching one scalar
        let mut target = serde_json::json!({ "a": 1, "b": "old" });
        let patch = serde_json::json!({ "b": "new" });
        // WHEN they are merged
        merge_json(&mut target, &patch);
        // THEN the patched key changes and the untouched one survives
        assert_eq!(target, serde_json::json!({ "a": 1, "b": "new" }));
    }

    #[test]
    fn test_merge_json_recurses_into_objects() {
        // GIVEN a target holding a nested object and a patch touching one of its keys
        let mut target = serde_json::json!({ "obj": { "a": 1, "b": 2 } });
        let patch = serde_json::json!({ "obj": { "b": 99 } });
        // WHEN they are merged
        merge_json(&mut target, &patch);
        // THEN the merge goes into the nested object rather than replacing it
        assert_eq!(target, serde_json::json!({ "obj": { "a": 1, "b": 99 } }));
    }

    #[test]
    fn test_merge_json_replaces_arrays_wholesale() {
        // GIVEN a target holding an array and a patch holding a shorter one
        let mut target = serde_json::json!({ "arr": [1, 2, 3] });
        let patch = serde_json::json!({ "arr": [9] });
        // WHEN they are merged
        merge_json(&mut target, &patch);
        // THEN the array is replaced whole, not appended to nor merged item by item
        assert_eq!(target["arr"], serde_json::json!([9]));
    }
}
