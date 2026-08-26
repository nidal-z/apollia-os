//! Everything `main` does around the Tauri builder: reading `apollia.toml`,
//! pointing the process at the bundled Python, populating the shared locks
//! once the supervisor is up, loading the installed agents, arming the STT
//! hotkey, and ending the process on a startup step that has no recovery.

use std::sync::Arc;

use apollia_core::{PendingApprovals, SttConfigRepository, ToolsConfig};
use apollia_llm::LlmRouter;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};

/// Searches for `apollia.toml` in priority order and applies all parsable sections
/// (llm, triggers, notifications) to the provided `EmbeddedConfig`.
///
/// Search order (first match wins):
///   1. `~/.apollia/apollia.toml` : canonical desktop config (the same file
///      `crate::commands::config` reads/writes and `sync_to_toml` mirrors backends to)
///   2. `./apollia.toml`          : CWD (useful when running from workspace root)
///
/// The legacy XDG location `~/.config/apollia/apollia.toml` is deliberately NOT
/// searched. It is not the desktop's canonical config path, yet a stale copy
/// left there on a developer machine would be resolved on a clean profile
/// (after `rm -rf ~/.apollia`) and its `[[llm.backends]]` entries would be
/// silently imported into a fresh `system.db` by the supervisor's
/// TOML-to-`system.db` migration, resurrecting a backend the user never
/// configured. `system.db` is the source of truth for LLM backends; a clean
/// install must start with none.
///
/// Returns the config unchanged if no file is found.
pub(crate) fn load_toml_config(config: EmbeddedConfig) -> EmbeddedConfig {
    let candidates = [
        apollia_core::paths::data_dir().map(|d| d.join("apollia.toml")),
        std::env::current_dir().ok().map(|d| d.join("apollia.toml")),
    ];

    for maybe_path in candidates.into_iter().flatten() {
        if maybe_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&maybe_path) {
                let mut updated = config.apply_toml(&content);
                updated.config_path = Some(maybe_path);
                return updated;
            }
        }
    }
    config
}

