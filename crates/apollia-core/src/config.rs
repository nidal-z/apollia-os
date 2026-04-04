//! Configuration du runtime Apollia OS.
//!
//! Définit les sections de configuration lues depuis `apollia.toml` :
//! - [`RuntimeConfig`] — section `[runtime]` pour la capacité de l'EventBus et des mailboxes.
//! - [`A2AConfig`] — section `[a2a]` pour le routing inter-agents.
//! - [`HitlConfig`] — section `[hitl]` pour le watcher Human-in-the-Loop.
//! - [`ORIAConfig`] — section `[oria]` pour le moteur Observer-Reasoner-Actor.
//! - [`ApiConfig`] — section `[api]` pour le listener TCP et le socket Unix.
//!
//! Tous les champs ont des valeurs par défaut saines via [`Default`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Erreurs
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

    /// Une valeur numérique est hors des bornes `[min, max]` attendues.
    #[error("configuration field '{key}' = {actual} is out of bounds (expected [{min}, {max}])")]
    OutOfBounds {
        /// Chemin du champ en notation pointée.
        key: String,
        /// Valeur minimale acceptable (inclusive).
        min: String,
        /// Valeur maximale acceptable (inclusive).
        max: String,
        /// Valeur effectivement fournie.
        actual: String,
    },

    /// Le répertoire parent du socket Unix n'existe pas.
    #[error("unix_socket parent directory does not exist: '{path}'")]
    SocketParentMissing {
        /// Chemin du socket Unix configuré.
        path: String,
    },
}

/// Valide qu'une valeur numérique est dans l'intervalle `[min, max]` (inclus).
///
/// Retourne [`ConfigError::OutOfBounds`] si `value < min || value > max`.
///
/// # Paramètres
///
/// - `key` : nom du champ en notation pointée (ex : `"runtime.eventbus_capacity"`).
/// - `value` : valeur à valider.
/// - `min` : borne inférieure inclusive.
/// - `max` : borne supérieure inclusive.
pub fn validate_bounds<T>(key: &str, value: T, min: T, max: T) -> Result<(), ConfigError>
where
    T: PartialOrd + std::fmt::Display,
{
    if value < min || value > max {
        return Err(ConfigError::OutOfBounds {
            key: key.to_string(),
            min: min.to_string(),
            max: max.to_string(),
            actual: value.to_string(),
        });
    }
    Ok(())
}

// ─────────────────────────────────────────────
// RuntimeConfig
// ─────────────────────────────────────────────

/// Configuration du runtime core (section `[runtime]` dans `apollia.toml`).
///
/// Contrôle la capacité des infrastructures de communication interne :
/// le channel broadcast de l'EventBus et les mailboxes acteur.
/// Tous les champs ont des valeurs par défaut saines via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// Capacité du channel broadcast EventBus.
    ///
    /// Nombre maximal d'événements bufferisés avant que les receivers lents
    /// reçoivent [`tokio::sync::broadcast::error::RecvError::Lagged`].
    /// Défaut : 1024. Bornes : [64, 65536].
    #[serde(default = "default_eventbus_capacity")]
    pub eventbus_capacity: usize,

    /// Capacité maximale d'une mailbox acteur.
    ///
    /// Nombre maximal de messages en attente par agent dans le [`AgentMailbox`].
    /// Au-delà, `send()` retourne `MailboxError::QueueFull`.
    /// Défaut : 100. Bornes : [10, 10000].
    #[serde(default = "default_mailbox_capacity")]
    pub mailbox_capacity: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            eventbus_capacity: default_eventbus_capacity(),
            mailbox_capacity: default_mailbox_capacity(),
        }
    }
}

impl RuntimeConfig {
    /// Valide les bornes de la configuration runtime au démarrage (Principe #4 — Fail fast).
    ///
    /// - `eventbus_capacity` : doit être dans [64, 65536].
    /// - `mailbox_capacity` : doit être dans [10, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "runtime.eventbus_capacity",
            self.eventbus_capacity,
            64,
            65536,
        )?;
        validate_bounds("runtime.mailbox_capacity", self.mailbox_capacity, 10, 10000)?;
        Ok(())
    }
}

fn default_eventbus_capacity() -> usize {
    1024
}

fn default_mailbox_capacity() -> usize {
    100
}

// ─────────────────────────────────────────────
// A2AConfig
// ─────────────────────────────────────────────

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
    /// Défaut : 300. Bornes : [10, 3600].
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

