//! Parsing TOML de la section `[[triggers]]` depuis le contenu d'`apollia.toml`.
//!
//! Utilisé par `apollia-runtime` pour le hot reload via `POST /api/v1/triggers/reload`
//! sans dépendre de `apollia-cli` (qui créerait une dépendance circulaire).
//!
//! La validation suit le **Principe #4 — Fail fast** : toute erreur de configuration
//! est détectée avant de transmettre les définitions au [`TriggerEngine`].

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerSourceConfig,
};

// ─── Erreur ───────────────────────────────────────────────────────────────────

/// Erreurs retournées par [`parse_triggers_from_toml_str`].
#[derive(Debug, thiserror::Error)]
pub enum TriggerTomlError {
    /// Le contenu TOML est malformé (syntaxe invalide, type incorrect).
    #[error("invalid TOML: {0}")]
    Parse(#[from] toml::de::Error),

    /// Un trigger a une configuration sémantiquement invalide.
    #[error("invalid trigger '{id}': {reason}")]
    InvalidTrigger {
        /// Identifiant du trigger fautif.
        id: String,
        /// Description de l'erreur de validation.
        reason: String,
    },
}

// ─── Types bruts TOML ─────────────────────────────────────────────────────────

/// Structure racine pour désérialiser uniquement la section `[[triggers]]`.
#[derive(Debug, Deserialize)]
struct RawRoot {
    #[serde(default)]
    triggers: Vec<RawTrigger>,
}

/// Format brut d'un trigger dans le TOML avant validation sémantique.
#[derive(Debug, Deserialize)]
struct RawTrigger {
    id: String,
    /// Agent cible — exclusif avec `pipeline`.
    #[serde(default)]
    agent: String,
    /// Pipeline cible — exclusif avec `agent`.
    #[serde(default)]
    pipeline: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    on_busy: String,
    input_template: String,
    source: RawTriggerSource,
}

/// Format brut de la sous-section `source` d'un trigger.
#[derive(Debug, Deserialize)]
struct RawTriggerSource {
    #[serde(rename = "type")]
    kind: String,
    schedule: Option<String>,
    every: Option<String>,
    fire_at: Option<String>,
    path: Option<String>,
    events: Option<Vec<String>>,
    secret: Option<String>,
    /// Suivre les symlinks pour les sources `file_watch` (défaut : false).
    #[serde(default)]
    follow_symlinks: bool,
    /// Patterns d'exclusion pour les sources `file_watch`.
    /// Absent du TOML → défauts appliqués.
    exclude_patterns: Option<Vec<String>>,
}

/// Retourne `true` — valeur par défaut pour le champ `enabled`.
fn default_true() -> bool {
    true
}

// ─── API publique ─────────────────────────────────────────────────────────────

/// Parse la section `[[triggers]]` depuis le contenu TOML d'un `apollia.toml`.
///
/// Valide sémantiquement chaque trigger activé (schedule cron, secret webhook, etc.).
/// Les triggers avec `enabled = false` ne sont pas validés sémantiquement.
///
/// # Erreurs
///
/// - [`TriggerTomlError::Parse`] — TOML malformé.
/// - [`TriggerTomlError::InvalidTrigger`] — trigger avec configuration invalide.
pub fn parse_triggers_from_toml_str(
    toml_str: &str,
) -> Result<Vec<TriggerDefinition>, TriggerTomlError> {
    let raw: RawRoot = toml::from_str(toml_str)?;
    raw.triggers.iter().map(validate_trigger).collect()
}

// ─── Validation ───────────────────────────────────────────────────────────────

/// Valide un trigger brut et le convertit en [`TriggerDefinition`].
fn validate_trigger(raw: &RawTrigger) -> Result<TriggerDefinition, TriggerTomlError> {
    if raw.id.is_empty() {
        return Err(TriggerTomlError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyId.to_string(),
        });
    }
    // Validation agent : requis uniquement si `pipeline` est absent.
    if raw.pipeline.is_none() && raw.agent.is_empty() {
        return Err(TriggerTomlError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyAgent.to_string(),
        });
    }

    let on_busy = match raw.on_busy.as_str() {
        "skip" | "drop" => OnBusyPolicy::Skip,
        "block" => OnBusyPolicy::Block,
        _ => OnBusyPolicy::Queue {
            max_depth: crate::DEFAULT_QUEUE_MAX_DEPTH,
        },
    };

    let source = if raw.enabled {
        validate_trigger_source(&raw.id, &raw.source)?
    } else {
        parse_trigger_source_unchecked(&raw.source)
    };

    Ok(TriggerDefinition {
        id: raw.id.clone(),
        agent: raw.agent.clone(),
        pipeline: raw.pipeline.clone(),
        enabled: raw.enabled,
        on_busy,
        source,
        input_template: InputTemplate(raw.input_template.clone()),
    })
}

