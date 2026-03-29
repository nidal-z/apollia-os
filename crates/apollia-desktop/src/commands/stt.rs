//! Tauri IPC commands for STT (Speech-to-Text) functionality.
//!
//! Exposes 7 commands to the Svelte frontend:
//! - `get_stt_config`      — read [stt] section from apollia.toml
//! - `update_stt_config`   — write [stt] section to apollia.toml
//! - `get_stt_status`      — query runtime engine status
//! - `list_transcriptions` — list transcription history
//! - `delete_transcription`— delete a transcription by ID
//! - `transcribe_file`     — transcribe a WAV file
//! - `list_stt_models`     — list available .bin model files

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
        }
    }
}

/// Returns the current `[stt]` configuration from `~/.apollia/apollia.toml`.
///
/// Reads and deserialises only the `[stt]` table. Returns defaults if the
/// file does not exist or the section is absent.
#[tauri::command]
pub async fn get_stt_config() -> Result<SttConfigView, String> {
    let config_path = resolve_home("~/.apollia/apollia.toml");

    if !config_path.exists() {
        return Ok(SttConfigView::default());
    }

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("failed to read apollia.toml: {e}"))?;

    #[derive(serde::Deserialize)]
    struct Wrapper {
        #[serde(default)]
        stt: apollia_core::SttConfig,
    }

    let wrapper: Wrapper = toml::from_str(&content)
        .map_err(|e| format!("failed to parse apollia.toml: {e}"))?;

    let stt = wrapper.stt;
    Ok(SttConfigView {
        enabled: stt.enabled,
        model_path: stt.model_path.display().to_string(),
        hotkey: stt.hotkey,
        clipboard_mode: stt.clipboard_mode,
        clipboard_restore: stt.clipboard_restore,
        silence_threshold_db: stt.silence_threshold_db,
        max_recording_sec: stt.max_recording_sec,
        language: stt.language,
        trigger_mode: stt.trigger_mode,
    })
}

/// Writes the `[stt]` section to `~/.apollia/apollia.toml`.
///
/// Finds and replaces the existing `[stt]` block (preserving all other
/// sections and comments), or appends it if absent. The caller must restart
/// the application for the new configuration to take effect.
#[tauri::command]
pub async fn update_stt_config(config: SttConfigView) -> Result<(), String> {
    let config_path = resolve_home("~/.apollia/apollia.toml");

    // Ensure the config directory exists.
    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }

    let existing = if config_path.exists() {
        tokio::fs::read_to_string(&config_path)
            .await
            .map_err(|e| format!("failed to read apollia.toml: {e}"))?
    } else {
        String::new()
    };

    let stt_block = format_stt_block(&config);
    let new_content = replace_or_append_stt_section(&existing, &stt_block);

    tokio::fs::write(&config_path, new_content)
        .await
        .map_err(|e| format!("failed to write apollia.toml: {e}"))?;

    tracing::info!(
        enabled = config.enabled,
        model_path = %config.model_path,
        "STT configuration updated via desktop UI"
    );

    Ok(())
}

/// Returns the current STT engine status.
///
/// Returns an error string if the engine is not available (disabled or
/// model failed to load).
#[tauri::command]
pub async fn get_stt_status(
    runtime: State<'_, RuntimeHandle>,
) -> Result<serde_json::Value, String> {
    let engine = runtime
        .stt_engine
        .as_ref()
        .ok_or_else(|| "STT engine not available".to_owned())?;

    let status = engine
        .status()
        .await
        .ok_or_else(|| "STT engine actor has stopped".to_owned())?;

    serde_json::to_value(&status).map_err(|e| format!("serialization error: {e}"))
}

/// Lists transcription history with optional limit.
///
/// Defaults to the 50 most recent transcriptions when `limit` is `None`.
#[tauri::command]
pub async fn list_transcriptions(
    runtime: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<TranscriptRow>, String> {
    let repo = stt_repo(&runtime)?;
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
    let repo = stt_repo(&runtime)?;

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
    let engine = runtime
        .stt_engine
        .as_ref()
        .ok_or_else(|| "STT engine not available".to_owned())?;
    let repo = stt_repo(&runtime)?;

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

// ── STT config helpers ───────────────────────────────────────────────

/// Formats a `[stt]` TOML block from a [`SttConfigView`].
fn format_stt_block(config: &SttConfigView) -> String {
    let lang_line = match &config.language {
        Some(lang) if !lang.is_empty() => format!("language             = \"{lang}\"\n"),
        _ => String::new(),
    };

    format!(
        "[stt]\n\
         enabled              = {enabled}\n\
         model_path           = \"{model_path}\"\n\
         hotkey               = \"{hotkey}\"\n\
         clipboard_mode       = \"{clipboard_mode}\"\n\
         clipboard_restore    = {clipboard_restore}\n\
         silence_threshold_db = {silence:.1}\n\
         max_recording_sec    = {max_rec}\n\
         {lang_line}\
         trigger_mode         = \"{trigger_mode}\"",
        enabled = config.enabled,
        model_path = config.model_path,
        hotkey = config.hotkey,
        clipboard_mode = config.clipboard_mode,
        clipboard_restore = config.clipboard_restore,
        silence = config.silence_threshold_db,
        max_rec = config.max_recording_sec,
        lang_line = lang_line,
        trigger_mode = config.trigger_mode,
    )
}

/// Finds the `[stt]` section in `existing` and replaces its content with
/// `stt_block`. If no `[stt]` section is found, appends the block.
///
/// Comments and other sections are preserved.
fn replace_or_append_stt_section(existing: &str, stt_block: &str) -> String {
    // Locate "[stt]" at start of a line.
    let stt_pos = if existing.starts_with("[stt]") {
        Some(0usize)
    } else {
        existing.find("\n[stt]").map(|i| i + 1)
    };

    if let Some(start) = stt_pos {
        let prefix = existing[..start].trim_end_matches(|c: char| c == '\n' || c == '\r');

        // Find the end of the [stt] section body: next top-level "[" at start
        // of a line, or end of file.
        let rest = &existing[start..]; // starts at "[stt]"
        let section_end = rest[1..]   // skip the "[" to avoid self-match
            .find("\n[")
            .map(|i| start + 1 + i + 1)
            .unwrap_or(existing.len());

        let suffix = existing[section_end..].trim_start_matches(|c: char| c == '\n' || c == '\r');

        if suffix.is_empty() {
            format!("{prefix}\n\n{stt_block}\n")
        } else {
            format!("{prefix}\n\n{stt_block}\n\n{suffix}")
        }
    } else {
        format!("{}\n\n{stt_block}\n", existing.trim_end())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Extracts the `SttRepository` from the runtime handle.
fn stt_repo(runtime: &RuntimeHandle) -> Result<&Arc<std::sync::Mutex<SttRepository>>, String> {
    runtime
        .stt_repository
        .as_ref()
        .ok_or_else(|| "STT repository not available".to_owned())
}

/// Resolves `~` prefix to `$HOME`.
fn resolve_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
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
        let existing = "[runtime]\nport = 7771\n\n[stt]\nenabled = false\nmodel_path = \"old.bin\"\n";
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
