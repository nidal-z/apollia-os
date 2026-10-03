//! The AI setup step of onboarding: what the machine can run, which GGUF and
//! Whisper models it already has on disk, and the Whisper model chosen for
//! dictation.
//!
//! Embedded, each command calls [`apollia_runtime::local_ai`] in this process.
//! Attached, it reaches the same functions through `/api/v1/local-ai/...` on
//! the daemon, which runs the models and owns the folders they are read from.

use std::time::Duration;

use apollia_runtime::api::routes_local_ai::SetupDictationBody;
use apollia_runtime::local_ai::{
    self, DictationConfigured, GgufModelInfo, SystemInfo, WhisperModelInfo,
};
use apollia_runtime::profile::ProfileStore;
use tauri::{Manager, State};

use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

/// The daemon route of the system information.
const SYSTEM_ROUTE: &str = "/api/v1/local-ai/system";
/// The daemon route of the GGUF scan.
const GGUF_ROUTE: &str = "/api/v1/local-ai/models/gguf";
/// The daemon route of the Whisper scan.
const WHISPER_ROUTE: &str = "/api/v1/local-ai/models/whisper";
/// The daemon route of the dictation setup.
const DICTATION_ROUTE: &str = "/api/v1/local-ai/dictation";
/// The daemon route that loads the stored dictation model.
const STT_RELOAD_ROUTE: &str = "/api/v1/stt/reload";

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

/// Lists the Whisper models the daemon finds on its machine.
pub(crate) async fn whisper_models_on(
    remote: &AttachedClient,
) -> Result<Vec<WhisperModelInfo>, AttachedError> {
    remote.get_within(WHISPER_ROUTE, SCAN_DEADLINE).await
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

/// Scans standard filesystem locations for Whisper GGML model files.
///
/// Locations scanned: the models folder of the data directory,
/// `~/Downloads/` and `~/.cache/whisper/`. Only `ggml-*.bin` files are
/// returned; an empty list is not an error.
#[tauri::command]
pub async fn scan_for_whisper_models(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<WhisperModelInfo>, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            let (home, data_dir) = local_roots();
            local_ai::whisper_models(home, data_dir)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(whisper_models_on(remote).await?),
    }
}

/// Stores the chosen Whisper model on the daemon, then asks it to load it.
///
/// A failed load is logged, not returned: the model is configured and the
/// engine still runs the previous one, as in the embedded mode.
pub(crate) async fn setup_dictation_on(
    remote: &AttachedClient,
    model_path: String,
    language: Option<String>,
) -> Result<DictationConfigured, AttachedError> {
    let body = SetupDictationBody {
        model_path,
        language,
    };
    let done: DictationConfigured = remote.post(DICTATION_ROUTE, &body).await?;
    if let Err(e) = remote
        .post::<serde_json::Value>(STT_RELOAD_ROUTE, &serde_json::json!({}))
        .await
    {
        tracing::event!(
            tracing::Level::WARN,
            error = %e,
            detail = "the model is configured but the engine still runs the previous one",
            "stt.engine.reload.failed"
        );
    }
    Ok(done)
}

/// Configures the Whisper STT backend with the selected model and marks
/// the onboarding state as STT-configured.
///
/// Persists `enabled = true` and the provided `model_path` to `system.db`,
/// then sets `stt_configured` and `voice_enabled` in the onboarding state, and
/// loads the model so onboarding "Tester" and the dictation hotkey work
/// immediately, without restarting the app.
#[tauri::command]
pub async fn setup_whisper_model(
    model_path: String,
    language: Option<String>,
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
) -> Result<DictationConfigured, String> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(setup_dictation_on(remote, model_path, language).await?);
        }
    };
    let data_dir = apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp());
    let done = local_ai::setup_dictation(
        &data_dir,
        runtime.user_memory.as_ref().map(ProfileStore),
        model_path,
        language,
    )
    .await
    .map_err(|e| {
        tracing::event!(tracing::Level::ERROR, error = %e, "stt.model.setup.failed");
        e.to_string()
    })?;

    if let Some(flow) = app.try_state::<crate::commands::stt::SttFlowState>() {
        if let Err(e) = crate::commands::stt::reload_stt_inner(runtime, &app, &flow).await {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                detail = "the model is configured but the engine still runs the previous one",
                "stt.engine.reload.failed"
            );
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests;
