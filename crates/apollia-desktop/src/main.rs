//! Apollia OS — Desktop application (Tauri v2).
//!
//! Single-process architecture: the Apollia runtime runs embedded inside the
//! Tauri process via [`apollia_runtime::init_embedded()`]. The Unix socket
//! remains active so the CLI can be used alongside the desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod bundled_agents;
mod commands;
mod events;
pub mod mcp;
mod project_context;
pub mod stt;
pub mod tray;

use std::sync::Arc;

use apollia_core::{PendingApprovals, SttConfigRepository};
use apollia_llm::LlmRouter;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use mcp::McpRegistryClient;
use mcp::SecretStore;
use tauri::Manager;

/// Shared mutable LLM router — updated by `reload_llm`, read by
/// `ProductionChatAgentRunner` and `ProductionBackendFactory`.
pub type SharedLlmRouter = Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>;

/// Resolves `~` prefix to `$HOME` in a path string.
fn expand_tilde(s: &str) -> std::path::PathBuf {
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(format!("{}{}", home, &s[1..]))
    } else {
        std::path::PathBuf::from(s)
    }
}

/// Searches for `apollia.toml` in priority order and applies all parsable sections
/// (llm, triggers, notifications) to the provided `EmbeddedConfig`.
///
/// Search order (first match wins):
///   1. `~/.apollia/apollia.toml`        — standard user config
///   2. `./apollia.toml`                 — CWD (useful when running from workspace root)
///   3. `~/.config/apollia/apollia.toml` — XDG fallback
///
/// Returns the config unchanged if no file is found.
fn load_toml_config(config: EmbeddedConfig) -> EmbeddedConfig {
    let candidates = [
        Some(expand_tilde("~/.apollia/apollia.toml")),
        std::env::current_dir().ok().map(|d| d.join("apollia.toml")),
        Some(expand_tilde("~/.config/apollia/apollia.toml")),
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
/// - If not found, logs a warning and leaves env vars alone (dev mode — the
///   developer's Homebrew/pyenv Python takes over, same as before).
fn setup_bundled_python() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "current_exe() failed — bundled Python skipped");
            return;
        }
    };
    let exe_dir = match exe.parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    // Candidate search order — first match wins.
    let candidates: [std::path::PathBuf; 3] = [
        // macOS: Contents/MacOS/apollia-desktop → Contents/Resources/python/
        exe_dir.join("../Resources/python"),
        // Linux AppImage / .deb: usr/bin/apollia-desktop → usr/lib/apollia-os/python/
        exe_dir.join("../lib/apollia-os/python"),
        // Dev build fallback: target/release/apollia-desktop → target/python-bundle/<triple>/python/
        // (populated by packaging/build-python-bundle.sh during dev)
        exe_dir.join("../../resources/python"),
    ];

    let python_root = match candidates.iter().find(|p| p.join("bin/python3.13").exists()) {
        Some(p) => match p.canonicalize() {
            Ok(abs) => abs,
            Err(e) => {
                tracing::warn!(path = %p.display(), error = %e, "canonicalize() failed on bundled Python");
                return;
            }
        },
        None => {
            tracing::warn!(
                "no bundled Python found adjacent to {} — falling back to system Python",
                exe.display()
            );
            return;
        }
    };

    let site_packages = python_root.join("lib/python3.13/site-packages");
    std::env::set_var("PYTHONHOME", &python_root);
    std::env::set_var("PYTHONPATH", &site_packages);

    // Help dyld find libpython when the Python interpreter is invoked as a
    // subprocess (e.g. by the PythonExecutor tool or `python3 -m venv`).
    #[cfg(target_os = "macos")]
    std::env::set_var("DYLD_FALLBACK_LIBRARY_PATH", python_root.join("lib"));
    #[cfg(target_os = "linux")]
    std::env::set_var("LD_LIBRARY_PATH", python_root.join("lib"));

    tracing::info!(
        python_root = %python_root.display(),
        "bundled Python configured — PYTHONHOME/PYTHONPATH exported"
    );
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

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(backend::ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        task_repository: task_repository_lock.clone(),
    });

    // Shared SttFlow state for push-to-talk IPC commands (`start_tour_recording`,
    // `stop_tour_recording`). Populated inside the Tauri setup closure once the
    // STT engine is confirmed available; remains `None` for graceful degradation.
    let stt_flow_state: commands::stt::SttFlowState = Arc::new(std::sync::Mutex::new(None));

    // Open AgentRepository for auto-load at boot (passed to Supervisor).
    // A separate instance is created later for Tauri IPC commands.
    let apollia_data_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
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
                tracing::warn!(error = %e, "AgentRepository failed to open — auto-load disabled");
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
            })),
            Err(e) => {
                tracing::warn!(error = %e, "failed to open agents.db for ChatAgentRunner — Chat Agent mode disabled");
                None
            }
        }
    };

    // Do NOT pass agent_repository here — auto-load inside the Supervisor
    // happens before OnceLocks are populated, causing "event bus not initialized"
    // errors. Instead, we auto-load manually after OnceLocks are set below.
    let config = load_toml_config(EmbeddedConfig {
        agent_loader: Arc::new(backend::AIPAgentLoader),
        backend_factory: Some(factory.clone()),
        agent_repository: None,
        chat_agent_runner,
        ..EmbeddedConfig::default()
    });

    let runtime_handle: RuntimeHandle =
        apollia_runtime::init_embedded(config).expect("failed to start embedded runtime");

    // Load the STT config from SQLite for the desktop hotkey listener.
    let stt_config_for_hotkey = {
        let system_db = apollia_data_dir.join("system.db");
        match SttConfigRepository::open(&system_db) {
            Ok(repo) => match repo.get_or_default() {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read STT config from SQLite — hotkey disabled");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "failed to open system.db for STT config — hotkey disabled");
                None
            }
        }
    };

    // Populate OnceLocks now that the supervisor is fully running.
    let _ = event_bus_lock.set(runtime_handle.event_sender.clone());
    *llm_router_lock.write().expect("llm_router_lock poisoned") = runtime_handle.llm_router.clone();
    let _ = tool_registry_lock.set(runtime_handle.tool_registry_handle.clone());
    if let Some(audit) = runtime_handle.audit_trail.clone() {
        let _ = audit_trail_lock.set(audit);
    }
    if let Some(pa) = runtime_handle.pending_approvals.clone() {
        let _ = pending_approvals_lock.set(pa);
    }
    if let Some(repo) = runtime_handle.task_repository.clone() {
        let _ = task_repository_lock.set(repo);
    }
    if let Some(chat) = runtime_handle.chat_manager.as_ref() {
        let _ = pending_user_inputs_lock.set(chat.pending_user_inputs());
    }

    // Auto-load installed agents NOW — OnceLocks are populated so the
    // ProductionBackendFactory can create real backends.
    if let Some(repo) = boot_agent_repo {
        let agent_loader_for_boot: Arc<dyn AgentLoader> = Arc::new(backend::AIPAgentLoader);
        match repo.list_enabled() {
            Ok(agents) => {
                for agent in &agents {
                    if !agent.enabled {
                        continue;
                    }
                    let manifest = match agent_loader_for_boot
                        .load_and_validate(&agent.install_path)
                    {
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

                    let max_concurrent = manifest.max_concurrent_tasks;
                    let agent_name = manifest.name.clone();

                    // Register in AgentRegistry.
                    let rt = tokio::runtime::Handle::try_current()
                        .or_else(|_| {
                            // We're on the main thread (not inside Tokio) — use the
                            // runtime handle from the embedded runtime thread.
                            // Since init_embedded() is blocking, we need a small
                            // runtime to send async messages.
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

                    rt.block_on(async {
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

                        let _ = router_handle.register_coordinator(agent_id.clone(), coordinator).await;
                        tracing::info!(name = %agent_name, id = %agent_id, "Auto-loaded installed agent (post-init)");
                    });
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list installed agents — skipping auto-load");
            }
        }
    }

    // Open a second AgentRepository instance for Tauri IPC commands.
    // SQLite WAL mode supports concurrent readers.
    let agent_repo: Arc<std::sync::Mutex<AgentRepository>> = {
        let db_path = apollia_data_dir.join("agents.db");
        Arc::new(std::sync::Mutex::new(
            AgentRepository::open(&db_path).expect("failed to open agents.db for desktop app"),
        ))
    };
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(backend::AIPAgentLoader);

    let mcp_registry_client = match McpRegistryClient::new(&apollia_data_dir) {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = %e, "McpRegistryClient failed to initialize — registry commands disabled");
            // Fall back to a client with a writable temp dir so Tauri state is always populated.
            McpRegistryClient::new(std::path::Path::new("/tmp"))
                .expect("fallback McpRegistryClient")
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(runtime_handle.clone())
        .manage(agent_repo)
        .manage(agent_loader)
        .manage(runtime_handle.event_sender.clone())
        .manage(llm_router_lock.clone())
        .manage(mcp_registry_client)
        .manage(SecretStore::new())
        .manage(stt_flow_state.clone())
        .setup(move |app| {
            tray::setup_tray(app)?;

            // bridge EventBus → Tauri events (replaces polling).
            events::spawn_event_bridge(app.handle().clone(), runtime_handle.event_sender.clone());

            // Register the global STT hotkey with the full SttFlow pipeline
            // when STT is enabled and the engine loaded successfully.
            if let Some(ref stt_cfg) = stt_config_for_hotkey {
                if stt_cfg.enabled {
                    if let Some(ref stt_engine) = runtime_handle.stt_engine {
                        let flow = Arc::new(stt::flow::SttFlow::new(
                            stt_cfg.clone(),
                            stt_engine.clone(),
                            runtime_handle.event_sender.clone(),
                            app.handle().clone(),
                        ));

                        let mode =
                            stt::hotkey::TriggerMode::from_config(&stt_cfg.trigger_mode);
                        let listener =
                            stt::hotkey::HotkeyListener::new(stt_cfg.hotkey.clone(), mode);

                        // Make the flow accessible to push-to-talk IPC commands.
                        if let Ok(mut guard) = stt_flow_state.lock() {
                            *guard = Some(Arc::clone(&flow));
                        }

                        let flow_start = Arc::clone(&flow);
                        let flow_stop = Arc::clone(&flow);

                        if let Err(e) = listener.register(
                            app.handle(),
                            move || {
                                flow_start.start_recording();
                            },
                            move || {
                                let flow = Arc::clone(&flow_stop);
                                tauri::async_runtime::spawn(async move {
                                    flow.stop_and_transcribe().await;
                                });
                            },
                        ) {
                            tracing::warn!(error = %e, "STT hotkey registration failed — recording via hotkey disabled");
                        }

                        // Recording overlay: secondary always-on-top window that
                        // shows a visual indicator while audio capture is active.
                        // Escape cancels (discards) the current recording.
                        let flow_cancel = Arc::clone(&flow);
                        let on_cancel = Arc::new(move || {
                            flow_cancel.cancel_recording();
                        });
                        match stt::overlay::RecordingOverlay::create(
                            app.handle(),
                            stt_cfg.hotkey.clone(),
                            on_cancel,
                        ) {
                            Ok(overlay) => {
                                stt::overlay::spawn_overlay_listener(
                                    overlay,
                                    &runtime_handle.event_sender,
                                );
                                tracing::info!("recording overlay window created");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "failed to create recording overlay — visual indicator disabled");
                            }
                        }
                    } else {
                        tracing::warn!("STT enabled in config but engine not loaded — hotkey disabled");
                    }
                }
            }

            // Closing the window hides it instead of quitting.
            // The runtime keeps running in the background and the tray icon
            // remains visible. The user re-opens via tray menu "Ouvrir Apollia OS"
            // or quits via "Quitter" which triggers graceful shutdown.
            let main_window = app
                .get_webview_window("main")
                .expect("main window not found in tauri.conf.json");

            let window_for_hide = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_for_hide.hide();
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agents::list_agents,
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
            commands::tasks::get_task_timeline,
            commands::hitl::list_pending_approvals,
            commands::hitl::list_resolved_approvals,
            commands::hitl::resume_task,
            commands::hitl::add_permission_prefix_rule,
            commands::hitl::respond_hitl_filesystem,
            commands::llm::list_llm_backends,
            commands::llm::create_llm_backend,
            commands::llm::update_llm_backend,
            commands::llm::delete_llm_backend,
            commands::llm::set_default_llm_backend,
            commands::llm::ping_llm_backend,
            commands::llm::get_llm_cost_stats,
            commands::llm::get_cost_alert_threshold,
            commands::llm::reload_llm,
            commands::triggers::list_triggers,
            commands::triggers::set_trigger_enabled,
            commands::triggers::fire_trigger,
            commands::triggers::get_trigger_logs,
            commands::triggers::reload_triggers,
            commands::triggers::create_trigger,
            commands::triggers::update_trigger,
            commands::triggers::delete_trigger,
            commands::triggers::get_trigger_definition,
            commands::pipelines::list_pipelines,
            commands::pipelines::list_pipeline_runs,
            commands::pipelines::list_all_pipeline_runs,
            commands::pipelines::get_pipeline_run_detail,
            commands::pipelines::run_pipeline,
            commands::pipelines::create_pipeline,
            commands::pipelines::update_pipeline,
            commands::pipelines::delete_pipeline,
            commands::pipelines::list_pipeline_definitions,
            commands::pipelines::get_pipeline_definition,
            commands::memory::list_memory_namespaces,
            commands::memory::list_memory_entries,
            commands::memory::search_memory,
            commands::memory::delete_memory_entry,
            commands::notifications::list_notification_channels,
            commands::notifications::test_notification_channel,
            commands::notifications::get_notification_logs,
            commands::notifications::create_notification_channel,
            commands::notifications::update_notification_channel,
            commands::notifications::delete_notification_channel,
            commands::notifications::get_notification_events,
            commands::notifications::set_notification_events,
            commands::observability::clear_plan_cache,
            commands::observability::get_global_timeline,
            commands::observability::get_llm_daily_costs,
            commands::observability::get_plan_cache_stats,
            commands::observability::get_tool_audit_trail,
            commands::config::get_config,
            commands::config::open_config_in_editor,
            commands::config::reset_onboarding,
            commands::config::check_onboarded,
            commands::config::mark_onboarded,
            commands::config::get_system_info,
            commands::config::check_python,
            commands::config::check_llm_configured,
            commands::config::check_hello_agent_exists,
            commands::config::list_available_agents,
            commands::config::setup_local_llm,
            commands::tools::list_tools,
            commands::tools::describe_tool,
            commands::chat::create_chat_session,
            commands::chat::list_chat_sessions,
            commands::chat::get_chat_session,
            commands::chat::close_chat_session,
            commands::chat::delete_chat_session,
            commands::chat::rename_chat_session,
            commands::chat::update_chat_session,
            commands::chat::send_chat_message,
            commands::chat::authorize_chat_tool,
            commands::chat::respond_user_input,
            commands::chat::list_chat_approval_history,
            commands::chat::link_chat_to_project,
            commands::chat::list_chats_by_project,
            commands::chat::orphan_project_chats,
            commands::chat::list_a2a_skills,
            commands::user::get_user_profile,
            commands::user::update_user_profile,
            commands::user::get_user_memory,
            commands::user::forget_user_memory,
            commands::onboarding::get_onboarding_state,
            commands::onboarding::advance_onboarding_phase,
            commands::onboarding::set_onboarding_profile,
            commands::onboarding::get_onboarding_status,
            commands::onboarding::trigger_onboarding,
            commands::onboarding::dismiss_onboarding,
            commands::onboarding::get_ai_setup_info,
            commands::onboarding::scan_for_gguf_models,
            commands::onboarding::scan_for_whisper_models,
            commands::onboarding::setup_whisper_model,
            commands::onboarding::get_companion_context,
            commands::onboarding::create_companion_session,
            commands::onboarding::set_companion_enabled,
            commands::onboarding::get_tour_steps,
            commands::onboarding::complete_tour_step,
            commands::user_memory::get_user_memory_profile,
            commands::user_memory::update_user_memory_entry,
            commands::user_memory::validate_user_memory,
            commands::user_memory::delete_user_memory_entry,
            commands::user_memory::search_user_memory,
            commands::user_memory::get_conversation_stats,
            commands::stt::get_stt_config,
            commands::stt::update_stt_config,
            commands::stt::get_stt_status,
            commands::stt::list_transcriptions,
            commands::stt::delete_transcription,
            commands::stt::transcribe_file,
            commands::stt::list_stt_models,
            commands::stt::start_tour_recording,
            commands::stt::stop_tour_recording,
            commands::onboarding::process_tour_voice_command,
            commands::mcp::list_mcp_enrichments,
            commands::mcp::list_mcp_servers,
            commands::mcp::get_mcp_server_detail,
            commands::mcp::add_mcp_server,
            commands::mcp::remove_mcp_server,
            commands::mcp::test_mcp_connection,
            commands::mcp::restart_mcp_server,
            commands::mcp::fetch_mcp_registry,
            commands::mcp::store_mcp_secret,
            commands::mcp::delete_mcp_secret,
            commands::mcp::set_mcp_server_approval,
            commands::mcp::discover_mcp_servers,
            commands::mcp::list_mcp_tool_pending_approvals,
            commands::mcp::revoke_mcp_tool_approval,
            commands::plan_alternatives::choose_plan,
            // Tasks
            commands::tasks::cancel_task,
            // Memory
            commands::memory::clear_memory,
            commands::memory::purge_memory,
            // Observability
            commands::observability::get_audit_stats,
            // Agents
            commands::agents::get_agent_detail,
            // Updates
            commands::updates::check_for_update,
            commands::updates::install_update,
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
            // CLI install
            commands::cli::get_cli_status,
            commands::cli::install_cli,
            commands::cli::uninstall_cli,
            // Pipelines (registry)
            commands::pipelines::install_pipeline,
            commands::pipelines::list_pipeline_registry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
