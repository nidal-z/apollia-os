//! Tauri IPC commands for the Advanced and Security sections of Settings, and
//! the first-launch helper that registers a local GGUF model as a backend.

use apollia_runtime::api::routes_local_ai::SetupLlmBody;
use apollia_runtime::local_ai::{self, SetupLlmResult};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use super::default_config_path;
use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

/// System information shown in the Advanced section of Settings.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    /// Apollia OS version (e.g. `"0.3.0"`), shown on the About screen.
    pub version: String,
    /// Operating system and architecture (e.g. `"macos aarch64"`).
    pub os: String,
    /// Processor architecture this build runs on (e.g. `"aarch64"`), shown
    /// beside the processor in System > Hardware.
    pub arch: String,
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
        arch: std::env::consts::ARCH.to_owned(),
        python_path,
        data_dir,
    })
}

/// Route of the engine list on the runtime.
pub(crate) const ENGINES_ROUTE: &str = "/api/v1/engines";

/// Returns the engines of this machine, embedded or installed by the engine
/// store, each with the version it states at its source, and the dictation
/// engine, over `GET /api/v1/engines`. Read by System > Engines.
#[tauri::command]
pub async fn list_engines(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(ENGINES_ROUTE).await
}

/// Route of the engine store settings on the runtime.
pub(crate) const ENGINE_SETTINGS_ROUTE: &str = "/api/v1/engines/settings";

/// Route of the engine update check on the runtime.
pub(crate) const ENGINE_CHECK_ROUTE: &str = "/api/v1/engines/check";

/// Route of the resource measurement on the runtime.
pub(crate) const RESOURCE_USAGE_ROUTE: &str = "/api/v1/llm/hardware/usage";

/// Writes the channel the engines follow on the runtime this application
/// drives, keeping the mirror address the store already holds: the settings
/// are read, the channel replaced, and the whole written back.
pub(crate) async fn save_engine_channel_on(
    link: &RuntimeLink,
    channel: &str,
) -> Result<Value, String> {
    let mut settings = link.get_json(ENGINE_SETTINGS_ROUTE).await?;
    settings["channel"] = Value::String(channel.to_owned());
    link.put_json(ENGINE_SETTINGS_ROUTE, &settings).await
}

/// Sets the channel the engines follow, `stable` or `preview`, over `PUT
/// /api/v1/engines/settings`. The engine store of the runtime holds the
/// value; the interface keeps none. Written by System > Engines.
#[tauri::command]
pub async fn set_engine_channel(
    link: State<'_, RuntimeLink>,
    channel: String,
) -> Result<Value, String> {
    save_engine_channel_on(link.inner(), &channel).await
}

/// Fetches and verifies the manifest of the followed channel and returns, per
/// variant, whether a newer engine exists, over `POST /api/v1/engines/check`.
/// A refusal carries the runtime's sentence. Asked by System > Engines.
#[tauri::command]
pub async fn check_engine_updates(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.post_json(ENGINE_CHECK_ROUTE, &json!({})).await
}

/// Measures once the memory, the processor load and the video memory the
/// machine of the runtime is using, over `GET /api/v1/llm/hardware/usage`.
/// Nothing samples between two calls: the resource monitor of System >
/// Hardware asks while it is shown and stops when it is not.
#[tauri::command]
pub async fn get_resource_usage(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(RESOURCE_USAGE_ROUTE).await
}

/// Route of the security posture on the runtime.
pub(crate) const POSTURE_ROUTE: &str = "/api/v1/security-posture";

