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

pub mod backend;
pub mod types;
#[cfg(feature = "stt-whisper-cpp")]
pub mod whisper_cpp;

pub use backend::SttBackend;
pub use types::{SttError, TranscriptResult, TranscriptSegment};
#[cfg(feature = "stt-whisper-cpp")]
pub use whisper_cpp::WhisperCppBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SttConfig;
    use std::path::PathBuf;

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

    // GIVEN SttConfig::default()
    // WHEN on inspecte les valeurs
    // THEN elles correspondent aux valeurs spécifiées
    #[test]
    fn stt_config_default_values() {
        let cfg = SttConfig::default();

        assert!(!cfg.enabled);
        assert_eq!(
            cfg.model_path,
            PathBuf::from("~/.apollia/models/whisper-large-v3-fr-q5_0.bin")
        );
        assert_eq!(cfg.hotkey, "ctrl+shift+space");
        assert_eq!(cfg.clipboard_mode, "paste");
        assert!(cfg.clipboard_restore);
        assert!((cfg.silence_threshold_db - (-40.0)).abs() < f32::EPSILON);
        assert_eq!(cfg.max_recording_sec, 60);
        assert_eq!(cfg.language.as_deref(), Some("fr"));
        assert_eq!(cfg.trigger_mode, "toggle");
    }

    // GIVEN un TOML contenant [stt] avec des valeurs personnalisées
    // WHEN on le désérialise en SttConfig
    // THEN les valeurs sont correctement parsées
    #[test]
    fn stt_config_toml_parsing() {
        let toml_str = r#"
enabled = true
model_path = "/opt/models/whisper-base.bin"
hotkey = "alt+space"
clipboard_mode = "clipboard"
clipboard_restore = false
silence_threshold_db = -35.0
max_recording_sec = 120
language = "en"
trigger_mode = "push-to-talk"
"#;
        let cfg: SttConfig = toml::from_str(toml_str).expect("TOML parsing should succeed");

        assert!(cfg.enabled);
        assert_eq!(
            cfg.model_path,
            PathBuf::from("/opt/models/whisper-base.bin")
        );
        assert_eq!(cfg.hotkey, "alt+space");
        assert_eq!(cfg.clipboard_mode, "clipboard");
        assert!(!cfg.clipboard_restore);
        assert!((cfg.silence_threshold_db - (-35.0)).abs() < f32::EPSILON);
        assert_eq!(cfg.max_recording_sec, 120);
        assert_eq!(cfg.language.as_deref(), Some("en"));
        assert_eq!(cfg.trigger_mode, "push-to-talk");
    }

    // GIVEN un TOML vide
    // WHEN on le désérialise en SttConfig
    // THEN les valeurs par défaut sont utilisées
    #[test]
    fn stt_config_toml_defaults() {
        let cfg: SttConfig = toml::from_str("").expect("empty TOML should parse");

        assert!(!cfg.enabled);
        assert_eq!(cfg.hotkey, "ctrl+shift+space");
        assert_eq!(cfg.trigger_mode, "toggle");
    }
}