impl A2AConfig {
    /// Valide les bornes de la configuration A2A au démarrage (Principe #4 — Fail fast).
    ///
    /// - `chain_timeout_secs` : doit être dans [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("a2a.chain_timeout_secs", self.chain_timeout_secs, 10, 3600)?;
        Ok(())
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
// HitlConfig
// ─────────────────────────────────────────────

/// Configuration Human-in-the-Loop (section `[hitl]` dans `apollia.toml`).
///
/// Contrôle le comportement du `TimeoutWatcher` : durée maximale d'attente
/// d'approbation humaine et fréquence de scan des tâches expirées.
/// Tous les champs ont des valeurs par défaut saines via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct HitlConfig {
    /// Durée maximale d'attente d'approbation humaine, en heures.
    ///
    /// Une tâche `input_required` suspendue depuis plus de `timeout_hours` heures
    /// est automatiquement annulée par le `TimeoutWatcher`.
    /// Défaut : 24. Bornes : [1, 168] (1 heure à 7 jours).
    #[serde(default = "default_timeout_hours")]
    pub timeout_hours: u64,

    /// Intervalle de scan des tâches HITL expirées, en secondes.
    ///
    /// Fréquence à laquelle le `TimeoutWatcher` vérifie les tâches suspirées.
    /// Défaut : 60. Bornes : [10, 3600].
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u64,
}

impl Default for HitlConfig {
    fn default() -> Self {
        Self {
            timeout_hours: default_timeout_hours(),
            scan_interval_secs: default_scan_interval_secs(),
        }
    }
}

impl HitlConfig {
    /// Valide les bornes de la configuration HITL au démarrage (Principe #4 — Fail fast).
    ///
    /// - `timeout_hours` : doit être dans [1, 168].
    /// - `scan_interval_secs` : doit être dans [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("hitl.timeout_hours", self.timeout_hours, 1, 168)?;
        validate_bounds("hitl.scan_interval_secs", self.scan_interval_secs, 10, 3600)?;
        Ok(())
    }
}

fn default_timeout_hours() -> u64 {
    24
}

fn default_scan_interval_secs() -> u64 {
    60
}

// ─────────────────────────────────────────────
// ORIAConfig
// ─────────────────────────────────────────────

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

    /// Seuil de confiance au-delà duquel l'Observer classifie une tâche comme Orchestrated.
    ///
    /// L'Observer calcule un score de complexité pondéré. Si ce score ≥ seuil,
    /// la tâche est exécutée en mode Orchestrated (planification LLM).
    /// Défaut : 0.40. Bornes : [0.0, 1.0].
    #[serde(default = "default_orchestrated_threshold")]
    pub orchestrated_threshold: f64,

    /// Longueur maximale de l'output d'un step stocké en mémoire épisodique.
    ///
    /// Tronque les sorties trop longues avant leur écriture en mémoire épisodique,
    /// pour limiter la consommation de contexte LLM lors des recalls futurs.
    /// Défaut : 200. Bornes : [50, 10000].
    #[serde(default = "default_step_memory_max_chars")]
    pub step_memory_max_chars: usize,

    /// Intervalle de vérification du StepBudget restant, en millisecondes.
    ///
    /// Le runtime interroge le budget à cette fréquence pour détecter l'épuisement
    /// pendant l'exécution directe. Trop court gaspille du CPU ; trop long
    /// retarde la détection.
    /// Défaut : 100. Bornes : [10, 5000].
    #[serde(default = "default_budget_poll_ms")]
    pub budget_poll_ms: u64,

    /// Seuil de déclenchement du compactage automatique (0.0–1.0).
    ///
    /// Fraction de la fenêtre de contexte LLM à partir de laquelle `ContextManager`
    /// compacte l'historique de conversation avant chaque appel au Reasoner.
    /// `0.80` laisse 20% de headroom pour au moins 1 tour supplémentaire complet.
    /// Défaut : 0.80. Bornes : [0.0, 1.0].
    #[serde(default = "default_compact_threshold")]
    pub context_compact_threshold: f32,

    /// Longueur maximale du résumé généré lors du compactage, en caractères.
    ///
    /// Borne haute appliquée à la sortie LLM lors de la synthèse de l'historique.
    /// `4000` correspond à ~1000 tokens — suffisant pour capturer l'état d'une
    /// tâche complexe avec fichiers modifiés et prochaines étapes.
    /// Défaut : 4000. Bornes : [500, 32000].
    #[serde(default = "default_summary_max_chars")]
    pub context_summary_max_chars: usize,
}

impl Default for ORIAConfig {
    fn default() -> Self {
        Self {
            max_replans: default_max_replans(),
            orchestrated_threshold: default_orchestrated_threshold(),
            step_memory_max_chars: default_step_memory_max_chars(),
            budget_poll_ms: default_budget_poll_ms(),
            context_compact_threshold: default_compact_threshold(),
            context_summary_max_chars: default_summary_max_chars(),
        }
    }
}

