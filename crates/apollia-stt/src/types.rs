//! Result and error types for the STT engine.
//!
//! Defines [`TranscriptResult`], [`TranscriptSegment`] and [`SttError`]
//! used by all backends through the [`super::SttBackend`] trait.

use serde::{Deserialize, Serialize};

/// Complete result of an audio transcription.
///
/// Holds the full text, time-aligned segments, the detected language,
/// and performance metrics (audio duration and processing time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptResult {
    /// Full transcribed text (concatenation of all segments).
    pub full_text: String,

    /// Individual time-aligned segments with timestamps and confidence.
    pub segments: Vec<TranscriptSegment>,

    /// Language detected or used for the transcription (ISO 639-1 code).
    pub language: Option<String>,

    /// Duration of the source audio in milliseconds.
    pub audio_duration_ms: u64,

    /// Transcription processing time in milliseconds.
    pub processing_time_ms: u64,
}

/// Individual transcription segment with timestamps.
///
/// Each segment represents a continuous portion of speech
/// with its time bounds and an optional confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Transcribed text for this segment.
    pub text: String,

    /// Segment start in milliseconds from the beginning of the audio.
    pub start_ms: u64,

    /// Segment end in milliseconds from the beginning of the audio.
    pub end_ms: u64,

    /// Backend confidence score for this segment (0.0 to 1.0).
    ///
    /// `None` means the backend provides no confidence score, which must
    /// not be confused with `Some(0.0)` that would mean "zero confidence".
    /// For example, whisper.cpp does not report this metric per segment.
    pub confidence: Option<f32>,
}

