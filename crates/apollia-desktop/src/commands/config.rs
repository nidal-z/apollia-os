//! Tauri IPC commands for the Settings view.
//!
//! The Settings view is **read-only**: a TOML parse -> modify -> serialize
//! round-trip would destroy the user's comments. Editing is delegated to the
//! system's native editor.
//!
//! The `apollia.toml` file is read from `~/.apollia/apollia.toml` (standard
//! location). If the file does not exist, default values are returned.

use std::path::PathBuf;

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider, ObservabilityConfig};

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
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "socket_path", "/tmp/apollia.sock"))
                        .unwrap_or_else(|| "/tmp/apollia.sock".to_string()),
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

/// Resolves the onboarding flag path `~/.apollia/.onboarded`.
fn onboarded_flag_path() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home).join(".onboarded")
}

/// Marks onboarding as complete by creating the flag file.
///
/// Creates `~/.apollia/.onboarded` (and the parent directory if needed).
#[tauri::command]
pub async fn mark_onboarded() -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if let Some(parent) = flag_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directory: {e}"))?;
    }

    tokio::fs::write(&flag_path, "")
        .await
        .map_err(|e| format!("failed to write onboarding flag: {e}"))
}

/// Fully resets onboarding: removes the completion flag, purges the desktop
/// flow's internal state markers, and purges the visible user profile (Tier 1
/// + extras) so the journey starts from scratch.
///
/// Details:
/// - UI markers (`onboarding_phase`, `onboarding_completed_at`, etc.) are
///   stored under the `__` prefix in `__user__`, so they are wiped via
///   `forget_internal`.
/// - The profile's Tier 1 facts are flat keys (`name`, `role`,
///   `agents.hitl`, `constraints.sovereignty`), so a plain `repo.reset()`
///   removes them all, including any extras.
#[tauri::command]
pub async fn reset_onboarding(
    state: tauri::State<'_, apollia_runtime::embedded::RuntimeHandle>,
) -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if flag_path.exists() {
        tokio::fs::remove_file(&flag_path)
            .await
            .map_err(|e| format!("failed to remove onboarding flag: {e}"))?;
    }

    let repo = state
        .user_memory
        .as_ref()
        .cloned()
        .ok_or_else(|| "user memory repository not initialized".to_string())?;

    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;

        // 1. Internal onboarding-flow markers (`__` prefix).
        let internal_keys = [
            "onboarding_phase",
            "onboarding_profile",
            "onboarding_llm_configured",
            "onboarding_stt_configured",
            "onboarding_topics_covered",
            "onboarding_mandatory_complete",
            "onboarding_tour_step_index",
            "onboarding_tour_total_steps",
            "onboarding_tour_completed",
            "onboarding_companion_session_id",
            "onboarding_voice_enabled",
            "onboarding_skipped",
            "onboarding_completed",
            "onboarding_started_at",
            "onboarding_completed_at",
            "onboarding_stats_total_time_sec",
            "onboarding_stats_actions_completed",
            "onboarding_stats_companion_questions",
            // Legacy key: the guided-tour voice path is gone, but installations
            // created before its removal still hold this entry.
            "onboarding_stats_voice_commands_used",
        ];
        for key in &internal_keys {
            if let Err(e) = repo.forget_internal(key) {
                tracing::warn!(
                    key = %key,
                    error = %e,
                    "failed to wipe internal onboarding marker during reset_onboarding"
                );
            }
        }

        // 2. Full purge of the visible user profile (Tier 1 + extras).
        //    The onboarding journey will repopulate Tier 1 on next launch.
        let removed = repo
            .reset()
            .map_err(|e| format!("failed to reset user profile during reset_onboarding: {e}"))?;
        tracing::info!(
            removed_profile_entries = removed,
            "reset_onboarding wiped user profile"
        );

        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // 3. Purge the memory database owned by the onboarding-agent (`onboarding.db`).
    //    It holds the dialogue episodes and `onboarding.completed_at` keys that
    //    otherwise prevent the journey from restarting cleanly.
    let onboarding_db =
        apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
            .join("memory")
            .join("onboarding.db");
    if onboarding_db.exists() {
        if let Err(e) = tokio::fs::remove_file(&onboarding_db).await {
            tracing::warn!(
                path = %onboarding_db.display(),
                error = %e,
                "could not remove onboarding.db during reset_onboarding"
            );
        }
        // WAL side-files; ignore errors.
        for ext in ["onboarding.db-wal", "onboarding.db-shm"] {
            let side = onboarding_db.with_file_name(ext);
            if side.exists() {
                let _ = tokio::fs::remove_file(&side).await;
            }
        }
    }

    Ok(())
}

