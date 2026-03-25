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
pub mod tray;

use std::sync::Arc;

use apollia_core::PendingApprovals;
use apollia_llm::LlmRouter;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use tauri::Manager;

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

fn main() {
    // Initialize tracing so Rust logs appear in the terminal during development.
    // RUST_LOG controls verbosity (e.g. RUST_LOG=apollia=debug); defaults to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("apollia=info,warn")),
        )
        .init();

    // OnceLocks shared between ProductionBackendFactory and main().
    // Populated after init_embedded() returns, before any HTTP request arrives.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::OnceLock::new());
    let tool_registry_lock: Arc<std::sync::OnceLock<ToolRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let audit_trail_lock: Arc<std::sync::OnceLock<AuditTrailHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_approvals_lock: Arc<std::sync::OnceLock<Arc<PendingApprovals>>> =
        Arc::new(std::sync::OnceLock::new());
    let task_repository_lock: Arc<std::sync::OnceLock<Arc<TaskRepository>>> =
        Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(backend::ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        task_repository: task_repository_lock.clone(),
    });

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

    // Do NOT pass agent_repository here — auto-load inside the Supervisor
    // happens before OnceLocks are populated, causing "event bus not initialized"
    // errors. Instead, we auto-load manually after OnceLocks are set below.
    let config = load_toml_config(EmbeddedConfig {
        agent_loader: Arc::new(backend::AIPAgentLoader),
        backend_factory: Some(factory.clone()),
        agent_repository: None,
        ..EmbeddedConfig::default()
    });

    let runtime_handle: RuntimeHandle =
        apollia_runtime::init_embedded(config).expect("failed to start embedded runtime");

    // Populate OnceLocks now that the supervisor is fully running.
    let _ = event_bus_lock.set(runtime_handle.event_sender.clone());
    let _ = llm_router_lock.set(runtime_handle.llm_router.clone());
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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(runtime_handle.clone())
        .manage(agent_repo)
        .manage(agent_loader)
        .manage(runtime_handle.event_sender.clone())
        .setup(move |app| {
            tray::setup_tray(app)?;

            // bridge EventBus → Tauri events (replaces polling).
            events::spawn_event_bridge(app.handle().clone(), runtime_handle.event_sender.clone());

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
            commands::llm::list_llm_backends,
            commands::llm::ping_llm_backend,
            commands::llm::get_llm_cost_stats,
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
            commands::tools::list_tools,
            commands::tools::describe_tool,
            commands::chat::create_chat_session,
            commands::chat::list_chat_sessions,
            commands::chat::get_chat_session,
            commands::chat::close_chat_session,
            commands::chat::update_chat_session,
            commands::chat::send_chat_message,
            commands::chat::authorize_chat_tool,
            commands::user::get_user_profile,
            commands::user::update_user_profile,
            commands::user::get_user_memory,
            commands::user::forget_user_memory,
            commands::onboarding::get_onboarding_status,
            commands::onboarding::trigger_onboarding,
            commands::onboarding::dismiss_onboarding,
            commands::user_memory::get_user_memory_profile,
            commands::user_memory::update_user_memory_entry,
            commands::user_memory::validate_user_memory,
            commands::user_memory::delete_user_memory_entry,
            commands::user_memory::search_user_memory,
            commands::user_memory::get_conversation_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