/// Errors from the STT engine.
///
/// Covers all possible error cases during model loading,
/// transcription, and backend management.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SttError {
    /// The specified model file could not be found.
    #[error("STT model not found: {path}")]
    ModelNotFound {
        /// Path of the model that was looked up.
        path: String,
    },

    /// Loading the model failed (invalid format, corruption, etc.).
    #[error("failed to load STT model: {reason}")]
    ModelLoadFailed {
        /// Description of the load error.
        reason: String,
    },

    /// Transcription failed after processing had started.
    #[error("transcription failed: {reason}")]
    TranscriptionFailed {
        /// Description of the transcription error.
        reason: String,
    },

    /// The provided audio data is invalid (wrong format, empty, etc.).
    #[error("invalid audio data: {reason}")]
    InvalidAudio {
        /// Description of the problem with the audio data.
        reason: String,
    },

    /// No audio input device (microphone) is available on the host.
    ///
    /// Distinct from [`SttError::InvalidAudio`]: the machine has no capture
    /// device at all, so the UI can surface a clear "no microphone" state
    /// instead of a generic audio failure.
    #[error("no audio input device available")]
    NoInputDevice,

    /// The transcription database carries a version this binary does not know.
    #[error(
        "stt database schema version {found} on disk is newer than the supported version {supported}; refusing to open"
    )]
    NewerThanBinary {
        /// Version read from the `_schema_version` table.
        found: u32,
        /// Highest version this binary supports.
        supported: u32,
    },

    /// The requested STT backend is not available (feature not enabled, etc.).
    #[error("STT backend unavailable: {backend}")]
    BackendUnavailable {
        /// Name of the unavailable backend.
        backend: String,
    },

    /// Transcription exceeded the maximum allowed time.
    #[error("STT operation timed out after {timeout_ms}ms")]
    Timeout {
        /// Exceeded timeout in milliseconds.
        timeout_ms: u64,
    },

    /// Unexpected internal error.
    #[error("internal STT error: {0}")]
    Internal(String),

    /// SQLite repository error (open, migration, query).
    #[error("STT repository error: {reason}")]
    Repository {
        /// Description of the repository error.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN each variant of SttError
    // WHEN Display is called
    // THEN the message contains the expected context
    #[test]
    fn stt_error_display_contains_context() {
        let err = SttError::ModelNotFound {
            path: "/tmp/model.bin".to_owned(),
        };
        assert!(
            err.to_string().contains("/tmp/model.bin"),
            "ModelNotFound display should contain path"
        );

        let err = SttError::ModelLoadFailed {
            reason: "corrupt header".to_owned(),
        };
        assert!(
            err.to_string().contains("corrupt header"),
            "ModelLoadFailed display should contain reason"
        );

        let err = SttError::TranscriptionFailed {
            reason: "out of memory".to_owned(),
        };
        assert!(
            err.to_string().contains("out of memory"),
            "TranscriptionFailed display should contain reason"
        );

        let err = SttError::InvalidAudio {
            reason: "empty buffer".to_owned(),
        };
        assert!(
            err.to_string().contains("empty buffer"),
            "InvalidAudio display should contain reason"
        );

        let err = SttError::BackendUnavailable {
            backend: "whisper-cpp".to_owned(),
        };
        assert!(
            err.to_string().contains("whisper-cpp"),
            "BackendUnavailable display should contain backend name"
        );

        let err = SttError::Timeout { timeout_ms: 5000 };
        assert!(
            err.to_string().contains("5000"),
            "Timeout display should contain timeout_ms"
        );

        let err = SttError::Internal("unexpected".to_owned());
        assert!(
            err.to_string().contains("unexpected"),
            "Internal display should contain message"
        );
    }

    // GIVEN a TranscriptSegment with confidence = None
    // WHEN serialized to JSON
    // THEN the confidence field is `null`, not `0.0`
    #[test]
    fn test_confidence_none_serialization() {
        // GIVEN
        let seg = TranscriptSegment {
            text: "hello".to_owned(),
            start_ms: 0,
            end_ms: 500,
            confidence: None,
        };

        // WHEN
        let json = serde_json::to_string(&seg).expect("serialization should succeed");

        // THEN
        assert!(
            json.contains("\"confidence\":null"),
            "None confidence must serialize as null, got: {json}"
        );
        assert!(
            !json.contains("0.0"),
            "None confidence must not serialize as 0.0, got: {json}"
        );

        let deserialized: TranscriptSegment =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert!(
            deserialized.confidence.is_none(),
            "deserialized confidence should be None"
        );
    }

    // GIVEN a TranscriptSegment with confidence = Some(0.95)
    // WHEN serialized to JSON and deserialized
    // THEN the score is preserved exactly
    #[test]
    fn test_transcript_segment_confidence_option_some() {
        // GIVEN
        let seg = TranscriptSegment {
            text: "world".to_owned(),
            start_ms: 100,
            end_ms: 800,
            confidence: Some(0.95),
        };

        // WHEN
        let json = serde_json::to_string(&seg).expect("serialization should succeed");
        let deserialized: TranscriptSegment =
            serde_json::from_str(&json).expect("deserialization should succeed");

        // THEN
        let score = deserialized
            .confidence
            .expect("confidence should be Some after round-trip");
        assert!(
            (score - 0.95).abs() < f32::EPSILON,
            "confidence score should be preserved, got: {score}"
        );
    }

    // GIVEN a TranscriptResult
    // WHEN serialized to JSON and deserialized back
    // THEN the round-trip is faithful
    #[test]
    fn transcript_result_serde_roundtrip() {
        let result = TranscriptResult {
            full_text: "Bonjour le monde".to_owned(),
            segments: vec![TranscriptSegment {
                text: "Bonjour le monde".to_owned(),
                start_ms: 0,
                end_ms: 1500,
                confidence: Some(0.95),
            }],
            language: Some("fr".to_owned()),
            audio_duration_ms: 2000,
            processing_time_ms: 350,
        };

        let json = serde_json::to_string(&result).expect("serialize should succeed");
        let deserialized: TranscriptResult =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(deserialized.full_text, "Bonjour le monde");
        assert_eq!(deserialized.segments.len(), 1);
        assert_eq!(deserialized.segments[0].start_ms, 0);
        assert_eq!(deserialized.segments[0].end_ms, 1500);
        assert_eq!(deserialized.language.as_deref(), Some("fr"));
        assert_eq!(deserialized.audio_duration_ms, 2000);
        assert_eq!(deserialized.processing_time_ms, 350);
    }
}
