//! Tauri IPC commands for the Settings view.
//!
//! The Settings view is **read-only**: a TOML parse -> modify -> serialize
//! round-trip would destroy the user's comments. Editing is delegated to the
//! system's native editor.
//!
//! The `apollia.toml` file is read from `~/.apollia/apollia.toml` (standard
//! location). If the file does not exist, default values are returned.
//!
//! Two neighbouring families live in their own modules: `lifecycle` for the
//! commands that reset or end a session, `system` for the Advanced and
//! Security sections and the local-model setup helper.

pub mod lifecycle;
pub mod system;

use std::path::PathBuf;

use apollia_core::ObservabilityConfig;

use serde::{Deserialize, Serialize};

/// Key/value entry of a configuration section.
#[derive(Debug, Serialize)]
pub struct ConfigEntry {
    /// Key name.
    pub key: String,
    /// Value as a human-readable string.
    pub value: String,
}

/// Configuration section grouped by theme.
#[derive(Debug, Serialize)]
pub struct ConfigSection {
    /// Section name (e.g. `"runtime"`, `"oria"`).
    pub name: String,
    /// Short section description.
    pub description: String,
    /// Key/value entries.
    pub entries: Vec<ConfigEntry>,
    /// Whether the section redirects to a dedicated view instead of displaying inline.
    pub redirect_route: Option<String>,
}

/// Flat view of the Apollia OS configuration for the UI.
#[derive(Debug, Serialize)]
pub struct ApollaConfigView {
    /// Absolute path to the `apollia.toml` file.
    pub config_path: String,
    /// Whether the file exists on disk.
    pub config_exists: bool,
    /// Configuration sections.
    pub sections: Vec<ConfigSection>,
}

/// Resolves the standard path `~/.apollia/apollia.toml`.
fn default_config_path() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home).join("apollia.toml")
}

/// Extracts a string value from a TOML table, or returns a default.
fn toml_string(table: &toml::Value, section: &str, key: &str, default: &str) -> String {
    table
        .get(section)
        .and_then(|s| s.get(key))
        .map(|v| match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(n) => n.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| default.to_string())
}

/// Builds the configuration view from the parsed TOML (or defaults).
fn build_config_view(
    toml_value: Option<&toml::Value>,
    config_path: &str,
    exists: bool,
) -> ApollaConfigView {
    let sections = vec![
        ConfigSection {
            name: "runtime".to_string(),
            description: "Runtime core settings".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "socket_path".to_string(),
                    value: {
                        let fallback = apollia_core::paths::socket_path_or_temp()
                            .display()
                            .to_string();
                        toml_value
                            .map(|t| toml_string(t, "runtime", "socket_path", &fallback))
                            .unwrap_or(fallback)
                    },
                },
                ConfigEntry {
                    key: "api_port".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "api_port", "7771"))
                        .unwrap_or_else(|| "7771".to_string()),
                },
                ConfigEntry {
                    key: "max_concurrent_agents".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "max_concurrent_agents", "10"))
                        .unwrap_or_else(|| "10".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "oria".to_string(),
            description: "ORIA engine (Observer-Reasoner-Actor)".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "max_steps".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "oria", "max_steps", "50"))
                        .unwrap_or_else(|| "50".to_string()),
                },
                ConfigEntry {
                    key: "wall_clock_timeout".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "oria", "wall_clock_timeout", "300s"))
                        .unwrap_or_else(|| "300s".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "observability".to_string(),
            description: "Observability and logging limits".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "max_input_bytes".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "observability", "max_input_bytes", "32768"))
                        .unwrap_or_else(|| "32768".to_string()),
                },
                ConfigEntry {
                    key: "debug_log_prompt".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "observability", "debug_log_prompt", "false"))
                        .unwrap_or_else(|| "false".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "memory".to_string(),
            description: "Memory engine settings".to_string(),
            entries: vec![ConfigEntry {
                key: "episodic_ttl_days".to_string(),
                value: toml_value
                    .map(|t| toml_string(t, "memory", "episodic_ttl_days", "30"))
                    .unwrap_or_else(|| "30".to_string()),
            }],
            redirect_route: None,
        },
        ConfigSection {
            name: "logging".to_string(),
            description: "Log level configuration".to_string(),
            entries: vec![ConfigEntry {
                key: "level".to_string(),
                value: toml_value
                    .map(|t| toml_string(t, "logging", "level", "info"))
                    .unwrap_or_else(|| "info".to_string()),
            }],
            redirect_route: None,
        },
        ConfigSection {
            name: "chat".to_string(),
            description: "Chat session defaults".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "plan_mode_default".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "chat", "plan_mode_default", "false"))
                        .unwrap_or_else(|| "false".to_string()),
                },
                ConfigEntry {
                    key: "default_workspace".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "chat", "default_workspace", "~/.apollia"))
                        .unwrap_or_else(|| "~/.apollia".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "llm".to_string(),
            description: "LLM backend configuration".to_string(),
            entries: vec![],
            redirect_route: Some("llm".to_string()),
        },
    ];

    ApollaConfigView {
        config_path: config_path.to_string(),
        config_exists: exists,
        sections,
    }
}