/// Valide sémantiquement la source d'un trigger activé.
fn validate_trigger_source(
    id: &str,
    raw: &RawTriggerSource,
) -> Result<TriggerSourceConfig, TriggerTomlError> {
    match raw.kind.as_str() {
        "cron" => {
            let schedule = raw.schedule.clone().unwrap_or_default();
            let normalized = normalize_cron(id, &schedule)?;
            Ok(TriggerSourceConfig::Cron {
                schedule: normalized,
            })
        }

        "interval" => {
            let every = raw.every.clone().unwrap_or_default();
            parse_interval(&every).map_err(|e| TriggerTomlError::InvalidTrigger {
                id: id.to_string(),
                reason: e.to_string(),
            })?;
            Ok(TriggerSourceConfig::Interval { every })
        }

        "oneshot" => {
            let fire_at_str = raw.fire_at.clone().unwrap_or_default();
            let fire_at = fire_at_str
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|e| TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: format!("invalid fire_at timestamp: {e}"),
                })?;
            Ok(TriggerSourceConfig::Oneshot { fire_at })
        }

        "file_watch" => {
            let path_str = raw.path.clone().unwrap_or_default();
            if path_str.is_empty() {
                return Err(TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyFileWatchPath.to_string(),
                });
            }
            let path = expand_tilde(Path::new(&path_str));
            let events = parse_file_event_kinds(raw.events.as_deref().unwrap_or(&[]));
            let exclude_patterns = raw
                .exclude_patterns
                .clone()
                .unwrap_or_else(crate::config::default_exclude_patterns);
            Ok(TriggerSourceConfig::FileWatch {
                path,
                events,
                follow_symlinks: raw.follow_symlinks,
                exclude_patterns,
            })
        }

        "webhook" => {
            let secret = raw.secret.clone().unwrap_or_default();
            if secret.is_empty() {
                return Err(TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyWebhookSecret.to_string(),
                });
            }
            Ok(TriggerSourceConfig::Webhook { secret })
        }

        unknown => Err(TriggerTomlError::InvalidTrigger {
            id: id.to_string(),
            reason: format!("unknown source type '{unknown}'"),
        }),
    }
}

/// Valide et normalise une expression cron (5 ou 6 champs).
fn normalize_cron(id: &str, schedule: &str) -> Result<String, TriggerTomlError> {
    if cron::Schedule::from_str(schedule).is_ok() {
        return Ok(schedule.to_string());
    }
    let field_count = schedule.split_whitespace().count();
    if field_count == 5 {
        let normalized = format!("0 {schedule}");
        if cron::Schedule::from_str(&normalized).is_ok() {
            return Ok(normalized);
        }
    }
    let reason = cron::Schedule::from_str(schedule)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "invalid cron expression".to_string());
    Err(TriggerTomlError::InvalidTrigger {
        id: id.to_string(),
        reason: TriggerDefinitionError::InvalidCronSchedule {
            schedule: schedule.to_string(),
            reason,
        }
        .to_string(),
    })
}

/// Convertit des noms d'événements fichier en [`FileEventKind`].
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

/// Parsing minimal d'une source sans validation — pour les triggers désactivés.
fn parse_trigger_source_unchecked(raw: &RawTriggerSource) -> TriggerSourceConfig {
    match raw.kind.as_str() {
        "cron" => TriggerSourceConfig::Cron {
            schedule: raw.schedule.clone().unwrap_or_default(),
        },
        "interval" => TriggerSourceConfig::Interval {
            every: raw.every.clone().unwrap_or_default(),
        },
        "oneshot" => {
            let fire_at = raw
                .fire_at
                .as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            TriggerSourceConfig::Oneshot { fire_at }
        }
        "file_watch" => {
            let path = expand_tilde(Path::new(raw.path.as_deref().unwrap_or("")));
            let events = parse_file_event_kinds(raw.events.as_deref().unwrap_or(&[]));
            let exclude_patterns = raw
                .exclude_patterns
                .clone()
                .unwrap_or_else(crate::config::default_exclude_patterns);
            TriggerSourceConfig::FileWatch {
                path,
                events,
                follow_symlinks: raw.follow_symlinks,
                exclude_patterns,
            }
        }
        _ => TriggerSourceConfig::Webhook {
            secret: raw.secret.clone().unwrap_or_default(),
        },
    }
}

/// Résout le composant `~` en tête de chemin vers `$HOME`.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
    } else {
        path.to_path_buf()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_cron_trigger() {
        // GIVEN un TOML avec un trigger cron valide
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
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN
        let defs = result.expect("doit parser sans erreur");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, "rapport-hebdo");
        assert!(defs[0].enabled);
    }

    #[test]
    fn test_parse_invalid_cron_returns_error() {
        // GIVEN un trigger cron avec schedule invalide
        let toml = r#"
[[triggers]]
id             = "bad-cron"
agent          = "agent"
enabled        = true
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "not-valid"
"#;
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN erreur contenant le trigger_id
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("bad-cron"),
            "message doit contenir 'bad-cron': {msg}"
        );
    }

    #[test]
    fn test_parse_empty_triggers_returns_empty_vec() {
        // GIVEN un TOML sans section [[triggers]]
        let result = parse_triggers_from_toml_str("[agents]\ndirectory = \"agents/\"");
        // THEN vec vide, pas d'erreur
        let defs = result.expect("doit parser sans erreur");
        assert!(defs.is_empty());
    }

    #[test]
    fn test_parse_webhook_empty_secret_returns_error() {
        // GIVEN un trigger webhook avec secret vide
        let toml = r#"
[[triggers]]
id             = "crm-sync"
agent          = "crm-agent"
enabled        = true
input_template = "{{body}}"

[triggers.source]
type   = "webhook"
secret = ""
"#;
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN erreur
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("crm-sync"),
            "message doit contenir 'crm-sync': {msg}"
        );
    }
}
