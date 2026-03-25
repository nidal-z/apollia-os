//! Configuration STT pour Apollia OS.
//!
//! Définit [`SttConfig`] correspondant à la section `[stt]` dans `apollia.toml`.
//! Tous les champs ont des valeurs par défaut via [`Default`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration de la section `[stt]` dans `apollia.toml`.
///
/// Contrôle le moteur Speech-to-Text embarqué : activation, chemin du modèle,
/// raccourci clavier, mode clipboard, seuils audio, et langue cible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    /// Active ou désactive le moteur STT au démarrage.
    #[serde(default)]
    pub enabled: bool,

    /// Chemin vers le fichier modèle GGML (supporte `~` résolu par l'appelant).
    #[serde(default = "default_model_path")]
    pub model_path: PathBuf,

    /// Raccourci clavier global pour déclencher l'enregistrement.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Mode d'injection du texte transcrit : `"paste"` ou `"clipboard"`.
    ///
    /// - `"paste"` : copie dans le clipboard puis simule Cmd/Ctrl+V.
    /// - `"clipboard"` : copie dans le clipboard sans coller automatiquement.
    #[serde(default = "default_clipboard_mode")]
    pub clipboard_mode: String,

    /// Restaure le contenu précédent du clipboard après injection.
    #[serde(default = "default_clipboard_restore")]
    pub clipboard_restore: bool,

    /// Seuil de silence en dB RMS pour la détection de fin de parole.
    #[serde(default = "default_silence_threshold_db")]
    pub silence_threshold_db: f32,

    /// Durée maximale d'enregistrement en secondes.
    #[serde(default = "default_max_recording_sec")]
    pub max_recording_sec: u32,

    /// Code langue ISO 639-1 pour la transcription (`None` = détection automatique).
    #[serde(default = "default_language")]
    pub language: Option<String>,

    /// Mode de déclenchement : `"toggle"` (appui/relâche) ou `"push-to-talk"` (maintenir).
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: default_model_path(),
            hotkey: default_hotkey(),
            clipboard_mode: default_clipboard_mode(),
            clipboard_restore: default_clipboard_restore(),
            silence_threshold_db: default_silence_threshold_db(),
            max_recording_sec: default_max_recording_sec(),
            language: default_language(),
            trigger_mode: default_trigger_mode(),
        }
    }
}

fn default_model_path() -> PathBuf {
    PathBuf::from("~/.apollia/models/whisper-large-v3-fr-q5_0.bin")
}

fn default_hotkey() -> String {
    "ctrl+shift+space".to_owned()
}

fn default_clipboard_mode() -> String {
    "paste".to_owned()
}

fn default_clipboard_restore() -> bool {
    true
}

fn default_silence_threshold_db() -> f32 {
    -40.0
}

fn default_max_recording_sec() -> u32 {
    60
}

fn default_language() -> Option<String> {
    Some("fr".to_owned())
}

fn default_trigger_mode() -> String {
    "toggle".to_owned()
}
