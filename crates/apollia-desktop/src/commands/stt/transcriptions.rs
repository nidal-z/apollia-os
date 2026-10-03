//! The transcription history and the transcription of a file, as the daemon
//! serves them to an attached window.
//!
//! The daemon owns the engine and the history. The file to transcribe stays on
//! the machine of the window, which reads it and sends its content, the way the
//! memory export and import do.

use std::path::Path;

use apollia_stt::TranscriptRow;
use serde::Deserialize;

use crate::attached::{segment, AttachedClient, AttachedError};

/// The answer of `GET /api/v1/stt/transcriptions`.
#[derive(Deserialize)]
struct Listed {
    transcriptions: Vec<TranscriptRow>,
}

/// The `limit` most recent transcriptions the daemon keeps.
pub(super) async fn list_on(
    remote: &AttachedClient,
    limit: u32,
) -> Result<Vec<TranscriptRow>, AttachedError> {
    let listed: Listed = remote
        .get(&format!("/api/v1/stt/transcriptions?limit={limit}"))
        .await?;
    Ok(listed.transcriptions)
}

/// Deletes one transcription on the daemon; an unknown identifier is no error,
/// as in the embedded mode.
pub(super) async fn delete_on(remote: &AttachedClient, id: &str) -> Result<(), AttachedError> {
    remote
        .delete_empty(&format!("/api/v1/stt/transcriptions/{}", segment(id)))
        .await
}

/// Reads the WAV at `file_path` on this machine and has the daemon transcribe
/// it, kept in the history as a file.
pub(super) async fn transcribe_on(
    remote: &AttachedClient,
    file_path: &str,
) -> Result<TranscriptRow, String> {
    let wav =
        std::fs::read(file_path).map_err(|e| format!("failed to read file '{file_path}': {e}"))?;
    let name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav");
    Ok(remote.transcribe(&wav, name, "file").await?)
}

#[cfg(test)]
mod tests;
