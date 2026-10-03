//! Tauri IPC commands for STT (Speech-to-Text) functionality.
//!
//! Exposes 10 commands to the Svelte frontend:
//! - `get_stt_config`         : read STT config from system.db
//! - `update_stt_config`      : write STT config to system.db (hot-reloads engine)
//! - `reload_stt`             : rebuild the engine from system.db without a restart
//! - `get_stt_status`         : query runtime engine status
//! - `list_transcriptions`    : list transcription history
//! - `delete_transcription`   : delete a transcription by ID
//! - `transcribe_file`        : transcribe a WAV file
//! - `list_stt_models`        : list available whisper model files
//! - `start_tour_recording`   : begin push-to-talk recording for the guided tour
//! - `stop_tour_recording`    : stop recording and trigger transcription

use std::io::Cursor;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::stt::TranscriptSource;
use apollia_stt::{SttRepository, TranscriptRow};

use crate::attached::RuntimeLink;

mod daemon;
mod transcriptions;

/// Description of an available STT model file on disk.
#[derive(Debug, Clone, Serialize)]
pub struct SttModelInfo {
    /// Model filename (e.g. `"whisper-large-v3-fr-q5_0.bin"`).
    pub name: String,
    /// Full filesystem path.
    pub path: String,
    /// File size in megabytes.
    pub size_mb: f64,
    /// Language hint from the filename, if detectable.
    pub language: Option<String>,
}

/// STT configuration read from (and written to) the singleton row of
/// `~/.apollia/system.db`.
///
/// There is no `[stt]` section in `apollia.toml`: nothing reads one. Mirror of
/// [`apollia_core::SttConfigRow`] with all paths as plain `String` so the Tauri
/// JSON bridge can serialise and deserialise without `PathBuf`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfigView {
    /// Whether the STT engine is enabled at startup.
    pub enabled: bool,
    /// Path to the GGML model file (may use `~` prefix).
    pub model_path: String,
    /// Global hotkey to trigger recording.
    pub hotkey: String,
    /// Text injection mode: `"paste"` or `"clipboard"`.
    pub clipboard_mode: String,
    /// Whether to restore clipboard content after injection.
    pub clipboard_restore: bool,
    /// RMS silence threshold in dB (negative, e.g. `-40.0`).
    pub silence_threshold_db: f32,
    /// Maximum recording duration in seconds.
    pub max_recording_sec: u32,
    /// ISO 639-1 language hint, or `null` for auto-detect.
    pub language: Option<String>,
    /// Recording trigger mode: `"toggle"` or `"push-to-talk"`.
    pub trigger_mode: String,
    /// Audio input device name, or `null` for the system default microphone.
    #[serde(default)]
    pub input_device: Option<String>,
}

impl SttModelInfo {
    /// The description of a model file, its language read from its name.
    pub(crate) fn from_file(name: String, path: String, size_bytes: u64) -> Self {
        let language = detect_language_from_name(&name);
        Self {
            name,
            path,
            size_mb: size_bytes as f64 / (1024.0 * 1024.0),
            language,
        }
    }
}

impl Default for SttConfigView {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: "~/.apollia/models/ggml-model-q5_0.bin".to_owned(),
            hotkey: "ctrl+shift+space".to_owned(),
            clipboard_mode: "paste".to_owned(),
            clipboard_restore: true,
            silence_threshold_db: -40.0,
            max_recording_sec: 60,
            language: Some("fr".to_owned()),
            trigger_mode: "toggle".to_owned(),
            input_device: None,
        }
    }
}

/// Returns the current STT configuration from `~/.apollia/system.db`.
///
/// Opens the repository, reads the singleton row, and returns it as a view.
/// If the row does not exist yet, defaults are inserted and returned. Attached,
/// the daemon reads the same row through `GET /api/v1/stt/config`.
#[tauri::command]
pub async fn get_stt_config(link: State<'_, RuntimeLink>) -> Result<SttConfigView, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => read_local_config().await,
        RuntimeLink::Attached(remote) => Ok(daemon::config_on(remote).await?),
    }
}

