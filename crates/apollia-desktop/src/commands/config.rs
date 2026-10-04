//! Tauri IPC commands for the Settings view.
//!
//! The configuration view is **read-only**: a TOML parse -> modify ->
//! serialize round-trip would destroy the user's comments. Editing the file is
//! delegated to the system's native editor; a named setting is written key by
//! key through `settings`, which keeps the comments and layout.
//!
//! The `apollia.toml` file is read from `~/.apollia/apollia.toml` (standard
//! location). If the file does not exist, default values are returned.
//!
//! Three neighbouring families live in their own modules: `lifecycle` for the
//! commands that reset or end a session, `settings` for the named settings
//! read with their source and written key by key, `system` for the Advanced
//! and Security sections and the local-model setup helper.

pub mod lifecycle;
pub mod settings;
pub mod system;

use std::path::PathBuf;

use apollia_runtime::api::routes_config::settings::ObservabilitySettings;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::attached::RuntimeLink;

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
    apollia_core::paths::config_file_under(apollia_core::paths::home_dir_or_temp())
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
                    value: {
                        let fallback = apollia_core::config::DEFAULT_API_PORT.to_string();
                        toml_value
                            .map(|t| toml_string(t, "api", "port", &fallback))
                            .unwrap_or(fallback)
                    },
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
            name: "filesystem".to_string(),
            description: "Paths an agent works in without being asked".to_string(),
            entries: vec![ConfigEntry {
                key: "trusted_paths".to_string(),
                value: toml_value
                    .map(|t| toml_string(t, "filesystem", "trusted_paths", "[\"~\"]"))
                    .unwrap_or_else(|| "[\"~\"]".to_string()),
            }],
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

    // Written at startup already; a file removed since is written again, with
    // every key documented, never over an existing one.
    apollia_core::config::write_default_file_if_absent(&path)
        .map_err(|e| format!("failed to create config file: {e}"))?;

    open::that(&path).map_err(|e| format!("failed to open editor: {e}"))
}

/// Route of the observability policy on the runtime.
pub(crate) const OBSERVABILITY_ROUTE: &str = "/api/v1/config/observability";

/// Reads the observability policy of the runtime this application drives:
/// the capture switches, byte limits and retention of `[observability]`, and
/// `debug_log_prompt` of `[llm.observability]`, in one form.
pub(crate) async fn observability_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(OBSERVABILITY_ROUTE).await
}

/// Writes the observability policy on the runtime this application drives,
/// through the one writer of its `apollia.toml`; applied at its next start.
pub(crate) async fn save_observability_on(
    link: &RuntimeLink,
    config: &ObservabilitySettings,
) -> Result<(), String> {
    let body = serde_json::to_value(config).map_err(|e| e.to_string())?;
    link.put_json(OBSERVABILITY_ROUTE, &body).await?;
    tracing::event!(
        tracing::Level::INFO,
        max_input_bytes = config.max_input_bytes,
        retention_days = config.retention_days,
        debug_log_prompt = config.debug_log_prompt,
        "observability.config_saved"
    );
    Ok(())
}

/// Returns the observability policy, over `GET /api/v1/config/observability`,
/// in both modes.
#[tauri::command]
pub async fn get_observability_config(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    observability_on(link.inner()).await
}

/// Persists the observability policy, over `PUT
/// /api/v1/config/observability`, in both modes.
#[tauri::command]
pub async fn set_observability_config(
    link: State<'_, RuntimeLink>,
    config: ObservabilitySettings,
) -> Result<(), String> {
    save_observability_on(link.inner(), &config).await
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
        assert_eq!(view.sections.len(), 8);

        // AND the trusted paths default to the home directory, which is what
        // makes an agent usable outside one folder without an approval on every
        // write. A path outside them is asked about, never refused.
        let filesystem = view
            .sections
            .iter()
            .find(|s| s.name == "filesystem")
            .expect("filesystem section");
        assert_eq!(
            filesystem
                .entries
                .iter()
                .find(|e| e.key == "trusted_paths")
                .expect("trusted_paths entry")
                .value,
            "[\"~\"]"
        );

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
        // GIVEN a TOML value with a custom native API port
        let toml_str = r#"
            [api]
            port = 9999

            [runtime]
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

    mod attached {
        use serde_json::json;
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::super::*;
        use crate::attached::{gate, AttachedClient, LinkMode};

        const TOKEN: &str = "token-of-this-installation";

        fn attached(server: &MockServer) -> RuntimeLink {
            RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
        }

        fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
            Mock::given(method(verb))
                .and(path(route))
                .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        }

        fn policy(retention_days: u32) -> ObservabilitySettings {
            ObservabilitySettings {
                retention_days,
                debug_log_prompt: true,
                ..ObservabilitySettings::default()
            }
        }

        #[tokio::test]
        async fn test_the_observability_policy_is_read_from_the_daemon() {
            // GIVEN a daemon holding a retention of 30 days
            let server = MockServer::start().await;
            let held = serde_json::to_value(policy(30)).unwrap();
            authorized("GET", OBSERVABILITY_ROUTE)
                .respond_with(ResponseTemplate::new(200).set_body_json(held.clone()))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the Observability screen reads the policy
            let read = observability_on(&attached(&server)).await.unwrap();

            // THEN it is the daemon's, as it answered
            assert_eq!(read, held);
        }

        #[tokio::test]
        async fn test_the_observability_policy_is_written_to_the_daemon() {
            // GIVEN a daemon that takes the whole policy
            let server = MockServer::start().await;
            let sent = serde_json::to_value(policy(7)).unwrap();
            authorized("PUT", OBSERVABILITY_ROUTE)
                .and(body_json(sent.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_json(sent))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the screen saves a retention of 7 days
            let saved = save_observability_on(&attached(&server), &policy(7)).await;

            // THEN the write succeeds
            assert!(saved.is_ok(), "{saved:?}");
        }

        #[tokio::test]
        async fn test_a_refusal_of_the_daemon_is_shown_as_it_said_it() {
            // GIVEN a daemon that refuses the write with its sentence
            let server = MockServer::start().await;
            authorized("PUT", OBSERVABILITY_ROUTE)
                .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                    "error": "observability.retention_days is fixed by the organisation policy",
                    "code": "locked",
                    "diagnostics": [],
                })))
                .expect(1)
                .mount(&server)
                .await;

            // WHEN the screen saves the policy
            let refused = save_observability_on(&attached(&server), &policy(7)).await;

            // THEN the error is the daemon's sentence, word for word
            assert_eq!(
                refused.unwrap_err(),
                "observability.retention_days is fixed by the organisation policy"
            );
        }

        #[test]
        fn test_the_observability_commands_are_routed_to_a_daemon() {
            // GIVEN the two commands of the Observability policy
            let commands = ["get_observability_config", "set_observability_config"];

            // WHEN each passes the gate of an attached window
            // THEN none is refused
            for command in commands {
                assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
            }
        }
    }
}
