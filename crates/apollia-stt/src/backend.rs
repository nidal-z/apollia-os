//! Trait [`SttBackend`]: common interface for all STT engines.
//!
//! The trait is **object-safe** (`Box<dyn SttBackend>`) and **synchronous**:
//! the caller is responsible for wrapping calls in `spawn_blocking`
//! so the Tokio runtime is not blocked.

use crate::types::{SttError, TranscriptResult};

/// Common interface for audio transcription backends.
///
/// Implemented by each STT engine (whisper.cpp, candle-whisper, etc.).
/// The trait is `Send + Sync` so it can be used behind an `Arc`
/// and passed between Tokio tasks via `spawn_blocking`.
///
/// The methods are **synchronous**: STT inference is CPU/GPU-bound, so the
/// caller runs them through `tokio::task::spawn_blocking`.
pub trait SttBackend: Send + Sync {
    /// Human-readable backend name (e.g. `"whisper-cpp"`, `"candle-whisper"`).
    fn name(&self) -> &str;

    /// Transcribes a mono f32 PCM audio buffer.
    ///
    /// # Arguments
    ///
    /// * `audio` - f32 PCM samples normalized to [-1.0, 1.0], mono.
    /// * `sample_rate` - Sample rate in Hz (typically 16000).
    /// * `language_hint` - Optional ISO 639-1 language code (e.g. `"fr"`, `"en"`).
    ///
    /// # Errors
    ///
    /// Returns [`SttError`] if transcription fails.
    fn transcribe(
        &self,
        audio: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<TranscriptResult, SttError>;

    /// Detects the dominant language in an audio buffer.
    ///
    /// Default implementation: returns `Ok(None)` (detection unsupported).
    /// Backends that support language detection override this method.
    fn detect_language(&self, audio: &[f32]) -> Result<Option<String>, SttError> {
        let _ = audio;
        Ok(None)
    }

    /// Releases the backend's resources (GPU model, memory, etc.).
    ///
    /// Default implementation: no-op. Backends holding heavy resources
    /// (GPU memory, mmap) override this method.
    fn unload(&self) {}
}
