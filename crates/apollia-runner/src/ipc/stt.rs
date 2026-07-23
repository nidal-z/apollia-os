//! Types STT : transcribe.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Params pour `POST /stt/transcribe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeParams {
    pub model_id: String,
    pub audio_path: PathBuf,
    /// Absolute path to the whisper model on disk. When set and the model is
    /// not already cached, the runner loads it on demand (the daemon owns the
    /// `model_id -> path` mapping and sends the path so the runner needs no
    /// separate load call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default = "default_task")]
    pub task: String,
}

fn default_task() -> String {
    "transcribe".into()
}

/// Segment temporel d'une transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// Réponse de `POST /stt/transcribe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeData {
    pub text: String,
    pub segments: Vec<TranscribeSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_detected: Option<String>,
    pub timing_ms: u64,
}
