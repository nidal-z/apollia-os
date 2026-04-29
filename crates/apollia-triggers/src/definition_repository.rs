//! Repository CRUD SQLite pour les définitions de triggers.
//!
//! Ce module fournit [`TriggerDefinitionRepository`], une interface de persistance
//! synchrone pour les définitions de triggers dans `triggers.db`. La validation
//! métier est déléguée au module [`crate::validation`] et exécutée automatiquement
//! avant chaque écriture (insert/update).
//!
//! Ce repository est utilisé par le boot Supervisor et les routes
//! REST CRUD.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::types::{
    FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition, TriggerSourceConfig,
};
use crate::validation;

// ─── Migration ───────────────────────────────────────────────────────────────

/// Migration idempotente pour la table `trigger_definitions`.
const MIGRATION_008: &str = "\
CREATE TABLE IF NOT EXISTS trigger_definitions (
    id              TEXT PRIMARY KEY,
    agent           TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    on_busy         TEXT NOT NULL DEFAULT 'queue',
    source_type     TEXT NOT NULL,
    source_config   TEXT NOT NULL,
    input_template  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_busy IN ('queue', 'drop')),
    CHECK (source_type IN ('cron', 'interval', 'oneshot', 'file_watch', 'webhook'))
);";

// ─── Types ───────────────────────────────────────────────────────────────────

/// Définition persistée d'un trigger (représentation SQLite à plat).
///
/// Contrairement à [`crate::types::TriggerDefinition`] qui utilise des types
/// riches (`TriggerSourceConfig`, `OnBusyPolicy`), cette struct utilise des
/// types plats (`source_type` + `source_config` JSON) adaptés au stockage SQLite
/// et à la sérialisation REST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDefinitionRow {
    /// Identifiant unique du trigger (clé primaire).
    pub id: String,
    /// Agent cible.
    pub agent: Option<String>,
    /// Indique si le trigger est actif.
    pub enabled: bool,
    /// Comportement quand l'agent cible est occupé.
    pub on_busy: OnBusy,
    /// Type de source : `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    pub source_type: String,
    /// Configuration JSON de la source (structure dépend du `source_type`).
    pub source_config: serde_json::Value,
    /// Template du message d'entrée (substitution `{{variables}}`).
    pub input_template: Option<String>,
    /// Horodatage de création (ISO 8601, renseigné automatiquement).
    pub created_at: String,
    /// Horodatage de dernière modification (ISO 8601, rafraîchi à chaque update).
    pub updated_at: String,
}

/// Comportement quand l'agent cible est occupé au moment du fire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnBusy {
    /// Soumet quand même — `TaskRouter` gère la file d'attente.
    Queue,
    /// Ignore le fire et trace "skipped" dans l'audit.
    Drop,
}

impl OnBusy {
    /// Retourne la représentation SQLite du comportement on_busy.
    fn as_sql(&self) -> &'static str {
        match self {
            OnBusy::Queue => "queue",
            OnBusy::Drop => "drop",
        }
    }

    /// Parse une valeur SQLite en [`OnBusy`], défaut `Queue` si non reconnu.
    fn from_sql(s: &str) -> Self {
        match s {
            "drop" => OnBusy::Drop,
            _ => OnBusy::Queue,
        }
    }
}

// ─── Conversion Row → TriggerDefinition ─────────────────────────────────────

impl TryFrom<TriggerDefinitionRow> for TriggerDefinition {
    type Error = TriggerDefinitionError;

    /// Convertit une [`TriggerDefinitionRow`] persistée en [`TriggerDefinition`] riche.
    ///
    /// Parse `source_type` + `source_config` JSON en [`TriggerSourceConfig`] typé.
    /// Retourne [`TriggerDefinitionError::ValidationError`] si la conversion échoue.
    fn try_from(row: TriggerDefinitionRow) -> Result<Self, Self::Error> {
        let source = parse_source_config(&row.source_type, &row.source_config)?;
        let on_busy = match row.on_busy {
            OnBusy::Queue => OnBusyPolicy::Queue {
                max_depth: crate::DEFAULT_QUEUE_MAX_DEPTH,
            },
            OnBusy::Drop => OnBusyPolicy::Skip,
        };
        Ok(TriggerDefinition {
            id: row.id,
            agent: row.agent.unwrap_or_default(),
            enabled: row.enabled,
            on_busy,
            source,
            input_template: InputTemplate(row.input_template.unwrap_or_default()),
        })
    }
}

