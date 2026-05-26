//! Types STT : transcribe.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Params pour `POST /stt/transcribe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeParams {
    pub model_id: String,
    pub audio_path: PathBuf,
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
