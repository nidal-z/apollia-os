//! Apollia OS — Desktop application (Tauri v2).
//!
//! Single-process architecture: the Apollia runtime runs embedded inside the
//! Tauri process via [`apollia_runtime::init_embedded()`]. The Unix socket
//! remains active so the CLI can be used alongside the desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};

fn main() {
    let config = EmbeddedConfig::default();

    let runtime_handle: RuntimeHandle =
        apollia_runtime::init_embedded(config).expect("failed to start embedded runtime");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(runtime_handle)
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