/// The configuration row of this machine's `system.db`, as a view.
async fn read_local_config() -> Result<SttConfigView, String> {
    let db_path = apollia_core::paths::DataFile::System
        .path(&apollia_core::paths::data_dir().unwrap_or_default());
    let row = tokio::task::spawn_blocking(move || {
        let repo = apollia_core::SttConfigRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        repo.get_or_default()
            .map_err(|e| format!("failed to read STT config: {e}"))
    })
    .await
    .map_err(|e| format!("task error: {e}"))??;

    Ok(SttConfigView {
        enabled: row.enabled,
        model_path: row.model_path,
        hotkey: row.hotkey,
        clipboard_mode: row.clipboard_mode,
        clipboard_restore: row.clipboard_restore,
        silence_threshold_db: row.silence_threshold_db,
        max_recording_sec: row.max_recording_sec,
        language: row.language,
        trigger_mode: row.trigger_mode,
        input_device: row.input_device,
    })
}

/// Lists the names of available audio input devices (microphones).
///
/// Returns an empty vector when the host exposes no microphone, which the UI
/// uses to show a "no microphone detected" warning. The first UI choice is
/// always the system default (represented as `null` / no selection), so this
/// list contains only the explicitly named devices.
#[tauri::command]
pub async fn list_audio_input_devices() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(apollia_stt::list_input_devices)
        .await
        .map_err(|e| format!("task error: {e}"))?
        .map_err(|e| format!("failed to enumerate input devices: {e}"))
}

/// Persists the STT configuration to `~/.apollia/system.db`, then hot-reloads
/// the engine so the change takes effect without a restart.
///
/// Upserts the singleton row, then rebuilds the engine from the new config:
/// enabling (with a model present) loads the model and arms the hotkey;
/// disabling tears the engine down. A reload failure is logged, not fatal.
/// Attached, the daemon persists the row and rebuilds its own engine.
#[tauri::command]
pub async fn update_stt_config(
    config: SttConfigView,
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    apollia_runtime::stt::validate_language(config.language.as_deref())
        .map_err(|e| e.to_string())?;
    match link.inner() {
        RuntimeLink::Embedded(runtime) => {
            let stt_flow_state = flow_state(&app)?;
            update_local_config(config, runtime, &app, &stt_flow_state).await
        }
        RuntimeLink::Attached(remote) => Ok(daemon::update_config_on(remote, &config).await?),
    }
}

/// Persists `config` to this machine's `system.db` and reloads the engine of
/// the runtime in this process.
async fn update_local_config(
    config: SttConfigView,
    runtime: &RuntimeHandle,
    app: &tauri::AppHandle,
    stt_flow_state: &SttFlowState,
) -> Result<(), String> {
    let db_path = apollia_core::paths::DataFile::System
        .path(&apollia_core::paths::data_dir().unwrap_or_default());

    // Ensure the data directory exists.
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create data directory: {e}"))?;
    }

    let row = apollia_core::SttConfigRow {
        enabled: config.enabled,
        model_path: config.model_path.clone(),
        hotkey: config.hotkey,
        clipboard_mode: config.clipboard_mode,
        clipboard_restore: config.clipboard_restore,
        silence_threshold_db: config.silence_threshold_db,
        max_recording_sec: config.max_recording_sec,
        language: config.language,
        trigger_mode: config.trigger_mode,
        input_device: config.input_device,
    };

    tokio::task::spawn_blocking(move || {
        let repo = apollia_core::SttConfigRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        repo.upsert(&row)
            .map_err(|e| format!("failed to persist STT config: {e}"))
    })
    .await
    .map_err(|e| format!("task error: {e}"))??;

    tracing::info!(
        enabled = config.enabled,
        model_path = %config.model_path,
        "stt.config.updated"
    );

    // Bring the running engine in line with the persisted config, no restart.
    if let Err(e) = reload_stt_inner(runtime, app, stt_flow_state).await {
        tracing::warn!(
            error = %e,
            detail = "the configuration is saved but the engine still runs the previous one",
            "stt.engine.reload.failed"
        );
    }

    Ok(())
}

/// Whether the armed dictation configuration differs from the persisted one.
///
/// `SttConfigRow` carries no `PartialEq` (it lives in `apollia-core`, whose
/// public surface this crate does not widen), so the comparison is spelled out
/// here. Both sides are destructured exhaustively on purpose: adding a field to
/// the row then fails to compile until it is classified, rather than silently
/// joining the set of settings that persist without ever taking effect.
fn differs(armed: &apollia_core::SttConfigRow, next: &apollia_core::SttConfigRow) -> bool {
    let apollia_core::SttConfigRow {
        enabled,
        model_path,
        hotkey,
        clipboard_mode,
        clipboard_restore,
        silence_threshold_db,
        max_recording_sec,
        language,
        trigger_mode,
        input_device,
    } = armed;

    // `model_path` and `language` are consumed by the engine, which
    // `reload_stt_inner` rebuilds unconditionally just above; they are compared
    // all the same so the armed snapshot never drifts from what is persisted.
    *enabled != next.enabled
        || *model_path != next.model_path
        || *hotkey != next.hotkey
        || *clipboard_mode != next.clipboard_mode
        || *clipboard_restore != next.clipboard_restore
        || (*silence_threshold_db - next.silence_threshold_db).abs() > f32::EPSILON
        || *max_recording_sec != next.max_recording_sec
        || *language != next.language
        || *trigger_mode != next.trigger_mode
        || *input_device != next.input_device
}

