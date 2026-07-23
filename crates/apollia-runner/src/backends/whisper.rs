//! whisper-rs wrapper for the runner.
//!
//! Strict scope:
//!
//! - Loads a whisper GGML/GGUF file from an absolute path.
//! - Transcribes a WAV file (PCM int16 or f32) at mono 16 kHz.
//! - Output matches the IPC protocol: text + segments + detected language.
//!
//! Resampling (rubato) and stereo-to-mono downmix are inlined here so this
//! crate does not depend on `apollia-stt` (which still handles microphone
//! capture on the daemon side).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use hound::{SampleFormat, WavReader};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::ipc::{ErrorBody, ErrorCode, TranscribeData, TranscribeParams, TranscribeSegment};

/// Sample rate expected by whisper.cpp.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

fn internal(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, msg)
}
fn load_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::ModelLoadFailed, msg)
}
fn inference_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::InferenceFailed, msg)
}
fn bad_request(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::BadRequest, msg)
}

/// A loaded whisper model.
struct LoadedWhisper {
    ctx: WhisperContext,
    memory_used_mb: u32,
}

// SAFETY: `WhisperContext` is `Send + Sync` per the whisper-rs API; each
// request creates a separate `state` via `create_state()`.
unsafe impl Send for LoadedWhisper {}
unsafe impl Sync for LoadedWhisper {}

/// Backend STT in-process via whisper.cpp.
///
/// Holds a `model_id -> WhisperContext` cache shared across the axum
/// handlers via an `Arc`.
pub struct WhisperBackend {
    models: Mutex<HashMap<String, std::sync::Arc<LoadedWhisper>>>,
}

impl Default for WhisperBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WhisperBackend {
    pub fn new() -> Self {
        Self {
            models: Mutex::new(HashMap::new()),
        }
    }

    pub fn loaded_ids(&self) -> Vec<String> {
        let guard = self.models.lock().expect("whisper models lock poisoned");
        guard.keys().cloned().collect()
    }

    pub fn total_memory_mb(&self) -> u32 {
        let guard = self.models.lock().expect("whisper models lock poisoned");
        guard.values().map(|m| m.memory_used_mb).sum()
    }

    pub fn unload(&self, model_id: &str) -> bool {
        let mut guard = self.models.lock().expect("whisper models lock poisoned");
        guard.remove(model_id).is_some()
    }

    fn get(&self, model_id: &str) -> Option<std::sync::Arc<LoadedWhisper>> {
        let guard = self.models.lock().expect("whisper models lock poisoned");
        guard.get(model_id).cloned()
    }

    /// Returns the cached model, loading it on demand from `model_path`.
    ///
    /// The `model_id -> disk path` mapping is owned by the daemon, which sends
    /// the path in [`TranscribeParams`]. When the model is already cached the
    /// path is ignored; when it is not and a path is given, the runner loads it
    /// once and caches it. Without a path an uncached model is an error.
    async fn ensure_loaded(
        &self,
        model_id: &str,
        model_path: Option<&str>,
    ) -> Result<std::sync::Arc<LoadedWhisper>, ErrorBody> {
        if let Some(m) = self.get(model_id) {
            return Ok(m);
        }
        if let Some(path) = model_path {
            self.load_model(model_id.to_string(), PathBuf::from(path))
                .await?;
            if let Some(m) = self.get(model_id) {
                return Ok(m);
            }
        }
        Err(ErrorBody::new(
            ErrorCode::ModelNotLoaded,
            format!("whisper model '{model_id}' not loaded"),
        ))
    }

    /// Explicitly loads a whisper GGML/GGUF model from a disk path.
    ///
    /// Called on demand by [`ensure_loaded`](Self::ensure_loaded) when the
    /// daemon sends a `model_path`, and directly by integration tests.
    pub async fn load_model(&self, model_id: String, model_path: PathBuf) -> Result<(), ErrorBody> {
        if !model_path.exists() {
            return Err(bad_request(format!(
                "whisper model not found: {}",
                model_path.display()
            )));
        }

        let memory_used_mb = std::fs::metadata(&model_path)
            .map(|m| (m.len() / (1024 * 1024)) as u32)
            .unwrap_or(0);

        let path_str = model_path
            .to_str()
            .ok_or_else(|| bad_request("model_path is not valid UTF-8"))?
            .to_string();

        let ctx = tokio::task::spawn_blocking(move || {
            WhisperContext::new_with_params(&path_str, WhisperContextParameters::default())
                .map_err(|e| load_failed(format!("whisper model load failed: {e}")))
        })
        .await
        .map_err(|e| internal(format!("whisper load task panicked: {e}")))??;

        let mut guard = self.models.lock().expect("whisper models lock poisoned");
        guard.insert(
            model_id,
            std::sync::Arc::new(LoadedWhisper {
                ctx,
                memory_used_mb,
            }),
        );
        Ok(())
    }