/// Removes **all** of Apollia's memory databases.
///
/// Hard wipe: removes every `*.db` file (and their WAL/SHM side-files) from the
/// `~/.apollia/memory/` directory, regardless of namespace (user profile, agent
/// memories, projects, etc.). Irreversible action.
///
/// Returns the number of `.db` files removed.
#[tauri::command]
pub async fn clear_all_memories() -> Result<usize, String> {
    let memory_dir = apollia_home().join("memory");
    if !memory_dir.exists() {
        return Ok(0);
    }

    let mut entries = tokio::fs::read_dir(&memory_dir)
        .await
        .map_err(|e| format!("failed to list memory dir: {e}"))?;

    let mut removed_db_count = 0usize;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("read_dir iteration failed: {e}"))?
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Broad removal: every file (db, wal, shm, possible backups) in the
        // memory directory is removed. Subdirectories are left untouched.
        if !path.is_file() {
            continue;
        }
        if let Err(e) = tokio::fs::remove_file(&path).await {
            tracing::warn!(path = %path.display(), error = %e, "clear_all_memories: skip file");
            continue;
        }
        if name.ends_with(".db") {
            removed_db_count += 1;
        }
    }

    tracing::info!(
        removed_db_count,
        "clear_all_memories wiped every memory namespace"
    );
    Ok(removed_db_count)
}

/// Resolves `~/.apollia/`.
fn apollia_home() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
}

/// Removes the logs directory (`~/.apollia/logs/`).
///
/// If the directory does not exist, the command is a no-op and returns `Ok(())`.
#[tauri::command]
pub async fn clear_logs() -> Result<(), String> {
    let logs = apollia_home().join("logs");
    if logs.exists() {
        tokio::fs::remove_dir_all(&logs)
            .await
            .map_err(|e| format!("failed to remove logs directory: {e}"))?;
    }
    Ok(())
}

/// Factory reset: removes **all** of `~/.apollia/`'s content.
///
/// Irreversible destructive action: removes config, memory, sessions, logs,
/// models and credentials. The user is prompted to restart after the call.
#[tauri::command]
pub async fn factory_reset() -> Result<(), String> {
    let home = apollia_home();
    if home.exists() {
        tokio::fs::remove_dir_all(&home)
            .await
            .map_err(|e| format!("failed to remove apollia home: {e}"))?;
    }
    Ok(())
}

/// Restarts the application. Called after a destructive action that needs a
/// cold-start (onboarding reset, factory reset).
///
/// **Important:** this function never returns on success because it kills the
/// current process and spawns a new one. In dev mode the restart may fail, in
/// which case the user must relaunch manually.
#[tauri::command]
pub async fn app_restart(app: tauri::AppHandle) -> Result<(), String> {
    // app.restart() kills the current process and spawns a new one, so this
    // function never returns on success. In dev mode, restart may fail silently
    // (no packaged bundle to relaunch); the frontend should handle this by
    // showing a manual reload prompt if the app doesn't actually restart.
    app.restart();
}

/// Quits the application gracefully from a webview surface (in-app user menu,
/// command palette).
///
/// Routes through the same shared quit as the tray and the macOS app menu:
/// it drains the embedded runtime, then exits via `AppHandle::exit`, which
/// bypasses the window `CloseRequested` handler (close-to-tray). Calling
/// `window.close()` from the webview instead would only hide the window.
#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::initiate_quit(&app);
    Ok(())
}

/// System information shown in the Advanced section of Settings.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    /// Apollia OS version (e.g. `"0.1.0-preview"`). The About screen reads the
    /// pre-release suffix from it to label the release channel.
    pub version: String,
    /// Operating system and architecture (e.g. `"macos aarch64"`).
    pub os: String,
    /// Absolute path to the Python 3 interpreter, if detected.
    pub python_path: Option<String>,
    /// Absolute path to the runtime data directory (`<home>/.apollia`), where
    /// the databases, models, configuration and audit journal live.
    ///
    /// `None` only when the home directory cannot be resolved, which is a real
    /// condition on a stripped environment and is reported rather than papered
    /// over with a literal `~/.apollia` the operator would then trust.
    pub data_dir: Option<String>,
}