/// Point the embedded Python interpreter at the bundled `python-build-standalone`
/// distribution shipped inside the app bundle, so PyO3 never resolves against the
/// user's system Python.
///
/// Must be called **before** any PyO3 code (including `init_embedded()`), because
/// PyO3's `auto-initialize` reads `PYTHONHOME` / `PYTHONPATH` only at first use.
///
/// Behaviour:
/// - If a `python/` directory is found adjacent to the executable (macOS
///   `Contents/Resources/python/`, Linux `../lib/apollia-os/python/` relative to
///   `/usr/bin/`, or `../python/` relative to a dev `target/release/` layout),
///   the interpreter is reconfigured against it.
/// - If not found, logs a warning and leaves env vars alone (dev mode: the
///   developer's Homebrew/pyenv Python takes over, same as before).
pub(crate) fn setup_bundled_python() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "the bundled Python is skipped",
                "python.bundled.exe_path.failed"
            );
            return;
        }
    };
    let exe_dir = match exe.parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    // Candidate search order: first match wins.
    let candidates: [std::path::PathBuf; 4] = [
        // macOS: Contents/MacOS/apollia-desktop -> Contents/Resources/python/
        exe_dir.join("../Resources/python"),
        // Linux AppImage / .deb: usr/bin/apollia-desktop -> usr/lib/apollia-os/python/
        exe_dir.join("../lib/apollia-os/python"),
        // Windows: Tauri stages resources next to the executable.
        exe_dir.join("python"),
        // Dev build fallback: target/release/apollia-desktop -> target/python-bundle/<triple>/python/
        // (populated by packaging/build-python-bundle.sh during dev)
        exe_dir.join("../../resources/python"),
    ];

    // The interpreter sits at a different place per platform: `bin/python3.13`
    // on POSIX, `python.exe` at the root on Windows. Probing only the POSIX path
    // made every candidate fail on Windows, so no bundled Python was ever found
    // and every Python agent fell back to the system interpreter (or none).
    let has_interpreter =
        |p: &std::path::Path| p.join("bin/python3.13").exists() || p.join("python.exe").exists();

    let python_root = match candidates.iter().find(|p| has_interpreter(p)) {
        Some(p) => match p.canonicalize() {
            Ok(abs) => abs,
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "python.bundled.canonicalize.failed"
                );
                return;
            }
        },
        None => {
            tracing::warn!(
                exe = %exe.display(),
                detail = "falling back to the system Python",
                "python.bundled.absent"
            );
            return;
        }
    };

    // PYTHONPATH must list the stdlib, its C-extension dir (lib-dynload) and
    // site-packages. The embedded interpreter does not reliably add the stdlib
    // from PYTHONHOME alone, so setting PYTHONPATH to only site-packages left C
    // stdlib modules (math, _opcode, ...) unresolvable and every Python agent
    // failed to import with ModuleNotFoundError. Windows uses a flat Lib/DLLs
    // layout; POSIX uses lib/python3.13.
    let (stdlib, dynload, site_packages) = if python_root.join("lib/python3.13").is_dir() {
        let base = python_root.join("lib/python3.13");
        (
            base.clone(),
            base.join("lib-dynload"),
            base.join("site-packages"),
        )
    } else {
        (
            python_root.join("Lib"),
            python_root.join("DLLs"),
            python_root.join("Lib/site-packages"),
        )
    };
    let sep = if cfg!(windows) { ";" } else { ":" };
    let pythonpath = [&stdlib, &dynload, &site_packages]
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(sep);
    std::env::set_var("PYTHONHOME", &python_root);
    std::env::set_var("PYTHONPATH", &pythonpath);

    // Help dyld find libpython when the Python interpreter is invoked as a
    // subprocess (e.g. by the PythonExecutor tool or `python3 -m venv`).
    #[cfg(target_os = "macos")]
    std::env::set_var("DYLD_FALLBACK_LIBRARY_PATH", python_root.join("lib"));
    #[cfg(target_os = "linux")]
    std::env::set_var("LD_LIBRARY_PATH", python_root.join("lib"));
    // Windows has no equivalent variable: the loader searches the executable's
    // directory and then PATH. `python313.dll` sits next to `python.exe` in the
    // bundle, so the bundle root has to be on PATH for a spawned interpreter to
    // start at all. Prepend, never replace.
    #[cfg(target_os = "windows")]
    {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let mut entries = vec![python_root.clone()];
        entries.extend(std::env::split_paths(&existing));
        match std::env::join_paths(entries) {
            Ok(joined) => std::env::set_var("PATH", joined),
            Err(e) => tracing::warn!(error = %e, "python.bundled.path.prepend.failed"),
        }
    }

    // Name the interpreter explicitly so the few call sites that genuinely want
    // the bundled environment can target it by absolute path. Reaching it
    // through `python3` on PATH only ever worked by accident: PATH is prepended
    // with the bundle on Windows alone, so on macOS and Linux `python3` is the
    // system interpreter being redirected by PYTHONHOME, which crashes outright
    // whenever its minor version differs from the bundled one.
    let interpreter = if cfg!(windows) {
        python_root.join("python.exe")
    } else {
        python_root.join("bin/python3.13")
    };
    std::env::set_var("APOLLIA_BUNDLED_PYTHON", &interpreter);

    tracing::info!(
        python_root = %python_root.display(),
        interpreter = %interpreter.display(),
        "python.bundled.configured"
    );
}

/// Load the STT config from `system.db` for the desktop hotkey listener.
///
/// Returns `None` (and logs a warning) when the DB cannot be opened or read,
/// disabling the hotkey gracefully.
pub(crate) fn load_stt_config(
    apollia_data_dir: &std::path::Path,
) -> Option<apollia_core::SttConfigRow> {
    let system_db = apollia_data_dir.join(apollia_core::paths::DataFile::System.file_name());
    let repo = match SttConfigRepository::open(&system_db) {
        Ok(repo) => repo,
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "the hotkey is disabled",
                "stt.config.store.open.failed"
            );
            return None;
        }
    };
    match repo.get_or_default() {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            tracing::warn!(error = %e, detail = "the hotkey is disabled", "stt.config.read.failed");
            None
        }
    }
}

/// Borrowed references to the shared `OnceLock`s / `RwLock` populated from the
/// `RuntimeHandle` once the supervisor is running.
pub(crate) struct RuntimeLocks<'a> {
    pub(crate) event_bus: &'a std::sync::OnceLock<EventBusSender>,
    pub(crate) llm_router: &'a std::sync::RwLock<Option<Arc<LlmRouter>>>,
    pub(crate) tool_registry: &'a std::sync::OnceLock<ToolRegistryHandle>,
    pub(crate) audit_trail: &'a std::sync::OnceLock<AuditTrailHandle>,
    pub(crate) pending_approvals: &'a std::sync::OnceLock<Arc<PendingApprovals>>,
    pub(crate) task_repository: &'a std::sync::OnceLock<Arc<TaskRepository>>,
    pub(crate) pending_user_inputs: &'a std::sync::OnceLock<PendingUserInputs>,
    pub(crate) mcp_handle: &'a std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>,
    pub(crate) agent_registry: &'a std::sync::OnceLock<AgentRegistryHandle>,
    pub(crate) task_router: &'a std::sync::OnceLock<TaskRouterHandle<DynBackend>>,
    pub(crate) mailbox_handle: &'a std::sync::OnceLock<AgentMailboxHandle>,
    pub(crate) user_memory: &'a std::sync::OnceLock<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pub(crate) tools_config: &'a std::sync::OnceLock<ToolsConfig>,
}

