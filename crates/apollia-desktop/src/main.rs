//! Apollia OS desktop application (Tauri v2).
//!
//! Single-process architecture: the Apollia runtime runs embedded inside the
//! Tauri process via [`apollia_runtime::init_embedded()`]. The Unix socket
//! remains active so the CLI can be used alongside the desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod backend;
mod bundled_agents;
mod commands;
mod connectors_bridge;
mod events;
pub mod mcp;
mod project_context;
pub mod stt;
mod token_coalescer;
pub mod tray;

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
use mcp::McpRegistryClient;
use mcp::SecretStore;
use tauri::Manager;

/// Shared mutable LLM router: updated by `reload_llm`, read by
/// `ProductionChatAgentRunner` and `ProductionBackendFactory`.
pub type SharedLlmRouter = Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>;

/// Resolves a leading `~` against the platform home directory.
///
/// Goes through `apollia_core::paths` rather than reading `HOME`: that variable
/// is unset on Windows, and the previous `unwrap_or_default()` turned
/// `~/.apollia/apollia.toml` into a root-relative `/.apollia/apollia.toml`
/// there, so the canonical config was never found.
fn expand_tilde(s: &str) -> std::path::PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => match apollia_core::paths::home_dir() {
            Some(home) => home.join(rest),
            None => std::path::PathBuf::from(rest),
        },
        None => std::path::PathBuf::from(s),
    }
}

/// Searches for `apollia.toml` in priority order and applies all parsable sections
/// (llm, triggers, notifications) to the provided `EmbeddedConfig`.
///
/// Search order (first match wins):
///   1. `~/.apollia/apollia.toml` : canonical desktop config (the same file
///      `commands::config` reads/writes and `sync_to_toml` mirrors backends to)
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
fn load_toml_config(config: EmbeddedConfig) -> EmbeddedConfig {
    let candidates = [
        Some(expand_tilde("~/.apollia/apollia.toml")),
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
fn setup_bundled_python() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "current_exe() failed - bundled Python skipped");
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
                tracing::warn!(path = %p.display(), error = %e, "canonicalize() failed on bundled Python");
                return;
            }
        },
        None => {
            tracing::warn!(
                "no bundled Python found adjacent to {} - falling back to system Python",
                exe.display()
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
            Err(e) => tracing::warn!(error = %e, "could not prepend the bundled Python to PATH"),
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
        "bundled Python configured - PYTHONHOME/PYTHONPATH/APOLLIA_BUNDLED_PYTHON exported"
    );
}

/// Load the STT config from `system.db` for the desktop hotkey listener.
///
/// Returns `None` (and logs a warning) when the DB cannot be opened or read,
/// disabling the hotkey gracefully.
fn load_stt_config(apollia_data_dir: &std::path::Path) -> Option<apollia_core::SttConfigRow> {
    let system_db = apollia_data_dir.join("system.db");
    let repo = match SttConfigRepository::open(&system_db) {
        Ok(repo) => repo,
        Err(e) => {
            tracing::warn!(error = %e, "failed to open system.db for STT config - hotkey disabled");
            return None;
        }
    };
    match repo.get_or_default() {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            tracing::warn!(error = %e, "failed to read STT config from SQLite - hotkey disabled");
            None
        }
    }
}

/// Borrowed references to the shared `OnceLock`s / `RwLock` populated from the
/// `RuntimeHandle` once the supervisor is running.
struct RuntimeLocks<'a> {
    event_bus: &'a std::sync::OnceLock<EventBusSender>,
    llm_router: &'a std::sync::RwLock<Option<Arc<LlmRouter>>>,
    tool_registry: &'a std::sync::OnceLock<ToolRegistryHandle>,
    audit_trail: &'a std::sync::OnceLock<AuditTrailHandle>,
    pending_approvals: &'a std::sync::OnceLock<Arc<PendingApprovals>>,
    task_repository: &'a std::sync::OnceLock<Arc<TaskRepository>>,
    pending_user_inputs: &'a std::sync::OnceLock<PendingUserInputs>,
    mcp_handle: &'a std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>,
    agent_registry: &'a std::sync::OnceLock<AgentRegistryHandle>,
    task_router: &'a std::sync::OnceLock<TaskRouterHandle<DynBackend>>,
    mailbox_handle: &'a std::sync::OnceLock<AgentMailboxHandle>,
    user_memory: &'a std::sync::OnceLock<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    tools_config: &'a std::sync::OnceLock<ToolsConfig>,
}