/// Rebuilds the STT engine from the persisted config and swaps it into the
/// shared cell, shutting the previous engine down. Arms the global hotkey and
/// recording flow whenever STT is enabled, matching the boot path: the flow
/// reads the shared engine cell lazily on each trigger, so it is armed even
/// before a model is loaded (a mid-session download plus reload then brings the
/// engine online without re-registering). Shared by the `reload_stt` command
/// and `update_stt_config`.
pub(crate) async fn reload_stt_inner(
    runtime: &RuntimeHandle,
    app: &tauri::AppHandle,
    stt_flow_state: &SttFlowState,
) -> Result<(), String> {
    let data_dir = apollia_core::paths::data_dir().unwrap_or_default();
    let db_path = apollia_core::paths::DataFile::System.path(&data_dir);

    let cfg = tokio::task::spawn_blocking(move || {
        let repo = apollia_core::SttConfigRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        repo.get_or_default()
            .map_err(|e| format!("failed to read STT config: {e}"))
    })
    .await
    .map_err(|e| format!("task error: {e}"))??;

    let runner_proxy = runtime.runner_supervisor.as_ref().map(|s| s.proxy());
    let (engine, repository) = apollia_runtime::stt::build_stt_engine(
        &data_dir,
        Some(&cfg),
        runner_proxy,
        &runtime.event_sender,
    )
    .await;
    let loaded = engine.is_some();

    // Swap the new engine/repository in, then shut the previous engine down
    // outside the write guard so no reader observes a half-swapped state.
    let previous = {
        let mut engine_cell = runtime.stt_engine.write().await;
        let mut repo_cell = runtime.stt_repository.write().await;
        let old = engine_cell.take();
        *engine_cell = engine;
        *repo_cell = repository;
        old
    };
    if let Some(old) = previous {
        old.shutdown().await;
    }

    // Arm the flow whenever STT is enabled and the armed configuration differs
    // from the persisted one; tear it down on an explicit disable. Arming does
    // not require a loaded model: the flow reads the shared engine cell on each
    // trigger and surfaces an honest "no model loaded" notification when it is
    // empty, so the hotkey and tour-recording commands stay wired the moment the
    // user enables dictation. Window + global-shortcut registration must run on
    // the main thread.
    //
    // The comparison covers the whole row, not just the hotkey. `SttFlow` owns a
    // snapshot of the configuration, and it reads `input_device`,
    // `silence_threshold_db`, `max_recording_sec`, `clipboard_mode`,
    // `clipboard_restore` and `trigger_mode` from that snapshot. Rebuilding only
    // on a changed hotkey left all six persisted but inert until the next
    // restart, which is why picking a different microphone in Settings appeared
    // to do nothing at all.
    let armed_config = stt_flow_state
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|f| f.config().clone()));
    let want_armed = cfg.enabled;
    if want_armed
        && armed_config
            .as_ref()
            .is_none_or(|armed| differs(armed, &cfg))
    {
        let app_for_main = app.clone();
        let runtime_handle = runtime.clone();
        let flow_state = Arc::clone(stt_flow_state);
        let cfg_for_hotkey = cfg.clone();
        app.run_on_main_thread(move || {
            crate::bootstrap::setup_stt_hotkey(
                &app_for_main,
                &cfg_for_hotkey,
                &runtime_handle,
                &flow_state,
            );
        })
        .map_err(|e| format!("failed to arm STT hotkey: {e}"))?;
    } else if !cfg.enabled && armed_config.is_some() {
        let app_for_main = app.clone();
        let flow_state = Arc::clone(stt_flow_state);
        app.run_on_main_thread(move || {
            let _ = crate::stt::hotkey::unregister_all(&app_for_main);
            if let Ok(mut guard) = flow_state.lock() {
                *guard = None;
            }
        })
        .map_err(|e| format!("failed to disarm STT hotkey: {e}"))?;
    }

    tracing::info!(loaded, enabled = cfg.enabled, "stt.engine.reloaded");
    Ok(())
}