/// Returns the system information for the Advanced section of Settings.
///
/// Detects the Apollia version, the OS, and the Python 3 path via
/// `python3 -c "import sys; print(sys.executable)"`.
#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    let mut which_python = tokio::process::Command::new("python3");
    apollia_core::subprocess_env::scrub_bundled_python_async(&mut which_python);
    apollia_core::subprocess_window::hide_console_async(&mut which_python);
    let python_path = match which_python
        .args(["-c", "import sys; print(sys.executable)"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        }
        _ => None,
    };

    let data_dir = apollia_core::paths::data_dir().map(|p| p.display().to_string());

    Ok(SystemInfo {
        version,
        os,
        python_path,
        data_dir,
    })
}

/// Returns the active security posture for the Security section of Settings.
///
/// Surfaces the isolation level of native tools and the agent-code trust model
/// so the operator can see, without reading logs, what confinement is active on
/// their platform.
#[tauri::command]
pub async fn get_security_posture() -> Result<apollia_core::SecurityPosture, String> {
    Ok(apollia_core::SecurityPosture::detect())
}

/// Result of the local LLM setup operation.
#[derive(Debug, Serialize)]
pub struct SetupLlmResult {
    /// Absolute path where the model is stored.
    pub model_path: String,
    /// Inferred quantization from the filename (e.g. `"q8_0"`, `"q4_k_m"`).
    pub quantization: String,
}