/// Returns the active security posture for the Security section of Settings,
/// as the runtime that runs the tools detects it, over `GET
/// /api/v1/security-posture`.
///
/// Surfaces the isolation level of native tools and the agent-code trust model
/// so the operator can see, without reading logs, what confinement is active on
/// their platform.
#[tauri::command]
pub async fn get_security_posture(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(POSTURE_ROUTE).await
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
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::attached::{gate, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
    }

    #[tokio::test]
    async fn test_the_interpreter_setting_and_the_posture_are_read_from_the_daemon() {
        // GIVEN a daemon with a chosen interpreter, on a machine without a
        // tool sandbox
        let server = MockServer::start().await;
        authorized("GET", PYTHON_INTERPRETER_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "chosen": "/opt/python/bin/python3", "bundled": null, "required_minor": 13,
            })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("GET", POSTURE_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "platform": "macos aarch64", "tool_sandbox": "dev_no_sandbox",
                "agent_execution": "in_process_trusted", "rlimits_active": true,
                "unshare_available": false,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the settings read the interpreter and the posture
        let interpreter = link.get_json(PYTHON_INTERPRETER_ROUTE).await.unwrap();
        let posture = link.get_json(POSTURE_ROUTE).await.unwrap();

        // THEN both are the daemon's
        assert_eq!(interpreter["chosen"], "/opt/python/bin/python3");
        assert_eq!(posture["tool_sandbox"], "dev_no_sandbox");
    }

    #[tokio::test]
    async fn test_an_interpreter_is_chosen_then_the_bundled_one_again_on_the_daemon() {
        // GIVEN a daemon that takes a path, then none
        let server = MockServer::start().await;
        authorized("PUT", PYTHON_INTERPRETER_ROUTE)
            .and(body_json(json!({ "path": "/opt/python/bin/python3" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "chosen": "/opt/python/bin/python3", "bundled": null, "required_minor": 13,
            })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("PUT", PYTHON_INTERPRETER_ROUTE)
            .and(body_json(json!({ "path": null })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "chosen": null, "bundled": null, "required_minor": 13,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the setting names an interpreter, then returns to the bundled one
        let chosen =
            save_python_interpreter_on(&link, Some("/opt/python/bin/python3".into())).await;
        let bundled = save_python_interpreter_on(&link, None).await;

        // THEN both writes succeed
        assert!(chosen.is_ok(), "{chosen:?}");
        assert!(bundled.is_ok(), "{bundled:?}");
    }

    #[tokio::test]
    async fn test_an_interpreter_the_daemon_refuses_shows_its_sentence() {
        // GIVEN a daemon that refuses a relative path
        let server = MockServer::start().await;
        authorized("PUT", PYTHON_INTERPRETER_ROUTE)
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "error": "the path is relative", "code": "invalid", "diagnostics": [],
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the setting names a relative path
        let refused =
            save_python_interpreter_on(&attached(&server), Some("bin/python3".into())).await;

        // THEN the error is the daemon's sentence, word for word
        assert_eq!(refused.unwrap_err(), "the path is relative");
    }

    #[test]
    fn test_the_system_settings_are_routed_and_the_system_info_stays_with_the_window() {
        // GIVEN the commands of the system settings
        let commands = [
            "get_python_interpreter",
            "set_python_interpreter",
            "get_security_posture",
            "get_system_info",
            "list_engines",
            "set_engine_channel",
            "check_engine_updates",
            "get_resource_usage",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
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
        assert_eq!(info.arch, std::env::consts::ARCH);
    }

    #[tokio::test]
    async fn test_the_engine_channel_is_written_through_the_store_route_and_keeps_the_mirror() {
        // GIVEN a daemon whose engine store follows stable from a mirror
        let server = MockServer::start().await;
        authorized("GET", ENGINE_SETTINGS_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "channel": "stable", "base_url": "https://mirror.example/engines",
            })))
            .expect(1)
            .mount(&server)
            .await;
        let written = json!({
            "channel": "preview", "base_url": "https://mirror.example/engines",
        });
        authorized("PUT", ENGINE_SETTINGS_ROUTE)
            .and(body_json(written.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_json(written.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window chooses the preview channel
        let saved = save_engine_channel_on(&attached(&server), "preview").await;

        // THEN the store route received the new channel with the mirror it
        //      already held, and its answer comes back unchanged
        assert_eq!(saved.unwrap(), written);
    }

    #[tokio::test]
    async fn test_a_refused_update_check_carries_the_sentence_of_the_daemon() {
        // GIVEN a daemon whose build carries no distribution key
        let server = MockServer::start().await;
        authorized("POST", ENGINE_CHECK_ROUTE)
            .respond_with(ResponseTemplate::new(503).set_body_json(json!({
                "error": "this build carries no distribution key",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window asks for the engine updates
        let checked = attached(&server)
            .post_json(ENGINE_CHECK_ROUTE, &json!({}))
            .await;

        // THEN the refusal is the daemon's sentence
        assert_eq!(
            checked.unwrap_err(),
            "this build carries no distribution key"
        );
    }

    #[tokio::test]
    async fn test_the_resource_usage_is_measured_by_the_daemon_and_read_as_it_answers() {
        // GIVEN a daemon measuring its machine
        let server = MockServer::start().await;
        let measured = json!({
            "ram_used_gb": 21.5, "ram_total_gb": 64.0, "cpu_percent": 37.5,
            "vram": { "kind": "dedicated", "used_gb": 6.0, "total_gb": 24.0 },
        });
        authorized("GET", RESOURCE_USAGE_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(measured.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window reads the usage through the seam
        let read = attached(&server).get_json(RESOURCE_USAGE_ROUTE).await;

        // THEN it is the daemon's measurement, unchanged
        assert_eq!(read.unwrap(), measured);
    }

    #[tokio::test]
    async fn test_the_engine_list_is_read_from_the_daemon_as_it_answers() {
        // GIVEN a daemon listing one engine with the version it states
        let server = MockServer::start().await;
        let listed = json!({
            "engines": [{
                "engine": "llama-server", "tag": null, "variant": "metal",
                "path": "/app/llama-server", "source": "embedded",
                "version": "7001 (0ff1ce0)",
            }],
            "channel": "stable", "manifest_sequence": null, "updates": [],
            "cuda_proposable": false, "dictation": null, "verifiable": true,
        });
        authorized("GET", ENGINES_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(listed.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window reads the engine list through the seam
        let read = attached(&server).get_json(ENGINES_ROUTE).await;

        // THEN it is the daemon's answer, version included, unchanged
        assert_eq!(read.unwrap(), listed);
    }
}

/// Route of the Python interpreter setting on the runtime.
pub(crate) const PYTHON_INTERPRETER_ROUTE: &str = "/api/v1/config/python-interpreter";

/// Chooses an interpreter on the runtime this application drives, or returns
/// to the bundled one with `None`; the runtime checks the path before it
/// writes it, and its refusal names the condition that failed.
pub(crate) async fn save_python_interpreter_on(
    link: &RuntimeLink,
    path: Option<String>,
) -> Result<(), String> {
    link.put_json(PYTHON_INTERPRETER_ROUTE, &json!({ "path": path }))
        .await
        .map(|_| ())
}

/// Returns the Python interpreter setting for the Advanced section of
/// Settings: the chosen path, the interpreter the runtime ships, and the minor
/// version it requires, over `GET /api/v1/config/python-interpreter`.
#[tauri::command]
pub async fn get_python_interpreter(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(PYTHON_INTERPRETER_ROUTE).await
}

/// Chooses a Python interpreter, or returns to the bundled one, over `PUT
/// /api/v1/config/python-interpreter`.
///
/// `path` names an interpreter by absolute path; `None`, or a blank string,
/// returns to the interpreter Apollia ships with. The change applies to the
/// agents started after it.
#[tauri::command]
pub async fn set_python_interpreter(
    link: State<'_, RuntimeLink>,
    path: Option<String>,
) -> Result<(), String> {
    save_python_interpreter_on(link.inner(), path).await
}
