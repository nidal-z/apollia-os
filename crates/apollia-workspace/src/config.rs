//! Configuration du `ProjectRuntime` (orchestrateur de providers).
//!
//! Seuls les paramètres d'orchestration restent ici : timeout global et TTL cache.
//! Chaque provider porte sa propre configuration dans son module.

use serde::Deserialize;

/// Configuration du `ProjectRuntime`.
///
/// Contrôle uniquement les comportements d'orchestration : timeout global
/// de collecte et durée de vie du cache TTL par répertoire.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// Timeout global appliqué à l'ensemble de la collecte, en secondes.
    ///
    /// Si le timeout est dépassé, la collecte retourne un [`WorkspaceSnapshot`] vide.
    /// Défaut : 2.
    #[serde(default = "default_collect_timeout")]
    pub collect_timeout_secs: u64,

    /// Durée de vie du cache, en secondes.
    ///
    /// Un second appel à `collect()` dans ce délai retourne le résultat mis en cache
    /// sans relancer les providers.
    /// Défaut : 30.
    #[serde(default = "default_ttl")]
    pub context_ttl_secs: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            collect_timeout_secs: default_collect_timeout(),
            context_ttl_secs: default_ttl(),
        }
    }
}

fn default_collect_timeout() -> u64 {
    2
}

fn default_ttl() -> u64 {
    30
}

// ─────────────────────────────────────────────────────────────────────────────
// Config des providers natifs
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration du [`GitProvider`](crate::providers::GitProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct GitProviderConfig {
    /// Nombre maximal de lignes conservées dans l'output de `git status --short`.
    /// Défaut : 200.
    #[serde(default = "default_git_status_lines")]
    pub git_status_max_lines: usize,
}

impl Default for GitProviderConfig {
    fn default() -> Self {
        Self {
            git_status_max_lines: default_git_status_lines(),
        }
    }
}

fn default_git_status_lines() -> usize {
    200
}

/// Configuration du [`RulesProvider`](crate::providers::RulesProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct RulesProviderConfig {
    /// Nom du fichier de règles à chercher (remonte la hiérarchie de répertoires).
    /// Défaut : "APOLLIA.md".
    #[serde(default = "default_rules_file")]
    pub file_name: String,

    /// Taille maximale du contenu injecté dans le prompt, en octets.
    /// Défaut : 8192.
    #[serde(default = "default_rules_max_bytes")]
    pub max_bytes: usize,

    /// Nombre maximal de répertoires parents à remonter.
    /// Défaut : 5.
    #[serde(default = "default_search_depth")]
    pub search_depth: usize,
}

impl Default for RulesProviderConfig {
    fn default() -> Self {
        Self {
            file_name: default_rules_file(),
            max_bytes: default_rules_max_bytes(),
            search_depth: default_search_depth(),
        }
    }
}

fn default_rules_file() -> String {
    "APOLLIA.md".to_owned()
}

fn default_rules_max_bytes() -> usize {
    8192
}

fn default_search_depth() -> usize {
    5
}

/// Configuration du [`StyleProvider`](crate::providers::StyleProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct StyleProviderConfig {
    /// Nombre de fichiers source à échantillonner pour la détection de style.
    /// Défaut : 7.
    #[serde(default = "default_style_sample_count")]
    pub sample_count: usize,

    /// Timeout en millisecondes pour l'appel LLM de détection de style.
    /// Défaut : 1000.
    #[serde(default = "default_style_timeout_ms")]
    pub timeout_ms: u64,

    /// Nombre de lignes lues par fichier lors de l'échantillonnage (head + tail).
    /// Défaut : 200.
    #[serde(default = "default_style_sample_lines")]
    pub sample_lines_per_file: usize,
}

impl Default for StyleProviderConfig {
    fn default() -> Self {
        Self {
            sample_count: default_style_sample_count(),
            timeout_ms: default_style_timeout_ms(),
            sample_lines_per_file: default_style_sample_lines(),
        }
    }
}

fn default_style_sample_count() -> usize {
    7
}

fn default_style_timeout_ms() -> u64 {
    1000
}

fn default_style_sample_lines() -> usize {
    200
}
