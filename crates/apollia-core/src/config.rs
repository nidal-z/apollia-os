//! Configuration du runtime Apollia OS.
//!
//! Définit les sections de configuration lues depuis `apollia.toml` :
//! - [`A2AConfig`] — section `[a2a]` pour le routing inter-agents.
//! - [`ORIAConfig`] — section `[oria]` pour le moteur Observer-Reasoner-Actor.
//!
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

// ─────────────────────────────────────────────
// ORIAConfig
// ─────────────────────────────────────────────

/// Erreur de validation de la configuration au démarrage.
///
/// Produite par les méthodes `validate()` des configs de section.
/// Le runtime doit traiter ces erreurs comme des erreurs fatales (Principe #4 — Fail fast).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Une valeur de configuration est hors des bornes acceptables.
    #[error("invalid configuration value for '{field}': {reason}")]
    InvalidValue {
        /// Chemin du champ en notation pointée, par exemple `"oria.max_replans"`.
        field: String,
        /// Description lisible de la contrainte non respectée.
        reason: String,
    },
}

/// Configuration du moteur ORIA (Observer-Reasoner-Actor).
///
/// Correspond à la section `[oria]` dans `apollia.toml`.
/// Tous les champs ont des valeurs par défaut saines via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct ORIAConfig {
    /// Nombre maximal de replans autorisés par exécution orchestrée.
    ///
    /// Contrôle combien de fois l'agent peut re-planifier suite à un échec
    /// ou un changement de contexte. Validé au démarrage : doit être compris
    /// entre 0 et 10 inclus.
    ///
    /// - `0` : aucun replan autorisé — la tâche échoue au premier plan raté.
    /// - `2` : valeur par défaut (comportement historique).
    /// - `10` : borne haute acceptée.
    #[serde(default = "default_max_replans")]
    pub max_replans: u32,
}

impl Default for ORIAConfig {
    fn default() -> Self {
        Self {
            max_replans: default_max_replans(),
        }
    }
}

impl ORIAConfig {
    /// Valide la configuration ORIA au démarrage (Principe #4 — Fail fast).
    ///
    /// Retourne une erreur si `max_replans` est supérieur à 10.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_replans > 10 {
            return Err(ConfigError::InvalidValue {
                field: "oria.max_replans".into(),
                reason: "must be between 0 and 10".into(),
            });
        }
        Ok(())
    }
}

fn default_max_replans() -> u32 {
    2
}

// ─────────────────────────────────────────────
// ApiConfig
// ─────────────────────────────────────────────

/// Configuration de l'API REST locale (section `[api]` dans `apollia.toml`).
///
/// Contrôle le binding TCP et l'authentification par token statique.
/// Le socket Unix reste non authentifié — seul le propriétaire du fichier socket y accède.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    /// Adresse IP sur laquelle binder le listener TCP.
    ///
    /// Défaut : `"127.0.0.1"` — loopback uniquement, inaccessible depuis le réseau.
    #[serde(default = "default_api_bind")]
    pub bind: String,

    /// Port TCP du serveur REST.
    ///
    /// Défaut : `7771`.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Exiger un token Bearer sur toutes les connexions TCP entrantes.
    ///
    /// Quand `true` (défaut), chaque requête TCP doit porter un header
    /// `Authorization: Bearer <token>` valide. Les requêtes sans header ou avec
    /// un token invalide reçoivent un `401 Unauthorized`.
    /// Le socket Unix n'est jamais soumis à cette vérification.
    #[serde(default = "default_require_token")]
    pub require_token: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            port: default_api_port(),
            require_token: default_require_token(),
        }
    }
}

fn default_api_bind() -> String {
    "127.0.0.1".to_owned()
}

fn default_api_port() -> u16 {
    7771
}

fn default_require_token() -> bool {
    true
}
