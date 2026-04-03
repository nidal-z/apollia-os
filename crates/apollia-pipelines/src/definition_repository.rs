//! Repository CRUD SQLite pour les definitions de pipelines.
//!
//! Ce module fournit [`PipelineDefinitionRepository`], une interface de persistance
//! synchrone pour les definitions de pipelines dans `pipelines.db`. La validation
//! structurelle (DAG acyclique, step_id uniques, fallback_for valides) est deleguee
//! au module [`crate::validation`] et executee automatiquement avant chaque ecriture
//! (insert/update).
//!
//! Les steps sont stockes comme JSON blob (`steps_json`) car toujours charges/sauves
//! en bloc. Ce repository est utilise par le boot Supervisor et les routes
//! REST CRUD.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::types::{GlobalFailurePolicy, PipelineStepDef};
use crate::validation;

// ── Migration ───────────────────────────────────────────────────────────────

/// Migration idempotente pour la table `pipeline_definitions`.
const MIGRATION_008: &str = "\
CREATE TABLE IF NOT EXISTS pipeline_definitions (
    id              TEXT PRIMARY KEY,
    description     TEXT NOT NULL DEFAULT '',
    on_failure      TEXT NOT NULL DEFAULT 'fail',
    steps_json      TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_failure IN ('fail', 'continue'))
);";

// ── Types ───────────────────────────────────────────────────────────────────

/// Definition persistee d'un pipeline (representation SQLite).
///
/// Contrairement a [`crate::types::PipelineDefinition`] qui est la representation
/// en memoire pour l'execution, cette struct ajoute les champs de persistence
/// (`enabled`, `created_at`, `updated_at`) et stocke les steps comme JSON blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinitionRow {
    /// Identifiant unique du pipeline (cle primaire).
    pub id: String,
    /// Description lisible du pipeline.
    pub description: String,
    /// Politique globale d'echec : `Fail` ou `Continue`.
    pub on_failure: GlobalFailurePolicy,
    /// Liste des etapes du pipeline (serialisee en JSON dans `steps_json`).
    pub steps: Vec<PipelineStepDef>,
    /// Indique si le pipeline est actif.
    pub enabled: bool,
    /// Horodatage de creation (ISO 8601, renseigne automatiquement).
    pub created_at: String,
    /// Horodatage de derniere modification (ISO 8601, rafraichi a chaque update).
    pub updated_at: String,
}

// ── Conversion Row → PipelineDefinition ─────────────────────────────────────

impl From<PipelineDefinitionRow> for crate::types::PipelineDefinition {
    /// Convertit une [`PipelineDefinitionRow`] persistée en [`PipelineDefinition`] en mémoire.
    fn from(row: PipelineDefinitionRow) -> Self {
        crate::types::PipelineDefinition {
            id: crate::types::PipelineId(row.id),
            description: row.description,
            on_failure: row.on_failure,
            steps: row.steps,
        }
    }
}

// ── Erreurs ─────────────────────────────────────────────────────────────────

/// Erreurs du repository de definitions de pipelines.
#[derive(Debug, thiserror::Error)]
pub enum PipelineDefinitionError {
    /// Le pipeline demande n'existe pas.
    #[error("pipeline not found: {0}")]
    NotFound(String),
    /// Un pipeline avec cet identifiant existe deja.
    #[error("duplicate pipeline id: {0}")]
    DuplicateId(String),
    /// La definition ne satisfait pas les regles de validation structurelle.
    #[error("validation error: {0}")]
    ValidationError(String),
    /// Erreur SQLite sous-jacente.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

// ── Helpers GlobalFailurePolicy ─────────────────────────────────────────────

/// Retourne la representation SQLite de la politique d'echec globale.
fn global_failure_to_sql(policy: &GlobalFailurePolicy) -> &'static str {
    match policy {
        GlobalFailurePolicy::Fail => "fail",
        GlobalFailurePolicy::Continue => "continue",
    }
}