/// Populate the shared locks from the running `RuntimeHandle`.
///
/// Required for parity between Chat Libre, Chat Agent, and task-mode flows.
/// Without these, Python agents in non-chat-libre flows lose `ctx.a2a_invoke`,
/// `ctx.mailbox`, `ctx.user_context`, and apollia.toml's `[tools]` overrides.
pub(crate) fn populate_runtime_locks(runtime_handle: &RuntimeHandle, locks: RuntimeLocks<'_>) {
    let _ = locks.event_bus.set(runtime_handle.event_sender.clone());
    *locks
        .llm_router
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = runtime_handle.llm_router.clone();
    let _ = locks
        .tool_registry
        .set(runtime_handle.tool_registry_handle.clone());
    if let Some(audit) = runtime_handle.audit_trail.clone() {
        let _ = locks.audit_trail.set(audit);
    }
    if let Some(pa) = runtime_handle.pending_approvals.clone() {
        let _ = locks.pending_approvals.set(pa);
    }
    if let Some(repo) = runtime_handle.task_repository.clone() {
        let _ = locks.task_repository.set(repo);
    }
    if let Some(chat) = runtime_handle.chat_manager.as_ref() {
        let _ = locks.pending_user_inputs.set(chat.pending_user_inputs());
    }
    if let Some(mcp) = runtime_handle.mcp_handle.clone() {
        let _ = locks.mcp_handle.set(mcp);
    }
    let _ = locks
        .agent_registry
        .set(runtime_handle.registry_handle.clone());
    let _ = locks.task_router.set(runtime_handle.router_handle.clone());
    if let Some(mailbox) = runtime_handle.mailbox_handle.clone() {
        let _ = locks.mailbox_handle.set(mailbox);
    }
    if let Some(um) = runtime_handle.user_memory.clone() {
        let _ = locks.user_memory.set(um);
    }
    let _ = locks.tools_config.set(runtime_handle.tools_config.clone());
}

/// Auto-load every enabled installed agent into the running supervisor.
///
/// Runs after the `OnceLock`s are populated so the `ProductionBackendFactory`
/// can build real backends. Failures per agent are logged and skipped: one
/// broken agent must not abort boot of the others.
pub(crate) fn auto_load_installed_agents(
    repo: AgentRepository,
    factory: &Arc<dyn AgentBackendFactory>,
    runtime_handle: &RuntimeHandle,
) {
    let agent_loader_for_boot: Arc<dyn AgentLoader> = Arc::new(crate::backend::AIPAgentLoader);
    let agents = match repo.list_enabled() {
        Ok(agents) => agents,
        Err(e) => {
            tracing::warn!(error = %e, detail = "the auto-load is skipped", "agent.list.failed");
            return;
        }
    };

    for agent in &agents {
        if !agent.enabled {
            continue;
        }
        let manifest = match agent_loader_for_boot.load_and_validate(&agent.install_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(name = %agent.name, error = %e, "agent.load.failed");
                let _ = runtime_handle.event_sender.send(
                    apollia_core::events::RuntimeEvent::AgentLoadFailed {
                        name: agent.name.clone(),
                        error: e.to_string(),
                    },
                );
                continue;
            }
        };

        // Register in AgentRegistry. We're on the main thread (not inside
        // Tokio); reuse the current handle or build a small current-thread
        // runtime to drive the async registration to completion.
        let rt = match tokio::runtime::Handle::try_current().or_else(|_| {
            Ok::<_, std::io::Error>(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .handle()
                    .clone(),
            )
        }) {
            Ok(handle) => handle,
            Err(e) => {
                // The contract of this loop is per-agent: a runtime that will
                // not build is the same class of failure as a manifest that
                // will not load, and used to abort the boot of every agent.
                tracing::warn!(
                    name = %agent.name,
                    error = %e,
                    detail = "the auto-load is skipped",
                    "agent.autoload.no_runtime"
                );
                continue;
            }
        };

        let registry_handle = runtime_handle.registry_handle.clone();
        let router_handle = runtime_handle.router_handle.clone();
        let event_sender = runtime_handle.event_sender.clone();
        let task_repository = runtime_handle.task_repository.clone();
        let factory_ref = factory.clone();
        let install_path = agent.install_path.clone();
        let agent_manifest = agent.manifest.clone();
        let max_concurrent = manifest.max_concurrent_tasks;
        let agent_name = manifest.name.clone();

        rt.block_on(async move {
            let agent_id = match registry_handle.register(manifest).await {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(name = %agent_name, error = %e, "agent.register.failed");
                    return;
                }
            };

            if let Err(e) = registry_handle
                .update_state(
                    agent_id.as_str(),
                    apollia_core::process::ProcessState::Active,
                )
                .await
            {
                tracing::warn!(name = %agent_name, error = %e, "agent.activate.failed");
                return;
            }

            let agent_backend = factory_ref.create_for_agent(&install_path, &agent_manifest);
            let mut coordinator = apollia_runtime::coordinator::ExecutionCoordinator::new(
                agent_id.clone(),
                max_concurrent,
                event_sender,
                agent_backend,
            )
            .with_agent_name(agent_name.clone());
            if let Some(ref repo) = task_repository {
                coordinator = coordinator.with_task_repository(
                    Arc::clone(repo),
                    apollia_core::observability::ObservabilityConfig::default(),
                );
            }

            let _ = router_handle
                .register_coordinator(agent_id.clone(), coordinator)
                .await;
            tracing::info!(name = %agent_name, id = %agent_id, "agent.autoloaded");
        });
    }
}

