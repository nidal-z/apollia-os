//! Parsing et validation de la configuration Apollia OS depuis `apollia.toml`.
//!
//! Fournit [`parse_apollia_toml`] pour lire et désérialiser le fichier de config,
//! [`validate_llm_config`] pour une validation non-fatale (warnings seulement)
//! des backends LLM, et [`parse_triggers`] pour valider la section `[[triggers]]`.
//!
//! La validation des triggers suit le **Principe #4 — Fail fast** : toute erreur
//! de configuration (schedule cron invalide, secret webhook vide, path file_watch vide)
//! est détectée dès l'appel à [`parse_apollia_toml`], avant le démarrage du runtime.
//!
//! # Exemple TOML minimal
//!
//! ```toml
//! [llm]
//! default = "anthropic"
//!
//! [[llm.backends]]
//! name        = "anthropic"
//! type        = "api"
//! api_url     = "https://api.anthropic.com"
//! api_key_env = "ANTHROPIC_API_KEY"
//! model       = "claude-haiku-4-5-20251001"
//!
//! [[triggers]]
//! id             = "rapport-hebdo"
//! agent          = "rapport-agent"
//! enabled        = true
//! on_busy        = "queue"
//! input_template = "Rapport du {{scheduled_at}}"
//!
//! [triggers.source]
//! type     = "cron"
//! schedule = "0 0 8 * * MON"
//! ```

use std::path::{Path, PathBuf};
use std::str::FromStr;

use apollia_llm::{BackendKind, LlmConfig};
use apollia_triggers::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerSourceConfig,
};

// ─────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────

/// Erreurs possibles lors du parsing de `apollia.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Erreur I/O lors de la lecture du fichier de configuration.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Erreur de désérialisation TOML (champ invalide, type incorrect, etc.).
    #[error("invalid TOML config: {0}")]
    Parse(#[from] toml::de::Error),

    /// La définition d'un trigger est invalide (schedule cron, secret webhook, etc.).
    ///
    /// Le message contient toujours l'identifiant du trigger fautif pour aider
    /// l'opérateur à localiser l'erreur dans `apollia.toml` (Principe #8 — CLI humaine).
    #[error("invalid trigger '{id}': {reason}")]
    InvalidTrigger {
        /// Identifiant du trigger fautif.
        id: String,
        /// Description de l'erreur de validation.
        reason: String,
    },
}

// ─────────────────────────────────────────────
// Structures de configuration
// ─────────────────────────────────────────────

/// Configuration de la section `[agents]` dans `apollia.toml`.
#[derive(Debug, serde::Deserialize, Clone)]
pub struct AgentsConfig {
    /// Répertoire de chargement des modules Python agents.
    pub directory: Option<String>,
}

/// Format brut TOML pour la section `[[triggers]]` avant validation sémantique.
#[derive(Debug, serde::Deserialize)]
struct RawTrigger {
    /// Identifiant unique du trigger.
    id: String,
    /// Nom de l'agent cible.
    agent: String,
    /// Indique si le trigger est actif (`true` par défaut).
    #[serde(default = "default_true")]
    enabled: bool,
    /// Politique quand l'agent est occupé : `"queue"` (défaut) ou `"drop"`.
    #[serde(default)]
    on_busy: String,
    /// Template de message envoyé à l'agent.
    input_template: String,
    /// Configuration de la source (cron, interval, file_watch, webhook, oneshot).
    source: RawTriggerSource,
}

/// Format brut TOML pour la sous-section `source` d'un trigger.
#[derive(Debug, serde::Deserialize)]
struct RawTriggerSource {
    /// Type de source : `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    #[serde(rename = "type")]
    kind: String,
    /// Expression cron (source `"cron"` uniquement).
    schedule: Option<String>,
    /// Intervalle sous forme `"30m"`, `"1h"`, etc. (source `"interval"` uniquement).
    every: Option<String>,
    /// Horodatage ISO-8601 (source `"oneshot"` uniquement).
    fire_at: Option<String>,
    /// Chemin surveillé (source `"file_watch"` uniquement).
    path: Option<String>,
    /// Types d'événements : `["create", "modify", "delete", "any"]` (source `"file_watch"`).
    events: Option<Vec<String>>,
    /// Secret HMAC-SHA256 (source `"webhook"` uniquement).
    secret: Option<String>,
}

