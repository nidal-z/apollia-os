//! Types fondamentaux du système de triggers d'Apollia OS.
//!
//! Ce module définit le contrat stable utilisé par `TriggerEngine` et toutes les
//! sources de déclenchement. Aucune logique d'acteur ici — uniquement les types,
//! erreurs de validation, et la substitution de template.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Identifiant unique d'un trigger (chaîne non vide).
pub type TriggerId = String;

/// Définition complète d'un trigger telle que parsée depuis `apollia.toml`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerDefinition {
    /// Identifiant unique du trigger (non vide).
    pub id: TriggerId,
    /// Nom de l'agent cible — exclusif avec `pipeline`.
    ///
    /// Chaîne vide lorsque `pipeline` est défini (validation exclusive).
    pub agent: String,
    /// Pipeline cible — exclusif avec `agent`.
    ///
    /// Si défini, le trigger dispatche vers `PipelineEngine` au lieu du `TaskRouter`.
    /// `#[serde(default)]` garantit la compatibilité avec les configs existantes.
    #[serde(default)]
    pub pipeline: Option<String>,
    /// Indique si le trigger est actif.
    pub enabled: bool,
    /// Comportement quand l'agent cible est occupé.
    pub on_busy: OnBusyPolicy,
    /// Configuration de la source de déclenchement.
    pub source: TriggerSourceConfig,
    /// Template du message d'entrée envoyé à l'agent.
    pub input_template: InputTemplate,
}

impl TriggerDefinition {
    /// Valide que l'identifiant du trigger est non vide.
    ///
    /// Retourne [`TriggerDefinitionError::EmptyId`] si `id` est une chaîne vide.
    pub fn validate_id(id: &str) -> Result<(), TriggerDefinitionError> {
        if id.is_empty() {
            Err(TriggerDefinitionError::EmptyId)
        } else {
            Ok(())
        }
    }

    /// Valide que le nom d'agent est non vide.
    ///
    /// Retourne [`TriggerDefinitionError::EmptyAgent`] si `agent` est une chaîne vide.
    pub fn validate_agent(agent: &str) -> Result<(), TriggerDefinitionError> {
        if agent.is_empty() {
            Err(TriggerDefinitionError::EmptyAgent)
        } else {
            Ok(())
        }
    }
}

/// Comportement quand l'agent cible est déjà occupé au moment du fire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnBusyPolicy {
    /// Ignorer le trigger — comportement historique.
    Skip,
    /// Mettre en file d'attente bornée FIFO par agent.
    ///
    /// Quand l'agent est occupé, le trigger est mis en attente jusqu'à
    /// `max_depth` éléments. Au-delà, le trigger est droppé et
    /// `RuntimeEvent::TriggerQueueFull` est émis.
    Queue {
        /// Nombre maximum d'éléments dans la file par agent.
        max_depth: usize,
    },
    /// Bloquer jusqu'à disponibilité de l'agent (non implémenté).
    Block,
}

/// Configuration de la source de déclenchement d'un trigger.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerSourceConfig {
    /// Expression cron standard (ex : `"0 8 * * MON"`).
    Cron {
        /// Expression cron (ex : `"0 8 * * MON"`).
        schedule: String,
    },
    /// Intervalle périodique (ex : `"30m"`, `"1h"`, `"6h"`, `"1d"`).
    Interval {
        /// Durée sous forme de chaîne (ex : `"30m"`).
        every: String,
    },
    /// Déclenchement unique à une date/heure précise.
    Oneshot {
        /// Horodatage du déclenchement unique.
        fire_at: DateTime<Utc>,
    },
    /// Surveillance d'un chemin de fichier pour des événements système de fichiers.
    FileWatch {
        /// Chemin surveillé.
        path: PathBuf,
        /// Types d'événements déclencheurs.
        events: Vec<FileEventKind>,
        /// Suivre les liens symboliques (défaut : `false`).
        ///
        /// Lorsque `false`, les événements dont le chemin est un lien symbolique
        /// sont ignorés avant propagation. Détecté via `fs::symlink_metadata`.
        #[serde(default)]
        follow_symlinks: bool,
        /// Segments de chemin et patterns fichiers à exclure des événements.
        ///
        /// Défaut : `[".git", "node_modules", "__pycache__", ".apollia"]`.
        /// Pattern `"*.ext"` pour les extensions, `"nom"` ou `"nom/"` pour les segments.
        #[serde(default = "crate::config::default_exclude_patterns")]
        exclude_patterns: Vec<String>,
    },
    /// Webhook HTTP avec vérification HMAC-SHA256.
    Webhook {
        /// Secret partagé utilisé pour vérifier la signature HMAC-SHA256.
        secret: String,
    },
}

