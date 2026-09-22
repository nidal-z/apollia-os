//! Tauri IPC commands for the Advanced and Security sections of Settings, and
//! the first-launch helper that registers a local GGUF model as a backend.

use std::path::PathBuf;

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
use serde::Serialize;

use super::default_config_path;

/// System information shown in the Advanced section of Settings.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    /// Apollia OS version (e.g. `"0.2.0-preview"`). The About screen reads the
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

/// The system interpreter, named by asking it rather than by naming a command.
///
/// `python3` alone is a Unix habit: Windows installs `python.exe` and its
/// `python3` is a Microsoft Store execution alias, a stub that sits on PATH,
/// answers a "which" probe and refuses to run. The panel therefore showed no
/// interpreter at all on Windows, and the two buttons that copy the path were
/// simply absent from the settings page and from About. Measured on
/// 2026-09-08 by the desktop books, which look for them.
///
/// Each candidate is tried by RUNNING it, so presence proves nothing and the
/// alias is skipped. `None` when none answers, which the interface reads as
/// "no system Python", and which stays true: the agents run on the bundled
/// interpreter either way.
async fn detect_system_python() -> Option<String> {
    for candidate in ["python3", "python"] {
        let mut probe = tokio::process::Command::new(candidate);
        apollia_core::subprocess_env::scrub_bundled_python_async(&mut probe);
        apollia_core::subprocess_window::hide_console_async(&mut probe);
        let output = probe
            .args(["-c", "import sys; print(sys.executable)"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await;
        if let Ok(output) = output {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Returns the system information for the Advanced section of Settings.
///
/// Detects the Apollia version, the OS, and the system Python, the last one
/// through [`detect_system_python`].
#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    let python_path = detect_system_python().await;

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
/// This is a first-launch helper. The backend is saved as `"local"`: inserted
/// as the default when no backend has that name, otherwise pointed at the new
/// file with its other settings and default flag kept, so choosing another
/// model during onboarding really switches engines.
///
/// `context_window` is the window chosen at onboarding, stored as
/// `config_json.context_window`, the key the engine is launched with and the
/// router sizes compaction against. `None` leaves the stored value, or the
/// runtime default, in place.
/// Call `reload_llm_from_db` afterwards to make the router available immediately.
#[tauri::command]
pub async fn setup_local_llm(
    gguf_path: String,
    context_window: Option<u32>,
) -> Result<SetupLlmResult, String> {
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

    // Upsert the "local" backend in system.db.
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
        let existing = repo
            .find_by_name("local")
            .map_err(|e| format!("failed to query system.db: {e}"))?;
        let config = local_backend(
            existing,
            &model_for_db,
            &device,
            &quant_for_db,
            context_window,
        );
        repo.save(&config)
            .map_err(|e| format!("failed to save LLM backend to system.db: {e}"))?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    tracing::info!(
        model = %model_path_str,
        quantization = %quantization,
        "llm.local.configured"
    );

    Ok(SetupLlmResult {
        model_path: dest.display().to_string(),
        quantization,
    })
}

/// The `"local"` backend row for a model chosen at onboarding.
///
/// A fresh row is the default. An existing one keeps its flags and every
/// setting the operator added (sampling, device), and takes the new model, its
/// quantisation and, when one was chosen, the new window. A row stored with the
/// legacy `context_size` key converges on `context_window`.
fn local_backend(
    existing: Option<LlmBackendConfig>,
    model_path: &str,
    device: &str,
    quantization: &str,
    context_window: Option<u32>,
) -> LlmBackendConfig {
    let mut config = existing.unwrap_or_else(|| LlmBackendConfig {
        name: "local".to_string(),
        provider: LlmProvider::LlamaCpp,
        model: String::new(),
        config_json: serde_json::json!({ "device": device }),
        enabled: true,
        is_default: true,
    });
    config.provider = LlmProvider::LlamaCpp;
    config.model = model_path.to_owned();
    if !config.config_json.is_object() {
        config.config_json = serde_json::json!({ "device": device });
    }
    if let Some(obj) = config.config_json.as_object_mut() {
        obj.insert("model_path".into(), model_path.into());
        obj.insert("quantization".into(), quantization.into());
        obj.remove("model_paths");
        if let Some(n) = context_window.filter(|n| *n > 0) {
            obj.insert("context_window".into(), n.into());
            obj.remove("context_size");
        }
    }
    config
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
    fn a_first_local_model_becomes_the_default_with_its_window() {
        // GIVEN no "local" backend yet
        // WHEN a model is chosen at onboarding with a 16k window
        let cfg = local_backend(
            None,
            "~/.apollia/models/a.gguf",
            "cpu",
            "q4_k_m",
            Some(16_384),
        );

        // THEN the row is the default and carries the window under the key the
        // runtime reads
        assert!(cfg.is_default);
        assert_eq!(cfg.model, "~/.apollia/models/a.gguf");
        assert_eq!(cfg.config_json["context_window"], 16_384);
        assert_eq!(cfg.config_json["model_path"], "~/.apollia/models/a.gguf");
    }

    #[test]
    fn choosing_another_model_updates_the_existing_row() {
        // GIVEN a "local" backend the operator already tuned, not the default,
        // stored with the legacy window key
        let existing = LlmBackendConfig {
            name: "local".into(),
            provider: LlmProvider::LlamaCpp,
            model: "~/.apollia/models/old.gguf".into(),
            config_json: serde_json::json!({
                "model_path": "~/.apollia/models/old.gguf",
                "temperature": 0.4,
                "context_size": 8192,
            }),
            enabled: true,
            is_default: false,
        };

        // WHEN another model is chosen with a new window
        let cfg = local_backend(
            Some(existing),
            "~/.apollia/models/new.gguf",
            "cpu",
            "q8_0",
            Some(65_536),
        );

        // THEN the row points at the new file, keeps its settings and flags, and
        // the window converges on the canonical key
        assert_eq!(cfg.model, "~/.apollia/models/new.gguf");
        assert_eq!(cfg.config_json["model_path"], "~/.apollia/models/new.gguf");
        assert_eq!(cfg.config_json["temperature"], 0.4);
        assert_eq!(cfg.config_json["context_window"], 65_536);
        assert!(cfg.config_json.get("context_size").is_none());
        assert!(!cfg.is_default);
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
    fn test_infer_quantization_common_patterns() {
        // GIVEN various GGUF filenames
        // WHEN the quantisation is inferred from each name
        // THEN the quantization is correctly inferred
        assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0"), "q8_0");
        assert_eq!(infer_quantization("llama-3-8b-Q4_K_M"), "q4_k_m");
        assert_eq!(infer_quantization("mistral-7b-Q5_K_S"), "q5_k_s");
        assert_eq!(infer_quantization("phi-3-mini-F16"), "f16");
        assert_eq!(infer_quantization("model-Q3_K_M"), "q3_k_m");
    }

    #[test]
    fn test_infer_quantization_fallback() {
        // GIVEN a filename with no recognizable quantization
        // WHEN the quantisation is inferred from it
        // THEN the default is returned
        assert_eq!(infer_quantization("some-random-model"), "q4_k_m");
    }
}

/// The Python interpreter setting, as the Advanced section shows it.
#[derive(Debug, Serialize)]
pub struct PythonInterpreterSetting {
    /// The absolute path in `[tools] python_interpreter`, or `None` when the
    /// bundled interpreter is in use.
    pub chosen: Option<String>,
    /// Absolute path of the bundled interpreter, when this process knows it.
    ///
    /// `None` in a development tree, where no bundle is staged. The panel says
    /// so rather than naming a path that does not exist.
    pub bundled: Option<String>,
    /// Minor version the bundled standard library requires of any chosen
    /// interpreter, so the panel can state the condition before it is failed.
    pub required_minor: u32,
}

/// Returns the Python interpreter setting for the Advanced section of Settings.
#[tauri::command]
pub async fn get_python_interpreter() -> Result<PythonInterpreterSetting, String> {
    let chosen = read_config_tools_python_interpreter().await;
    Ok(PythonInterpreterSetting {
        chosen,
        bundled: apollia_tools::tools::python_discovery::bundled_interpreter()
            .map(|p| p.display().to_string()),
        required_minor: apollia_tools::tools::python_discovery::BUNDLED_PYTHON_MINOR,
    })
}

/// Chooses a Python interpreter, or returns to the bundled one.
///
/// `path` names an interpreter by absolute path; `None`, or a blank string,
/// removes the setting and returns to the interpreter Apollia ships with, which
/// is the configuration that depends on nothing installed on the machine.
///
/// The path is checked before it is written: it has to exist, start, and report
/// a minor version the bundled standard library can be read by. A refusal comes
/// back as the message to show, naming which of the three conditions failed.
/// The change applies to the agents started after it.
#[tauri::command]
pub async fn set_python_interpreter(path: Option<String>) -> Result<(), String> {
    let chosen = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());

    if let Some(candidate) = chosen.as_deref() {
        apollia_tools::tools::python_discovery::validate_chosen_interpreter(std::path::Path::new(
            candidate,
        ))
        .map_err(|e| e.to_string())?;
    }

    let config_path = default_config_path();
    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }
    let mut doc = if config_path.exists() {
        tokio::fs::read_to_string(&config_path)
            .await
            .map_err(|e| format!("failed to read {}: {e}", config_path.display()))?
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("failed to parse {}: {e}", config_path.display()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    let tools = doc
        .entry("tools")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    if tools.as_table_mut().is_none() {
        *tools = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if let Some(table) = tools.as_table_mut() {
        match chosen.as_deref() {
            Some(candidate) => table["python_interpreter"] = toml_edit::value(candidate),
            // Removed rather than blanked: an absent key and the default are the
            // same thing, and a blank string in the file reads as a setting
            // somebody meant.
            None => {
                table.remove("python_interpreter");
            }
        }
    }

    tokio::fs::write(&config_path, doc.to_string())
        .await
        .map_err(|e| format!("failed to write {}: {e}", config_path.display()))?;

    tracing::info!(
        chosen = chosen.as_deref().unwrap_or("<bundled>"),
        "python.interpreter.setting_saved"
    );
    Ok(())
}

/// Read `[tools] python_interpreter` out of `apollia.toml`.
///
/// `None` when the file is absent, unreadable, or does not carry the key, which
/// are all the same answer: the bundled interpreter is in use.
async fn read_config_tools_python_interpreter() -> Option<String> {
    let path = default_config_path();
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let parsed: toml::Value = content.parse().ok()?;
    parsed
        .get("tools")?
        .get("python_interpreter")?
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}