/// Wire the global STT hotkey + recording overlay to the full `SttFlow`
/// pipeline. Best-effort: each failure degrades gracefully with a warning.
///
/// The flow holds the shared engine cell and reads it on each trigger, so a
/// model brought online mid-session (via `reload_stt`) is picked up without
/// re-registering. Registration itself is one-shot: the caller only invokes
/// this when no flow is armed yet. Changing the hotkey binding still needs a
/// restart. Takes an [`AppHandle`](tauri::AppHandle) so it can run both from
/// the Tauri `setup` closure and from the reload command.
pub(crate) fn setup_stt_hotkey(
    app: &tauri::AppHandle,
    stt_cfg: &apollia_core::SttConfigRow,
    runtime_handle: &RuntimeHandle,
    stt_flow_state: &crate::commands::stt::SttFlowState,
) {
    // Release any previously registered global shortcut so a changed binding
    // replaces the old one instead of leaving the stale shortcut active. STT is
    // the only consumer of global shortcuts, so this affects nothing else.
    let _ = crate::stt::hotkey::unregister_all(app);

    let flow = Arc::new(crate::stt::flow::SttFlow::new(
        stt_cfg.clone(),
        runtime_handle.stt_engine.clone(),
        runtime_handle.event_sender.clone(),
        app.clone(),
    ));

    let mode = crate::stt::hotkey::TriggerMode::from_config(&stt_cfg.trigger_mode);
    let listener = crate::stt::hotkey::HotkeyListener::new(stt_cfg.hotkey.clone(), mode);

    // Make the flow accessible to push-to-talk IPC commands.
    if let Ok(mut guard) = stt_flow_state.lock() {
        *guard = Some(Arc::clone(&flow));
    }

    let flow_start = Arc::clone(&flow);
    let flow_stop = Arc::clone(&flow);

    if let Err(e) = listener.register(
        app,
        move || {
            // Global hotkey: the result is pasted into the focused application.
            flow_start.start_recording(crate::stt::flow::RecordingOrigin::Hotkey);
        },
        move || {
            let flow = Arc::clone(&flow_stop);
            tauri::async_runtime::spawn(async move {
                flow.stop_and_transcribe().await;
            });
        },
    ) {
        tracing::warn!(
            error = %e,
            detail = "recording by hotkey is disabled",
            "stt.hotkey.register.failed"
        );
    }

    // Recording overlay: secondary always-on-top window that shows a visual
    // indicator while audio capture is active. Escape cancels the recording.
    let flow_cancel = Arc::clone(&flow);
    let on_cancel = Arc::new(move || {
        flow_cancel.cancel_recording();
    });
    match crate::stt::overlay::RecordingOverlay::create(app, stt_cfg.hotkey.clone(), on_cancel) {
        Ok(overlay) => {
            crate::stt::overlay::spawn_overlay_listener(overlay, &runtime_handle.event_sender);
            tracing::info!("stt.overlay.created");
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "the visual indicator is disabled",
                "stt.overlay.create.failed"
            );
        }
    }
}

/// Returns `value`, or ends the process on a startup step that has no recovery.
///
/// `main` has no caller to return an error to, and the six steps that used
/// this wrote the same thing as a panic: an abort with a backtrace, in a
/// windowless GUI process where nobody reads one. A tracing line naming the
/// step is the same failure, said where the operator can find it.
pub(crate) fn or_exit<T, E: std::fmt::Display>(result: Result<T, E>, step: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(error = %error, step = %step, "desktop.startup.failed");
            std::process::exit(1);
        }
    }
}