impl ORIAConfig {
    /// Valide la configuration ORIA au démarrage (Principe #4 — Fail fast).
    ///
    /// - `max_replans` : doit être entre 0 et 10 inclus.
    /// - `orchestrated_threshold` : doit être dans [0.0, 1.0].
    /// - `step_memory_max_chars` : doit être dans [50, 10000].
    /// - `budget_poll_ms` : doit être dans [10, 5000].
    /// - `context_compact_threshold` : doit être dans [0.0, 1.0].
    /// - `context_summary_max_chars` : doit être dans [500, 32000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_replans > 10 {
            return Err(ConfigError::InvalidValue {
                field: "oria.max_replans".into(),
                reason: "must be between 0 and 10".into(),
            });
        }
        validate_bounds(
            "oria.orchestrated_threshold",
            self.orchestrated_threshold,
            0.0_f64,
            1.0_f64,
        )?;
        validate_bounds(
            "oria.step_memory_max_chars",
            self.step_memory_max_chars,
            50_usize,
            10_000_usize,
        )?;
        validate_bounds(
            "oria.budget_poll_ms",
            self.budget_poll_ms,
            10_u64,
            5_000_u64,
        )?;
        validate_bounds(
            "oria.context_compact_threshold",
            self.context_compact_threshold,
            0.0_f32,
            1.0_f32,
        )?;
        validate_bounds(
            "oria.context_summary_max_chars",
            self.context_summary_max_chars,
            500_usize,
            32_000_usize,
        )?;
        Ok(())
    }
}

fn default_max_replans() -> u32 {
    2
}

fn default_orchestrated_threshold() -> f64 {
    0.40
}

fn default_step_memory_max_chars() -> usize {
    200
}

fn default_budget_poll_ms() -> u64 {
    100
}

fn default_compact_threshold() -> f32 {
    0.80
}

fn default_summary_max_chars() -> usize {
    4000
}

// ─────────────────────────────────────────────
// PipelinesConfig
// ─────────────────────────────────────────────

/// Configuration du moteur de pipelines (section `[pipelines]` dans `apollia.toml`).
///
/// Contrôle les comportements d'exécution des pipelines déclaratifs.
/// Tous les champs ont des valeurs par défaut saines via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct PipelinesConfig {
    /// Timeout par défaut d'un step de pipeline, en secondes.
    ///
    /// Chaque step attend au maximum cette durée pour recevoir un événement
    /// `TaskCompleted` ou `TaskInputRequired` sur l'EventBus. Au-delà,
    /// le step est considéré en échec.
    /// Défaut : 60. Bornes : [5, 3600].
    #[serde(default = "default_step_timeout_secs")]
    pub default_step_timeout_secs: u64,
}

impl Default for PipelinesConfig {
    fn default() -> Self {
        Self {
            default_step_timeout_secs: default_step_timeout_secs(),
        }
    }
}

impl PipelinesConfig {
    /// Valide la configuration pipelines au démarrage (Principe #4 — Fail fast).
    ///
    /// - `default_step_timeout_secs` : doit être dans [5, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "pipelines.default_step_timeout_secs",
            self.default_step_timeout_secs,
            5_u64,
            3600_u64,
        )?;
        Ok(())
    }
}

fn default_step_timeout_secs() -> u64 {
    60
}

// ─────────────────────────────────────────────
// ApiConfig
// ─────────────────────────────────────────────

/// Configuration de l'API REST locale (section `[api]` dans `apollia.toml`).
///
/// Contrôle le binding TCP, l'authentification par token statique,
/// et le chemin du socket Unix local.
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

    /// Chemin du socket Unix local.
    ///
    /// Utilisé par la CLI et l'application desktop pour communiquer avec
    /// le runtime sans authentification (accès local uniquement).
    /// Défaut : `/tmp/apollia.sock`. Le répertoire parent doit exister.
    #[serde(default = "default_unix_socket")]
    pub unix_socket: PathBuf,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            port: default_api_port(),
            require_token: default_require_token(),
            unix_socket: default_unix_socket(),
        }
    }
}

impl ApiConfig {
    /// Valide la configuration de l'API au démarrage (Principe #4 — Fail fast).
    ///
    /// Vérifie que le répertoire parent du socket Unix existe.
    /// Un socket Unix dont le répertoire parent est absent ne peut pas être bindé.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let parent = self.unix_socket.parent().unwrap_or_else(|| {
            // Fallback: racine — toujours accessible
            std::path::Path::new("/")
        });
        if !parent.exists() {
            return Err(ConfigError::SocketParentMissing {
                path: self.unix_socket.display().to_string(),
            });
        }
        Ok(())
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