/// Parse une valeur SQLite en [`GlobalFailurePolicy`], defaut `Fail` si non reconnu.
fn global_failure_from_sql(s: &str) -> GlobalFailurePolicy {
    match s {
        "continue" => GlobalFailurePolicy::Continue,
        _ => GlobalFailurePolicy::Fail,
    }
}

// ── Repository ──────────────────────────────────────────────────────────────

/// Repository CRUD pour les definitions de pipelines dans SQLite.
///
/// Struct synchrone (pas d'acteur Tokio). La connexion SQLite est `Send`,
/// compatible avec `spawn_blocking` si necessaire. La validation DAG est
/// executee automatiquement avant chaque ecriture.
pub struct PipelineDefinitionRepository {
    conn: Connection,
}

impl PipelineDefinitionRepository {
    /// Ouvre (ou cree) la base SQLite et applique la migration idempotente.
    ///
    /// Active WAL pour de meilleures performances en ecriture concurrente.
    /// Cree la table `pipeline_definitions` si elle n'existe pas (idempotent).
    pub fn open(path: &Path) -> Result<Self, PipelineDefinitionError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_008)?;
        Ok(Self { conn })
    }

    /// Ouvre un repository en memoire (pour les tests).
    ///
    /// Utilisé par les tests unitaires de cette crate et par les tests
    /// des routes REST dans `apollia-runtime`.
    pub fn open_in_memory() -> Result<Self, PipelineDefinitionError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(MIGRATION_008)?;
        Ok(Self { conn })
    }

    /// Insere une nouvelle definition de pipeline apres validation DAG.
    ///
    /// Les champs `created_at` et `updated_at` sont renseignes automatiquement
    /// par les DEFAULT SQLite. Retourne [`PipelineDefinitionError::DuplicateId`]
    /// si l'identifiant existe deja.
    pub fn insert(&self, def: &PipelineDefinitionRow) -> Result<(), PipelineDefinitionError> {
        validation::validate_pipeline(def)?;

        let steps_json = serde_json::to_string(&def.steps).map_err(|e| {
            PipelineDefinitionError::ValidationError(format!("invalid steps JSON: {e}"))
        })?;

        self.conn
            .execute(
                "INSERT INTO pipeline_definitions \
                 (id, description, on_failure, steps_json, enabled) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    def.id,
                    def.description,
                    global_failure_to_sql(&def.on_failure),
                    steps_json,
                    def.enabled,
                ],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(err, _) = &e {
                    if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                        return PipelineDefinitionError::DuplicateId(def.id.clone());
                    }
                }
                PipelineDefinitionError::Database(e)
            })?;

        Ok(())
    }

    /// Met a jour une definition existante apres re-validation DAG.
    ///
    /// Rafraichit `updated_at` automatiquement. Retourne
    /// [`PipelineDefinitionError::NotFound`] si l'identifiant n'existe pas.
    pub fn update(
        &self,
        id: &str,
        def: &PipelineDefinitionRow,
    ) -> Result<(), PipelineDefinitionError> {
        validation::validate_pipeline(def)?;

        let steps_json = serde_json::to_string(&def.steps).map_err(|e| {
            PipelineDefinitionError::ValidationError(format!("invalid steps JSON: {e}"))
        })?;

        let rows = self.conn.execute(
            "UPDATE pipeline_definitions \
             SET description = ?1, on_failure = ?2, steps_json = ?3, enabled = ?4, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?5",
            params![
                def.description,
                global_failure_to_sql(&def.on_failure),
                steps_json,
                def.enabled,
                id,
            ],
        )?;

        if rows == 0 {
            return Err(PipelineDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Supprime une definition de pipeline.
    ///
    /// Retourne [`PipelineDefinitionError::NotFound`] si l'identifiant n'existe pas.
    pub fn delete(&self, id: &str) -> Result<(), PipelineDefinitionError> {
        let rows = self.conn.execute(
            "DELETE FROM pipeline_definitions WHERE id = ?1",
            params![id],
        )?;

        if rows == 0 {
            return Err(PipelineDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Retourne la definition d'un pipeline par son identifiant.
    ///
    /// Retourne `None` si aucun pipeline ne correspond.
    pub fn get(&self, id: &str) -> Result<Option<PipelineDefinitionRow>, PipelineDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, description, on_failure, steps_json, enabled, created_at, updated_at \
             FROM pipeline_definitions WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_definition)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Liste toutes les definitions de pipelines, triees par identifiant.
    pub fn list(&self) -> Result<Vec<PipelineDefinitionRow>, PipelineDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, description, on_failure, steps_json, enabled, created_at, updated_at \
             FROM pipeline_definitions ORDER BY id",
        )?;

        let rows = stmt.query_map([], row_to_definition)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

/// Convertit une ligne SQLite en [`PipelineDefinitionRow`].
fn row_to_definition(row: &rusqlite::Row) -> rusqlite::Result<PipelineDefinitionRow> {
    let on_failure_str: String = row.get(2)?;
    let steps_json_str: String = row.get(3)?;

    let steps: Vec<PipelineStepDef> = serde_json::from_str(&steps_json_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(PipelineDefinitionRow {
        id: row.get(0)?,
        description: row.get(1)?,
        on_failure: global_failure_from_sql(&on_failure_str),
        steps,
        enabled: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GlobalFailurePolicy, StepFailurePolicy, StepId};

    /// Cree un step simple pour les tests.
    fn step(id: &str, depends_on: &[&str]) -> PipelineStepDef {
        PipelineStepDef {
            id: StepId(id.into()),
            agent: format!("{id}-agent"),
            input: "input".into(),
            depends_on: depends_on.iter().map(|s| StepId(s.to_string())).collect(),
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: None,
            timeout_secs: None,
        }
    }

    /// Cree une definition de pipeline valide avec 3 steps en chaine.
    fn make_valid_def(id: &str) -> PipelineDefinitionRow {
        PipelineDefinitionRow {
            id: id.to_string(),
            description: "Pipeline de test".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("A", &[]), step("B", &["A"]), step("C", &["B"])],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    // ── Insert + Get ─────────────────────────────────────────────────

    #[test]
    fn test_insert_and_get() {
        // GIVEN un repository ouvert
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = make_valid_def("pipeline-factures");

        // WHEN insert puis get
        repo.insert(&def).expect("insert");
        let got = repo.get("pipeline-factures").expect("get");

        // THEN la definition est retrouvee avec les memes champs
        let got = got.expect("should exist");
        assert_eq!(got.id, "pipeline-factures");
        assert_eq!(got.description, "Pipeline de test");
        assert_eq!(got.on_failure, GlobalFailurePolicy::Fail);
        assert_eq!(got.steps.len(), 3);
        assert_eq!(got.steps[0].id, StepId("A".into()));
        assert_eq!(got.steps[1].id, StepId("B".into()));
        assert_eq!(got.steps[2].id, StepId("C".into()));
        assert!(got.enabled);
        assert!(!got.created_at.is_empty(), "created_at doit etre renseigne");
        assert!(!got.updated_at.is_empty(), "updated_at doit etre renseigne");
    }

    // ── Insert duplicate ID ──────────────────────────────────────────

    #[test]
    fn test_insert_duplicate_id() {
        // GIVEN un repository contenant "pipeline-factures"
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = make_valid_def("pipeline-factures");
        repo.insert(&def).expect("first insert");

        // WHEN insert avec le meme ID
        let result = repo.insert(&def);

        // THEN erreur DuplicateId
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::DuplicateId(ref id)) if id == "pipeline-factures"
            ),
            "expected DuplicateId, got: {result:?}"
        );
    }

    // ── Update existant ──────────────────────────────────────────────

    #[test]
    fn test_update_existing() {
        // GIVEN un pipeline avec 3 steps
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = make_valid_def("pipeline-factures");
        repo.insert(&def).expect("insert");

        let original = repo.get("pipeline-factures").expect("get").expect("exists");
        let original_updated_at = original.updated_at.clone();

        // WHEN update avec description modifiee
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut updated_def = make_valid_def("pipeline-factures");
        updated_def.description = "Description modifiee".to_string();
        repo.update("pipeline-factures", &updated_def)
            .expect("update");

        // THEN la description est mise a jour et updated_at rafraichi
        let got = repo.get("pipeline-factures").expect("get").expect("exists");
        assert_eq!(got.description, "Description modifiee");
        assert!(
            got.updated_at >= original_updated_at,
            "updated_at doit etre rafraichi"
        );
    }

    // ── Delete + Get + List ──────────────────────────────────────────

    #[test]
    fn test_delete_and_list() {
        // GIVEN un repository contenant 2 pipelines
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        repo.insert(&make_valid_def("pipeline-1")).expect("insert");
        repo.insert(&make_valid_def("pipeline-2")).expect("insert");
        assert_eq!(repo.list().expect("list").len(), 2);

        // WHEN delete("pipeline-1")
        repo.delete("pipeline-1").expect("delete");

        // THEN list() retourne 1 pipeline, get("pipeline-1") retourne None
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1);
        assert!(repo.get("pipeline-1").expect("get").is_none());
    }

    // ── Validation cycle detecte ─────────────────────────────────────

    #[test]
    fn test_validation_cycle() {
        // GIVEN steps A->B->A (cycle)
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = PipelineDefinitionRow {
            id: "cyclic".to_string(),
            description: "Cyclic pipeline".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("A", &["B"]), step("B", &["A"])],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert
        let result = repo.insert(&def);

        // THEN erreur "cycle detected in pipeline DAG"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "cycle detected in pipeline DAG"
            ),
            "expected cycle error, got: {result:?}"
        );
    }

    // ── Validation step_id duplique ──────────────────────────────────

    #[test]
    fn test_validation_duplicate_step_id() {
        // GIVEN deux steps avec le meme id "etape-1"
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = PipelineDefinitionRow {
            id: "dup-steps".to_string(),
            description: "Duplicate steps".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("etape-1", &[]), step("etape-1", &[])],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert
        let result = repo.insert(&def);

        // THEN erreur "duplicate step id: etape-1"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "duplicate step id: etape-1"
            ),
            "expected duplicate step id error, got: {result:?}"
        );
    }

    // ── Validation depends_on reference invalide ─────────────────────

    #[test]
    fn test_validation_invalid_depends_on() {
        // GIVEN un step avec depends_on="step-inexistant"
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = PipelineDefinitionRow {
            id: "bad-deps".to_string(),
            description: "Bad depends_on".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("A", &["step-inexistant"])],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert
        let result = repo.insert(&def);

        // THEN erreur "depends_on references unknown step: step-inexistant"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "depends_on references unknown step: step-inexistant"
            ),
            "expected unknown depends_on error, got: {result:?}"
        );
    }

    // ── Validation fallback_for invalide ─────────────────────────────

    #[test]
    fn test_validation_invalid_fallback_for() {
        // GIVEN fallback_for="etape-1" mais etape-1 n'a pas on_failure="fallback"
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let mut fallback = step("fallback-1", &[]);
        fallback.fallback_for = Some(StepId("etape-1".into()));
        let def = PipelineDefinitionRow {
            id: "bad-fallback".to_string(),
            description: "Bad fallback".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("etape-1", &[]), fallback],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert
        let result = repo.insert(&def);

        // THEN erreur contenant "fallback_for references step without on_failure=fallback"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg.contains("without on_failure=fallback")
            ),
            "expected fallback_for validation error, got: {result:?}"
        );
    }

    // ── Validation pipeline vide ────────────────────────────────────

    #[test]
    fn test_validation_empty_steps() {
        // GIVEN steps=[]
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = PipelineDefinitionRow {
            id: "empty".to_string(),
            description: "Empty pipeline".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert
        let result = repo.insert(&def);

        // THEN erreur "pipeline must have at least one step"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "pipeline must have at least one step"
            ),
            "expected empty steps error, got: {result:?}"
        );
    }

    // ── Extra — steps_json roundtrip ────────────────────────────────────────

    #[test]
    fn test_steps_json_roundtrip() {
        // GIVEN un pipeline avec condition et fallback
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");

        let mut primary = step("validation", &["ocr"]);
        primary.on_failure = StepFailurePolicy::Fallback;
        primary.condition = Some(crate::types::StepCondition {
            when: crate::types::ConditionKind::Contains,
            field: "steps.ocr.output".into(),
            value: "FRAUDE".into(),
        });

        let mut fallback = step("validation-fallback", &["ocr"]);
        fallback.fallback_for = Some(StepId("validation".into()));

        let def = PipelineDefinitionRow {
            id: "roundtrip".to_string(),
            description: "Roundtrip test".to_string(),
            on_failure: GlobalFailurePolicy::Continue,
            steps: vec![step("ocr", &[]), primary, fallback],
            enabled: false,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert puis get
        repo.insert(&def).expect("insert");
        let got = repo.get("roundtrip").expect("get").expect("exists");

        // THEN les steps sont deserialises correctement
        assert_eq!(got.steps.len(), 3);
        assert_eq!(got.on_failure, GlobalFailurePolicy::Continue);
        assert!(!got.enabled);

        let ocr = &got.steps[0];
        assert_eq!(ocr.id, StepId("ocr".into()));
        assert!(ocr.depends_on.is_empty());

        let val = &got.steps[1];
        assert_eq!(val.id, StepId("validation".into()));
        assert_eq!(val.depends_on, vec![StepId("ocr".into())]);
        assert_eq!(val.on_failure, StepFailurePolicy::Fallback);
        assert!(val.condition.is_some());

        let fb = &got.steps[2];
        assert_eq!(fb.fallback_for, Some(StepId("validation".into())));
    }

    // ── Extra — Update not found ────────────────────────────────────────────

    #[test]
    fn test_update_not_found() {
        // GIVEN un repository vide
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        let def = make_valid_def("inexistant");

        // WHEN update("inexistant")
        let result = repo.update("inexistant", &def);

        // THEN erreur NotFound
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::NotFound(ref id)) if id == "inexistant"
            ),
            "expected NotFound, got: {result:?}"
        );
    }

    // ── Extra — Delete not found ────────────────────────────────────────────

    #[test]
    fn test_delete_not_found() {
        // GIVEN un repository vide
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");

        // WHEN delete("inexistant")
        let result = repo.delete("inexistant");

        // THEN erreur NotFound
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::NotFound(ref id)) if id == "inexistant"
            ),
            "expected NotFound, got: {result:?}"
        );
    }

    // ── Extra — List vide ───────────────────────────────────────────────────

    #[test]
    fn test_list_empty() {
        // GIVEN un repository vide
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");

        // WHEN list
        let all = repo.list().expect("list");

        // THEN Vec vide
        assert!(all.is_empty());
    }

    // ── Extra — Update re-validates DAG ─────────────────────────────────────

    #[test]
    fn test_update_validates_dag() {
        // GIVEN un pipeline valide
        let repo = PipelineDefinitionRepository::open_in_memory().expect("open");
        repo.insert(&make_valid_def("pipeline-1")).expect("insert");

        // WHEN update avec un cycle
        let cyclic = PipelineDefinitionRow {
            id: "pipeline-1".to_string(),
            description: "Now cyclic".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![step("X", &["Y"]), step("Y", &["X"])],
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let result = repo.update("pipeline-1", &cyclic);

        // THEN erreur de validation (cycle)
        assert!(
            matches!(result, Err(PipelineDefinitionError::ValidationError(_))),
            "update with cycle should fail: {result:?}"
        );

        // ET la definition originale est preservee
        let got = repo.get("pipeline-1").expect("get").expect("exists");
        assert_eq!(got.steps.len(), 3, "original 3 steps preserved");
    }
}
