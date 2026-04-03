//! Types de résultat et d'erreur pour le moteur STT.
//!
//! Définit [`TranscriptResult`], [`TranscriptSegment`] et [`SttError`]
//! utilisés par tous les backends via le trait [`super::SttBackend`].

use serde::{Deserialize, Serialize};

/// Résultat complet d'une transcription audio.
///
/// Contient le texte intégral, les segments temporels, la langue détectée,
/// et les métriques de performance (durée audio / temps de traitement).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptResult {
    /// Texte complet transcrit (concaténation de tous les segments).
    pub full_text: String,

    /// Segments temporels individuels avec timestamps et confiance.
    pub segments: Vec<TranscriptSegment>,

    /// Langue détectée ou utilisée pour la transcription (code ISO 639-1).
    pub language: Option<String>,

    /// Durée de l'audio source en millisecondes.
    pub audio_duration_ms: u64,

    /// Temps de traitement de la transcription en millisecondes.
    pub processing_time_ms: u64,
}

/// Segment individuel d'une transcription avec timestamps.
///
/// Chaque segment représente une portion continue de parole
/// avec ses bornes temporelles et un score de confiance optionnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Texte transcrit pour ce segment.
    pub text: String,

    /// Début du segment en millisecondes depuis le début de l'audio.
    pub start_ms: u64,

    /// Fin du segment en millisecondes depuis le début de l'audio.
    pub end_ms: u64,

    /// Score de confiance du backend pour ce segment (0.0 – 1.0).
    ///
    /// `None` signifie que le backend ne fournit pas de score de confiance —
    /// à ne pas confondre avec `Some(0.0)` qui signifierait "confiance nulle".
    /// Par exemple, whisper.cpp ne remonte pas cette métrique par segment.
    pub confidence: Option<f32>,
}

/// Erreurs du moteur STT.
///
/// Couvre l'ensemble des cas d'erreur possibles lors du chargement
/// de modèle, de la transcription, et de la gestion des backends.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    /// Le fichier modèle spécifié est introuvable.
    #[error("STT model not found: {path}")]
    ModelNotFound {
        /// Chemin du modèle recherché.
        path: String,
    },

    /// Le chargement du modèle a échoué (format invalide, corruption, etc.).
    #[error("failed to load STT model: {reason}")]
    ModelLoadFailed {
        /// Description de l'erreur de chargement.
        reason: String,
    },

    /// La transcription a échoué après le démarrage du traitement.
    #[error("transcription failed: {reason}")]
    TranscriptionFailed {
        /// Description de l'erreur de transcription.
        reason: String,
    },

    /// Les données audio fournies sont invalides (mauvais format, vides, etc.).
    #[error("invalid audio data: {reason}")]
    InvalidAudio {
        /// Description du problème avec les données audio.
        reason: String,
    },

    /// Le backend STT demandé n'est pas disponible (feature non activée, etc.).
    #[error("STT backend unavailable: {backend}")]
    BackendUnavailable {
        /// Nom du backend indisponible.
        backend: String,
    },

    /// La transcription a dépassé le délai maximum autorisé.
    #[error("STT operation timed out after {timeout_ms}ms")]
    Timeout {
        /// Délai dépassé en millisecondes.
        timeout_ms: u64,
    },

    /// Erreur interne inattendue.
    #[error("internal STT error: {0}")]
    Internal(String),

    /// Erreur du repository SQLite (ouverture, migration, requête).
    #[error("STT repository error: {reason}")]
    Repository {
        /// Description de l'erreur du repository.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN chaque variant de SttError
    // WHEN on appelle Display
    // THEN le message contient le contexte attendu
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

    // GIVEN un TranscriptResult
    // WHEN on le sérialise en JSON puis le désérialise
    // THEN le round-trip est fidèle
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