fn default_unix_socket() -> PathBuf {
    PathBuf::from("/tmp/apollia.sock")
}

// ─────────────────────────────────────────────
// ToolsConfig
// ─────────────────────────────────────────────

/// Configuration des outputs d'outils natifs (section `[tools]` dans `apollia.toml`).
///
/// Contrôle les limites appliquées par le runtime aux outputs des outils natifs
/// avant leur transmission au LLM. Protège la fenêtre de contexte LLM contre
/// les outputs volumineux (Principe #7 — Garde-fous non-négociables).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    /// Taille maximale d'un output d'outil transmis au LLM, en bytes UTF-8.
    ///
    /// Les outputs dépassant cette limite sont tronqués par la stratégie
    /// "middle-trim" : le début et la fin sont préservés, le milieu supprimé
    /// et remplacé par un message indiquant le nombre de lignes perdues.
    /// Défaut : 30 000. Bornes : [10, 1 000 000].
    #[serde(default = "default_max_output_chars")]
    pub max_output_chars: usize,

    /// Pattern regex d'extraction de paths depuis l'output bash.
    ///
    /// Utilisé par `FilePathExtractor` pour parser la réponse du LLM léger.
    /// Défaut : `None` — le pattern intégré (chemins Unix POSIX.1-2017 §3.265
    /// et Windows UNC RFC 8089) est appliqué.
    /// Configurable pour des environnements avec des conventions de nommage custom.
    ///
    /// Exemple `apollia.toml` :
    /// ```toml
    /// [tools]
    /// # file_path_extraction_pattern = "(?:/[^\\s]+)"
    /// ```
    #[serde(default)]
    pub file_path_extraction_pattern: Option<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_chars: default_max_output_chars(),
            file_path_extraction_pattern: None,
        }
    }
}

impl ToolsConfig {
    /// Valide les bornes de la configuration tools au démarrage (Principe #4 — Fail fast).
    ///
    /// - `max_output_chars` : doit être dans [10, 1 000 000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.max_output_chars",
            self.max_output_chars,
            10_usize,
            1_000_000_usize,
        )?;
        Ok(())
    }
}

fn default_max_output_chars() -> usize {
    30_000
}

// ─────────────────────────────────────────────
// PermissionsConfig
// ─────────────────────────────────────────────

/// Configuration du moteur de permissions (section `[permissions]` dans `apollia.toml`).
///
/// Contrôle les trois couches du moteur de permissions :
/// - SafeList (couche 1) : commandes auto-approuvées sans HITL.
/// - PrefixRuleEngine (couche 2) : règles préfixe persistées en SQLite.
/// - InjectionDetector (couche 3) : détection des patterns shell dangereux.
///
/// La SafeList est **vide par défaut** — l'opérateur définit explicitement
/// ce qui est sûr (principe de moindre privilège, OWASP ASVS V1.4, CWE-272).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PermissionsConfig {
    /// Commandes auto-approuvées sans HITL, configurées par l'opérateur.
    ///
    /// Format : `"tool_name(arg_text)"` ou `"tool_name"`.
    /// Exemples : `"bash_executor(git status)"`, `"bash_executor(git log)"`.
    /// **Vide par défaut** — aucune commande n'est auto-approuvée sans configuration explicite.
    #[serde(default)]
    pub safe_commands: Vec<String>,

    /// Active la détection d'injections shell (couche 3, priorité absolue).
    ///
    /// Défaut : `true`. Désactiver uniquement pour les environnements de test contrôlés.
    #[serde(default = "default_injection_detection")]
    pub injection_detection: bool,

    /// Durée de vie des règles préfixe SQLite, en heures.
    ///
    /// Défaut : 168 (7 jours). Les règles plus anciennes peuvent être purgées par maintenance.
    #[serde(default = "default_prefix_rule_ttl_hours")]
    pub prefix_rule_ttl_hours: u64,

    /// Chemin de la base SQLite pour les règles préfixe et l'audit log.
    ///
    /// Défaut : `~/.apollia/permissions.db`.
    #[serde(default = "default_permissions_db_path")]
    pub db_path: std::path::PathBuf,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            safe_commands: vec![],
            injection_detection: default_injection_detection(),
            prefix_rule_ttl_hours: default_prefix_rule_ttl_hours(),
            db_path: default_permissions_db_path(),
        }
    }
}

fn default_injection_detection() -> bool {
    true
}

fn default_prefix_rule_ttl_hours() -> u64 {
    168
}

fn default_permissions_db_path() -> std::path::PathBuf {
    std::path::PathBuf::from("~/.apollia/permissions.db")
}

// ─────────────────────────────────────────────
// BashValidatorConfig
// ─────────────────────────────────────────────