/// Type d'événement fichier filtré par `FileWatchTrigger`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    /// Création d'un fichier.
    Create,
    /// Modification d'un fichier.
    Modify,
    /// Suppression d'un fichier.
    Delete,
    /// Tout type d'événement.
    Any,
}

impl std::fmt::Display for FileEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileEventKind::Create => write!(f, "create"),
            FileEventKind::Modify => write!(f, "modify"),
            FileEventKind::Delete => write!(f, "delete"),
            FileEventKind::Any => write!(f, "any"),
        }
    }
}

/// Payload brut attaché à un [`TriggerEvent`].
#[derive(Debug, Clone, serde::Serialize)]
pub enum TriggerPayload {
    /// Payload d'un déclencheur temporel (cron, interval, oneshot).
    Timer {
        /// Heure planifiée du déclenchement.
        scheduled_at: DateTime<Utc>,
        /// Heure effective du déclenchement.
        fired_at: DateTime<Utc>,
    },
    /// Payload d'un événement système de fichiers.
    File {
        /// Chemin complet du fichier concerné.
        path: PathBuf,
        /// Nom du fichier uniquement (sans répertoire parent).
        filename: String,
        /// Taille en octets au moment de l'événement.
        size_bytes: u64,
        /// Type d'événement fichier.
        event_kind: FileEventKind,
    },
    /// Payload d'une requête webhook entrante.
    Webhook {
        /// Corps brut de la requête HTTP.
        body: String,
        /// En-têtes HTTP de la requête.
        headers: HashMap<String, String>,
    },
}

/// Événement produit par une source de déclenchement et consommé par `TriggerEngine`.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    /// Identifiant du trigger qui a produit cet événement.
    pub trigger_id: TriggerId,
    /// Nom de l'agent cible.
    pub agent: String,
    /// Payload brut de l'événement.
    pub payload: TriggerPayload,
    /// Heure effective du fire.
    pub fired_at: DateTime<Utc>,
}

/// Template de message d'entrée avec substitution de `{{variables}}`.
///
/// Variables disponibles selon le type de payload :
/// - `Timer`   : `{{scheduled_at}}`, `{{fired_at}}`
/// - `File`    : `{{path}}`, `{{filename}}`, `{{size_bytes}}`, `{{event_kind}}`
/// - `Webhook` : `{{body}}`, `{{header.<name>}}`
///
/// Les variables absentes du payload sont remplacées par une chaîne vide (pas de panique).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputTemplate(pub String);