/// Représentation interne brute du TOML complet — utilisée uniquement par [`parse_apollia_toml`].
#[derive(Debug, serde::Deserialize)]
struct RawApolliaCConfig {
    agents: Option<AgentsConfig>,
    llm: Option<LlmConfig>,
    #[serde(default)]
    triggers: Vec<RawTrigger>,
}

/// Configuration globale Apollia OS validée depuis `apollia.toml`.
///
/// Tous les champs sont optionnels : un fichier minimal peut ne contenir
/// qu'une seule section ou être entièrement vide.
///
/// Pour désérialiser depuis un fichier avec validation complète (y compris
/// la section `[[triggers]]`), utiliser [`parse_apollia_toml`].
#[derive(Debug, serde::Deserialize)]
pub struct ApolliaCConfig {
    /// Section `[agents]` — répertoire des agents Python.
    pub agents: Option<AgentsConfig>,

    /// Section `[llm]` — configuration des backends LLM.
    ///
    /// Vaut `None` si la section `[llm]` est absente du fichier (AC-3).
    pub llm: Option<LlmConfig>,

    /// Triggers validés depuis la section `[[triggers]]` dans `apollia.toml`.
    ///
    /// Toujours vide si désérialisé directement via `toml::from_str` —
    /// utiliser [`parse_apollia_toml`] pour obtenir les triggers validés.
    #[serde(skip)]
    pub triggers: Vec<TriggerDefinition>,
}

/// Retourne `true` — valeur par défaut pour le champ `enabled` d'un trigger.
fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────
// Fonctions publiques
// ─────────────────────────────────────────────

/// Résout `~` vers `$HOME` dans un chemin de fichier.
///
/// Remplace un composant `~` en tête de chemin par la valeur de la
/// variable d'environnement `HOME`. Si `HOME` n'est pas définie,
/// `~` est remplacé par une chaîne vide (comportement non-fatal).
///
/// Cette fonction est **pure** — aucun accès au système de fichiers,
/// uniquement une transformation de chaîne.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
    } else {
        path.to_path_buf()
    }
}

/// Lit et désérialise `apollia.toml` depuis le chemin indiqué.
///
/// Après le parsing TOML :
/// - Les chemins `model_path` des backends embarqués sont normalisés via [`expand_tilde`].
/// - La section `[[triggers]]` est validée via [`parse_triggers`] (Principe #4 — Fail fast).
///
/// La section `[llm]` et la section `[[triggers]]` sont **optionnelles** : leur absence
/// produit respectivement `config.llm = None` et `config.triggers = vec![]` sans erreur.
///
/// # Erreurs
///
/// - [`ConfigError::Io`] — le fichier est inaccessible ou illisible.
/// - [`ConfigError::Parse`] — le TOML est malformé ou contient des types invalides.
/// - [`ConfigError::InvalidTrigger`] — un trigger activé a une configuration invalide.
pub fn parse_apollia_toml(path: &Path) -> Result<ApolliaCConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let mut raw: RawApolliaCConfig = toml::from_str(&content)?;

    // Normalise les chemins model_path des backends embarqués (~ → $HOME).
    if let Some(ref mut llm) = raw.llm {
        for _backend in &mut llm.backends {
            #[cfg(feature = "local")]
            if let BackendKind::Embedded(ref mut cfg) = _backend.kind {
                cfg.model_path = expand_tilde(&cfg.model_path);
            }
        }
    }

    // Valide et convertit les triggers bruts en TriggerDefinition.
    let triggers = parse_triggers(&raw.triggers)?;

    Ok(ApolliaCConfig {
        agents: raw.agents,
        llm: raw.llm,
        triggers,
    })
}

// ─────────────────────────────────────────────
// Trigger parsing & validation
// ─────────────────────────────────────────────

/// Valide et convertit la liste brute de triggers TOML en [`TriggerDefinition`].
///
/// Respecte le Principe #4 (Fail fast) : toute erreur sémantique est détectée ici,
/// avant le démarrage du `TriggerEngine`. Si un trigger a `enabled = false`, la
/// validation de sa source est ignorée — le trigger ne sera pas démarré.
///
/// # Erreurs
///
/// Retourne [`ConfigError::InvalidTrigger`] au premier trigger invalide rencontré,
/// avec le `trigger_id` fautif dans le message d'erreur.
fn parse_triggers(raws: &[RawTrigger]) -> Result<Vec<TriggerDefinition>, ConfigError> {
    raws.iter().map(validate_trigger).collect()
}

