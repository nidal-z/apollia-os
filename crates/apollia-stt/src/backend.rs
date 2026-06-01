//! Trait [`SttBackend`] - interface commune pour tous les moteurs STT.
//!
//! Le trait est **object-safe** (`Box<dyn SttBackend>`) et **synchrone** :
//! l'appelant est responsable de wrapper les appels dans `spawn_blocking`
//! pour ne pas bloquer le runtime Tokio.

use crate::types::{SttError, TranscriptResult};

/// Interface commune pour les backends de transcription audio.
///
/// Implémenté par chaque moteur STT (whisper.cpp, candle-whisper, etc.).
/// Le trait est `Send + Sync` pour permettre l'utilisation derrière un `Arc`
/// et le passage entre tâches Tokio via `spawn_blocking`.
///
/// Les méthodes sont **synchrones** : l'inférence STT est CPU/GPU-bound,
/// l'appelant utilise `tokio::task::spawn_blocking` pour l'exécuter.
pub trait SttBackend: Send + Sync {
    /// Nom lisible du backend (ex: `"whisper-cpp"`, `"candle-whisper"`).
    fn name(&self) -> &str;

    /// Transcrit un buffer audio PCM f32 mono.
    ///
    /// # Arguments
    ///
    /// * `audio` - Échantillons PCM f32 normalisés [-1.0, 1.0], mono.
    /// * `sample_rate` - Fréquence d'échantillonnage en Hz (typiquement 16000).
    /// * `language_hint` - Code langue ISO 639-1 optionnel (ex: `"fr"`, `"en"`).
    ///
    /// # Erreurs
    ///
    /// Retourne [`SttError`] si la transcription échoue.
    fn transcribe(
        &self,
        audio: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<TranscriptResult, SttError>;

    /// Détecte la langue dominante dans un buffer audio.
    ///
    /// Implémentation par défaut : retourne `Ok(None)` (détection non supportée).
    /// Les backends qui supportent la détection de langue overrident cette méthode.
    fn detect_language(&self, audio: &[f32]) -> Result<Option<String>, SttError> {
        let _ = audio;
        Ok(None)
    }

    /// Libère les ressources du backend (modèle GPU, mémoire, etc.).
    ///
    /// Implémentation par défaut : no-op. Les backends avec des ressources lourdes
    /// (GPU memory, mmap) overrident cette méthode.
    fn unload(&self) {}
}