/// Populate the shared locks from the running `RuntimeHandle`.
///
/// Required for parity between Chat Libre, Chat Agent, and task-mode flows.
/// Without these, Python agents in non-chat-libre flows lose `ctx.a2a_invoke`,
/// `ctx.mailbox`, `ctx.user_context`, and apollia.toml's `[tools]` overrides.
fn populate_runtime_locks(runtime_handle: &RuntimeHandle, locks: RuntimeLocks<'_>) {
    let _ = locks.event_bus.set(runtime_handle.event_sender.clone());
    *locks.llm_router.write().expect("llm_router_lock poisoned") =
        runtime_handle.llm_router.clone();
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
fn auto_load_installed_agents(
    repo: AgentRepository,
    factory: &Arc<dyn AgentBackendFactory>,
    runtime_handle: &RuntimeHandle,
) {
    let agent_loader_for_boot: Arc<dyn AgentLoader> = Arc::new(backend::AIPAgentLoader);
    let agents = match repo.list_enabled() {
        Ok(agents) => agents,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list installed agents - skipping auto-load");
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
                tracing::warn!(name = %agent.name, error = %e, "Failed to load installed agent at boot");
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
        let rt = tokio::runtime::Handle::try_current()
            .or_else(|_| {
                Ok::<_, std::io::Error>(
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .handle()
                        .clone(),
                )
            })
            .expect("failed to get tokio handle for agent auto-load");

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
                    tracing::warn!(name = %agent_name, error = %e, "Failed to register agent at boot");
                    return;
                }
            };

            if let Err(e) = registry_handle
                .update_state(agent_id.as_str(), apollia_core::process::ProcessState::Active)
                .await
            {
                tracing::warn!(name = %agent_name, error = %e, "Failed to activate agent at boot");
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
            tracing::info!(name = %agent_name, id = %agent_id, "Auto-loaded installed agent (post-init)");
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
    stt_flow_state: &commands::stt::SttFlowState,
) {
    // Release any previously registered global shortcut so a changed binding
    // replaces the old one instead of leaving the stale shortcut active. STT is
    // the only consumer of global shortcuts, so this affects nothing else.
    let _ = stt::hotkey::unregister_all(app);

    let flow = Arc::new(stt::flow::SttFlow::new(
        stt_cfg.clone(),
        runtime_handle.stt_engine.clone(),
        runtime_handle.event_sender.clone(),
        app.clone(),
    ));

    let mode = stt::hotkey::TriggerMode::from_config(&stt_cfg.trigger_mode);
    let listener = stt::hotkey::HotkeyListener::new(stt_cfg.hotkey.clone(), mode);

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
            flow_start.start_recording(stt::flow::RecordingOrigin::Hotkey);
        },
        move || {
            let flow = Arc::clone(&flow_stop);
            tauri::async_runtime::spawn(async move {
                flow.stop_and_transcribe().await;
            });
        },
    ) {
        tracing::warn!(error = %e, "STT hotkey registration failed - recording via hotkey disabled");
    }

    // Recording overlay: secondary always-on-top window that shows a visual
    // indicator while audio capture is active. Escape cancels the recording.
    let flow_cancel = Arc::clone(&flow);
    let on_cancel = Arc::new(move || {
        flow_cancel.cancel_recording();
    });
    match stt::overlay::RecordingOverlay::create(app, stt_cfg.hotkey.clone(), on_cancel) {
        Ok(overlay) => {
            stt::overlay::spawn_overlay_listener(overlay, &runtime_handle.event_sender);
            tracing::info!("recording overlay window created");
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to create recording overlay - visual indicator disabled");
        }
    }
}

