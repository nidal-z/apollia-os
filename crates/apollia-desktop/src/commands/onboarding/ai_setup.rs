//! The AI setup step of onboarding: what the machine can run, and which GGUF
//! models it already has on disk. Dictation is set up in the settings, not
//! here.
//!
//! Embedded, each command calls [`apollia_runtime::local_ai`] in this process.
//! Attached, it reaches the same functions through `/api/v1/local-ai/...` on
//! the daemon, which runs the models and owns the folders they are read from.

use std::time::Duration;

use apollia_runtime::local_ai::{self, GgufModelInfo, SystemInfo};
use tauri::State;

use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

/// The daemon route of the system information.
const SYSTEM_ROUTE: &str = "/api/v1/local-ai/system";
/// The daemon route of the GGUF scan.
const GGUF_ROUTE: &str = "/api/v1/local-ai/models/gguf";

/// How long the daemon may take to scan its folders: each GGUF header is
/// probed, and a large LM Studio library holds dozens of files.
const SCAN_DEADLINE: Duration = Duration::from_secs(120);

/// The home folder and the data directory of this machine.
fn local_roots() -> (std::path::PathBuf, std::path::PathBuf) {
    let home = apollia_core::paths::home_dir().unwrap_or_default();
    let data_dir = apollia_core::paths::data_dir_under(&home);
    (home, data_dir)
}

/// Reads what the daemon's machine can run.
pub(crate) async fn system_info_on(remote: &AttachedClient) -> Result<SystemInfo, AttachedError> {
    remote.get(SYSTEM_ROUTE).await
}

/// Lists the GGUF models the daemon finds on its machine.
pub(crate) async fn gguf_models_on(
    remote: &AttachedClient,
) -> Result<Vec<GgufModelInfo>, AttachedError> {
    remote.get_within(GGUF_ROUTE, SCAN_DEADLINE).await
}

/// Returns system information for AI setup model recommendations.
///
/// Queries total and available RAM, OS, architecture, and basic GPU
/// availability, on the machine that runs the models.
#[tauri::command]
pub async fn get_ai_setup_info(link: State<'_, RuntimeLink>) -> Result<SystemInfo, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => local_ai::system_info().await.map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(system_info_on(remote).await?),
    }
}

/// Scans standard filesystem locations for `.gguf` model files.
///
/// Locations scanned: the models folder of the data directory and
/// `~/Downloads/` (flat), `~/.cache/lm-studio/models/` (up to 4 levels deep).
/// Results are sorted by file size descending; an empty list is not an error.
#[tauri::command]
pub async fn scan_for_gguf_models(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<GgufModelInfo>, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            let (home, data_dir) = local_roots();
            local_ai::gguf_models(home, data_dir)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(gguf_models_on(remote).await?),
    }
}

#[cfg(test)]
mod tests;