/// Valide un trigger brut et le convertit en [`TriggerDefinition`].
///
/// - `id` et `agent` doivent être non vides, quelle que soit la valeur de `enabled`.
/// - Si `enabled = false`, la source n'est PAS validée sémantiquement (cron, path, etc.).
/// - Si `enabled = true`, la source est entièrement validée.
fn validate_trigger(raw: &RawTrigger) -> Result<TriggerDefinition, ConfigError> {
    // id et agent toujours requis, même pour les triggers désactivés.
    if raw.id.is_empty() {
        return Err(ConfigError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyId.to_string(),
        });
    }
    if raw.agent.is_empty() {
        return Err(ConfigError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyAgent.to_string(),
        });
    }

    let on_busy = match raw.on_busy.as_str() {
        "drop" => OnBusyPolicy::Drop,
        _ => OnBusyPolicy::Queue,
    };

    let source = if raw.enabled {
        // Validation sémantique complète pour les triggers actifs.
        validate_trigger_source(&raw.id, &raw.source)?
    } else {
        // Trigger désactivé : parsing minimal sans validation.
        parse_trigger_source_unchecked(&raw.source)
    };

    Ok(TriggerDefinition {
        id: raw.id.clone(),
        agent: raw.agent.clone(),
        enabled: raw.enabled,
        on_busy,
        source,
        input_template: InputTemplate(raw.input_template.clone()),
    })
}

/// Valide sémantiquement la source d'un trigger activé.
///
/// - `"cron"` : valide l'expression via `cron::Schedule::from_str` ;
///   normalise les expressions 5-champs (unix standard) en 6-champs pour la crate `cron`.
/// - `"interval"` : valide le format (`30m`, `1h`, etc.) via [`parse_interval`].
/// - `"oneshot"` : valide le format ISO-8601 de `fire_at`.
/// - `"file_watch"` : vérifie que `path` n'est pas vide ; émet un `warn!` si le répertoire
///   n'existe pas (pas fatal — l'agent peut le créer avant le premier fire).
/// - `"webhook"` : vérifie que `secret` n'est pas vide.
fn validate_trigger_source(
    id: &str,
    raw_src: &RawTriggerSource,
) -> Result<TriggerSourceConfig, ConfigError> {
    match raw_src.kind.as_str() {
        "cron" => {
            let schedule = raw_src.schedule.clone().unwrap_or_default();
            let normalized = normalize_and_validate_cron(id, &schedule)?;
            Ok(TriggerSourceConfig::Cron {
                schedule: normalized,
            })
        }

        "interval" => {
            let every = raw_src.every.clone().unwrap_or_default();
            parse_interval(&every).map_err(|e| ConfigError::InvalidTrigger {
                id: id.to_string(),
                reason: e.to_string(),
            })?;
            Ok(TriggerSourceConfig::Interval { every })
        }

        "oneshot" => {
            let fire_at_str = raw_src.fire_at.clone().unwrap_or_default();
            let fire_at = fire_at_str
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|e| ConfigError::InvalidTrigger {
                    id: id.to_string(),
                    reason: format!("invalid fire_at timestamp: {e}"),
                })?;
            Ok(TriggerSourceConfig::Oneshot { fire_at })
        }

        "file_watch" => {
            let path_str = raw_src.path.clone().unwrap_or_default();
            if path_str.is_empty() {
                return Err(ConfigError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyFileWatchPath.to_string(),
                });
            }
            let path = expand_tilde(Path::new(&path_str));
            if !path.exists() {
                tracing::warn!(
                    trigger = %id,
                    path = %path.display(),
                    "file_watch path does not exist — will start watching when created"
                );
            }
            let events = parse_file_event_kinds(raw_src.events.as_deref().unwrap_or(&[]));
            Ok(TriggerSourceConfig::FileWatch { path, events })
        }

        "webhook" => {
            let secret = raw_src.secret.clone().unwrap_or_default();
            if secret.is_empty() {
                return Err(ConfigError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyWebhookSecret.to_string(),
                });
            }
            Ok(TriggerSourceConfig::Webhook { secret })
        }

        unknown => Err(ConfigError::InvalidTrigger {
            id: id.to_string(),
            reason: format!("unknown source type '{unknown}'"),
        }),
    }
}