/// Rebuilds the STT engine from `system.db` and swaps it into the shared cell.
///
/// Called by the frontend after a model is downloaded or STT is toggled so
/// dictation works without restarting the app. Attached, the daemon rebuilds
/// its engine through `POST /api/v1/stt/reload`.
#[tauri::command]
pub async fn reload_stt(link: State<'_, RuntimeLink>, app: tauri::AppHandle) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => {
            let stt_flow_state = flow_state(&app)?;
            reload_stt_inner(runtime, &app, &stt_flow_state).await
        }
        RuntimeLink::Attached(remote) => Ok(daemon::reload_on(remote).await?),
    }
}

/// The dictation flow of this window, managed only when the runtime runs in
/// this process.
fn flow_state(app: &tauri::AppHandle) -> Result<SttFlowState, String> {
    app.try_state::<SttFlowState>()
        .map(|state| Arc::clone(state.inner()))
        .ok_or_else(|| "STT flow not available".to_owned())
}

/// Returns the current STT engine status.
///
/// Returns an error string if the engine is not available (disabled or
/// model failed to load). Attached, the status is the daemon's
/// (`GET /api/v1/stt/status`), which answers `model_loaded: false` rather
/// than an error when it holds no engine. In both modes the microphones are
/// those of this machine.
#[tauri::command]
pub async fn get_stt_status(link: State<'_, RuntimeLink>) -> Result<serde_json::Value, String> {
    let value = match link.inner() {
        RuntimeLink::Embedded(runtime) => local_engine_status(runtime).await?,
        RuntimeLink::Attached(remote) => daemon::status_on(remote).await?,
    };
    Ok(with_input_available(value).await)
}

/// The status of the engine of the runtime in this process.
async fn local_engine_status(runtime: &RuntimeHandle) -> Result<serde_json::Value, String> {
    let engine = runtime.stt_engine.read().await.clone().ok_or_else(|| {
        "STT engine not available (enable dictation and load a model in Settings)".to_owned()
    })?;

    let status = engine
        .status()
        .await
        .ok_or_else(|| "STT engine actor has stopped".to_owned())?;

    serde_json::to_value(&status).map_err(|e| format!("serialization error: {e}"))
}

/// Adds `input_available` to an engine status.
async fn with_input_available(mut value: serde_json::Value) -> serde_json::Value {
    // Enrich with live microphone availability so the UI can flag a machine
    // whose STT engine is loaded but has no capture device, instead of the
    // engine appearing armed while dictation silently does nothing.
    let devices = tokio::task::spawn_blocking(apollia_stt::list_input_devices)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "input_available".to_owned(),
            serde_json::Value::Bool(!devices.is_empty()),
        );
    }
    value
}

/// Lists transcription history with optional limit.
///
/// Defaults to the 50 most recent transcriptions when `limit` is `None`.
#[tauri::command]
pub async fn list_transcriptions(
    link: State<'_, RuntimeLink>,
    limit: Option<u32>,
) -> Result<Vec<TranscriptRow>, String> {
    let limit = limit.unwrap_or(50);
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(transcriptions::list_on(remote, limit).await?),
    };
    let repo = stt_repo(runtime).await?;

    let rows = repo
        .lock()
        .map_err(|e| format!("repository lock error: {e}"))?
        .list(limit, 0)
        .map_err(|e| format!("failed to list transcriptions: {e}"))?;

    Ok(rows)
}

/// Deletes a transcription by its ID.
///
/// Silently succeeds if the ID does not exist (no-op delete).
#[tauri::command]
pub async fn delete_transcription(link: State<'_, RuntimeLink>, id: String) -> Result<(), String> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(transcriptions::delete_on(remote, &id).await?),
    };
    let repo = stt_repo(runtime).await?;

    repo.lock()
        .map_err(|e| format!("repository lock error: {e}"))?
        .delete(&id)
        .map_err(|e| format!("failed to delete transcription: {e}"))?;

    Ok(())
}

