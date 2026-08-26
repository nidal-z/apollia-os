//! Tauri IPC commands for the Advanced and Security sections of Settings, and
//! the first-launch helper that registers a local GGUF model as a backend.

use std::path::PathBuf;

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
use serde::Serialize;

use super::default_config_path;

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
        "llm.local.configured"
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
