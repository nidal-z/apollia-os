//! Configuration A2A du runtime Apollia OS.
//!
//! Définit [`A2AConfig`] correspondant à la section `[a2a]` dans `apollia.toml`.
//! Tous les champs ont des valeurs par défaut saines via [`Default`].

use serde::{Deserialize, Serialize};

/// Configuration du routing A2A appliquée par le runtime.
///
/// Contrôle les trois garde-fous automatiques déclenchés lors des invocations
/// inter-agents : profondeur de récursivité, timeout par invocation,
/// et timeout cumulé de la chaîne.
///
/// Les valeurs par défaut sont conçues pour la majorité des cas d'usage :
/// `max_depth = 3`, `invocation_timeout_secs = 120`, `chain_timeout_secs = 300`.
/// Tous les champs peuvent être surchargés dans `apollia.toml` sous `[a2a]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AConfig {
    /// Profondeur maximale de récursivité A2A autorisée.
    ///
    /// Une valeur de `3` signifie qu'une chaîne peut atteindre trois niveaux
    /// d'imbrication avant d'être bloquée. La vérification est appliquée
    /// par le runtime avant chaque invocation, non contournable côté agent.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Timeout par invocation A2A individuelle, en secondes.
    ///
    /// Appliqué à chaque appel `invoke()` indépendamment de la chaîne globale.
    /// Une invocation dépassant ce délai est annulée.
    #[serde(default = "default_invocation_timeout")]
    pub invocation_timeout_secs: u64,

    /// Timeout cumulé de la chaîne A2A complète, en secondes.
    ///
    /// Initialisé à la première invocation d'une chaîne (`chain_deadline = None`).
    /// Le délai résiduel est utilisé comme borne supérieure pour toutes les
    /// invocations suivantes dans la même chaîne, empêchant les chaînes longues
    /// de monopoliser les ressources au-delà de ce budget total.
    #[serde(default = "default_chain_timeout")]
    pub chain_timeout_secs: u64,
}

impl Default for A2AConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            invocation_timeout_secs: default_invocation_timeout(),
            chain_timeout_secs: default_chain_timeout(),
        }
    }
}

fn default_max_depth() -> u32 {
    3
}

fn default_invocation_timeout() -> u64 {
    120
}

fn default_chain_timeout() -> u64 {
    300
}