    /// Transcribes a WAV file (`POST /stt/transcribe`).
    pub async fn transcribe(&self, params: TranscribeParams) -> Result<TranscribeData, ErrorBody> {
        let entry = self
            .ensure_loaded(&params.model_id, params.model_path.as_deref())
            .await?;
        let audio_path = params.audio_path.clone();
        let language = params.language.clone();
        let translate = params.task.eq_ignore_ascii_case("translate");

        let started = Instant::now();

        let entry_clone = entry.clone();
        let result = tokio::task::spawn_blocking(move || {
            let audio = load_and_resample_wav(&audio_path)?;
            run_whisper(&entry_clone.ctx, &audio, language.as_deref(), translate)
        })
        .await
        .map_err(|e| internal(format!("transcribe task panicked: {e}")))??;

        let timing_ms = started.elapsed().as_millis() as u64;
        Ok(TranscribeData {
            text: result.text,
            segments: result.segments,
            language_detected: result.language_detected,
            timing_ms,
        })
    }
}

/// Loads a WAV from disk and normalizes it to mono 16 kHz f32.
fn load_and_resample_wav(path: &Path) -> Result<Vec<f32>, ErrorBody> {
    if !path.exists() {
        return Err(bad_request(format!(
            "audio file not found: {}",
            path.display()
        )));
    }

    let reader = WavReader::open(path)
        .map_err(|e| bad_request(format!("failed to open WAV {}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels;
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Float, 32) => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| inference_failed(format!("WAV f32 decode failed: {e}")))?,
        (SampleFormat::Int, 16) => reader
            .into_samples::<i16>()
            .map(|r| r.map(|s| s as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| inference_failed(format!("WAV i16 decode failed: {e}")))?,
        (fmt, bits) => {
            return Err(bad_request(format!(
                "unsupported WAV format: {fmt:?} {bits}-bit"
            )));
        }
    };

    if samples.is_empty() {
        return Err(bad_request("audio buffer is empty"));
    }
    if channels == 0 {
        return Err(bad_request("WAV has 0 channels"));
    }

    let mono = if channels == 1 {
        samples
    } else {
        mix_to_mono(&samples, channels)
    };

    if sample_rate == WHISPER_SAMPLE_RATE {
        Ok(mono)
    } else {
        resample_to_16k(&mono, sample_rate)
    }
}

fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample_to_16k(mono: &[f32], from_sr: u32) -> Result<Vec<f32>, ErrorBody> {
    let ratio = f64::from(WHISPER_SAMPLE_RATE) / f64::from(from_sr);
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk_size = mono.len();
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk_size, 1)
        .map_err(|e| inference_failed(format!("resampler init failed: {e}")))?;

    let input = vec![mono.to_vec()];
    let output = resampler
        .process(&input, None)
        .map_err(|e| inference_failed(format!("resampling failed: {e}")))?;
    output
        .into_iter()
        .next()
        .ok_or_else(|| inference_failed("resampler produced no output channels"))
}

struct WhisperRun {
    text: String,
    segments: Vec<TranscribeSegment>,
    language_detected: Option<String>,
}

fn run_whisper(
    ctx: &WhisperContext,
    audio: &[f32],
    language: Option<&str>,
    translate: bool,
) -> Result<WhisperRun, ErrorBody> {
    if audio.is_empty() {
        return Err(bad_request("empty audio buffer"));
    }

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_translate(translate);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    if let Some(lang) = language {
        params.set_language(Some(lang));
    }

    let mut state = ctx
        .create_state()
        .map_err(|e| inference_failed(format!("whisper state creation failed: {e}")))?;
    state
        .full(params, audio)
        .map_err(|e| inference_failed(format!("whisper transcribe failed: {e}")))?;

    let num_segments = state.full_n_segments();
    let mut segments = Vec::with_capacity(num_segments as usize);
    let mut full_text = String::new();

    for i in 0..num_segments {
        let seg = state
            .get_segment(i)
            .ok_or_else(|| inference_failed(format!("segment {i} out of bounds")))?;
        let text = seg
            .to_str_lossy()
            .map_err(|e| inference_failed(format!("segment {i} text decode failed: {e}")))?
            .into_owned();
        let t0 = seg.start_timestamp().max(0) as f32 * 0.01;
        let t1 = seg.end_timestamp().max(0) as f32 * 0.01;

        full_text.push_str(&text);
        segments.push(TranscribeSegment {
            start: t0,
            end: t1,
            text,
        });
    }

    let lang_id = state.full_lang_id_from_state();
    let language_detected = whisper_rs::get_lang_str(lang_id).map(|s| s.to_owned());

    Ok(WhisperRun {
        text: full_text,
        segments,
        language_detected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_to_mono_averages_channels() {
        let stereo = vec![0.0, 1.0, 0.5, -0.5];
        let mono = mix_to_mono(&stereo, 2);
        assert_eq!(mono, vec![0.5, 0.0]);
    }

    #[test]
    fn loaded_ids_starts_empty() {
        let b = WhisperBackend::new();
        assert!(b.loaded_ids().is_empty());
    }
}