impl InputTemplate {
    /// Substitue les `{{variables}}` présentes dans le template avec les valeurs du payload.
    ///
    /// Les variables inconnues (absentes de la map construite depuis le payload) sont
    /// remplacées par une chaîne vide. Cette méthode ne peut jamais paniquer.
    pub fn render(&self, payload: &TriggerPayload) -> String {
        let vars = build_payload_vars(payload);
        let mut result = self.0.clone();
        for (key, value) in &vars {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        replace_unknown_vars(result)
    }
}

/// Construit la map `variable → valeur` depuis un [`TriggerPayload`].
fn build_payload_vars(payload: &TriggerPayload) -> HashMap<String, String> {
    let mut map = HashMap::new();
    match payload {
        TriggerPayload::Timer {
            scheduled_at,
            fired_at,
        } => {
            map.insert("scheduled_at".into(), scheduled_at.to_rfc3339());
            map.insert("fired_at".into(), fired_at.to_rfc3339());
        }
        TriggerPayload::File {
            path,
            filename,
            size_bytes,
            event_kind,
        } => {
            map.insert("path".into(), path.to_string_lossy().into_owned());
            map.insert("filename".into(), filename.clone());
            map.insert("size_bytes".into(), size_bytes.to_string());
            map.insert("event_kind".into(), event_kind.to_string());
        }
        TriggerPayload::Webhook { body, headers } => {
            map.insert("body".into(), body.clone());
            for (name, value) in headers {
                map.insert(format!("header.{}", name), value.clone());
            }
        }
    }
    map
}

/// Remplace les motifs `{{...}}` restants (variables inconnues) par une chaîne vide.
///
/// Utilise une recherche séquentielle pour préserver les caractères UTF-8.
fn replace_unknown_vars(mut s: String) -> String {
    while let Some(start) = s.find("{{") {
        match s[start..].find("}}") {
            Some(end_rel) => {
                let end = start + end_rel + 2;
                s.replace_range(start..end, "");
            }
            None => break,
        }
    }
    s
}

/// Parse une chaîne d'intervalle au format `"30m"`, `"1h"`, `"6h"` ou `"1d"`.
///
/// Retourne [`TriggerDefinitionError::InvalidInterval`] si le format est invalide.
/// Les unités reconnues sont : `m` (minutes), `h` (heures), `d` (jours).
pub fn parse_interval(value: &str) -> Result<Duration, TriggerDefinitionError> {
    // Handle "ms" (milliseconds) before single-char suffixes to avoid ambiguity
    if let Some(s) = value.strip_suffix("ms") {
        let n: u64 = s
            .parse()
            .map_err(|_| TriggerDefinitionError::InvalidInterval {
                value: value.to_string(),
            })?;
        return Ok(Duration::from_millis(n));
    }

    let (num_str, multiplier) = if let Some(s) = value.strip_suffix('m') {
        (s, 60u64)
    } else if let Some(s) = value.strip_suffix('h') {
        (s, 3_600u64)
    } else if let Some(s) = value.strip_suffix('d') {
        (s, 86_400u64)
    } else {
        return Err(TriggerDefinitionError::InvalidInterval {
            value: value.to_string(),
        });
    };

    let n: u64 = num_str
        .parse()
        .map_err(|_| TriggerDefinitionError::InvalidInterval {
            value: value.to_string(),
        })?;

    Ok(Duration::from_secs(n * multiplier))
}

/// Erreurs de validation d'une [`TriggerDefinition`].
#[derive(thiserror::Error, Debug)]
pub enum TriggerDefinitionError {
    /// L'identifiant du trigger est vide.
    #[error("trigger id cannot be empty")]
    EmptyId,

    /// Le nom de l'agent cible est vide.
    #[error("agent name cannot be empty")]
    EmptyAgent,

    /// L'expression cron est invalide.
    #[error("invalid cron schedule '{schedule}': {reason}")]
    InvalidCronSchedule {
        /// Expression cron invalide.
        schedule: String,
        /// Raison de l'invalidité.
        reason: String,
    },

    /// L'intervalle ne respecte pas le format attendu.
    #[error("invalid interval '{value}': expected format '30m', '1h', '6h' or '1d'")]
    InvalidInterval {
        /// Valeur reçue.
        value: String,
    },

    /// Le secret webhook est vide.
    #[error("webhook secret cannot be empty")]
    EmptyWebhookSecret,