/// Transcribes a WAV audio file from the local filesystem.
///
/// Reads the file at `file_path`, decodes the WAV data, resamples to 16 kHz
/// mono, and submits it to the STT engine. The result is persisted and returned.
/// Attached, the file is read on this machine and the daemon transcribes it.
#[tauri::command]
pub async fn transcribe_file(
    link: State<'_, RuntimeLink>,
    file_path: String,
) -> Result<TranscriptRow, String> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return transcriptions::transcribe_on(remote, &file_path).await
        }
    };
    let engine = runtime.stt_engine.read().await.clone().ok_or_else(|| {
        "STT engine not available (enable dictation and load a model in Settings)".to_owned()
    })?;
    let repo = stt_repo(runtime).await?;

    let raw_bytes =
        std::fs::read(&file_path).map_err(|e| format!("failed to read file '{file_path}': {e}"))?;

    let (samples, sample_rate, channels) = decode_wav(&raw_bytes)?;

    let audio = apollia_stt::to_whisper_format(&samples, sample_rate, channels)
        .map_err(|e| format!("audio resampling error: {e}"))?;

    let transcript = engine
        .transcribe(audio, 16000, TranscriptSource::File(file_path))
        .await
        .map_err(|e| format!("transcription failed: {e}"))?;

    let id = repo
        .lock()
        .map_err(|e| format!("repository lock error: {e}"))?
        .insert("file", &transcript, None)
        .map_err(|e| format!("failed to persist transcription: {e}"))?;

    let row = repo
        .lock()
        .map_err(|e| format!("repository lock error: {e}"))?
        .get(&id)
        .map_err(|e| format!("failed to retrieve transcription: {e}"))?
        .ok_or_else(|| "transcription persisted but could not be retrieved".to_owned())?;

    Ok(row)
}

/// Lists available STT model files in `~/.apollia/models/`.
///
/// Scans for the extensions in [`apollia_runtime::stt::MODEL_EXTENSIONS`] and
/// returns their metadata.
/// Returns an empty list if the models directory does not exist.
/// Attached, the daemon scans the same folder (`GET /api/v1/stt/models`).
#[tauri::command]
pub async fn list_stt_models(link: State<'_, RuntimeLink>) -> Result<Vec<SttModelInfo>, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => list_local_models(),
        RuntimeLink::Attached(remote) => Ok(daemon::models_on(remote).await?),
    }
}

/// The model files of this machine's models folder.
fn list_local_models() -> Result<Vec<SttModelInfo>, String> {
    let models_dir = apollia_core::paths::data_dir()
        .unwrap_or_default()
        .join("models");

    if !models_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&models_dir).map_err(|e| format!("failed to read models dir: {e}"))?;

    let mut models = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !apollia_runtime::stt::is_model_file(&path) {
            continue;
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        models.push(SttModelInfo::from_file(
            name,
            path.display().to_string(),
            size_bytes,
        ));
    }

    Ok(models)
}

// ── TOML helpers ─────────────────────────────────────────────────────

/// Formats an `[stt]` TOML block from a config view.
///
/// Keys are padded to 21 characters for alignment. The `language` key is
/// omitted when its value is `None`.
#[cfg(test)]
fn format_stt_block(config: &SttConfigView) -> String {
    let mut lines = vec![
        "[stt]".to_owned(),
        format!("{:<21}= {}", "enabled", config.enabled),
        format!("{:<21}= \"{}\"", "model_path", config.model_path),
        format!("{:<21}= \"{}\"", "hotkey", config.hotkey),
        format!("{:<21}= \"{}\"", "clipboard_mode", config.clipboard_mode),
        format!("{:<21}= {}", "clipboard_restore", config.clipboard_restore),
        format!(
            "{:<21}= {}",
            "silence_threshold_db", config.silence_threshold_db
        ),
        format!("{:<21}= {}", "max_recording_sec", config.max_recording_sec),
        format!("{:<21}= \"{}\"", "trigger_mode", config.trigger_mode),
    ];
    if let Some(ref lang) = config.language {
        lines.push(format!("{:<21}= \"{}\"", "language", lang));
    }
    lines.join("\n")
}

/// Replaces the `[stt]` section in `toml_content` with `new_block`,
/// or appends `new_block` when no `[stt]` section exists.
///
/// Sections other than `[stt]` are preserved in their original order.
#[cfg(test)]
fn replace_or_append_stt_section(toml_content: &str, new_block: &str) -> String {
    // Find the byte offset of "[stt]" in the content
    if let Some(start) = find_section_start(toml_content, "[stt]") {
        // Find where the next section starts (or end of string)
        let after_stt = &toml_content[start + "[stt]".len()..];
        let end_offset = find_next_section_offset(after_stt)
            .map(|o| start + "[stt]".len() + o)
            .unwrap_or(toml_content.len());

        let before = toml_content[..start].trim_end_matches('\n');
        let after = toml_content[end_offset..].trim_start_matches('\n');

        if after.is_empty() {
            format!("{}\n\n{}\n", before, new_block)
        } else {
            format!("{}\n\n{}\n\n{}\n", before, new_block, after)
        }
    } else {
        let base = toml_content.trim_end_matches('\n');
        format!("{}\n\n{}\n", base, new_block)
    }
}