/// Parse `source_type` et `source_config` JSON en [`TriggerSourceConfig`].
fn parse_source_config(
    source_type: &str,
    source_config: &serde_json::Value,
) -> Result<TriggerSourceConfig, TriggerDefinitionError> {
    match source_type {
        "cron" => {
            let schedule = source_config
                .get("schedule")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(TriggerSourceConfig::Cron { schedule })
        }
        "interval" => {
            let every = source_config
                .get("every")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(TriggerSourceConfig::Interval { every })
        }
        "oneshot" => {
            let fire_at_str = source_config
                .get("fire_at")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TriggerDefinitionError::ValidationError(
                        "oneshot source requires 'fire_at' field".into(),
                    )
                })?;
            let fire_at = fire_at_str.parse().map_err(|e| {
                TriggerDefinitionError::ValidationError(format!("invalid fire_at datetime: {e}"))
            })?;
            Ok(TriggerSourceConfig::Oneshot { fire_at })
        }
        "file_watch" => {
            let path = source_config
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let events: Vec<FileEventKind> = source_config
                .get("events")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| vec![FileEventKind::Any]);
            let follow_symlinks = source_config
                .get("follow_symlinks")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let exclude_patterns = source_config
                .get("exclude_patterns")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                .unwrap_or_else(crate::config::default_exclude_patterns);
            Ok(TriggerSourceConfig::FileWatch {
                path: std::path::PathBuf::from(path),
                events,
                follow_symlinks,
                exclude_patterns,
            })
        }
        "webhook" => {
            let secret = source_config
                .get("secret")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(TriggerSourceConfig::Webhook { secret })
        }
        other => Err(TriggerDefinitionError::ValidationError(format!(
            "unknown source_type: {other}"
        ))),
    }
}

// ─── Erreurs ─────────────────────────────────────────────────────────────────

