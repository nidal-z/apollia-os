//! Apollia OS — Desktop application (Tauri v2).
//!
//! Single-process architecture: the Apollia runtime runs embedded inside the
//! Tauri process via [`apollia_runtime::init_embedded()`]. The Unix socket
//! remains active so the CLI can be used alongside the desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};

fn main() {
    let config = EmbeddedConfig::default();

    let runtime_handle: RuntimeHandle =
        apollia_runtime::init_embedded(config).expect("failed to start embedded runtime");

    tauri::Builder::default()
        .manage(runtime_handle)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