/// Sets up a local embedded LLM from a user-selected GGUF file.
///
/// Copies the model into `~/.apollia/models/`, registers it as a backend
/// in `system.db`, and returns the path for confirmation.
///
/// This is a first-launch helper. The backend is inserted as `"local"` and
/// marked as default if no backend with that name already exists.
/// Call `reload_llm_from_db` afterwards to make the router available immediately.
#[tauri::command]
pub async fn setup_local_llm(gguf_path: String) -> Result<SetupLlmResult, String> {
    let source = PathBuf::from(&gguf_path);

    // Validate the file exists and is a .gguf
    if !source.exists() {
        return Err(format!("file not found: {gguf_path}"));
    }
    if source.extension().and_then(|e| e.to_str()) != Some("gguf") {
        return Err("expected a .gguf file".into());
    }

    // Infer quantization from filename (e.g. "Qwen3-0.6B-Q8_0.gguf" → "q8_0")
    let file_stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let quantization = infer_quantization(file_stem);

    // Copy into ~/.apollia/models/
    let models_dir = default_config_path()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("models");
    tokio::fs::create_dir_all(&models_dir)
        .await
        .map_err(|e| format!("failed to create models directory: {e}"))?;

    let file_name = source
        .file_name()
        .ok_or("invalid file name")?
        .to_string_lossy()
        .to_string();
    let dest = models_dir.join(&file_name);

    // Only copy if not already there
    if dest != source {
        tokio::fs::copy(&source, &dest)
            .await
            .map_err(|e| format!("failed to copy model: {e}"))?;
    }

    let model_path_str = format!("~/.apollia/models/{file_name}");

    // Insert backend into system.db; idempotent (skips if "local" already exists).
    // LlmBackendRepository is !Send, so DB work runs in spawn_blocking.
    let db_path = default_config_path()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(apollia_core::paths::DataFile::System.file_name());
    let device = if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    }
    .to_string();
    let model_for_db = model_path_str.clone();
    let quant_for_db = quantization.clone();

    tokio::task::spawn_blocking(move || {
        let repo = LlmBackendRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        if repo
            .find_by_name("local")
            .map_err(|e| format!("failed to query system.db: {e}"))?
            .is_none()
        {
            let config = LlmBackendConfig {
                name: "local".to_string(),
                provider: LlmProvider::LlamaCpp,
                model: model_for_db.clone(),
                config_json: serde_json::json!({
                    "model_path": model_for_db,
                    "device": device,
                    "quantization": quant_for_db,
                }),
                enabled: true,
                is_default: true,
            };
            repo.save(&config)
                .map_err(|e| format!("failed to save LLM backend to system.db: {e}"))?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    tracing::info!(
        model = %model_path_str,
        quantization = %quantization,
        "local LLM configured via onboarding setup"
    );

    Ok(SetupLlmResult {
        model_path: dest.display().to_string(),
        quantization,
    })
}

/// Infers the quantization type from a GGUF filename.
///
/// Looks for common patterns like `Q8_0`, `Q4_K_M`, `Q5_K_S`, etc.
/// Returns `"q4_k_m"` as a safe default if nothing is detected.
fn infer_quantization(stem: &str) -> String {
    let upper = stem.to_uppercase();
    // Common GGUF quantization suffixes (ordered by specificity)
    let patterns = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M", "Q3_K_S",
        "Q2_K", "IQ4_XS", "IQ3_M", "IQ2_S", "F16", "F32",
    ];
    for p in &patterns {
        if upper.contains(p) {
            return p.to_lowercase();
        }
    }
    "q4_k_m".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path_ends_with_apollia_toml() {
        // GIVEN the default config path
        let path = default_config_path();
        // THEN it ends with apollia.toml inside .apollia directory
        assert!(path.ends_with("apollia.toml"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }

    // Deliberately not a `#[tokio::test]`. The home guard is a `std` mutex, and
    // holding one across an await point is denied workspace-wide, for the usual
    // reason: the task can be parked on another thread while the lock stays
    // taken. Dropping the guard before the call would defeat its purpose, since
    // the value being asserted is read inside that call. Driving the future on a
    // runtime this test owns keeps the whole read under the guard with no await
    // in sight.
    #[test]
    fn test_system_info_reports_the_resolved_data_directory() {
        // GIVEN the home directory the runtime itself resolves.
        // The guard keeps the tests that fake a home from swapping it out from
        // under this one: the variable is a process global and the harness runs
        // them concurrently.
        let _guard = crate::commands::home_env_lock();
        let home = apollia_core::paths::home_dir();

        // WHEN the About page asks the desktop for its system information
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime");
        let info = runtime
            .block_on(get_system_info())
            .expect("system info is available");

        // THEN the data directory is reported, absolute, rooted in that home,
        // and never a literal "~/.apollia" the operator would read as fact
        match home {
            Some(home) => {
                let data_dir = info
                    .data_dir
                    .expect("data_dir is present whenever the home directory resolves");
                assert!(
                    std::path::Path::new(&data_dir).is_absolute(),
                    "reported data dir is not absolute: {data_dir}"
                );
                assert!(
                    data_dir.starts_with(&home.display().to_string()),
                    "reported data dir {data_dir} is not rooted in the resolved home {}",
                    home.display()
                );
                assert!(
                    data_dir.ends_with(apollia_core::paths::DATA_DIR_NAME),
                    "reported data dir {data_dir} does not end with {}",
                    apollia_core::paths::DATA_DIR_NAME
                );
                assert!(
                    !data_dir.contains('~'),
                    "reported data dir {data_dir} is unexpanded and cannot be opened as-is"
                );
            }
            None => assert!(
                info.data_dir.is_none(),
                "a data dir was reported while no home directory resolves"
            ),
        }
    }

    #[test]
    fn test_onboarded_flag_path_ends_with_onboarded() {
        // GIVEN the onboarded flag path
        let path = onboarded_flag_path();
        // THEN it ends with .onboarded inside .apollia directory
        assert!(path.ends_with(".onboarded"));
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

        // THEN llm section redirects to dedicated view
        let llm = view
            .sections
            .iter()
            .find(|s| s.name == "llm")
            .expect("llm section");
        assert_eq!(llm.redirect_route, Some("llm".to_string()));
        assert!(llm.entries.is_empty());

        // AND triggers section is absent (migrated to SQLite CRUD)
        let triggers = view.sections.iter().find(|s| s.name == "triggers");
        assert!(triggers.is_none());
    }

    #[tokio::test]
    async fn test_get_system_info_returns_valid_data() {
        // GIVEN the get_system_info command
        // WHEN called
        let result = get_system_info().await;

        // THEN it succeeds with valid fields
        let info = result.expect("get_system_info should succeed");
        assert!(!info.version.is_empty(), "version should not be empty");
        assert!(!info.os.is_empty(), "os should not be empty");
        assert!(
            info.os.contains(std::env::consts::OS),
            "os should contain the current OS"
        );
        assert!(
            info.os.contains(std::env::consts::ARCH),
            "os should contain the current architecture"
        );
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
    fn test_infer_quantization_common_patterns() {
        // GIVEN various GGUF filenames
        // THEN the quantization is correctly inferred
        assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0"), "q8_0");
        assert_eq!(infer_quantization("llama-3-8b-Q4_K_M"), "q4_k_m");
        assert_eq!(infer_quantization("mistral-7b-Q5_K_S"), "q5_k_s");
        assert_eq!(infer_quantization("phi-3-mini-F16"), "f16");
        assert_eq!(infer_quantization("model-Q3_K_M"), "q3_k_m");
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

    #[test]
    fn test_infer_quantization_fallback() {
        // GIVEN a filename with no recognizable quantization
        // THEN the default is returned
        assert_eq!(infer_quantization("some-random-model"), "q4_k_m");
    }
}
