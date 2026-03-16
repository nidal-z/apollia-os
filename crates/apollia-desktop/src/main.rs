//! Apollia OS — Desktop application (Tauri v2).
//!
//! Single-process architecture: the Apollia runtime runs embedded inside the
//! Tauri process via [`apollia_runtime::init_embedded()`]. The Unix socket
//! remains active so the CLI can be used alongside the desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod commands;
mod events;
pub mod tray;

use std::sync::Arc;

use apollia_core::PendingApprovals;
use apollia_llm::LlmRouter;
use apollia_runtime::api::routes_agents::AgentBackendFactory;
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
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

    let config = load_toml_config(EmbeddedConfig {
        agent_loader: Arc::new(backend::AIPAgentLoader),
        backend_factory: Some(factory),
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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(runtime_handle.clone())
        .setup(move |app| {
            tray::setup_tray(app)?;

            // STORY-156: bridge EventBus → Tauri events (replaces polling).
            events::spawn_event_bridge(app.handle().clone(), runtime_handle.event_sender.clone());

            // AC-3: Closing the window hides it instead of quitting.
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
            commands::agents::start_agent,
            commands::agents::stop_agent,
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
            commands::pipelines::list_pipelines,
            commands::pipelines::list_pipeline_runs,
            commands::pipelines::list_all_pipeline_runs,
            commands::pipelines::get_pipeline_run_detail,
            commands::pipelines::run_pipeline,
            commands::memory::list_memory_namespaces,
            commands::memory::list_memory_entries,
            commands::memory::search_memory,
            commands::memory::delete_memory_entry,
            commands::notifications::list_notification_channels,
            commands::notifications::test_notification_channel,
            commands::notifications::get_notification_logs,
            commands::observability::get_global_timeline,
            commands::observability::get_tool_audit_trail,
            commands::observability::get_llm_daily_costs,
            commands::config::get_config,
            commands::config::open_config_in_editor,
            commands::config::reset_onboarding,
            commands::config::check_onboarded,
            commands::config::mark_onboarded,
            commands::config::check_python,
            commands::config::check_llm_configured,
            commands::config::check_hello_agent_exists,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
