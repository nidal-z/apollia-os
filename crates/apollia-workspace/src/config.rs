//! Configuration du collecteur de contexte workspace.
//!
//! Toutes les valeurs proviennent de la section `[workspace]` dans `apollia.toml`.
//! Aucune constante n'est codée en dur dans le binaire.

use serde::Deserialize;

/// Configuration du collecteur de contexte workspace (section `[workspace]` dans `apollia.toml`).
///
/// Contrôle les timeouts, la durée du cache TTL, et les limites de taille
/// appliquées lors de la collecte du contexte projet au démarrage d'une tâche ORIA.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceConfig {
    /// Timeout global appliqué à l'ensemble de `collect()`, en secondes.
    ///
    /// Laisse le temps à git sur des dépôts > 10 000 fichiers.
    /// Si le timeout est dépassé, `collect()` retourne [`WorkspaceContext::default()`].
    /// Défaut : 2.
    #[serde(default = "default_collect_timeout")]
    pub collect_timeout_secs: u64,

    /// Durée de vie du cache, en secondes.
    ///
    /// Un second appel à `collect()` dans ce délai retourne le résultat mis en cache
    /// sans relancer les sous-processus git ni les lectures disque.
    /// Ratio coût/fraîcheur standard pour les sessions interactives.
    /// Défaut : 30.
    #[serde(default = "default_ttl")]
    pub context_ttl_secs: u64,

    /// Nombre maximal de lignes conservées dans l'output de `git status --short`.
    ///
    /// Les lignes excédentaires sont tronquées. Correspond à environ 5 écrans
    /// de terminal 80×40.
    /// Défaut : 200.
    #[serde(default = "default_git_status_lines")]
    pub git_status_max_lines: usize,

    /// Taille maximale du contenu de `APOLLIA.md` injecté dans le prompt, en octets.
    ///
    /// Le contenu excédentaire est tronqué par middle-trim.
    /// 8 192 octets ≈ 2 000 tokens — budget raisonnable dans un system prompt.
    /// Défaut : 8192.
    #[serde(default = "default_apollia_md_bytes")]
    pub apollia_md_max_bytes: usize,

    /// Nombre maximal de répertoires parents à remonter lors de la recherche de `APOLLIA.md`.
    ///
    /// Couvre les structures monorepo typiques (project/team/org/…).
    /// Défaut : 5.
    #[serde(default = "default_search_depth")]
    pub apollia_md_search_depth: usize,

    /// Nombre de fichiers source à échantillonner pour la détection de style.
    ///
    /// Minimum statistiquement représentatif pour détecter des conventions
    /// sans surcharger le contexte LLM.
    /// Défaut : 7.
    #[serde(default = "default_style_sample_count")]
    pub style_sample_count: usize,

    /// Timeout en millisecondes pour l'appel LLM de détection de style.
    ///
    /// Dépasse ce délai → `code_style = None`, jamais de blocage.
    /// Défaut : 1000.
    #[serde(default = "default_style_detection_timeout_ms")]
    pub style_detection_timeout_ms: u64,

    /// Nombre de lignes lues par fichier lors de l'échantillonnage (head + tail).
    ///
    /// Défaut : 200 (100 lignes head + 100 lignes tail).
    #[serde(default = "default_style_sample_lines_per_file")]
    pub style_sample_lines_per_file: usize,

    /// Liste des providers configurés dans `[[workspace.providers]]`.
    ///
    /// Si la liste est vide, le provider git builtin est utilisé par défaut.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            collect_timeout_secs: default_collect_timeout(),
            context_ttl_secs: default_ttl(),
            git_status_max_lines: default_git_status_lines(),
            apollia_md_max_bytes: default_apollia_md_bytes(),
            apollia_md_search_depth: default_search_depth(),
            style_sample_count: default_style_sample_count(),
            style_detection_timeout_ms: default_style_detection_timeout_ms(),
            style_sample_lines_per_file: default_style_sample_lines_per_file(),
            providers: vec![],
        }
    }
}

/// Configuration d'un provider de contexte déclaré dans `apollia.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Nom unique du provider.
    pub name: String,
    /// Type du provider : `"builtin"`, `"python"` ou `"script"`.
    #[serde(rename = "type")]
    pub provider_type: String,
    /// Chemin vers le fichier Python ou script (pour `type = "python"` ou `"script"`).
    pub path: Option<String>,
    /// Active ou désactive ce provider sans le supprimer.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Timeout d'exécution en millisecondes (providers `"script"` uniquement).
    pub timeout_ms: Option<u64>,
    /// Priorité d'affichage (écrase la valeur déclarée dans le provider).
    pub priority: Option<u8>,
    /// Intervalle de rafraîchissement en secondes (écrase la valeur du provider).
    pub refresh_secs: Option<u64>,
}

fn default_enabled() -> bool {
    true
}

fn default_collect_timeout() -> u64 {
    2
}

fn default_ttl() -> u64 {
    30
}

fn default_git_status_lines() -> usize {
    200
}

fn default_apollia_md_bytes() -> usize {
    8192
}

fn default_search_depth() -> usize {
    5
}

fn default_style_sample_count() -> usize {
    7
}

fn default_style_detection_timeout_ms() -> u64 {
    1000
}

fn default_style_sample_lines_per_file() -> usize {
    200
}