/// Returns the byte offset of the line starting with `header` in `content`.
#[cfg(test)]
fn find_section_start(content: &str, header: &str) -> Option<usize> {
    let mut offset = 0;
    for line in content.lines() {
        if line.trim() == header {
            return Some(offset);
        }
        offset += line.len() + 1; // +1 for '\n'
    }
    None
}

/// Returns the byte offset within `content` where the next TOML section
/// header (`[…]`) begins, or `None` if no such header is found.
#[cfg(test)]
fn find_next_section_offset(content: &str) -> Option<usize> {
    let mut offset = 0;
    let mut first = true;
    for line in content.lines() {
        if !first && line.starts_with('[') && !line.starts_with("[[") {
            return Some(offset);
        }
        first = false;
        offset += line.len() + 1;
    }
    None
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Clones the current `SttRepository` handle out of the shared cell.
async fn stt_repo(runtime: &RuntimeHandle) -> Result<Arc<std::sync::Mutex<SttRepository>>, String> {
    runtime
        .stt_repository
        .read()
        .await
        .clone()
        .ok_or_else(|| "STT repository not available".to_owned())
}

/// Decodes a WAV byte buffer into f32 samples.
fn decode_wav(data: &[u8]) -> Result<(Vec<f32>, u32, u16), String> {
    let cursor = Cursor::new(data);
    let reader = hound::WavReader::new(cursor).map_err(|e| format!("invalid WAV file: {e}"))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max_val))
                .collect::<Result<Vec<f32>, _>>()
                .map_err(|e| format!("WAV sample read error: {e}"))?
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()
            .map_err(|e| format!("WAV sample read error: {e}"))?,
    };

    Ok((samples, sample_rate, channels))
}