/// Returns the current configuration as a flat view for the UI.
///
/// Reads `~/.apollia/apollia.toml` and extracts the values by section.
/// If the file does not exist, returns the default values.
#[tauri::command]
pub async fn get_config() -> Result<ApollaConfigView, String> {
    let path = default_config_path();
    let path_str = path.display().to_string();

    if !path.exists() {
        return Ok(build_config_view(None, &path_str, false));
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let toml_value: toml::Value = content
        .parse()
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;

    Ok(build_config_view(Some(&toml_value), &path_str, true))
}

/// Opens the `apollia.toml` file in the system's default editor.
///
/// Uses `open::that()` for cross-platform compatibility
/// (macOS: `open`, Linux: `xdg-open`, Windows: `start`).
#[tauri::command]
pub async fn open_config_in_editor() -> Result<(), String> {
    let path = default_config_path();

    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| "cannot resolve config directory".to_string())?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
        tokio::fs::write(
            &path,
            "# Apollia OS configuration\n# See documentation for available options.\n",
        )
        .await
        .map_err(|e| format!("failed to create config file: {e}"))?;
    }

    open::that(&path).map_err(|e| format!("failed to open editor: {e}"))
}

/// Returns the full observability policy currently stored on disk.
///
/// What the settings page reads and writes, spanning the two TOML sections that
/// actually govern observability.
///
/// The capture switches and byte limits live under `[observability]`. The one
/// setting that can expose prompt content, `debug_log_prompt`, lives under
/// `[llm.observability]` and is read by a different type in `apollia-llm`. It is
/// flattened into the same payload so the settings page stays one form, but it
/// must be written to its own section: a `debug_log_prompt` under
/// `[observability]` is read by nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityView {
    /// `[observability]`: capture switches and truncation limits.
    #[serde(flatten)]
    pub capture: ObservabilityConfig,
    /// `[llm.observability] debug_log_prompt`. Logs the full prompt at `TRACE`
    /// and persists nothing. Requires a `TRACE`-level log filter to have any
    /// visible effect: the default filter is `apollia=info`.
    #[serde(default)]
    pub debug_log_prompt: bool,
}