/// Valide une expression cron, en acceptant les formats 5-champs (unix standard)
/// et 6-champs (crate `cron`).
///
/// Les expressions 5-champs (`min hour dom month dow`) sont automatiquement
/// normalisées en 6-champs (`0 min hour dom month dow`) pour compatibilité avec
/// la crate `cron` qui exige au minimum 6 champs.
///
/// Retourne l'expression normalisée (6 ou 7 champs) en cas de succès.
fn normalize_and_validate_cron(id: &str, schedule: &str) -> Result<String, ConfigError> {
    // Tentative directe (6 ou 7 champs).
    if cron::Schedule::from_str(schedule).is_ok() {
        return Ok(schedule.to_string());
    }

    // Normalisation 5-champs → 6-champs (unix standard → format crate cron).
    let field_count = schedule.split_whitespace().count();
    if field_count == 5 {
        let normalized = format!("0 {schedule}");
        if cron::Schedule::from_str(&normalized).is_ok() {
            return Ok(normalized);
        }
    }

    // Retourne l'erreur avec l'expression originale et le trigger_id.
    let reason = cron::Schedule::from_str(schedule)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "invalid cron expression".to_string());

    Err(ConfigError::InvalidTrigger {
        id: id.to_string(),
        reason: TriggerDefinitionError::InvalidCronSchedule {
            schedule: schedule.to_string(),
            reason,
        }
        .to_string(),
    })
}

/// Convertit des noms d'événements fichier en [`FileEventKind`].
///
/// Les valeurs inconnues sont mappées sur [`FileEventKind::Any`].
/// Un slice vide produit `vec![FileEventKind::Create]` (comportement par défaut).
fn parse_file_event_kinds(raw: &[String]) -> Vec<FileEventKind> {
    if raw.is_empty() {
        return vec![FileEventKind::Create];
    }
    raw.iter()
        .map(|s| match s.as_str() {
            "create" => FileEventKind::Create,
            "modify" => FileEventKind::Modify,
            "delete" => FileEventKind::Delete,
            _ => FileEventKind::Any,
        })
        .collect()
}

/// Parsing minimal d'une source sans validation sémantique — pour les triggers désactivés.
///
/// Les champs manquants sont remplacés par des valeurs par défaut inoffensives.
/// Les erreurs de format (ex. schedule invalide) sont silencieusement ignorées.
fn parse_trigger_source_unchecked(raw_src: &RawTriggerSource) -> TriggerSourceConfig {
    match raw_src.kind.as_str() {
        "cron" => TriggerSourceConfig::Cron {
            schedule: raw_src.schedule.clone().unwrap_or_default(),
        },
        "interval" => TriggerSourceConfig::Interval {
            every: raw_src.every.clone().unwrap_or_default(),
        },
        "oneshot" => {
            let fire_at = raw_src
                .fire_at
                .as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            TriggerSourceConfig::Oneshot { fire_at }
        }
        "file_watch" => {
            let path = expand_tilde(Path::new(raw_src.path.as_deref().unwrap_or("")));
            let events = parse_file_event_kinds(raw_src.events.as_deref().unwrap_or(&[]));
            TriggerSourceConfig::FileWatch { path, events }
        }
        // "webhook" ou type inconnu — secret vide, non démarré donc sans impact.
        _ => TriggerSourceConfig::Webhook {
            secret: raw_src.secret.clone().unwrap_or_default(),
        },
    }
}

