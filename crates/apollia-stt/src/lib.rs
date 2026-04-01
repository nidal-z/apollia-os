//! Apollia OS — moteur Speech-to-Text embarqué.
//!
//! Cette crate fournit :
//! - Le trait [`SttBackend`] (object-safe, `Send + Sync`) pour abstraire les moteurs STT.
//! - Les types [`TranscriptResult`], [`TranscriptSegment`] pour les résultats de transcription.
//! - L'enum [`SttError`] (7 variants, `thiserror`) pour les erreurs STT.
//!
//! Les backends concrets (whisper.cpp, etc.) sont activés via feature flags :
//! - `stt-whisper-cpp` (défaut) — backend whisper.cpp via `whisper-rs`
//! - `stt-metal` — accélération Metal (Apple Silicon)
//! - `stt-cuda` — accélération CUDA (NVIDIA)

pub mod audio;
pub mod backend;
pub mod repository;
pub mod types;
#[cfg(feature = "stt-whisper-cpp")]
pub mod whisper_cpp;

pub use audio::{to_whisper_format, trim_silence, AudioCapture, CaptureBuffer};
pub use backend::SttBackend;
pub use repository::{SttRepository, TranscriptRow};
pub use types::{SttError, TranscriptResult, TranscriptSegment};
#[cfg(feature = "stt-whisper-cpp")]
pub use whisper_cpp::WhisperCppBackend;

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN le trait SttBackend
    // WHEN on crée un Box<dyn SttBackend>
    // THEN il compile — le trait est object-safe
    #[test]
    fn stt_backend_is_object_safe() {
        struct Dummy;

        impl SttBackend for Dummy {
            fn name(&self) -> &str {
                "dummy"
            }

            fn transcribe(
                &self,
                _audio: &[f32],
                _sample_rate: u32,
                _language_hint: Option<&str>,
            ) -> Result<TranscriptResult, SttError> {
                Ok(TranscriptResult {
                    full_text: String::new(),
                    segments: vec![],
                    language: None,
                    audio_duration_ms: 0,
                    processing_time_ms: 0,
                })
            }
        }

        let backend: Box<dyn SttBackend> = Box::new(Dummy);
        assert_eq!(backend.name(), "dummy");

        let lang = backend.detect_language(&[]);
        assert!(lang.is_ok());
        assert!(lang.unwrap().is_none());
    }
}