    /// Le chemin de surveillance FileWatch est vide.
    #[error("file_watch path is empty")]
    EmptyFileWatchPath,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_ac2_render_timer_variables() {
        // GIVEN
        let template = InputTemplate("Rapport du {{scheduled_at}} — généré à {{fired_at}}".into());
        let scheduled = Utc.with_ymd_and_hms(2026, 3, 9, 8, 0, 0).unwrap();
        let fired = Utc.with_ymd_and_hms(2026, 3, 9, 8, 0, 1).unwrap();
        let payload = TriggerPayload::Timer {
            scheduled_at: scheduled,
            fired_at: fired,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert!(result.contains("2026-03-09"), "résultat: {result}");
        assert!(
            !result.contains("{{"),
            "des variables non substituées restent: {result}"
        );
    }

    #[test]
    fn test_ac3_render_file_variables() {
        // GIVEN
        let template =
            InputTemplate("Nouvelle facture : {{filename}} ({{size_bytes}} octets)".into());
        let payload = TriggerPayload::File {
            path: "/inbox/facture-001.pdf".into(),
            filename: "facture-001.pdf".into(),
            size_bytes: 42100,
            event_kind: FileEventKind::Create,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, "Nouvelle facture : facture-001.pdf (42100 octets)");
    }

    #[test]
    fn test_ac4_unknown_variable_replaced_by_empty() {
        // GIVEN
        let template = InputTemplate("{{unknown}} texte".into());
        let payload = TriggerPayload::Timer {
            scheduled_at: Utc::now(),
            fired_at: Utc::now(),
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, " texte");
        assert!(
            !result.contains("{{"),
            "des patterns {{}} restent: {result}"
        );
    }

    #[test]
    fn test_ac5_empty_id_returns_error() {
        // GIVEN / WHEN
        let result = TriggerDefinition::validate_id("");
        // THEN
        assert!(matches!(result, Err(TriggerDefinitionError::EmptyId)));
    }

    #[test]
    fn test_ac6_trigger_definition_json_roundtrip() {
        // GIVEN
        let def = TriggerDefinition {
            id: "rapport-hebdo".into(),
            agent: "rapport-agent".into(),
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("Rapport du {{scheduled_at}}".into()),
        };
        // WHEN
        let json = serde_json::to_string(&def).expect("sérialisation JSON");
        let back: TriggerDefinition = serde_json::from_str(&json).expect("désérialisation JSON");
        // THEN
        assert_eq!(back.id, "rapport-hebdo");
        assert_eq!(back.agent, "rapport-agent");
        assert!(back.pipeline.is_none());
        assert!(back.enabled);
        assert_eq!(back.on_busy, OnBusyPolicy::Queue { max_depth: 10 });
    }

    // ── Triggers → Pipelines ─────────────────────────────────────────────

    /// Le champ `pipeline` est backward-compatible — absent = None, pas d'erreur.
    #[test]
    fn test_ac1_pipeline_field_backward_compatible() {
        // GIVEN un JSON sans champ pipeline (config existante Sprint 9)
        let json = r#"{
            "id": "t1",
            "agent": "hello-agent",
            "enabled": true,
            "on_busy": {"queue": {"max_depth": 5}},
            "source": {"type": "interval", "every": "30m"},
            "input_template": "test"
        }"#;
        // WHEN
        let def: TriggerDefinition = serde_json::from_str(json).expect("désérialisation");
        // THEN pipeline == None — pas d'erreur, comportement inchangé
        assert!(
            def.pipeline.is_none(),
            "pipeline doit être None si absent du JSON"
        );
        assert_eq!(def.agent, "hello-agent");
    }

    /// Le champ `pipeline` est correctement désérialisé quand présent.
    #[test]
    fn test_pipeline_field_deserialized() {
        // GIVEN un JSON avec champ pipeline
        let json = r#"{
            "id": "t1",
            "agent": "",
            "pipeline": "traitement-facture",
            "enabled": true,
            "on_busy": {"queue": {"max_depth": 5}},
            "source": {"type": "interval", "every": "30m"},
            "input_template": "{{filename}}"
        }"#;
        // WHEN
        let def: TriggerDefinition = serde_json::from_str(json).expect("désérialisation");
        // THEN
        assert_eq!(
            def.pipeline.as_deref(),
            Some("traitement-facture"),
            "pipeline doit être Some(\"traitement-facture\")"
        );
    }

    #[test]
    fn test_render_webhook_body_variable() {
        // GIVEN
        let template = InputTemplate("{{body}}".into());
        let payload = TriggerPayload::Webhook {
            body: r#"{"event":"sale"}"#.into(),
            headers: HashMap::new(),
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, r#"{"event":"sale"}"#);
    }

    #[test]
    fn test_render_webhook_header_variable() {
        // GIVEN
        let template = InputTemplate("Content-Type: {{header.content-type}}".into());
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let payload = TriggerPayload::Webhook {
            body: "{}".into(),
            headers,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, "Content-Type: application/json");
    }

    #[test]
    fn test_parse_interval_valid_formats() {
        // GIVEN / WHEN / THEN
        assert_eq!(parse_interval("30m").unwrap(), Duration::from_secs(1_800));
        assert_eq!(parse_interval("1h").unwrap(), Duration::from_secs(3_600));
        assert_eq!(parse_interval("6h").unwrap(), Duration::from_secs(21_600));
        assert_eq!(parse_interval("1d").unwrap(), Duration::from_secs(86_400));
    }

    #[test]
    fn test_parse_interval_invalid_returns_error() {
        // GIVEN / WHEN
        let result = parse_interval("invalid");
        // THEN
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::InvalidInterval { .. })
        ));
    }

    #[test]
    fn test_validate_agent_empty_returns_error() {
        // GIVEN / WHEN
        let result = TriggerDefinition::validate_agent("");
        // THEN
        assert!(matches!(result, Err(TriggerDefinitionError::EmptyAgent)));
    }
}