/// Configuration du validateur bash pré-exécution (section `[tools.bash]` dans `apollia.toml`).
///
/// Contrôle deux mécanismes de protection appliqués par `BashValidator` avant
/// chaque invocation de `BashExecutor` :
/// - La classification des risques par catégorie (`RiskClassifier`), synchrone et rapide.
/// - La validation syntaxique via `bash -n -c`, asynchrone avec timeout.
///
/// Toutes les catégories sont **activées** (`block_* = true`) mais les listes de patterns
/// sont **vides par défaut** : aucun blocage n'est appliqué sans configuration explicite
/// (principe opt-in — l'opérateur définit ce qu'il veut bloquer).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BashValidatorConfig {
    /// Active le blocage des commandes d'accès réseau sortant.
    ///
    /// Référence : OWASP A10:2021 (SSRF) et Principe #1 Apollia (local-first).
    /// Défaut : `true`. Aucun blocage effectif sans `network_egress_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_network_egress: bool,

    /// Active le blocage des opérations destructrices irréversibles.
    ///
    /// Référence : NIST SP 800-190 §4.4.
    /// Défaut : `true`. Aucun blocage effectif sans `destructive_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_destructive: bool,

    /// Active le blocage des élévations de privilèges.
    ///
    /// Référence : CWE-269 (Improper Privilege Management).
    /// Défaut : `true`. Aucun blocage effectif sans `privilege_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_privilege_escalation: bool,

    /// Active le blocage des commandes d'épuisement de ressources.
    ///
    /// Référence : CWE-400 (Uncontrolled Resource Consumption).
    /// Défaut : `true`. Aucun blocage effectif sans `exhaustion_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_resource_exhaustion: bool,

    /// Patterns déclenchant la catégorie `NetworkEgress`.
    ///
    /// Chaque entrée est une sous-chaîne recherchée dans la commande (ex: `"curl"`, `"wget"`).
    /// Vide par défaut — l'opérateur définit les patterns selon les outils installés.
    #[serde(default)]
    pub network_egress_patterns: Vec<String>,

    /// Patterns déclenchant la catégorie `DestructiveOp`.
    ///
    /// Exemples : `"rm -rf /"`, `"dd if="`, `"mkfs"`.
    /// Vide par défaut.
    #[serde(default)]
    pub destructive_patterns: Vec<String>,

    /// Patterns déclenchant la catégorie `PrivilegeEscalation`.
    ///
    /// Exemples : `"sudo"`, `"su "`, `"chmod 777 /"`.
    /// Vide par défaut.
    #[serde(default)]
    pub privilege_patterns: Vec<String>,

    /// Patterns déclenchant la catégorie `ResourceExhaustion`.
    ///
    /// Exemples : `":(){ :|:& };:"` (fork bomb).
    /// Vide par défaut.
    #[serde(default)]
    pub exhaustion_patterns: Vec<String>,

    /// Timeout de la validation syntaxique `bash -n -c`, en millisecondes.
    ///
    /// Au-delà de cette durée, `BashValidator::validate_syntax()` retourne
    /// `SyntaxValidationTimeout`. Défaut : 1000ms. Bornes : [100, 10000].
    #[serde(default = "default_syntax_check_timeout_ms")]
    pub syntax_check_timeout_ms: u64,
}

impl Default for BashValidatorConfig {
    fn default() -> Self {
        Self {
            block_network_egress: default_block_flag(),
            block_destructive: default_block_flag(),
            block_privilege_escalation: default_block_flag(),
            block_resource_exhaustion: default_block_flag(),
            network_egress_patterns: vec![],
            destructive_patterns: vec![],
            privilege_patterns: vec![],
            exhaustion_patterns: vec![],
            syntax_check_timeout_ms: default_syntax_check_timeout_ms(),
        }
    }
}

impl BashValidatorConfig {
    /// Valide les bornes de la configuration bash validator au démarrage (Principe #4 — Fail fast).
    ///
    /// - `syntax_check_timeout_ms` : doit être dans [100, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.bash.syntax_check_timeout_ms",
            self.syntax_check_timeout_ms,
            100_u64,
            10_000_u64,
        )?;
        Ok(())
    }
}

fn default_block_flag() -> bool {
    true
}

fn default_syntax_check_timeout_ms() -> u64 {
    1000
}

// ─────────────────────────────────────────────
// LlmRoutingConfig
// ─────────────────────────────────────────────

