//! `RunnerSttBackend`: adapts `SttBackend` (apollia-stt) onto the
//! [`RunnerProxy`] via HTTP/JSON IPC.
//!
//! Routes all STT through the runner sidecar.

use std::path::Path;

use apollia_stt::{SttBackend, SttError, TranscriptResult};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::Value;
use tokio::runtime::Handle;

use super::proxy::RunnerProxy;

/// `SttBackend` that routes calls to the runner sidecar.
///
/// `SttBackend::transcribe` is synchronous and receives a PCM buffer. The HTTP
/// proxy is async and the `/stt/transcribe` endpoint expects a file path.
/// So we write the buffer to a temporary WAV, call the runner, then clean up
/// (the `tempfile` is dropped at end of scope).
pub struct RunnerSttBackend {
    proxy: RunnerProxy,
    model_id: String,
    /// Absolute path to the whisper model, sent with each transcribe so the
    /// runner loads it on demand (the runner has no separate load endpoint).
    model_path: String,
}

impl RunnerSttBackend {
    pub fn new(proxy: RunnerProxy, model_id: String, model_path: String) -> Self {
        Self {
            proxy,
            model_id,
            model_path,
        }
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
            "model_path": self.model_path,
            "audio_path": tmp.path(),
            "language": language_hint,
            "task": "transcribe",
        });

        // SttEngine calls `transcribe` from `spawn_blocking`, so we are on a
        // blocking thread: `Handle::current().block_on` dispatches the async
        // call without deadlocking the Tokio runtime.
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
