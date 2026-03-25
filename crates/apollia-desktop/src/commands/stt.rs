//! Tauri IPC commands for STT (Speech-to-Text) functionality.
//!
//! Exposes 5 commands to the Svelte frontend for querying STT engine status,
//! listing/deleting transcriptions, transcribing audio files, and listing
//! available models. All commands delegate to [`SttEngineHandle`] and
//! [`SttRepository`] via the managed [`RuntimeHandle`].

use std::io::Cursor;
use std::sync::Arc;

use serde::Serialize;
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
