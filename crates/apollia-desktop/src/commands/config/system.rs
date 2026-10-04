//! Tauri IPC commands for the Advanced and Security sections of Settings, and
//! the first-launch helper that registers a local GGUF model as a backend.

use apollia_runtime::api::routes_local_ai::SetupLlmBody;
use apollia_runtime::local_ai::{self, SetupLlmResult};
use serde::Serialize;
use tauri::State;

use super::default_config_path;
use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

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

/// The port of the native API this window serves or attaches to, for the
/// addresses the interface shows (a webhook endpoint).
#[tauri::command]
pub async fn get_api_port(port: State<'_, crate::startup::ApiPort>) -> Result<u16, String> {
    Ok(port.0)
}

/// The daemon route of the local language model setup.
const LOCAL_LLM_ROUTE: &str = "/api/v1/local-ai/llm";

/// How long the daemon may take to copy a model into its models folder: a
/// large GGUF file copied across volumes takes minutes, not seconds.
const MODEL_COPY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Asks the daemon to copy `gguf_path` into its models folder and make it the
/// `local` backend.
pub(crate) async fn setup_local_llm_on(
    remote: &AttachedClient,
    gguf_path: String,
    context_window: Option<u32>,
) -> Result<SetupLlmResult, AttachedError> {
    let body = SetupLlmBody {
        gguf_path,
        context_window,
    };
    remote
        .post_within(LOCAL_LLM_ROUTE, &body, MODEL_COPY_DEADLINE)
        .await
}

/// Sets up a local embedded LLM from a user-selected GGUF file.
///
/// Copies the model into the models folder of the data directory, registers
/// it as the `local` backend in `system.db`, and returns the path for
/// confirmation. `context_window` is the window chosen at onboarding; `None`
/// leaves the stored value in place. Attached, the daemon does both, since it
/// loads the model. Call `reload_llm_from_db` afterwards to make the router
/// available immediately.
#[tauri::command]
pub async fn setup_local_llm(
    gguf_path: String,
    context_window: Option<u32>,
    link: State<'_, RuntimeLink>,
) -> Result<SetupLlmResult, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            let data_dir = default_config_path()
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            local_ai::setup_local_llm(&data_dir, &gguf_path, context_window)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            Ok(setup_local_llm_on(remote, gguf_path, context_window).await?)
        }
    }
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

    #[test]
    fn test_the_interpreter_is_written_then_removed_through_the_writer() {
        // GIVEN a file with a comment in its [tools] table
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[tools]\n# mine\n").unwrap();
        let none = apollia_core::config::PolicySettings::none();

        // WHEN an interpreter is chosen, then the bundled one is chosen again
        write_python_interpreter_at(&path, Some("/usr/bin/python3"), &none).expect("set");
        let chosen = std::fs::read_to_string(&path).unwrap();
        write_python_interpreter_at(&path, None, &none).expect("removed");
        let bundled = std::fs::read_to_string(&path).unwrap();

        // THEN the key is written next to the comment, then removed, the
        // comment kept both times
        assert!(
            chosen.contains("python_interpreter = \"/usr/bin/python3\""),
            "{chosen}"
        );
        assert!(chosen.contains("# mine"), "{chosen}");
        assert!(!bundled.contains("python_interpreter"), "{bundled}");
        assert!(bundled.contains("# mine"), "{bundled}");
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
    let policy = super::settings::policy_in_force();
    let written = chosen.clone();
    tokio::task::spawn_blocking(move || {
        write_python_interpreter_at(&config_path, written.as_deref(), &policy)
    })
    .await
    .map_err(|e| e.to_string())??;

    tracing::info!(
        chosen = chosen.as_deref().unwrap_or("<bundled>"),
        "python.interpreter.setting_saved"
    );
    Ok(())
}

/// Writes `[tools] python_interpreter` to the `apollia.toml` at `path` through
/// the one writer of the file. `None` removes the key rather than blanking it:
/// an absent key and the default are the same thing, and a blank string in the
/// file reads as a setting somebody meant.
pub(crate) fn write_python_interpreter_at(
    path: &std::path::Path,
    chosen: Option<&str>,
    policy: &apollia_core::config::PolicySettings,
) -> Result<(), String> {
    use apollia_core::config::writer::{update_config, ConfigEdit, KeyChange};

    const KEY: &str = "tools.python_interpreter";
    let change = match chosen {
        Some(candidate) => KeyChange::set(KEY, serde_json::json!(candidate)),
        None => KeyChange::remove(KEY),
    };
    update_config(path, ConfigEdit::Keys(vec![change]), policy)
        .map(|_| ())
        .map_err(|e| e.to_string())
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
