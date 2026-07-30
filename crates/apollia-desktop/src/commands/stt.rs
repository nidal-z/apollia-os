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
//! - `list_stt_models`        : list available .bin model files
//! - `start_tour_recording`   : begin push-to-talk recording for the guided tour
//! - `stop_tour_recording`    : stop recording and trigger transcription

use std::io::Cursor;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::stt::TranscriptSource;
use apollia_stt::{SttRepository, TranscriptRow};

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

/// STT configuration read from (and written to) the `[stt]` section of `apollia.toml`.
///
/// Mirror of [`apollia_core::SttConfig`] with all paths as plain `String`
/// so the Tauri JSON bridge can serialise/deserialise without PathBuf.
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
/// If the row does not exist yet, defaults are inserted and returned.
#[tauri::command]
pub async fn get_stt_config() -> Result<SttConfigView, String> {
    let db_path = resolve_home("~/.apollia/system.db");
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
#[tauri::command]
pub async fn update_stt_config(
    config: SttConfigView,
    runtime: State<'_, RuntimeHandle>,
    app: tauri::AppHandle,
    stt_flow_state: State<'_, SttFlowState>,
) -> Result<(), String> {
    let db_path = resolve_home("~/.apollia/system.db");

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
        "STT configuration updated via desktop UI"
    );

    // Bring the running engine in line with the persisted config, no restart.
    if let Err(e) = reload_stt_inner(&runtime, &app, &stt_flow_state).await {
        tracing::warn!(error = %e, "STT config saved but engine reload failed");
    }

    Ok(())
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
    let data_dir = resolve_home("~/.apollia");
    let db_path = data_dir.join("system.db");

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

    // Arm the hotkey whenever STT is enabled and the binding is not yet
    // registered or has changed; tear it down on an explicit disable. Arming
    // does not require a loaded model: the flow reads the shared engine cell on
    // each trigger and surfaces an honest "no model loaded" notification when it
    // is empty, so the hotkey and tour-recording commands stay wired the moment
    // the user enables dictation. Window + global-shortcut registration must run
    // on the main thread.
    let armed_hotkey = stt_flow_state
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|f| f.hotkey().to_owned()));
    let want_armed = cfg.enabled;
    if want_armed && armed_hotkey.as_deref() != Some(cfg.hotkey.as_str()) {
        let app_for_main = app.clone();
        let runtime_handle = runtime.clone();
        let flow_state = Arc::clone(stt_flow_state);
        let cfg_for_hotkey = cfg.clone();
        app.run_on_main_thread(move || {
            crate::setup_stt_hotkey(&app_for_main, &cfg_for_hotkey, &runtime_handle, &flow_state);
        })
        .map_err(|e| format!("failed to arm STT hotkey: {e}"))?;
    } else if !cfg.enabled && armed_hotkey.is_some() {
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

    tracing::info!(loaded, enabled = cfg.enabled, "STT engine reloaded");
    Ok(())
}

/// Rebuilds the STT engine from `system.db` and swaps it into the shared cell.
///
/// Called by the frontend after a model is downloaded or STT is toggled so
/// dictation works without restarting the app.
#[tauri::command]
pub async fn reload_stt(
    runtime: State<'_, RuntimeHandle>,
    app: tauri::AppHandle,
    stt_flow_state: State<'_, SttFlowState>,
) -> Result<(), String> {
    reload_stt_inner(&runtime, &app, &stt_flow_state).await
}

/// Returns the current STT engine status.
///
/// Returns an error string if the engine is not available (disabled or
/// model failed to load).
#[tauri::command]
pub async fn get_stt_status(
    runtime: State<'_, RuntimeHandle>,
) -> Result<serde_json::Value, String> {
    let engine = runtime.stt_engine.read().await.clone().ok_or_else(|| {
        "STT engine not available (enable dictation and load a model in Settings)".to_owned()
    })?;

    let status = engine
        .status()
        .await
        .ok_or_else(|| "STT engine actor has stopped".to_owned())?;

    let mut value =
        serde_json::to_value(&status).map_err(|e| format!("serialization error: {e}"))?;

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

    Ok(value)
}

/// Lists transcription history with optional limit.
///
/// Defaults to the 50 most recent transcriptions when `limit` is `None`.
#[tauri::command]
pub async fn list_transcriptions(
    runtime: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<TranscriptRow>, String> {
    let repo = stt_repo(&runtime).await?;
    let limit = limit.unwrap_or(50);

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
pub async fn delete_transcription(
    runtime: State<'_, RuntimeHandle>,
    id: String,
) -> Result<(), String> {
    let repo = stt_repo(&runtime).await?;

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
#[tauri::command]
pub async fn transcribe_file(
    runtime: State<'_, RuntimeHandle>,
    file_path: String,
) -> Result<TranscriptRow, String> {
    let engine = runtime.stt_engine.read().await.clone().ok_or_else(|| {
        "STT engine not available (enable dictation and load a model in Settings)".to_owned()
    })?;
    let repo = stt_repo(&runtime).await?;

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
/// Scans for `.bin` files and returns their metadata. Returns an empty
/// list if the models directory does not exist.
#[tauri::command]
pub async fn list_stt_models(
    _runtime: State<'_, RuntimeHandle>,
) -> Result<Vec<SttModelInfo>, String> {
    let models_dir = resolve_home("~/.apollia/models");

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
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
        let language = detect_language_from_name(&name);

        models.push(SttModelInfo {
            name,
            path: path.display().to_string(),
            size_mb,
            language,
        });
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

/// Resolves `~` prefix to `$HOME`.
fn resolve_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = apollia_core::paths::home_string() {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
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
            flow.start_recording();
            Ok(())
        }
        None => Err("STT engine not available".to_owned()),
    }
}

/// Stops microphone capture and triggers Whisper transcription.
///
/// The transcription result is broadcast as a `stt-transcribed` Tauri event
/// with the recognised text, which the guided tour consumes to execute the
/// corresponding navigation action.
#[tauri::command]
pub async fn stop_tour_recording(flow_state: tauri::State<'_, SttFlowState>) -> Result<(), String> {
    // Clone the Arc out of the lock so transcription runs outside the guard.
    let maybe_flow = {
        let guard = flow_state.lock().map_err(|e| format!("lock error: {e}"))?;
        guard.as_ref().cloned()
    };

    match maybe_flow {
        Some(flow) => {
            // In-app recording (mic button, onboarding test): the webview
            // consumes the `stt-transcribed` event, so skip the OS paste to
            // avoid a double insertion when the Apollia window is focused.
            flow.stop_and_transcribe(false).await;
            Ok(())
        }
        None => Err("STT engine not available".to_owned()),
    }
}
