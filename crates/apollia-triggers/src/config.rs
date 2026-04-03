//! Configuration du file watcher.
//!
//! Ce module expose [`FileWatchConfig`] pour la section `[triggers.file_watch]`
//! dans `apollia.toml`, ainsi que les valeurs par défaut des patterns d'exclusion
//! utilisées par [`crate::types::TriggerSourceConfig::FileWatch`].

use std::path::PathBuf;

use serde::Deserialize;

/// Configuration globale du file watcher, correspondant à `[triggers.file_watch]`
/// dans `apollia.toml`.
///
/// Ces paramètres s'appliquent à toutes les sources `FileWatch` qui ne les
/// surchargent pas individuellement via leur propre définition de trigger.
#[derive(Debug, Clone, Deserialize)]
pub struct FileWatchConfig {
    /// Répertoires racine à surveiller.
    pub watch_paths: Vec<PathBuf>,

    /// Suivre les liens symboliques lors de la surveillance (défaut : `false`).
    ///
    /// Lorsque `false`, les événements dont le chemin résolu est un lien symbolique
    /// sont silencieusement ignorés avant propagation vers le `TriggerEngine`.
    #[serde(default)]
    pub follow_symlinks: bool,

    /// Segments de chemin ou patterns fichiers à exclure des événements.
    ///
    /// Patterns supportés :
    /// - `"nom"` ou `"nom/"` — exclut tout chemin contenant le segment `nom`
    /// - `"*.ext"` — exclut tout fichier dont le nom se termine par `.ext`
    ///
    /// Défaut : `[".git", "node_modules", "__pycache__", ".apollia"]`.
    #[serde(default = "default_exclude_patterns")]
    pub exclude_patterns: Vec<String>,
}

/// Retourne les patterns d'exclusion appliqués par défaut à toutes les sources `FileWatch`.
///
/// Liste : `.git`, `node_modules`, `__pycache__`, `.apollia`.
pub(crate) fn default_exclude_patterns() -> Vec<String> {
    vec![
        ".git".into(),
        "node_modules".into(),
        "__pycache__".into(),
        ".apollia".into(),
    ]
}