/// Erreurs du repository de définitions de triggers.
#[derive(Debug, thiserror::Error)]
pub enum TriggerDefinitionError {
    /// Le trigger demandé n'existe pas.
    #[error("trigger not found: {0}")]
    NotFound(String),
    /// Un trigger avec cet identifiant existe déjà.
    #[error("duplicate trigger id: {0}")]
    DuplicateId(String),
    /// La définition ne satisfait pas les règles de validation métier.
    #[error("validation error: {0}")]
    ValidationError(String),
    /// Erreur SQLite sous-jacente.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

// ─── Repository ──────────────────────────────────────────────────────────────

/// Repository CRUD pour les définitions de triggers dans SQLite.
///
/// Struct synchrone (pas d'acteur Tokio). La connexion SQLite est `Send`,
/// compatible avec `spawn_blocking` si nécessaire.
pub struct TriggerDefinitionRepository {
    conn: Connection,
}

impl TriggerDefinitionRepository {
    /// Ouvre (ou crée) la base SQLite et applique la migration idempotente.
    ///
    /// Active WAL pour de meilleures performances en écriture concurrente.
    /// Crée la table `trigger_definitions` si elle n'existe pas (idempotent).
    pub fn open(path: &Path) -> Result<Self, TriggerDefinitionError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_008)?;
        Ok(Self { conn })
    }

    /// Insère une nouvelle définition de trigger après validation.
    ///
    /// Les champs `created_at` et `updated_at` sont renseignés automatiquement
    /// par les DEFAULT SQLite. Retourne [`TriggerDefinitionError::DuplicateId`]
    /// si l'identifiant existe déjà.
    pub fn insert(&self, def: &TriggerDefinitionRow) -> Result<(), TriggerDefinitionError> {
        validation::validate_trigger(def)?;

        let source_config_json = serde_json::to_string(&def.source_config).map_err(|e| {
            TriggerDefinitionError::ValidationError(format!("invalid source_config JSON: {e}"))
        })?;

        self.conn
            .execute(
                "INSERT INTO trigger_definitions \
                 (id, agent, enabled, on_busy, source_type, source_config, input_template) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    def.id,
                    def.agent,
                    def.enabled,
                    def.on_busy.as_sql(),
                    def.source_type,
                    source_config_json,
                    def.input_template,
                ],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(err, _) = &e {
                    if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                        return TriggerDefinitionError::DuplicateId(def.id.clone());
                    }
                }
                TriggerDefinitionError::Database(e)
            })?;

        Ok(())
    }

    /// Met à jour une définition existante après validation.
    ///
    /// Rafraîchit `updated_at` automatiquement. Retourne
    /// [`TriggerDefinitionError::NotFound`] si l'identifiant n'existe pas.
    pub fn update(
        &self,
        id: &str,
        def: &TriggerDefinitionRow,
    ) -> Result<(), TriggerDefinitionError> {
        validation::validate_trigger(def)?;

        let source_config_json = serde_json::to_string(&def.source_config).map_err(|e| {
            TriggerDefinitionError::ValidationError(format!("invalid source_config JSON: {e}"))
        })?;

        let rows = self.conn.execute(
            "UPDATE trigger_definitions \
             SET agent = ?1, enabled = ?2, on_busy = ?3, \
                 source_type = ?4, source_config = ?5, input_template = ?6, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?7",
            params![
                def.agent,
                def.enabled,
                def.on_busy.as_sql(),
                def.source_type,
                source_config_json,
                def.input_template,
                id,
            ],
        )?;

        if rows == 0 {
            return Err(TriggerDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Supprime une définition de trigger.
    ///
    /// Retourne [`TriggerDefinitionError::NotFound`] si l'identifiant n'existe pas.
    pub fn delete(&self, id: &str) -> Result<(), TriggerDefinitionError> {
        let rows = self
            .conn
            .execute("DELETE FROM trigger_definitions WHERE id = ?1", params![id])?;

        if rows == 0 {
            return Err(TriggerDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Retourne la définition d'un trigger par son identifiant.
    ///
    /// Retourne `None` si aucun trigger ne correspond.
    pub fn get(&self, id: &str) -> Result<Option<TriggerDefinitionRow>, TriggerDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, enabled, on_busy, source_type, \
                    source_config, input_template, created_at, updated_at \
             FROM trigger_definitions WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_definition)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Liste toutes les définitions de triggers, triées par identifiant.
    pub fn list(&self) -> Result<Vec<TriggerDefinitionRow>, TriggerDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, enabled, on_busy, source_type, \
                    source_config, input_template, created_at, updated_at \
             FROM trigger_definitions ORDER BY id",
        )?;

        let rows = stmt.query_map([], row_to_definition)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

/// Convertit une ligne SQLite en [`TriggerDefinitionRow`].
fn row_to_definition(row: &rusqlite::Row) -> rusqlite::Result<TriggerDefinitionRow> {
    let on_busy_str: String = row.get(3)?;
    let source_config_str: String = row.get(5)?;

    let source_config: serde_json::Value =
        serde_json::from_str(&source_config_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(TriggerDefinitionRow {
        id: row.get(0)?,
        agent: row.get(1)?,
        enabled: row.get(2)?,
        on_busy: OnBusy::from_sql(&on_busy_str),
        source_type: row.get(4)?,
        source_config,
        input_template: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Ouvre un repository de test dans un répertoire temporaire.
    fn open_test_repo() -> (TempDir, TriggerDefinitionRepository) {
        let dir = TempDir::new().expect("tempdir creation");
        let repo =
            TriggerDefinitionRepository::open(&dir.path().join("triggers.db")).expect("repo open");
        (dir, repo)
    }

    /// Crée une définition cron valide pour les tests.
    fn make_cron_def(id: &str, agent: &str, schedule: &str) -> TriggerDefinitionRow {
        TriggerDefinitionRow {
            id: id.to_string(),
            agent: Some(agent.to_string()),
            pipeline: None,
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": schedule }),
            input_template: Some("Rapport du {{scheduled_at}}".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    // ── Insert + Get ─────────────────────────────────────────────────

    #[test]
    fn test_insert_and_get() {
        // GIVEN un repository ouvert
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");

        // WHEN insert puis get
        repo.insert(&def).expect("insert");
        let got = repo.get("rapport-hebdo").expect("get");

        // THEN la définition est retrouvée avec les mêmes champs
        let got = got.expect("should exist");
        assert_eq!(got.id, "rapport-hebdo");
        assert_eq!(got.agent.as_deref(), Some("rapport-agent"));
        assert!(got.pipeline.is_none());
        assert!(got.enabled);
        assert_eq!(got.on_busy, OnBusy::Queue);
        assert_eq!(got.source_type, "cron");
        assert_eq!(
            got.source_config.get("schedule").and_then(|v| v.as_str()),
            Some("0 0 8 * * MON *")
        );
        assert!(!got.created_at.is_empty(), "created_at doit être renseigné");
        assert!(!got.updated_at.is_empty(), "updated_at doit être renseigné");
    }

    // ── Insert duplicate ID ──────────────────────────────────────────

    #[test]
    fn test_insert_duplicate_id() {
        // GIVEN un repository contenant "rapport-hebdo"
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "agent", "0 0 8 * * MON *");
        repo.insert(&def).expect("first insert");

        // WHEN insert avec le même ID
        let result = repo.insert(&def);

        // THEN erreur DuplicateId
        assert!(
            matches!(result, Err(TriggerDefinitionError::DuplicateId(ref id)) if id == "rapport-hebdo"),
            "expected DuplicateId, got: {result:?}"
        );
    }

    // ── Update existant ──────────────────────────────────────────────

    #[test]
    fn test_update_existing() {
        // GIVEN un trigger avec schedule "0 0 8 * * MON *"
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "agent", "0 0 8 * * MON *");
        repo.insert(&def).expect("insert");

        let original = repo.get("rapport-hebdo").expect("get").expect("exists");
        let original_updated_at = original.updated_at.clone();

        // WHEN update avec schedule "0 0 9 * * MON *"
        // (petit délai pour que updated_at change — SQLite subsecond precision)
        std::thread::sleep(std::time::Duration::from_millis(10));
        let updated_def = make_cron_def("rapport-hebdo", "agent", "0 0 9 * * MON *");
        repo.update("rapport-hebdo", &updated_def).expect("update");

        // THEN la source_config est mise à jour et updated_at rafraîchi
        let got = repo.get("rapport-hebdo").expect("get").expect("exists");
        assert_eq!(
            got.source_config.get("schedule").and_then(|v| v.as_str()),
            Some("0 0 9 * * MON *")
        );
        assert!(
            got.updated_at >= original_updated_at,
            "updated_at doit être rafraîchi"
        );
    }

    // ── Update ID inexistant ─────────────────────────────────────────

    #[test]
    fn test_update_not_found() {
        // GIVEN un repository vide
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("inconnu", "agent", "0 0 8 * * MON *");

        // WHEN update("inconnu")
        let result = repo.update("inconnu", &def);

        // THEN erreur NotFound
        assert!(
            matches!(result, Err(TriggerDefinitionError::NotFound(ref id)) if id == "inconnu"),
            "expected NotFound, got: {result:?}"
        );
    }

    // ── Delete + Get + List ──────────────────────────────────────────

    #[test]
    fn test_delete_and_list() {
        // GIVEN un repository contenant 3 triggers
        let (_dir, repo) = open_test_repo();
        for i in 1..=3 {
            let def = make_cron_def(
                &format!("trigger-{i}"),
                &format!("agent-{i}"),
                "0 0 8 * * MON *",
            );
            repo.insert(&def).expect("insert");
        }
        assert_eq!(repo.list().expect("list").len(), 3);

        // WHEN delete("trigger-2")
        repo.delete("trigger-2").expect("delete");

        // THEN list() retourne 2, get("trigger-2") retourne None
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 2);
        assert!(repo.get("trigger-2").expect("get").is_none());

        // ET delete("trigger-2") à nouveau retourne NotFound
        let result = repo.delete("trigger-2");
        assert!(
            matches!(result, Err(TriggerDefinitionError::NotFound(ref id)) if id == "trigger-2"),
            "expected NotFound on double delete, got: {result:?}"
        );
    }

    // ── Validation cron invalide ─────────────────────────────────────

    #[test]
    fn test_validation_invalid_cron() {
        // GIVEN un repository ouvert
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("bad-cron", "agent", "not-a-cron");

        // WHEN insert avec cron invalide
        let result = repo.insert(&def);

        // THEN erreur ValidationError contenant "invalid cron expression"
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("invalid cron expression")),
            "expected ValidationError with cron message, got: {result:?}"
        );
    }

    // ── Validation XOR agent/pipeline ────────────────────────────────

    #[test]
    fn test_validation_xor_agent_pipeline() {
        let (_dir, repo) = open_test_repo();

        // GIVEN agent=None et pipeline=None
        let def_neither = TriggerDefinitionRow {
            id: "test-neither".to_string(),
            agent: None,
            pipeline: None,
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": "0 0 8 * * MON *" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert → THEN erreur "agent or pipeline must be set"
        let result = repo.insert(&def_neither);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("agent or pipeline must be set")),
            "expected 'agent or pipeline must be set', got: {result:?}"
        );

        // GIVEN agent=Some et pipeline=Some
        let def_both = TriggerDefinitionRow {
            id: "test-both".to_string(),
            agent: Some("a".to_string()),
            pipeline: Some("p".to_string()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": "0 0 8 * * MON *" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert → THEN erreur "agent and pipeline are mutually exclusive"
        let result = repo.insert(&def_both);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("agent and pipeline are mutually exclusive")),
            "expected 'mutually exclusive', got: {result:?}"
        );
    }

    // ── Validation webhook secret < 32 chars ─────────────────────────

    #[test]
    fn test_validation_webhook_short_secret() {
        let (_dir, repo) = open_test_repo();
        let def = TriggerDefinitionRow {
            id: "webhook-short".to_string(),
            agent: Some("agent".to_string()),
            pipeline: None,
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "webhook".to_string(),
            source_config: serde_json::json!({ "secret": "short" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert → THEN erreur "webhook secret must be at least 32 characters"
        let result = repo.insert(&def);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("webhook secret must be at least 32 characters")),
            "expected webhook secret validation error, got: {result:?}"
        );
    }

    // ── Extra — List vide ───────────────────────────────────────────────────

    #[test]
    fn test_list_empty() {
        // GIVEN un repository vide
        let (_dir, repo) = open_test_repo();

        // WHEN list
        let all = repo.list().expect("list");

        // THEN Vec vide
        assert!(all.is_empty());
    }

    // ── Extra — Open idempotent ─────────────────────────────────────────────

    #[test]
    fn test_open_idempotent() {
        // GIVEN une base déjà migrée
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("triggers.db");
        {
            let _repo = TriggerDefinitionRepository::open(&path).expect("first open");
        }

        // WHEN on ouvre une seconde fois
        let result = TriggerDefinitionRepository::open(&path);

        // THEN pas d'erreur
        assert!(result.is_ok(), "second open should succeed");
    }

    // ── Extra — Webhook avec secret valide (>= 32 chars) ───────────────────

    #[test]
    fn test_webhook_valid_secret() {
        let (_dir, repo) = open_test_repo();
        let secret = "a".repeat(32);
        let def = TriggerDefinitionRow {
            id: "webhook-valid".to_string(),
            agent: Some("agent".to_string()),
            pipeline: None,
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "webhook".to_string(),
            source_config: serde_json::json!({ "secret": secret }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        repo.insert(&def).expect("insert valid webhook");
        let got = repo.get("webhook-valid").expect("get").expect("exists");
        assert_eq!(got.source_type, "webhook");
    }

    // ── Extra — Pipeline target (sans agent) ────────────────────────────────

    #[test]
    fn test_pipeline_target() {
        let (_dir, repo) = open_test_repo();
        let def = TriggerDefinitionRow {
            id: "pipeline-trigger".to_string(),
            agent: None,
            pipeline: Some("my-pipeline".to_string()),
            enabled: true,
            on_busy: OnBusy::Drop,
            source_type: "interval".to_string(),
            source_config: serde_json::json!({ "every": "30m" }),
            input_template: Some("{{body}}".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        };

        repo.insert(&def).expect("insert");
        let got = repo.get("pipeline-trigger").expect("get").expect("exists");
        assert!(got.agent.is_none());
        assert_eq!(got.pipeline.as_deref(), Some("my-pipeline"));
        assert_eq!(got.on_busy, OnBusy::Drop);
    }
}