fn main() {
    // Initialize tracing so Rust logs appear in the terminal during development.
    // RUST_LOG controls verbosity (e.g. RUST_LOG=apollia=debug); defaults to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("apollia=info,warn")),
        )
        .init();

    // Point PyO3 at the bundled Python BEFORE any Python runtime code runs.
    setup_bundled_python();

    // OnceLocks shared between ProductionBackendFactory and main().
    // Populated after init_embedded() returns, before any HTTP request arrives.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::RwLock::new(None));
    let tool_registry_lock: Arc<std::sync::OnceLock<ToolRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let audit_trail_lock: Arc<std::sync::OnceLock<AuditTrailHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_approvals_lock: Arc<std::sync::OnceLock<Arc<PendingApprovals>>> =
        Arc::new(std::sync::OnceLock::new());
    let task_repository_lock: Arc<std::sync::OnceLock<Arc<TaskRepository>>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_user_inputs_lock: Arc<std::sync::OnceLock<PendingUserInputs>> =
        Arc::new(std::sync::OnceLock::new());
    // Populated post-init_embedded() so the BridgeRunner can inject one
    // McpToolExecutor per registered MCP tool into the agent's dispatcher.
    let mcp_handle_lock: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>> =
        Arc::new(std::sync::OnceLock::new());
    // Agent registry + task router, required to build A2A delegate/invoker so
    // task-mode (triggers) and chat-agent flows can call `ctx.a2a_invoke(...)`
    // and discover virtual `a2a:*` tools, on parity with Chat Libre.
    let agent_registry_lock: Arc<std::sync::OnceLock<AgentRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let task_router_lock: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>> =
        Arc::new(std::sync::OnceLock::new());
    // Inter-agent mailbox, exposed as `ctx.mailbox` in Python agents.
    let mailbox_handle_lock: Arc<std::sync::OnceLock<AgentMailboxHandle>> =
        Arc::new(std::sync::OnceLock::new());
    // Global user memory, used by Chat Agent runner to build `ctx.user_context`.
    let user_memory_lock: Arc<std::sync::OnceLock<Arc<std::sync::Mutex<UserMemoryRepository>>>> =
        Arc::new(std::sync::OnceLock::new());
    // Tools config (`[tools]` from apollia.toml) drives web_search/web_read
    // params and statically-disabled tools. Populated from RuntimeHandle.
    let tools_config_lock: Arc<std::sync::OnceLock<ToolsConfig>> =
        Arc::new(std::sync::OnceLock::new());
    // Plan-gate registry, forwarded to per-agent engines so the desktop UI can
    // resolve a pending plan gate via submit_plan_decision.
    let plan_gates_lock: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>> =
        Arc::new(std::sync::OnceLock::new());
    // Plan cache, forwarded to per-agent engines so an orchestrated run can
    // reuse a plan instead of re-planning. The supervisor owns the repository;
    // this shares it rather than opening a second database.
    #[allow(clippy::type_complexity)]
    let plan_cache_lock: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    > = Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(backend::ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        task_repository: task_repository_lock.clone(),
        mcp_handle: mcp_handle_lock.clone(),
        agent_registry: agent_registry_lock.clone(),
        task_router: task_router_lock.clone(),
        mailbox_handle: mailbox_handle_lock.clone(),
        tools_config: tools_config_lock.clone(),
        plan_gates: plan_gates_lock.clone(),
        plan_cache: plan_cache_lock.clone(),
    });

    // Shared SttFlow state for push-to-talk IPC commands (`start_tour_recording`,
    // `stop_tour_recording`). Populated inside the Tauri setup closure once the
    // STT engine is confirmed available; remains `None` for graceful degradation.
    let stt_flow_state: commands::stt::SttFlowState = Arc::new(std::sync::Mutex::new(None));

    // Open AgentRepository for auto-load at boot (passed to Supervisor).
    // A separate instance is created later for Tauri IPC commands.
    let apollia_data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        std::path::PathBuf::from(home).join(".apollia")
    };
    let _ = std::fs::create_dir_all(&apollia_data_dir);
    let boot_agent_repo = {
        let db_path = apollia_data_dir.join("agents.db");
        match AgentRepository::open(&db_path) {
            Ok(repo) => {
                tracing::info!("AgentRepository opened for auto-load at boot");
                Some(repo)
            }
            Err(e) => {
                tracing::warn!(error = %e, "AgentRepository failed to open - auto-load disabled");
                None
            }
        }
    };

    // Extract bundled system agents (e.g. onboarding-agent) to disk and register
    // them in the repository so the auto-load loop picks them up.
    if let Some(ref repo) = boot_agent_repo {
        bundled_agents::ensure_bundled_agents(repo, &apollia_data_dir);
    }

    // Build the ChatAgentRunner so Chat Agent mode works in the ChatSessionManager.
    // Uses its own AgentRepository instance (SQLite WAL supports concurrent readers).
    let chat_agent_runner: Option<Arc<dyn apollia_runtime::chat::ChatAgentRunner>> = {
        let db_path = apollia_data_dir.join("agents.db");
        match apollia_tools::AgentRepository::open(&db_path) {
            Ok(repo) => Some(Arc::new(backend::ProductionChatAgentRunner {
                agent_repo: Arc::new(std::sync::Mutex::new(repo)),
                event_bus: event_bus_lock.clone(),
                llm_router: llm_router_lock.clone(),
                tool_registry: tool_registry_lock.clone(),
                audit_trail: audit_trail_lock.clone(),
                pending_approvals: pending_approvals_lock.clone(),
                task_repository: task_repository_lock.clone(),
                pending_user_inputs: pending_user_inputs_lock.clone(),
                mcp_handle: mcp_handle_lock.clone(),
                agent_registry: agent_registry_lock.clone(),
                task_router: task_router_lock.clone(),
                mailbox_handle: mailbox_handle_lock.clone(),
                user_memory: user_memory_lock.clone(),
                tools_config: tools_config_lock.clone(),
            })),
            Err(e) => {
                tracing::warn!(error = %e, "failed to open agents.db for ChatAgentRunner - Chat Agent mode disabled");
                None
            }
        }
    };

    // Load (or generate) the API bearer token so the embedded runtime binds an
    // authenticated TCP listener for the desktop REST bridge. The Unix socket
    // stays local-trust; the TCP port is never served unauthenticated.
    let api_token = apollia_runtime::api::load_or_generate_token(&apollia_data_dir)
        .expect("failed to load or generate API token");
    commands::set_api_token(api_token.clone());

    // Do NOT pass agent_repository here: auto-load inside the Supervisor
    // happens before OnceLocks are populated, causing "event bus not initialized"
    // errors. Instead, we auto-load manually after OnceLocks are set below.
    let config = load_toml_config(EmbeddedConfig {
        agent_loader: Arc::new(backend::AIPAgentLoader),
        backend_factory: Some(factory.clone()),
        agent_repository: None,
        chat_agent_runner,
        tcp_port: Some(7771),
        api_token: Some(api_token),
        ..EmbeddedConfig::default()
    });

    let runtime_handle: RuntimeHandle =
        apollia_runtime::init_embedded(config).expect("failed to start embedded runtime");

    // Pin the PyO3 async bridge onto the worker runtime now that it is up, before
    // any Python agent runs a coroutine.
    apollia_aip::pin_async_runtime();

    // Load the STT config from SQLite for the desktop hotkey listener.
    let stt_config_for_hotkey = load_stt_config(&apollia_data_dir);

    // Populate OnceLocks now that the supervisor is fully running.
    populate_runtime_locks(
        &runtime_handle,
        RuntimeLocks {
            event_bus: &event_bus_lock,
            llm_router: &llm_router_lock,
            tool_registry: &tool_registry_lock,
            audit_trail: &audit_trail_lock,
            pending_approvals: &pending_approvals_lock,
            task_repository: &task_repository_lock,
            pending_user_inputs: &pending_user_inputs_lock,
            mcp_handle: &mcp_handle_lock,
            agent_registry: &agent_registry_lock,
            task_router: &task_router_lock,
            mailbox_handle: &mailbox_handle_lock,
            user_memory: &user_memory_lock,
            tools_config: &tools_config_lock,
        },
    );
    if let Some(gates) = runtime_handle.plan_gates.clone() {
        let _ = plan_gates_lock.set(gates);
    }
    if let Some(cache) = runtime_handle.plan_cache.clone() {
        let _ = plan_cache_lock.set(cache);
    }

    // Auto-load installed agents now that the OnceLocks are populated so the
    // ProductionBackendFactory can create real backends.
    if let Some(repo) = boot_agent_repo {
        auto_load_installed_agents(repo, &factory, &runtime_handle);
    }

    // Open a second AgentRepository instance for Tauri IPC commands.
    // SQLite WAL mode supports concurrent readers.
    let agent_repo: Arc<std::sync::Mutex<AgentRepository>> = {
        let db_path = apollia_data_dir.join("agents.db");
        Arc::new(std::sync::Mutex::new(
            AgentRepository::open(&db_path).expect("failed to open agents.db for desktop app"),
        ))
    };

    // Open PackageRepository for Tauri IPC commands (same agents.db, WAL allows concurrent readers).
    let pkg_repo: Arc<std::sync::Mutex<apollia_tools::PackageRepository>> = {
        let db_path = apollia_data_dir.join("agents.db");
        Arc::new(std::sync::Mutex::new(
            apollia_tools::PackageRepository::open(&db_path)
                .expect("failed to open agents.db for PackageRepository"),
        ))
    };
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(backend::AIPAgentLoader);

    let mcp_registry_client = match McpRegistryClient::new(&apollia_data_dir) {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = %e, "McpRegistryClient failed to initialize - registry commands disabled");
            // Fall back to a client with a writable temp dir so Tauri state is always populated.
            McpRegistryClient::new(std::path::Path::new("/tmp"))
                .expect("fallback McpRegistryClient")
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // External URL opener: bridges the webview to the system browser for
        // OAuth flows and outbound help/docs links.
        // Tauri 2 disabled `window.open()` and `<a target="_blank">` by
        // default; this plugin (plus the `opener:default` capability permission)
        // is the canonical replacement.
        .plugin(tauri_plugin_opener::init())
        .manage(runtime_handle.clone())
        .manage(agent_repo)
        .manage(pkg_repo)
        .manage(agent_loader)
        .manage(runtime_handle.event_sender.clone())
        .manage(llm_router_lock.clone())
        .manage(mcp_registry_client)
        .manage(SecretStore::new())
        .manage(stt_flow_state.clone())
        .manage(std::sync::Arc::new(
            tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()),
        ) as commands::model_hub::SharedDownloadManager)
        .manage(std::sync::Arc::new(apollia_llm::HfModelTypeCache::new()))
        .manage(commands::llm::LlmPingCache::default())
        // Route the macOS app-menu / Cmd+Q "Quit" (id `MENU_QUIT_APP`) to the
        // shared graceful quit. Without this, the default native quit is
        // swallowed by the window `CloseRequested` handler below.
        .on_menu_event(|app, event| {
            if event.id().as_ref() == tray::MENU_QUIT_APP {
                tray::initiate_quit(app);
            }
        })
        .setup(move |app| {
            tray::setup_tray(app)?;

            // Replace the default macOS menu so Cmd+Q / the app-menu "Quit"
            // route through `initiate_quit` instead of the native `terminate:`
            // that `prevent_close` cancels.
            #[cfg(target_os = "macos")]
            tray::setup_app_menu(app)?;

            // bridge EventBus → Tauri events (replaces polling).
            events::spawn_event_bridge(app.handle().clone(), runtime_handle.event_sender.clone());

            // Periodic heartbeat so the frontend `runtimeHealth` watchdog
            // never trips during idle chat sessions.
            events::spawn_heartbeat(app.handle().clone());

            // Session metrics aggregator.
            // .setup() runs outside any Tokio context; enter the Tauri async
            // runtime before calling spawn_detached so tokio::spawn is valid.
            let metrics_store = {
                // .setup() runs outside any Tokio context; enter the Tauri
                // async runtime so tokio::spawn inside spawn_detached is valid.
                let rt_handle = tauri::async_runtime::handle();
                let _guard = match &rt_handle {
                    tauri::async_runtime::RuntimeHandle::Tokio(h) => Some(h.enter()),
                };
                apollia_runtime::session_metrics::spawn_detached(
                    runtime_handle.event_sender.subscribe(),
                    runtime_handle.event_sender.clone(),
                    apollia_core::session_metrics::SessionThresholds::default(),
                    0,
                    0,
                )
            };
            app.manage(metrics_store);

            // Register the global STT hotkey with the full SttFlow pipeline
            // when STT is enabled. The flow reads the shared engine cell on each
            // trigger, so it is armed even if no model is loaded yet (a
            // mid-session download + reload brings the engine online without
            // re-registering).
            if let Some(ref stt_cfg) = stt_config_for_hotkey {
                if stt_cfg.enabled {
                    setup_stt_hotkey(app.handle(), stt_cfg, &runtime_handle, &stt_flow_state);
                }
            }

            // What the window close button does is a platform convention, so
            // `tray::close_button_action` decides rather than this handler.
            //
            // On macOS it hides the window: the runtime keeps running, the tray
            // icon stays visible, and the user re-opens via the tray "open"
            // item. On Windows and Linux the close button means quit, and
            // hiding there left the runtime and its child processes alive with
            // no window to point at them.
            //
            // `prevent_close` on both branches. On the quit branch it stops the
            // window teardown from racing `initiate_quit`, which needs to reach
            // `POST /api/v1/shutdown` before `AppHandle::exit` runs.
            let main_window = app
                .get_webview_window("main")
                .expect("main window not found in tauri.conf.json");

            let window_for_close = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    match tray::close_button_action() {
                        tray::CloseAction::HideToTray => {
                            let _ = window_for_close.hide();
                        }
                        tray::CloseAction::Quit => {
                            tray::initiate_quit(window_for_close.app_handle());
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_packages::preview_agent_package,
            commands::agent_packages::install_agent_package,
            commands::agent_packages::list_agent_packages,
            commands::agent_packages::get_agent_package_detail,
            commands::agent_packages::uninstall_agent_package,
            commands::agents::list_agents,
            commands::agents::agent_status_snapshot,
            commands::agents::list_agent_messages,
            commands::agents::start_agent,
            commands::agents::stop_agent,
            commands::agents::install_agent,
            commands::agents::uninstall_agent,
            commands::agents::enable_agent,
            commands::agents::disable_agent,
            commands::agents::update_agent,
            commands::agents::create_agent_from_template,
            commands::agents::check_sdk_available,
            commands::agents::check_agent_name_available,
            commands::tasks::list_tasks,
            commands::tasks::submit_task,
            commands::tasks::delete_task,
            commands::tasks::get_task_timeline,
            commands::tasks::get_delegation_tree,
            commands::session::get_session_metrics,
            commands::session::list_session_metrics_ids,
            commands::session_meta::compute_session_meta,
            commands::session_meta::get_session_replay_events,
            commands::session_meta::get_session_replay_state,
            commands::session_meta::append_session_replay_event,
            commands::hitl::list_pending_approvals,
            commands::hitl::list_resolved_approvals,
            commands::hitl::resume_task,
            commands::hitl::add_permission_prefix_rule,
            commands::hitl::respond_hitl_filesystem,
            commands::integrations::oauth_start_flow,
            commands::integrations::oauth_complete_flow,
            commands::integrations::oauth_list_accounts,
            commands::integrations::oauth_disconnect,
            commands::integrations::oauth_get_status,
            commands::integrations::oauth_list_client_ids,
            commands::integrations::oauth_set_client_id,
            commands::integrations::oauth_set_client_secret,
            commands::integrations::oauth_import_client_json,
            commands::integrations::oauth_list_drive_folders,
            commands::integrations::oauth_set_drive_folder,
            commands::integrations::oauth_reset_drive_folder,
            commands::integrations::oauth_set_api_key,
            commands::integrations::oauth_google_picker_session,
            commands::integrations::oauth_list_picked_drive_folders,
            commands::integrations::oauth_add_picked_drive_folder,
            commands::integrations::oauth_remove_picked_drive_folder,
            commands::integrations::oauth_test_client,
            commands::llm::list_llm_backends,
            commands::llm::create_llm_backend,
            commands::llm::update_llm_backend,
            commands::llm::delete_llm_backend,
            commands::llm::set_default_llm_backend,
            commands::llm::ping_llm_backend,
            commands::llm::get_llm_cost_stats,
            commands::llm::get_cost_alert_threshold,
            commands::llm::reload_llm,
            commands::llm::reload_llm_from_db,
            commands::model_hub::get_hardware_profile,
            commands::model_hub::search_hf_models,
            commands::model_hub::get_hf_model,
            commands::model_hub::start_model_download,
            commands::model_hub::cancel_model_download,
            commands::model_hub::list_model_downloads,
            commands::model_hub::list_installed_models,
            commands::model_hub::delete_installed_model,
            commands::model_hub::import_model_file,
            commands::meta_automation::meta_parse_automation,
            commands::meta_digest::meta_generate_daily_digest,
            commands::meta_next_steps::meta_generate_next_steps,
            commands::mcp_coaching::meta_generate_capabilities_coaching,
            commands::triggers::list_triggers,
            commands::triggers::set_trigger_enabled,
            commands::triggers::fire_trigger,
            commands::triggers::get_trigger_logs,
            commands::triggers::reload_triggers,
            commands::triggers::create_trigger,
            commands::triggers::update_trigger,
            commands::triggers::delete_trigger,
            commands::triggers::get_trigger_definition,
            commands::memory::list_memory_namespaces,
            commands::memory::list_memory_entries,
            commands::memory::search_memory,
            commands::memory::delete_memory_entry,
            commands::memory::get_injected_memory_entries,
            commands::notifications::list_notification_channels,
            commands::notifications::test_notification_channel,
            commands::notifications::get_notification_logs,
            commands::notifications::list_runtime_activity,
            commands::notifications::create_notification_channel,
            commands::notifications::update_notification_channel,
            commands::notifications::delete_notification_channel,
            commands::notifications::get_notification_events,
            commands::notifications::set_notification_events,
            commands::observability::clear_plan_cache,
            commands::observability::get_global_timeline,
            commands::observability::get_llm_daily_costs,
            commands::observability::list_mailbox_messages,
            commands::observability::get_plan_cache_stats,
            commands::observability::get_tool_audit_trail,
            commands::observability::verify_audit_run,
            commands::observability::get_active_hooks,
            commands::trace::get_task_trace,
            commands::config::get_config,
            commands::updates::check_for_update,
            commands::updates::install_update,
            commands::config::get_observability_config,
            commands::config::set_observability_config,
            commands::config::open_config_in_editor,
            commands::config::reset_onboarding,
            commands::config::clear_all_memories,
            commands::config::clear_logs,
            commands::config::factory_reset,
            commands::config::app_restart,
            commands::config::quit_app,
            commands::config::check_onboarded,
            commands::config::mark_onboarded,
            commands::config::get_system_info,
            commands::config::get_security_posture,
            commands::config::check_python,
            commands::config::check_llm_configured,
            commands::config::check_hello_agent_exists,
            commands::config::list_available_agents,
            commands::config::setup_local_llm,
            commands::tools::list_tools,
            commands::tools::describe_tool,
            commands::tool_governance::governance_list_tools,
            commands::tool_governance::governance_set_tool_enabled,
            commands::tool_governance::governance_get_tool_config,
            commands::tool_governance::governance_set_tool_config,
            commands::tool_governance::governance_list_credentials,
            commands::tool_governance::governance_set_credential,
            commands::tool_governance::governance_delete_credential,
            commands::tool_governance::governance_test_credential,
            commands::tool_governance::governance_list_permission_rules,
            commands::tool_governance::governance_revoke_permission_rule,
            commands::tool_governance::governance_revoke_all_rules,
            commands::tool_governance::governance_list_audit,
            commands::chat::create_chat_session,
            commands::chat::list_chat_sessions,
            commands::chat::get_session_todo,
            commands::chat::get_chat_session,
            commands::chat::chat_session_metrics,
            commands::chat::close_chat_session,
            commands::chat::delete_chat_session,
            commands::chat::rename_chat_session,
            commands::chat::generate_chat_session_name,
            commands::chat::update_chat_session,
            commands::chat::send_chat_message,
            commands::chat::regenerate_chat_response,
            commands::chat::edit_and_resend_chat_message,
            commands::chat::pause_chat_session,
            commands::chat::resume_chat_session,
            commands::chat::authorize_chat_tool,
            commands::chat_libre::get_chat_libre_config,
            commands::chat_libre::update_chat_libre_config,
            commands::chat_libre::list_chat_permission_rules,
            commands::chat_libre::delete_chat_permission_rule,
            commands::chat_libre::list_active_chat_session_authorizations,
            commands::chat_libre::revoke_chat_session_authorization,
            commands::chat::respond_user_input,
            commands::chat::respond_user_input_rejected,
            commands::chat::list_pending_user_inputs,
            commands::chat::list_chat_approval_history,
            commands::chat::link_chat_to_project,
            commands::chat::list_chats_by_project,
            commands::chat::orphan_project_chats,
            commands::chat::list_a2a_skills,
            commands::chat::list_a2a_skill_telemetry,
            commands::chat::list_a2a_step_provenance,
            commands::chat::check_a2a_compatibility,
            commands::chat::export_conversation,
            commands::chat::reveal_session_path,
            commands::artifacts::save_artifact,
            commands::artifacts::list_artifacts,
            commands::artifacts::get_artifact,
            commands::artifacts::update_artifact,
            commands::artifacts::delete_artifact,
            commands::onboarding::get_onboarding_state,
            commands::onboarding::check_onboarding_finalized,
            commands::onboarding::finalize_onboarding_chat,
            commands::onboarding::advance_onboarding_phase,
            commands::onboarding::set_onboarding_profile,
            commands::onboarding::get_onboarding_status,
            commands::onboarding::trigger_onboarding,
            commands::onboarding::resume_onboarding,
            commands::onboarding::dismiss_onboarding,
            commands::permissions_proposals::list_proposed_permission_rules,
            commands::permissions_proposals::apply_proposed_permission_rule,
            commands::permissions_proposals::dismiss_proposed_permission_rule,
            commands::onboarding::get_ai_setup_info,
            commands::onboarding::scan_for_gguf_models,
            commands::onboarding::scan_for_whisper_models,
            commands::onboarding::setup_whisper_model,
            commands::onboarding::get_companion_context,
            commands::onboarding::create_companion_session,
            commands::onboarding::set_companion_enabled,
            commands::onboarding::get_companion_enabled,
            commands::user_memory::get_profile_schema,
            commands::user_memory::get_profile,
            commands::user_memory::set_profile_entry,
            commands::user_memory::delete_profile_entry,
            commands::user_memory::reset_user_profile,
            commands::user_memory::get_conversation_stats,
            commands::stt::get_stt_config,
            commands::stt::update_stt_config,
            commands::stt::list_audio_input_devices,
            commands::stt::reload_stt,
            commands::stt::get_stt_status,
            commands::stt::list_transcriptions,
            commands::stt::delete_transcription,
            commands::stt::transcribe_file,
            commands::stt::list_stt_models,
            commands::stt::start_tour_recording,
            commands::stt::stop_tour_recording,
            commands::mcp::list_mcp_enrichments,
            commands::mcp::list_mcp_servers,
            commands::mcp::list_mcp_resources,
            commands::mcp::get_mcp_server_detail,
            commands::mcp::add_mcp_server,
            commands::mcp::remove_mcp_server,
            commands::mcp::get_mcp_server_raw_config,
            commands::mcp::update_mcp_server_config,
            commands::mcp::test_mcp_connection,
            commands::mcp::test_mcp_live_server,
            commands::mcp::mcp_oauth_discover,
            commands::mcp::mcp_oauth_resolve_client_id,
            commands::mcp::mcp_oauth_store_client_id,
            commands::mcp::mcp_oauth_clear_client_id,
            commands::mcp::mcp_oauth_login,
            commands::mcp::restart_mcp_server,
            commands::mcp::fetch_mcp_registry,
            commands::mcp::fetch_mcp_curated,
            commands::mcp::refresh_mcp_server_detail,
            commands::mcp::store_mcp_secret,
            commands::mcp::delete_mcp_secret,
            commands::mcp::set_mcp_server_approval,
            commands::mcp::discover_mcp_servers,
            commands::mcp::list_mcp_tool_pending_approvals,
            commands::mcp::revoke_mcp_tool_approval,
            commands::plan_alternatives::choose_plan,
            commands::plan_mode::submit_plan_decision,
            commands::plan_mode::get_plan_mode_default,
            commands::plan_mode::set_plan_mode,
            commands::plan_mode::approve_plan,
            commands::plan_mode::reject_plan,
            commands::plan_mode::list_plan_mutations,
            commands::plan_mode::get_chat_plan,
            // Tasks
            commands::tasks::cancel_task,
            // Memory
            commands::memory::clear_memory,
            commands::memory::purge_memory,
            commands::memory::memory_export_namespace,
            commands::memory::memory_import_namespace,
            // Observability
            commands::observability::get_audit_stats,
            // Agents
            commands::agents::get_agent_detail,
            // Updates
            // Workspace
            commands::workspace::get_workspace_status,
            commands::workspace::init_workspace,
            // Projects
            commands::projects::suggest_workspace_path,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::delete_project,
            commands::projects::upload_project_document,
            commands::projects::delete_project_document,
            commands::projects::list_project_templates,
            commands::projects::get_project_snapshot,
            commands::projects::add_project_agent,
            commands::projects::remove_project_agent,
            commands::projects::list_project_agents,
            commands::projects::list_projects_for_agent,
            commands::projects::set_project_provider,
            commands::projects::remove_project_provider,
            commands::projects::toggle_project_provider,
            // Review
            commands::review::start_code_review,
            commands::link_preview::link_preview,
            // CLI install
            commands::cli::get_cli_status,
            commands::cli::install_cli,
            commands::cli::uninstall_cli,
            // Automation harness (dev-only, absent from release builds)
            #[cfg(debug_assertions)]
            commands::automation::automation_script,
            #[cfg(debug_assertions)]
            commands::automation::automation_capture,
            #[cfg(debug_assertions)]
            commands::automation::automation_finish,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Kill every child process before the process exits. Otherwise they
            // are orphaned: `kill_on_drop` never runs because `app.exit`
            // terminates the process before the managed `RuntimeHandle` (and
            // its child handles) is dropped. ExitRequested covers every quit
            // path: window close, Cmd+Q, and the tray "Quitter".
            //
            // This used to stop the runner alone, which left `llama-server`
            // holding its VRAM and its loopback port after the app had quit.
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                if let Some(rt) = app.try_state::<RuntimeHandle>() {
                    let rt = rt.inner().clone();
                    tauri::async_runtime::block_on(async move {
                        rt.stop_child_processes().await;
                    });
                }
            }
        });
}
