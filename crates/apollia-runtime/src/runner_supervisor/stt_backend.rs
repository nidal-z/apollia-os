//! `RunnerSttBackend` : adapte `SttBackend` (apollia-stt) sur le
//! [`RunnerProxy`] via HTTP/JSON IPC.
//!
//! Cf. ADR-113 et IPC-PROTOCOL §3.8.

use std::path::Path;

use apollia_stt::{SttBackend, SttError, TranscriptResult};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::Value;
use tokio::runtime::Handle;

use super::proxy::RunnerProxy;

/// Backend `SttBackend` qui route les appels vers le runner sidecar.
///
/// `SttBackend::transcribe` est synchrone et reçoit un buffer PCM. Le proxy
/// HTTP est async et l'endpoint `/stt/transcribe` attend un chemin de fichier.
/// On écrit donc le buffer dans un WAV temporaire, on appelle le runner, puis
/// on cleanup (le `tempfile` est drop-supprimé en fin de scope).
pub struct RunnerSttBackend {
    proxy: RunnerProxy,
    model_id: String,
}

impl RunnerSttBackend {
    pub fn new(proxy: RunnerProxy, model_id: String) -> Self {
        Self { proxy, model_id }
    }
}

impl SttBackend for RunnerSttBackend {
    fn name(&self) -> &str {
        "runner-whisper"
    }

    fn transcribe(
        &self,
        audio: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<TranscriptResult, SttError> {
        let started = std::time::Instant::now();

        let tmp = tempfile::Builder::new()
            .prefix("apollia-stt-")
            .suffix(".wav")
            .tempfile()
            .map_err(|e| SttError::Internal(format!("create temp wav: {e}")))?;

        write_wav(tmp.path(), audio, sample_rate)?;

        let params = serde_json::json!({
            "model_id": self.model_id,
            "audio_path": tmp.path(),
            "language": language_hint,
            "task": "transcribe",
        });

        // SttEngine invoque `transcribe` depuis `spawn_blocking`, on est donc
        // dans un thread bloquant : `Handle::current().block_on` permet de
        // dispatcher l'appel async sans deadlock du runtime Tokio.
        let handle = Handle::try_current().map_err(|e| SttError::BackendUnavailable {
            backend: format!("no tokio runtime in scope: {e}"),
        })?;

        let data: Value = handle
            .block_on(self.proxy.post_json("/stt/transcribe", params))
            .map_err(|e| SttError::TranscriptionFailed {
                reason: format!("runner /stt/transcribe: {e}"),
            })?;

        let text = data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let language = data
            .get("language_detected")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let audio_duration_ms = (audio.len() as u64 * 1000) / u64::from(sample_rate.max(1));

        Ok(TranscriptResult {
            full_text: text,
            segments: Vec::new(),
            language,
            audio_duration_ms,
            processing_time_ms: started.elapsed().as_millis() as u64,
        })
    }
}

fn write_wav(path: &Path, audio: &[f32], sample_rate: u32) -> Result<(), SttError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| SttError::Internal(format!("create wav writer: {e}")))?;
    for sample in audio {
        writer
            .write_sample(*sample)
            .map_err(|e| SttError::Internal(format!("write sample: {e}")))?;
    }
    writer
        .finalize()
        .map_err(|e| SttError::Internal(format!("finalize wav: {e}")))?;
    Ok(())
}