/// Attempts to detect the language from a model filename.
///
/// Looks for common ISO 639-1 language codes in the filename
/// (e.g. `whisper-large-v3-fr-q5_0.bin` → `Some("fr")`).
fn detect_language_from_name(name: &str) -> Option<String> {
    let known = [
        "fr", "en", "de", "es", "it", "pt", "nl", "pl", "ru", "zh", "ja", "ko", "ar",
    ];
    let lower = name.to_lowercase();
    for lang in &known {
        let pattern = format!("-{lang}-");
        if lower.contains(&pattern) {
            return Some((*lang).to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_stt_block_with_language() {
        // GIVEN a config with a language set
        let config = SttConfigView {
            enabled: true,
            model_path: "~/.apollia/models/test.bin".to_owned(),
            hotkey: "ctrl+shift+space".to_owned(),
            clipboard_mode: "paste".to_owned(),
            clipboard_restore: true,
            silence_threshold_db: -40.0,
            max_recording_sec: 60,
            language: Some("fr".to_owned()),
            trigger_mode: "toggle".to_owned(),
            input_device: None,
        };
        // WHEN formatted
        let block = format_stt_block(&config);
        // THEN key fields are present
        assert!(block.contains("[stt]"));
        assert!(block.contains("enabled              = true"));
        assert!(block.contains("language             = \"fr\""));
        assert!(block.contains("model_path           = \"~/.apollia/models/test.bin\""));
    }

    #[test]
    fn format_stt_block_without_language() {
        // GIVEN a config with no language (auto-detect)
        let config = SttConfigView {
            language: None,
            ..SttConfigView::default()
        };
        // WHEN formatted
        let block = format_stt_block(&config);
        // THEN no language line is emitted
        assert!(!block.contains("language             ="));
    }

    #[test]
    fn replace_stt_section_replaces_existing() {
        // GIVEN an apollia.toml with an existing [stt] section
        let existing =
            "[runtime]\nport = 7771\n\n[stt]\nenabled = false\nmodel_path = \"old.bin\"\n";
        let new_block = "[stt]\nenabled = true\nmodel_path = \"new.bin\"";
        // WHEN replacing
        let result = replace_or_append_stt_section(existing, new_block);
        // THEN [stt] has new values
        assert!(result.contains("enabled = true"));
        assert!(result.contains("new.bin"));
        // AND [runtime] is preserved
        assert!(result.contains("[runtime]"));
        // AND old values are gone
        assert!(!result.contains("enabled = false"));
        assert!(!result.contains("old.bin"));
    }

    #[test]
    fn replace_stt_section_appends_when_absent() {
        // GIVEN a config without [stt]
        let existing = "[runtime]\nport = 7771\n";
        let new_block = "[stt]\nenabled = true";
        // WHEN replacing
        let result = replace_or_append_stt_section(existing, new_block);
        // THEN [stt] is appended and [runtime] preserved
        assert!(result.contains("[stt]"));
        assert!(result.contains("[runtime]"));
    }

    #[test]
    fn replace_stt_section_preserves_suffix_sections() {
        // GIVEN a config with [stt] followed by another section
        let existing = "[stt]\nenabled = false\n\n[tools]\nsandbox = true\n";
        let new_block = "[stt]\nenabled = true";
        // WHEN replacing
        let result = replace_or_append_stt_section(existing, new_block);
        // THEN [tools] is still present
        assert!(result.contains("[tools]"));
        assert!(result.contains("enabled = true"));
    }

    #[test]
    fn changing_only_the_input_device_rearms_the_flow() {
        // GIVEN a flow armed on the system default microphone
        let armed = apollia_core::SttConfigRow {
            enabled: true,
            ..apollia_core::SttConfigRow::default()
        };
        // WHEN the operator picks a named microphone and saves
        let next = apollia_core::SttConfigRow {
            input_device: Some("Razer Seiren V3 Chroma".to_owned()),
            ..armed.clone()
        };
        // THEN the flow is rebuilt, so the choice applies without a restart
        assert!(differs(&armed, &next));
    }

    #[test]
    fn every_field_the_flow_reads_rearms_it() {
        // GIVEN a flow armed on a baseline configuration
        let armed = apollia_core::SttConfigRow {
            enabled: true,
            ..apollia_core::SttConfigRow::default()
        };

        // WHEN any single setting the flow snapshots changes
        let mutations: Vec<apollia_core::SttConfigRow> = vec![
            apollia_core::SttConfigRow {
                silence_threshold_db: -20.0,
                ..armed.clone()
            },
            apollia_core::SttConfigRow {
                max_recording_sec: 30,
                ..armed.clone()
            },
            apollia_core::SttConfigRow {
                clipboard_mode: "clipboard".to_owned(),
                ..armed.clone()
            },
            apollia_core::SttConfigRow {
                clipboard_restore: !armed.clipboard_restore,
                ..armed.clone()
            },
            apollia_core::SttConfigRow {
                trigger_mode: "push-to-talk".to_owned(),
                ..armed.clone()
            },
            apollia_core::SttConfigRow {
                hotkey: "ctrl+alt+d".to_owned(),
                ..armed.clone()
            },
        ];

        // THEN each one re-arms; none stays persisted but inert
        for next in mutations {
            assert!(differs(&armed, &next), "should re-arm for {next:?}");
        }
    }

    #[test]
    fn an_unchanged_configuration_does_not_rearm() {
        // GIVEN a saved configuration identical to the armed one
        let armed = apollia_core::SttConfigRow {
            enabled: true,
            input_device: Some("Razer Seiren V3 Chroma".to_owned()),
            ..apollia_core::SttConfigRow::default()
        };
        // WHEN it is saved again unchanged
        // THEN the hotkey and overlay are left alone
        assert!(!differs(&armed, &armed.clone()));
    }

    #[test]
    fn detect_language_from_model_name() {
        // GIVEN a model filename containing a language code
        // WHEN detect_language_from_name is called
        // THEN the correct language is extracted
        assert_eq!(
            detect_language_from_name("whisper-large-v3-fr-q5_0.bin"),
            Some("fr".to_owned())
        );
        assert_eq!(
            detect_language_from_name("whisper-base-en-q4.bin"),
            Some("en".to_owned())
        );
    }

    #[test]
    fn detect_language_returns_none_for_generic_model() {
        // GIVEN a model filename without a language code
        // WHEN detect_language_from_name is called
        // THEN None is returned
        assert_eq!(detect_language_from_name("whisper-large-v3-q5_0.bin"), None);
    }

    #[test]
    fn stt_model_info_serializes_correctly() {
        // GIVEN an SttModelInfo
        let info = SttModelInfo {
            name: "whisper-large-v3-fr-q5_0.bin".to_owned(),
            path: "/home/user/.apollia/models/whisper-large-v3-fr-q5_0.bin".to_owned(),
            size_mb: 921.5,
            language: Some("fr".to_owned()),
        };
        // WHEN serialized to JSON
        let json = serde_json::to_value(&info).expect("serialize");
        // THEN all fields are present
        assert_eq!(json["name"], "whisper-large-v3-fr-q5_0.bin");
        assert_eq!(json["size_mb"], 921.5);
        assert_eq!(json["language"], "fr");
    }

    #[test]
    fn decode_wav_rejects_invalid_data() {
        // GIVEN invalid WAV data
        let result = decode_wav(b"not a wav file");
        // WHEN it is decoded
        // THEN an error is returned
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid WAV"));
    }
}

// ---------------------------------------------------------------------------
// Push-to-talk commands for the guided tour
// ---------------------------------------------------------------------------

/// Convenience type alias for the shared SttFlow state managed by Tauri.
///
/// Populated during app setup when the STT engine is configured and loaded.
/// Remains `None` when STT is unavailable so all callers can degrade gracefully.
pub type SttFlowState = Arc<std::sync::Mutex<Option<Arc<crate::stt::flow::SttFlow>>>>;

/// Starts microphone capture for a push-to-talk voice command in the guided tour.
///
/// Returns an error (without panicking) when the STT engine is not configured,
/// allowing the frontend to hide the microphone button gracefully.
#[tauri::command]
pub async fn start_tour_recording(
    flow_state: tauri::State<'_, SttFlowState>,
) -> Result<(), String> {
    let guard = flow_state.lock().map_err(|e| format!("lock error: {e}"))?;

    match guard.as_ref() {
        Some(flow) => {
            // In-app surface: the webview consumes `stt-transcribed`, so the
            // result must never also be pasted through the OS.
            flow.start_recording(crate::stt::flow::RecordingOrigin::InApp);
            Ok(())
        }
        None => Err("STT engine not available".to_owned()),
    }
}

/// Stops microphone capture and triggers Whisper transcription.
///
/// Answers as soon as the capture is stopped. The transcription runs on its
/// own task and lands as a `stt-transcribed` Tauri event (or a dictation
/// failure), which is what every in-app surface listens for. This used to
/// await the transcription before answering: on a processor-bound machine the
/// reply took as long as the transcription itself, tens of seconds, and every
/// surface kept its "speak, click to stop" state for the whole wait, a second
/// click being swallowed as a stop of nothing. Measured on 2026-09-10 on
/// Windows: 19.5 s of processing for 5.5 s of audio.
///
/// A stop with no capture running is an error rather than a silent success,
/// since no event follows it and a caller waiting on one would wait forever.
#[tauri::command]
pub async fn stop_tour_recording(flow_state: tauri::State<'_, SttFlowState>) -> Result<(), String> {
    // Clone the Arc out of the lock so transcription runs outside the guard.
    let maybe_flow = {
        let guard = flow_state.lock().map_err(|e| format!("lock error: {e}"))?;
        guard.as_ref().cloned()
    };

    let Some(flow) = maybe_flow else {
        return Err("STT engine not available".to_owned());
    };
    stop_request_outcome(flow.is_recording())?;
    // Delivery follows the origin recorded at start, so stopping an in-app
    // recording from here never triggers an OS paste even if the global
    // hotkey is what actually stopped it.
    tauri::async_runtime::spawn(async move {
        flow.stop_and_transcribe().await;
    });
    Ok(())
}

/// What a stop request answers, given whether a capture is running.
///
/// Pure, so the refusal is assertable without a microphone.
pub(crate) fn stop_request_outcome(recording: bool) -> Result<(), String> {
    if recording {
        Ok(())
    } else {
        Err("no recording in progress".to_owned())
    }
}

#[cfg(test)]
mod stop_request_tests {
    use super::stop_request_outcome;

    #[test]
    fn a_stop_with_a_capture_running_is_accepted() {
        // GIVEN a flow whose capture is running
        // WHEN a stop is requested
        let outcome = stop_request_outcome(true);

        // THEN the request goes through, the transcription follows as an event
        assert!(outcome.is_ok());
    }

    #[test]
    fn a_stop_with_nothing_running_is_refused() {
        // GIVEN a flow with no capture running. This is the control: a stop
        // that answered Ok here is the silent success that left surfaces
        // waiting for an event that never came.
        // WHEN a stop is requested
        let outcome = stop_request_outcome(false);

        // THEN the caller is told, and can reset instead of waiting
        assert_eq!(outcome, Err("no recording in progress".to_owned()));
    }
}