/// Reads the `[observability]` section of `~/.apollia/apollia.toml` and
/// deserialises it into an [`ObservabilityConfig`]. Every field carries a serde
/// default, so a partial (or missing) section yields the default value for each
/// absent key. This exposes all ten fields (`capture_*` flags,
/// `debug_log_prompt`, the three byte limits, `retention_days`), unlike
/// [`get_config`], which surfaces only two of them in its flat view.
#[tauri::command]
pub async fn get_observability_config() -> Result<ObservabilityView, String> {
    let path = default_config_path();
    if !path.exists() {
        return Ok(ObservabilityView {
            capture: ObservabilityConfig::default(),
            debug_log_prompt: false,
        });
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let doc: toml::Value = content
        .parse()
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;

    let capture = match doc.get("observability") {
        Some(section) => section
            .clone()
            .try_into()
            .map_err(|e| format!("invalid [observability] section: {e}"))?,
        None => ObservabilityConfig::default(),
    };

    let debug_log_prompt = doc
        .get("llm")
        .and_then(|llm| llm.get("observability"))
        .and_then(|obs| obs.get("debug_log_prompt"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    Ok(ObservabilityView {
        capture,
        debug_log_prompt,
    })
}

/// Persists the observability policy to the `[observability]` section of
/// `~/.apollia/apollia.toml`.
///
/// The write is comment-preserving: it parses the file with `toml_edit`, edits
/// only the `[observability]` keys, and re-serialises, so the operator's
/// hand-written comments and other sections survive the round-trip.
///
/// The change is applied at the next runtime start (the loader reads the
/// section into the embedded config on boot). The already-running runtime keeps
/// the config it captured at startup; there is no live-reload channel for
/// observability today.
#[tauri::command]
pub async fn set_observability_config(config: ObservabilityView) -> Result<(), String> {
    let path = default_config_path();

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }

    let mut doc = if path.exists() {
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    apply_observability_to_doc(&mut doc, &config.capture);
    apply_debug_log_prompt_to_doc(&mut doc, config.debug_log_prompt);

    tokio::fs::write(&path, doc.to_string())
        .await
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    tracing::info!(
        max_input_bytes = config.capture.max_input_bytes,
        retention_days = config.capture.retention_days,
        debug_log_prompt = config.debug_log_prompt,
        "observability.config_saved"
    );

    Ok(())
}

/// Writes every [`ObservabilityConfig`] field into the `[observability]` table
/// of `doc`, creating the table if absent and leaving other content untouched.
fn apply_observability_to_doc(doc: &mut toml_edit::DocumentMut, config: &ObservabilityConfig) {
    let obs = doc
        .entry("observability")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    let table = match obs.as_table_mut() {
        Some(t) => t,
        None => {
            // The key exists but is not a table (malformed config); replace it.
            *obs = toml_edit::Item::Table(toml_edit::Table::new());
            match obs.as_table_mut() {
                Some(t) => t,
                None => return,
            }
        }
    };

    table["max_input_bytes"] = toml_edit::value(config.max_input_bytes as i64);
    table["max_output_bytes"] = toml_edit::value(config.max_output_bytes as i64);
    table["max_tool_output_bytes"] = toml_edit::value(config.max_tool_output_bytes as i64);
    table["capture_thoughts"] = toml_edit::value(config.capture_thoughts);
    table["capture_tool_args"] = toml_edit::value(config.capture_tool_args);
    table["capture_tool_outputs"] = toml_edit::value(config.capture_tool_outputs);
    table["capture_agent_logs"] = toml_edit::value(config.capture_agent_logs);
    table["retention_days"] = toml_edit::value(i64::from(config.retention_days));
}

/// Writes `debug_log_prompt` into `[llm.observability]`, creating both tables if
/// needed. Deliberately not merged into [`apply_observability_to_doc`]: the two
/// settings look alike in the interface and land in different sections, and
/// writing this one under `[observability]` would silently do nothing.
fn apply_debug_log_prompt_to_doc(doc: &mut toml_edit::DocumentMut, enabled: bool) {
    let llm = doc
        .entry("llm")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(llm_table) = llm.as_table_mut() else {
        return;
    };
    let obs = llm_table
        .entry("observability")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    if let Some(table) = obs.as_table_mut() {
        table["debug_log_prompt"] = toml_edit::value(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path_ends_with_apollia_toml() {
        // GIVEN the default config path
        let path = default_config_path();
        // WHEN it is resolved
        // THEN it ends with apollia.toml inside .apollia directory
        assert!(path.ends_with("apollia.toml"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }

    #[test]
    fn test_build_config_view_without_toml() {
        // GIVEN no TOML content
        // WHEN building the config view
        let view = build_config_view(None, "/fake/path.toml", false);

        // THEN all sections are present with defaults
        assert!(!view.config_exists);
        assert_eq!(view.sections.len(), 7);

        let chat = view
            .sections
            .iter()
            .find(|s| s.name == "chat")
            .expect("chat section");
        assert_eq!(
            chat.entries
                .iter()
                .find(|e| e.key == "default_workspace")
                .expect("default_workspace entry")
                .value,
            "~/.apollia"
        );

        let runtime = &view.sections[0];
        assert_eq!(runtime.name, "runtime");
        assert_eq!(runtime.entries.len(), 3);
        assert_eq!(runtime.entries[1].key, "api_port");
        assert_eq!(runtime.entries[1].value, "7771");
    }

    #[test]
    fn test_build_config_view_with_toml() {
        // GIVEN a TOML value with custom runtime port
        let toml_str = r#"
            [runtime]
            api_port = 9999
            socket_path = "/custom/path.sock"

            [logging]
            level = "debug"
        "#;
        let toml_value: toml::Value = toml_str.parse().expect("valid toml");

        // WHEN building the config view
        let view = build_config_view(Some(&toml_value), "/test/apollia.toml", true);

        // THEN custom values are extracted
        assert!(view.config_exists);

        let runtime = &view.sections[0];
        assert_eq!(runtime.entries[0].value, "/custom/path.sock");
        assert_eq!(runtime.entries[1].value, "9999");
        assert_eq!(runtime.entries[2].value, "10"); // default

        // Locate `logging` by name; tools was inserted between memory and logging.
        let logging = view
            .sections
            .iter()
            .find(|s| s.name == "logging")
            .expect("logging section");
        assert_eq!(logging.entries[0].value, "debug");
    }

    #[test]
    fn test_build_config_view_llm_redirect() {
        // GIVEN a config view
        let view = build_config_view(None, "/fake.toml", false);

        // WHEN the llm section is looked up
        // THEN llm section redirects to dedicated view
        let llm = view
            .sections
            .iter()
            .find(|s| s.name == "llm")
            .expect("llm section");
        assert_eq!(llm.redirect_route, Some("llm".to_string()));
        assert!(llm.entries.is_empty());

        // WHEN the triggers section is looked up
        // AND triggers section is absent (migrated to SQLite CRUD)
        let triggers = view.sections.iter().find(|s| s.name == "triggers");
        assert!(triggers.is_none());
    }

    #[test]
    fn test_toml_string_extracts_values() {
        // GIVEN a TOML table
        let toml_value: toml::Value = r#"
            [section]
            str_key = "hello"
            int_key = 42
            bool_key = true
        "#
        .parse()
        .expect("valid toml");

        // WHEN each key is read as a string, present or missing
        // THEN values are extracted correctly
        assert_eq!(
            toml_string(&toml_value, "section", "str_key", "default"),
            "hello"
        );
        assert_eq!(toml_string(&toml_value, "section", "int_key", "0"), "42");
        assert_eq!(
            toml_string(&toml_value, "section", "bool_key", "false"),
            "true"
        );
        assert_eq!(
            toml_string(&toml_value, "section", "missing_key", "default"),
            "default"
        );
        assert_eq!(
            toml_string(&toml_value, "missing_section", "key", "default"),
            "default"
        );
    }

    #[test]
    fn test_apply_observability_preserves_comments_and_sections() {
        // GIVEN a config file with comments and an unrelated section
        let original = "# top comment\n[runtime]\napi_port = 7771 # inline\n";
        let mut doc = original
            .parse::<toml_edit::DocumentMut>()
            .expect("parse doc");
        let config = ObservabilityConfig {
            max_input_bytes: 1024,
            retention_days: 7,
            ..ObservabilityConfig::default()
        };

        // WHEN writing both observability sections
        apply_observability_to_doc(&mut doc, &config);
        apply_debug_log_prompt_to_doc(&mut doc, true);
        let rendered = doc.to_string();

        // THEN comments and the runtime section survive
        assert!(rendered.contains("# top comment"));
        assert!(rendered.contains("api_port = 7771 # inline"));
        // AND the observability values are written
        assert!(rendered.contains("max_input_bytes = 1024"));
        assert!(rendered.contains("debug_log_prompt = true"));
        assert!(rendered.contains("retention_days = 7"));

        // AND the round-trip re-parses into the same policy (defaults for the rest)
        let reparsed: toml::Value = rendered.parse().expect("reparse");
        let obs: ObservabilityConfig = reparsed
            .get("observability")
            .expect("observability section")
            .clone()
            .try_into()
            .expect("deserialize");
        assert_eq!(obs.max_input_bytes, 1024);
        assert_eq!(obs.retention_days, 7);
        // Untouched fields keep their defaults.
        assert!(obs.capture_thoughts);
        assert_eq!(obs.max_output_bytes, 32_768);

        // AND debug_log_prompt landed under [llm.observability], not here: the
        // two settings sit side by side in the interface and are read by two
        // different types, so writing it in the wrong section is a silent no-op.
        assert!(
            reparsed
                .get("observability")
                .and_then(|o| o.get("debug_log_prompt"))
                .is_none(),
            "debug_log_prompt must not be written under [observability]"
        );
        assert_eq!(
            reparsed
                .get("llm")
                .and_then(|l| l.get("observability"))
                .and_then(|o| o.get("debug_log_prompt"))
                .and_then(toml::Value::as_bool),
            Some(true),
        );
    }

    #[test]
    fn test_apply_observability_creates_section_when_absent() {
        // GIVEN an empty document
        let mut doc = toml_edit::DocumentMut::new();

        // WHEN writing the default policy
        apply_observability_to_doc(&mut doc, &ObservabilityConfig::default());
        let rendered = doc.to_string();

        // THEN the section is created with all ten fields
        assert!(rendered.contains("[observability]"));
        assert!(rendered.contains("capture_agent_logs = true"));
        assert!(rendered.contains("max_tool_output_bytes = 10240"));
    }
}