/// Validation non-fatale de la config LLM — émet des warnings, ne retourne jamais d'erreur.
///
/// Pour chaque backend configuré :
/// - **Backend embarqué** : vérifie que le fichier `.gguf` existe après expansion du `~`.
///   Si absent → `tracing::warn!` avec le chemin manquant (backend ignoré par le router).
/// - **Backend API** : vérifie que la variable d'environnement `api_key_env` est définie.
///   Si absente → `tracing::warn!` avec le nom de la variable (backend ignoré par le router).
///
/// Cette fonction est **intentionnellement non-fatale** (AC-4) : la validation stricte
/// est déléguée à [`apollia_llm::LlmRouter::from_config`] au démarrage du Supervisor.
///
/// Retourne toujours `Ok(())`.
pub fn validate_llm_config(config: &LlmConfig) -> Result<(), ConfigError> {
    for backend in &config.backends {
        match &backend.kind {
            #[cfg(feature = "local")]
            BackendKind::Embedded(cfg) => {
                let expanded = expand_tilde(&cfg.model_path);
                if !expanded.exists() {
                    tracing::warn!(
                        backend = %backend.name(),
                        path = %expanded.display(),
                        "model file not found — backend will be skipped"
                    );
                }
            }

            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => {
                if std::env::var(&cfg.api_key_env).is_err() {
                    tracing::warn!(
                        backend = %backend.name(),
                        env_var = %cfg.api_key_env,
                        "API key env var not set — backend will be skipped"
                    );
                }
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN un TOML avec [[llm.backends]] type = "embedded"
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm.default == "local" ET backends[0] est Embedded
    #[cfg(feature = "local")]
    #[test]
    fn test_ac1_parse_embedded_backend() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "local"

[[llm.backends]]
name = "local"
type = "embedded"
model_path = "/tmp/model.gguf"
quantization = "q4_k_m"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let llm = config.llm.unwrap();
        assert_eq!(llm.default, "local");
        assert_eq!(llm.backends[0].name(), "local");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Embedded(_)),
            "le kind doit être Embedded"
        );
    }

    // GIVEN un TOML avec [[llm.backends]] type = "api"
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm.backends[0] est Api et le nom est correct
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac2_parse_api_backend() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let llm = config.llm.unwrap();
        assert_eq!(llm.backends[0].name(), "anthropic");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Api(_)),
            "le kind doit être Api"
        );
    }

    // GIVEN un TOML sans section [llm]
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm est None (pas d'erreur)
    #[test]
    fn test_ac3_no_llm_section_is_none() {
        // GIVEN
        let toml_str = r#"
[agents]
directory = "agents/"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        assert!(
            config.llm.is_none(),
            "llm doit être None si section absente"
        );
    }

    // GIVEN une LlmConfig minimale (cloud, cloud backend sans clé API)
    // WHEN on appelle validate_llm_config
    // THEN Ok(()) est retourné — jamais d'erreur fatale (AC-4)
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac4_validate_llm_config_always_returns_ok() {
        // GIVEN — un backend dont la clé API est probablement absente en CI
        let toml_str = r#"
[llm]
default = "test-api"

[[llm.backends]]
name = "test-api"
type = "api"
api_url = "https://api.openai.com/v1"
api_key_env = "APOLLIA_NONEXISTENT_KEY_FOR_TEST_XYZ"
model = "gpt-4o-mini"
"#;
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();
        let llm = config.llm.unwrap();

        // WHEN
        let result = validate_llm_config(&llm);

        // THEN — toujours Ok, même si la clé est absente
        assert!(
            result.is_ok(),
            "validate_llm_config doit retourner Ok même avec une clé API manquante"
        );
    }

    // GIVEN un TOML avec [llm] mais sans [llm.observability]
    // WHEN on désérialise
    // THEN les valeurs par défaut sont log_token_usage=true, log_latency=true, debug_log_prompt=false
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac5_observability_defaults() {
        // GIVEN — section [llm.observability] absente
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let obs = &config.llm.unwrap().observability;
        assert!(
            obs.log_token_usage,
            "log_token_usage doit valoir true par défaut"
        );
        assert!(obs.log_latency, "log_latency doit valoir true par défaut");
        assert!(
            !obs.debug_log_prompt,
            "debug_log_prompt doit valoir false par défaut"
        );
    }

    // GIVEN un chemin commençant par ~/
    // WHEN on appelle expand_tilde
    // THEN le ~ est remplacé et le chemin ne contient plus ~
    #[test]
    fn test_expand_tilde_replaces_home() {
        // GIVEN
        let path = Path::new("~/.apollia/models/test.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        let s = expanded.to_string_lossy();
        assert!(!s.contains('~'), "le ~ doit être remplacé, obtenu : {s}");
        assert!(s.contains(".apollia"), "le chemin doit contenir .apollia");
    }

    // GIVEN un chemin sans ~ (chemin absolu)
    // WHEN on appelle expand_tilde
    // THEN le chemin est retourné inchangé
    #[test]
    fn test_expand_tilde_noop_for_absolute_path() {
        // GIVEN
        let path = Path::new("/tmp/model.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        assert_eq!(expanded, PathBuf::from("/tmp/model.gguf"));
    }

    // ─── STORY-071 — Parsing [[triggers]] ───────────────────────────────────

    /// Écrit le contenu dans un fichier temporaire et retourne le handle.
    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f
    }

    // GIVEN un apollia.toml sans section [[triggers]]
    // WHEN parse_apollia_toml est appelé
    // THEN config.triggers est un Vec vide — pas d'erreur (AC-1)
    #[test]
    fn test_ac1_no_triggers_section_returns_empty_vec() {
        // GIVEN
        let file = write_toml("[agents]\ndirectory = \"agents/\"\n");

        // WHEN
        let config = parse_apollia_toml(file.path()).unwrap();

        // THEN
        assert!(
            config.triggers.is_empty(),
            "triggers doit être vide quand la section est absente"
        );
    }

    // GIVEN un trigger cron valide (expression 5-champs unix standard)
    // WHEN parse_apollia_toml est appelé
    // THEN le trigger est parsé avec le bon id (AC-2)
    #[test]
    fn test_ac2_valid_cron_trigger_parsed() {
        // GIVEN
        let toml = r#"
[[triggers]]
id             = "rapport-hebdo"
agent          = "rapport-agent"
enabled        = true
on_busy        = "queue"
input_template = "Rapport du {{scheduled_at}}"

[triggers.source]
type     = "cron"
schedule = "0 8 * * MON"
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).unwrap();

        // THEN
        assert_eq!(config.triggers.len(), 1, "un trigger doit être présent");
        assert_eq!(
            config.triggers[0].id, "rapport-hebdo",
            "id incorrect : {:?}",
            config.triggers[0].id
        );
        assert_eq!(config.triggers[0].agent, "rapport-agent");
        assert!(config.triggers[0].enabled);
    }

    // GIVEN un trigger cron avec un schedule invalide
    // WHEN parse_apollia_toml est appelé
    // THEN une erreur contenant le trigger_id est retournée (AC-3)
    #[test]
    fn test_ac3_invalid_cron_schedule_returns_error_with_id() {
        // GIVEN
        let toml = r#"
[[triggers]]
id             = "bad-trigger"
agent          = "some-agent"
enabled        = true
on_busy        = "queue"
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "not-a-cron"
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN
        assert!(result.is_err(), "une erreur doit être retournée");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("bad-trigger"),
            "le message doit contenir le trigger_id, obtenu : {err_msg}"
        );
    }

    // GIVEN un trigger webhook avec secret vide
    // WHEN parse_apollia_toml est appelé
    // THEN une erreur contenant le trigger_id est retournée (AC-4)
    #[test]
    fn test_ac4_empty_webhook_secret_returns_error_with_id() {
        // GIVEN
        let toml = r#"
[[triggers]]
id             = "crm-sync"
agent          = "crm-agent"
enabled        = true
on_busy        = "queue"
input_template = "{{body}}"

[triggers.source]
type   = "webhook"
secret = ""
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN
        assert!(
            result.is_err(),
            "une erreur doit être retournée pour secret vide"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("crm-sync"),
            "le message doit contenir le trigger_id, obtenu : {err_msg}"
        );
    }

    // GIVEN un trigger avec enabled=false et schedule invalide
    // WHEN parse_apollia_toml est appelé
    // THEN pas d'erreur — la source n'est pas validée pour les triggers désactivés (AC-6)
    #[test]
    fn test_ac6_disabled_trigger_skips_source_validation() {
        // GIVEN — schedule invalide mais trigger désactivé
        let toml = r#"
[[triggers]]
id             = "disabled-trigger"
agent          = "some-agent"
enabled        = false
on_busy        = "drop"
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "invalid-schedule"
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN — pas d'erreur, le trigger est désactivé
        assert!(
            result.is_ok(),
            "pas d'erreur attendue pour trigger désactivé, erreur : {:?}",
            result.err()
        );
        let config = result.unwrap();
        assert!(
            !config.triggers[0].enabled,
            "le trigger doit être marqué disabled"
        );
        assert_eq!(config.triggers[0].id, "disabled-trigger");
    }

    // GIVEN un trigger interval avec format valide
    // WHEN parse_apollia_toml est appelé
    // THEN le trigger interval est parsé correctement
    #[test]
    fn test_valid_interval_trigger_parsed() {
        // GIVEN
        let toml = r#"
[[triggers]]
id             = "sync-crm"
agent          = "crm-agent"
enabled        = true
on_busy        = "drop"
input_template = "Sync {{fired_at}}"

[triggers.source]
type  = "interval"
every = "30m"
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).unwrap();

        // THEN
        assert_eq!(config.triggers.len(), 1);
        assert_eq!(config.triggers[0].id, "sync-crm");
        assert!(matches!(
            config.triggers[0].on_busy,
            apollia_triggers::OnBusyPolicy::Drop
        ));
        assert!(matches!(
            &config.triggers[0].source,
            apollia_triggers::TriggerSourceConfig::Interval { every } if every == "30m"
        ));
    }
}