/// Configuration du routing LLM par niveau de précision (section `[llm.routing]` dans `apollia.toml`).
///
/// Découple les appels LLM selon deux axes naturels issus des scaling laws (Kaplan et al., 2020) :
/// - tâches de raisonnement profond → backend précis mais coûteux
/// - tâches d'extraction légère → backend rapide et économique
///
/// La section `[llm.routing]` est **obligatoire** — son absence est une erreur fatale au démarrage
/// (Principe #4 — Fail fast). Configurer les deux champs explicitement dans `apollia.toml`.
///
/// Exemple `apollia.toml` :
/// ```toml
/// [llm.routing]
/// precise = "claude-opus-4-6"
/// fast    = "claude-haiku-4-5-20251001"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRoutingConfig {
    /// Backend pour les tâches de raisonnement profond (planification ORIA, analyse, jugement).
    ///
    /// Critère : tâche où une erreur a un impact élevé ou nécessite de la nuance.
    /// Doit correspondre au nom d'un backend déclaré dans `[[llm.backends]]`.
    pub precise: String,

    /// Backend pour les tâches d'extraction légère (métadonnées, résumés, classification, paths).
    ///
    /// Critère : tâche déterministe, résultat vérifiable, faible coût d'erreur.
    /// Doit correspondre au nom d'un backend déclaré dans `[[llm.backends]]`.
    pub fast: String,
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── AC-1 : config absente → tous les défauts préservés ─────────────────

    #[test]
    fn test_default_config_preserves_all_defaults() {
        // GIVEN default configs (no TOML)
        let runtime = RuntimeConfig::default();
        let a2a = A2AConfig::default();
        let hitl = HitlConfig::default();
        let api = ApiConfig::default();

        // THEN all defaults are the expected values
        assert_eq!(runtime.eventbus_capacity, 1024);
        assert_eq!(runtime.mailbox_capacity, 100);
        assert_eq!(a2a.chain_timeout_secs, 300);
        assert_eq!(hitl.timeout_hours, 24);
        assert_eq!(hitl.scan_interval_secs, 60);
        assert_eq!(api.unix_socket, PathBuf::from("/tmp/apollia.sock"));

        // AND all defaults pass validation
        runtime
            .validate()
            .expect("default RuntimeConfig must be valid");
        a2a.validate().expect("default A2AConfig must be valid");
        hitl.validate().expect("default HitlConfig must be valid");
    }

    // ── AC-2 : valeurs custom → utilisées ──────────────────────────────────

    #[test]
    fn test_custom_eventbus_capacity_used() {
        // GIVEN
        let toml = r#"eventbus_capacity = 2048"#;
        let cfg: RuntimeConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.eventbus_capacity, 2048);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_mailbox_capacity_used() {
        // GIVEN
        let toml = r#"mailbox_capacity = 200"#;
        let cfg: RuntimeConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.mailbox_capacity, 200);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_chain_timeout_used() {
        // GIVEN
        let toml = r#"chain_timeout_secs = 600"#;
        let cfg: A2AConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.chain_timeout_secs, 600);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_hitl_timeout_used() {
        // GIVEN
        let toml = r#"timeout_hours = 48"#;
        let cfg: HitlConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.timeout_hours, 48);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_scan_interval_used() {
        // GIVEN
        let toml = r#"scan_interval_secs = 120"#;
        let cfg: HitlConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.scan_interval_secs, 120);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_unix_socket_used() {
        // GIVEN — /tmp always exists
        let toml = r#"unix_socket = "/tmp/custom-apollia.sock""#;
        let cfg: ApiConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.unix_socket, PathBuf::from("/tmp/custom-apollia.sock"));
        cfg.validate().expect("/tmp parent must exist");
    }

    // ── AC-3 : valeur hors bornes → erreur au démarrage ────────────────────

    #[test]
    fn test_eventbus_capacity_below_min_fails() {
        // GIVEN capacity = 10, below min 64
        let cfg = RuntimeConfig {
            eventbus_capacity: 10,
            mailbox_capacity: 100,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.eventbus_capacity"),
            "expected OutOfBounds for runtime.eventbus_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_eventbus_capacity_above_max_fails() {
        // GIVEN capacity = 100000, above max 65536
        let cfg = RuntimeConfig {
            eventbus_capacity: 100_000,
            mailbox_capacity: 100,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.eventbus_capacity"),
            "expected OutOfBounds for runtime.eventbus_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_mailbox_capacity_out_of_bounds_fails() {
        // GIVEN capacity = 5, below min 10
        let cfg = RuntimeConfig {
            eventbus_capacity: 1024,
            mailbox_capacity: 5,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "runtime.mailbox_capacity"),
            "expected OutOfBounds for runtime.mailbox_capacity, got: {result:?}"
        );
    }

    #[test]
    fn test_chain_timeout_out_of_bounds_fails() {
        // GIVEN chain_timeout_secs = 5, below min 10
        let cfg = A2AConfig {
            chain_timeout_secs: 5,
            ..A2AConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "a2a.chain_timeout_secs"),
            "expected OutOfBounds for a2a.chain_timeout_secs, got: {result:?}"
        );
    }

    #[test]
    fn test_hitl_timeout_out_of_bounds_fails() {
        // GIVEN timeout_hours = 0, below min 1
        let cfg = HitlConfig {
            timeout_hours: 0,
            scan_interval_secs: 60,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "hitl.timeout_hours"),
            "expected OutOfBounds for hitl.timeout_hours, got: {result:?}"
        );
    }

    #[test]
    fn test_scan_interval_out_of_bounds_fails() {
        // GIVEN scan_interval_secs = 5, below min 10
        let cfg = HitlConfig {
            timeout_hours: 24,
            scan_interval_secs: 5,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "hitl.scan_interval_secs"),
            "expected OutOfBounds for hitl.scan_interval_secs, got: {result:?}"
        );
    }

    #[test]
    fn test_boundary_values_accepted() {
        // GIVEN min and max exact values for all fields

        let runtime_min = RuntimeConfig {
            eventbus_capacity: 64,
            mailbox_capacity: 10,
        };
        let runtime_max = RuntimeConfig {
            eventbus_capacity: 65536,
            mailbox_capacity: 10000,
        };
        let a2a_min = A2AConfig {
            chain_timeout_secs: 10,
            ..A2AConfig::default()
        };
        let a2a_max = A2AConfig {
            chain_timeout_secs: 3600,
            ..A2AConfig::default()
        };
        let hitl_min = HitlConfig {
            timeout_hours: 1,
            scan_interval_secs: 10,
        };
        let hitl_max = HitlConfig {
            timeout_hours: 168,
            scan_interval_secs: 3600,
        };

        // THEN all boundary values are accepted
        runtime_min.validate().expect("min RuntimeConfig valid");
        runtime_max.validate().expect("max RuntimeConfig valid");
        a2a_min
            .validate()
            .expect("min A2AConfig chain_timeout valid");
        a2a_max
            .validate()
            .expect("max A2AConfig chain_timeout valid");
        hitl_min.validate().expect("min HitlConfig valid");
        hitl_max.validate().expect("max HitlConfig valid");
    }

    // ── ORIAConfig defaults ─────────────────────────────────────────────────

    #[test]
    fn test_default_oria_config_preserves_defaults() {
        // GIVEN no TOML for [oria]
        let cfg = ORIAConfig::default();

        // THEN all defaults are the expected values
        assert_eq!(cfg.max_replans, 2);
        assert!((cfg.orchestrated_threshold - 0.40).abs() < f64::EPSILON);
        assert_eq!(cfg.step_memory_max_chars, 200);
        assert_eq!(cfg.budget_poll_ms, 100);

        // AND defaults pass validation
        cfg.validate().expect("default ORIAConfig must be valid");
    }

    #[test]
    fn test_default_pipelines_config_preserves_defaults() {
        // GIVEN no TOML for [pipelines]
        let cfg = PipelinesConfig::default();

        // THEN default is 60 seconds
        assert_eq!(cfg.default_step_timeout_secs, 60);

        // AND default passes validation
        cfg.validate()
            .expect("default PipelinesConfig must be valid");
    }

    // ── ORIAConfig custom values ────────────────────────────────────────────

    #[test]
    fn test_custom_orchestrated_threshold_used() {
        // GIVEN
        let toml = r#"orchestrated_threshold = 0.65"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert!((cfg.orchestrated_threshold - 0.65).abs() < f64::EPSILON);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_step_memory_max_chars_used() {
        // GIVEN
        let toml = r#"step_memory_max_chars = 500"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.step_memory_max_chars, 500);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_budget_poll_ms_used() {
        // GIVEN
        let toml = r#"budget_poll_ms = 200"#;
        let cfg: ORIAConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.budget_poll_ms, 200);
        cfg.validate().expect("valid bounds");
    }

    #[test]
    fn test_custom_step_timeout_used() {
        // GIVEN
        let toml = r#"default_step_timeout_secs = 120"#;
        let cfg: PipelinesConfig = toml::from_str(toml).expect("valid toml");

        // THEN
        assert_eq!(cfg.default_step_timeout_secs, 120);
        cfg.validate().expect("valid bounds");
    }

    // ── AC-3 : orchestrated_threshold hors bornes ───────────────────────────

    #[test]
    fn test_orchestrated_threshold_above_1_fails() {
        // GIVEN orchestrated_threshold = 1.5, above max 1.0
        let cfg = ORIAConfig {
            orchestrated_threshold: 1.5,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.orchestrated_threshold"),
            "expected OutOfBounds for oria.orchestrated_threshold, got: {result:?}"
        );
    }

    #[test]
    fn test_orchestrated_threshold_negative_fails() {
        // GIVEN orchestrated_threshold = -0.1, below min 0.0
        let cfg = ORIAConfig {
            orchestrated_threshold: -0.1,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.orchestrated_threshold"),
            "expected OutOfBounds for oria.orchestrated_threshold, got: {result:?}"
        );
    }

    // ── AC-4 : step_memory_max_chars hors bornes ────────────────────────────

    #[test]
    fn test_step_memory_max_chars_below_50_fails() {
        // GIVEN step_memory_max_chars = 10, below min 50
        let cfg = ORIAConfig {
            step_memory_max_chars: 10,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.step_memory_max_chars"),
            "expected OutOfBounds for oria.step_memory_max_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_step_memory_max_chars_above_10000_fails() {
        // GIVEN step_memory_max_chars = 20000, above max 10000
        let cfg = ORIAConfig {
            step_memory_max_chars: 20_000,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.step_memory_max_chars"),
            "expected OutOfBounds for oria.step_memory_max_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_budget_poll_ms_out_of_bounds_fails() {
        // GIVEN budget_poll_ms = 5, below min 10
        let cfg = ORIAConfig {
            budget_poll_ms: 5,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "oria.budget_poll_ms"),
            "expected OutOfBounds for oria.budget_poll_ms, got: {result:?}"
        );
    }

    #[test]
    fn test_step_timeout_out_of_bounds_fails() {
        // GIVEN default_step_timeout_secs = 1, below min 5
        let cfg = PipelinesConfig {
            default_step_timeout_secs: 1,
        };

        // WHEN
        let result = cfg.validate();

        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "pipelines.default_step_timeout_secs"),
            "expected OutOfBounds for pipelines.default_step_timeout_secs, got: {result:?}"
        );
    }

    #[test]
    fn test_oria_pipelines_boundary_values_accepted() {
        // GIVEN min and max exact values for ORIA and PipelinesConfig fields
        let oria_min = ORIAConfig {
            orchestrated_threshold: 0.0,
            step_memory_max_chars: 50,
            budget_poll_ms: 10,
            ..ORIAConfig::default()
        };
        let oria_max = ORIAConfig {
            orchestrated_threshold: 1.0,
            step_memory_max_chars: 10_000,
            budget_poll_ms: 5_000,
            ..ORIAConfig::default()
        };
        let pipelines_min = PipelinesConfig {
            default_step_timeout_secs: 5,
        };
        let pipelines_max = PipelinesConfig {
            default_step_timeout_secs: 3600,
        };

        // THEN all boundary values are accepted
        oria_min.validate().expect("min ORIAConfig valid");
        oria_max.validate().expect("max ORIAConfig valid");
        pipelines_min.validate().expect("min PipelinesConfig valid");
        pipelines_max.validate().expect("max PipelinesConfig valid");
    }

    // ── ToolsConfig ────────────────────────────────────────────────────────

    #[test]
    fn test_tools_config_default_values() {
        // GIVEN config par défaut
        let cfg = ToolsConfig::default();
        // THEN
        assert_eq!(cfg.max_output_chars, 30_000);
        cfg.validate().expect("default ToolsConfig must be valid");
    }

    #[test]
    fn test_tools_config_serde_override() {
        // GIVEN un TOML avec une valeur custom
        let toml = r#"max_output_chars = 100"#;
        // WHEN
        let cfg: ToolsConfig = toml::from_str(toml).expect("valid toml");
        // THEN
        assert_eq!(cfg.max_output_chars, 100);
        cfg.validate().expect("100 is within bounds");
    }

    #[test]
    fn test_tools_config_below_min_fails() {
        // GIVEN max_output_chars below minimum (min = 10)
        let cfg = ToolsConfig {
            max_output_chars: 5,
            file_path_extraction_pattern: None,
        };
        // WHEN
        let result = cfg.validate();
        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "tools.max_output_chars"),
            "expected OutOfBounds for tools.max_output_chars, got: {result:?}"
        );
    }

    #[test]
    fn test_tools_config_above_max_fails() {
        // GIVEN max_output_chars above maximum
        let cfg = ToolsConfig {
            max_output_chars: 2_000_000,
            file_path_extraction_pattern: None,
        };
        // WHEN
        let result = cfg.validate();
        // THEN
        assert!(
            matches!(result, Err(ConfigError::OutOfBounds { ref key, .. }) if key == "tools.max_output_chars"),
            "expected OutOfBounds for tools.max_output_chars, got: {result:?}"
        );
    }
}
