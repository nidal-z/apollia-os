//! Persistance SQLite des plans d'exécution ORIA.
//!
//! `PlanRepository` est un wrapper synchrone autour de `rusqlite` — même pattern
//! que `AuditTrail` (STORY-016). Toutes les méthodes sont synchrones ; l'`ActorLoop`
//! async les appelle via `tokio::task::spawn_blocking` si nécessaire.
//!
//! La migration `004_execution_plans.sql` est incluse au moment de la compilation
//! et appliquée idempotentiellement à l'ouverture de la base.

use std::cell::RefCell;

use rusqlite::{params, Connection};

use apollia_core::observability::{truncate_with_marker, ObservabilityConfig};

use crate::plan::{ExecutionPlan, PlanStep};

/// SQL de migration embarqué — appliqué idempotentiellement à chaque ouverture.
const MIGRATION_SQL: &str = include_str!("../../apollia-tools/migrations/004_execution_plans.sql");

/// Colonnes d'observabilité STORY-127 à ajouter sur `plan_steps`.
///
/// Chaque tuple : (nom colonne, type SQL). Appliquées idempotentiellement
/// via [`apply_observability_migration`] — les colonnes déjà existantes
/// sont silencieusement ignorées.
const OBSERVABILITY_COLUMNS: &[(&str, &str)] = &[
    ("input_rendered", "TEXT"),
    ("input_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("output_text", "TEXT"),
    ("output_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("tool_used", "TEXT"),
    ("error_detail", "TEXT"),
    ("duration_ms", "INTEGER"),
];

/// Applique la migration d'observabilité STORY-127 de façon idempotente.
///
/// Utilise `ALTER TABLE ADD COLUMN` individuellement et ignore l'erreur
/// « duplicate column name » (code SQLite 1) si la colonne existe déjà.
fn apply_observability_migration(conn: &Connection) -> Result<(), PlanRepositoryError> {
    for (col, col_type) in OBSERVABILITY_COLUMNS {
        let sql = format!("ALTER TABLE plan_steps ADD COLUMN {col} {col_type}");
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::SqliteFailure(err, _)) if err.extended_code == 1 => {
                // « duplicate column name » — colonne déjà présente, ignoré.
            }
            Err(e) => return Err(PlanRepositoryError::Sqlite(e)),
        }
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Erreurs
// ────────────────────────────────────────────────────────────────────────────

/// Erreurs possibles du [`PlanRepository`].
#[derive(Debug, thiserror::Error)]
pub enum PlanRepositoryError {
    /// Erreur SQLite sous-jacente.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Aucun plan trouvé pour le `task_id` donné.
    #[error("Plan not found for task_id: {0}")]
    NotFound(String),
}

// ────────────────────────────────────────────────────────────────────────────
// Types de retour
// ────────────────────────────────────────────────────────────────────────────

/// Représentation complète d'un plan avec ses steps, utilisée par `task inspect`.
#[derive(Debug)]
pub struct PlanWithSteps {
    /// Identifiant unique du plan (UUID v4).
    pub plan_id: String,
    /// Identifiant de la tâche associée.
    pub task_id: String,
    /// Nom de l'agent propriétaire du plan.
    pub agent_name: String,
    /// Statut courant : `running` | `completed` | `failed` | `replanning`.
    pub status: String,
    /// Nombre de replanifications effectuées depuis la création.
    pub replan_count: u32,
    /// Horodatage ISO 8601 de création du plan.
    pub created_at: String,
    /// Steps du plan dans l'ordre de récupération SQLite.
    pub steps: Vec<StepRecord>,
}

/// État complet d'un step individuel tel que persisté en SQLite.
#[derive(Debug)]
pub struct StepRecord {
    /// Identifiant unique dans le plan (ex : `"s1"`).
    pub step_id: String,
    /// Description en langage naturel de l'action à réaliser.
    pub description: String,
    /// Outil suggéré par le LLM, s'il y en a un.
    pub tool_hint: Option<String>,
    /// Identifiants des steps dont ce step dépend (désérialisés depuis JSON).
    pub depends_on: Vec<String>,
    /// Statut courant : `pending` | `running` | `completed` | `failed` | `skipped`.
    pub status: String,
    /// Sortie produite par le step, présente une fois `completed`.
    pub output: Option<String>,
    /// Message d'erreur, présent si `failed` ou `skipped`.
    pub error: Option<String>,
    /// Horodatage de démarrage du step.
    pub started_at: Option<String>,
    /// Horodatage de fin du step.
    pub completed_at: Option<String>,
    /// Input rendu après interpolation template (potentiellement tronqué).
    pub input_rendered: Option<String>,
    /// Indique si `input_rendered` a été tronqué.
    pub input_truncated: bool,
    /// Texte d'output d'observabilité (potentiellement tronqué).
    pub output_text: Option<String>,
    /// Indique si `output_text` a été tronqué.
    pub output_truncated: bool,
    /// Nom de l'outil effectivement utilisé (vs `tool_hint` suggéré par le LLM).
    pub tool_used: Option<String>,
    /// Détail complet de l'erreur pour diagnostic (vs `error` qui est le message bref).
    pub error_detail: Option<String>,
    /// Durée d'exécution du step en millisecondes.
    pub duration_ms: Option<i64>,
}

// ────────────────────────────────────────────────────────────────────────────
// Repository
// ────────────────────────────────────────────────────────────────────────────

/// Repository SQLite pour la persistance des plans d'exécution ORIA.
///
/// Encapsule une connexion `rusqlite` derrière un [`RefCell`] afin d'offrir
/// une API `&self` sur toutes les méthodes tout en autorisant l'emprunt mutable
/// nécessaire aux transactions atomiques (cf. [`Self::begin_replan`]).
///
/// **Thread safety :** `PlanRepository` n'est pas `Send` (car `RefCell`). Il doit
/// être créé et utilisé dans le même thread, ou passé à `spawn_blocking`.
pub struct PlanRepository {
    conn: RefCell<Connection>,
}

impl PlanRepository {
    /// Ouvre la base SQLite et applique la migration `004_execution_plans.sql`.
    ///
    /// La migration est idempotente (`CREATE TABLE IF NOT EXISTS`), donc sûre
    /// à réexécuter sur une base existante.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] si l'ouverture ou la migration échoue.
    pub fn new(db_path: &str) -> Result<Self, PlanRepositoryError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        apply_observability_migration(&conn)?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    /// Insère un nouveau plan avec le statut `running`.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite (ex : `plan_id` dupliqué).
    pub fn insert_plan(
        &self,
        plan: &ExecutionPlan,
        agent_name: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "INSERT INTO execution_plans (plan_id, task_id, agent_name, status) \
             VALUES (?1, ?2, ?3, 'running')",
            params![plan.plan_id, plan.task_id, agent_name],
        )?;
        Ok(())
    }

    /// Insère les steps d'un plan, tous avec le statut `pending`.
    ///
    /// Le champ `depends_on` de chaque step est sérialisé en JSON avant stockage.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] si un step ne peut pas être inséré.
    pub fn insert_steps(
        &self,
        plan_id: &str,
        steps: &[PlanStep],
    ) -> Result<(), PlanRepositoryError> {
        let conn = self.conn.borrow();
        for step in steps {
            let depends_on_json =
                serde_json::to_string(&step.depends_on).unwrap_or_else(|_| "[]".to_string());
            conn.execute(
                "INSERT INTO plan_steps \
                     (step_id, plan_id, description, tool_hint, depends_on, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
                params![
                    step.step_id,
                    plan_id,
                    step.description,
                    step.tool_hint,
                    depends_on_json,
                ],
            )?;
        }
        Ok(())
    }

    /// Marque un step comme `running` avec `started_at = CURRENT_TIMESTAMP`.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn start_step(&self, plan_id: &str, step_id: &str) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET status = 'running', started_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1 AND step_id = ?2",
            params![plan_id, step_id],
        )?;
        Ok(())
    }

    /// Marque un step comme `completed` avec son `output` et `completed_at = CURRENT_TIMESTAMP`.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn complete_step(
        &self,
        plan_id: &str,
        step_id: &str,
        output: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET status = 'completed', output = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![output, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Marque un step comme `failed` avec un message d'erreur.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn fail_step(
        &self,
        plan_id: &str,
        step_id: &str,
        error: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET status = 'failed', error = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![error, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Prépare la replanification de façon atomique.
    ///
    /// En une seule transaction SQLite :
    /// - Passe `execution_plans.status` à `replanning`
    /// - Met à jour `replan_count` avec `new_count`
    /// - **Supprime** tous les steps au statut `pending` (les steps `completed`/`failed` sont conservés)
    ///
    /// Les nouveaux steps doivent ensuite être insérés via [`Self::insert_steps`].
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] si la transaction échoue.
    pub fn begin_replan(&self, plan_id: &str, new_count: u32) -> Result<(), PlanRepositoryError> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM plan_steps WHERE plan_id = ?1 AND status = 'pending'",
            params![plan_id],
        )?;
        tx.execute(
            "UPDATE execution_plans \
             SET status = 'replanning', replan_count = ?1, updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2",
            params![new_count, plan_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Marque le plan comme `failed` et passe tous ses steps `pending` en `skipped`.
    ///
    /// La raison d'échec est conservée dans le champ `error` des steps skippés.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn fail_plan(&self, plan_id: &str, reason: &str) -> Result<(), PlanRepositoryError> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE plan_steps \
             SET status = 'skipped', error = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND status = 'pending'",
            params![reason, plan_id],
        )?;
        conn.execute(
            "UPDATE execution_plans \
             SET status = 'failed', updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1",
            params![plan_id],
        )?;
        Ok(())
    }

    /// Marque le plan comme `completed`.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn complete_plan(&self, plan_id: &str) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE execution_plans \
             SET status = 'completed', updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1",
            params![plan_id],
        )?;
        Ok(())
    }

    /// Persiste l'input rendu d'un step avec troncature selon [`ObservabilityConfig`].
    ///
    /// L'input est tronqué si sa taille dépasse `config.max_input_bytes`.
    /// Le flag `input_truncated` est positionné en conséquence.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn save_step_input(
        &self,
        step_id: &str,
        plan_id: &str,
        rendered_input: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), PlanRepositoryError> {
        let (text, truncated) = truncate_with_marker(rendered_input, config.max_input_bytes);
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET input_rendered = ?1, input_truncated = ?2 \
             WHERE plan_id = ?3 AND step_id = ?4",
            params![text, truncated as i32, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persiste l'output d'observabilité d'un step avec troncature.
    ///
    /// Distinct de la colonne `output` (utilisée par ORIA pour le chaînage).
    /// L'output est tronqué si sa taille dépasse `config.max_output_bytes`.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn save_step_output(
        &self,
        step_id: &str,
        plan_id: &str,
        output: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), PlanRepositoryError> {
        let (text, truncated) = truncate_with_marker(output, config.max_output_bytes);
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET output_text = ?1, output_truncated = ?2 \
             WHERE plan_id = ?3 AND step_id = ?4",
            params![text, truncated as i32, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persiste le détail complet d'erreur d'un step pour diagnostic.
    ///
    /// Distinct de la colonne `error` (message bref utilisé par ORIA).
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn save_step_error(
        &self,
        step_id: &str,
        plan_id: &str,
        error_detail: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET error_detail = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![error_detail, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persiste le nom de l'outil effectivement utilisé par le step.
    ///
    /// Distinct de `tool_hint` (outil suggéré par le LLM).
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn save_step_tool(
        &self,
        step_id: &str,
        plan_id: &str,
        tool_name: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET tool_used = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![tool_name, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persiste la durée d'exécution du step en millisecondes.
    ///
    /// # Errors
    /// Retourne [`PlanRepositoryError::Sqlite`] en cas d'erreur SQLite.
    pub fn save_step_duration(
        &self,
        step_id: &str,
        plan_id: &str,
        duration_ms: i64,
    ) -> Result<(), PlanRepositoryError> {
        self.conn.borrow().execute(
            "UPDATE plan_steps \
             SET duration_ms = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![duration_ms, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Récupère le plan complet avec ses steps depuis SQLite.
    ///
    /// Utilisé par `apollia-os task inspect` (STORY-089).
    ///
    /// # Errors
    /// - [`PlanRepositoryError::NotFound`] si aucun plan n'est associé à `task_id`.
    /// - [`PlanRepositoryError::Sqlite`] pour toute autre erreur SQLite.
    pub fn get_plan_with_steps(&self, task_id: &str) -> Result<PlanWithSteps, PlanRepositoryError> {
        let conn = self.conn.borrow();

        // ── Récupération du plan ──────────────────────────────────────────────
        let (plan_id, agent_name, status, replan_count, created_at) = conn
            .query_row(
                "SELECT plan_id, agent_name, status, replan_count, created_at \
                 FROM execution_plans WHERE task_id = ?1",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    PlanRepositoryError::NotFound(task_id.to_string())
                }
                other => PlanRepositoryError::Sqlite(other),
            })?;

        // ── Récupération des steps ────────────────────────────────────────────
        let mut stmt = conn.prepare(
            "SELECT step_id, description, tool_hint, depends_on, status, \
                    output, error, started_at, completed_at, \
                    input_rendered, input_truncated, output_text, output_truncated, \
                    tool_used, error_detail, duration_ms \
             FROM plan_steps WHERE plan_id = ?1",
        )?;

        let steps = stmt
            .query_map(params![plan_id], |row| {
                let depends_on_str: String = row.get(3)?;
                // Données insérées par notre propre code — fallback sûr si JSON malformé.
                let depends_on: Vec<String> =
                    serde_json::from_str(&depends_on_str).unwrap_or_default();
                let input_truncated_raw: i32 = row.get(10)?;
                let output_truncated_raw: i32 = row.get(12)?;
                Ok(StepRecord {
                    step_id: row.get(0)?,
                    description: row.get(1)?,
                    tool_hint: row.get(2)?,
                    depends_on,
                    status: row.get(4)?,
                    output: row.get(5)?,
                    error: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    input_rendered: row.get(9)?,
                    input_truncated: input_truncated_raw != 0,
                    output_text: row.get(11)?,
                    output_truncated: output_truncated_raw != 0,
                    tool_used: row.get(13)?,
                    error_detail: row.get(14)?,
                    duration_ms: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PlanWithSteps {
            plan_id,
            task_id: task_id.to_string(),
            agent_name,
            status,
            replan_count,
            created_at,
            steps,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn make_repo() -> (PlanRepository, NamedTempFile) {
        let f = NamedTempFile::new().unwrap();
        let repo = PlanRepository::new(f.path().to_str().unwrap()).unwrap();
        (repo, f)
    }

    fn make_plan(task_id: &str) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            steps: vec![
                PlanStep {
                    step_id: "s1".into(),
                    description: "Step 1".into(),
                    tool_hint: Some("file_io".into()),
                    depends_on: vec![],
                },
                PlanStep {
                    step_id: "s2".into(),
                    description: "Step 2".into(),
                    tool_hint: None,
                    depends_on: vec!["s1".into()],
                },
            ],
        }
    }

    // GIVEN / WHEN : PlanRepository::new() sur une base vide
    // THEN : tables créées sans erreur (AC-1)
    #[test]
    fn test_ac1_migration_appliquee() {
        let (_repo, _f) = make_repo();
        // La migration réussit implicitement si new() ne retourne pas d'erreur.
    }

    // GIVEN : un PlanRepository ouvert
    // WHEN  : cycle de vie complet — insert_plan → start/complete steps → complete_plan
    // THEN  : get_plan_with_steps retourne status=completed et outputs corrects (AC-2)
    #[test]
    fn test_ac2_cycle_de_vie_complet() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-001");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();
        repo.start_step(&plan.plan_id, "s1").unwrap();
        repo.complete_step(&plan.plan_id, "s1", "output-1").unwrap();
        repo.start_step(&plan.plan_id, "s2").unwrap();
        repo.complete_step(&plan.plan_id, "s2", "output-2").unwrap();
        repo.complete_plan(&plan.plan_id).unwrap();

        let result = repo.get_plan_with_steps("task-001").unwrap();
        assert_eq!(result.status, "completed");
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.status, "completed");
        assert_eq!(s1.output.as_deref(), Some("output-1"));
        let s2 = result.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(s2.output.as_deref(), Some("output-2"));
    }

    // GIVEN : un plan avec s1 completed, s2 pending
    // WHEN  : begin_replan(plan_id, 1)
    // THEN  : status=replanning, replan_count=1, s2 supprimé, s1 conservé (AC-3)
    #[test]
    fn test_ac3_replan_supprime_pending_garde_completed() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-002");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();
        repo.complete_step(&plan.plan_id, "s1", "done").unwrap();

        repo.begin_replan(&plan.plan_id, 1).unwrap();

        let result = repo.get_plan_with_steps("task-002").unwrap();
        assert_eq!(result.status, "replanning");
        assert_eq!(result.replan_count, 1);

        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.status, "completed"); // conservé

        let s2 = result.steps.iter().find(|s| s.step_id == "s2");
        assert!(s2.is_none()); // supprimé
    }

    // GIVEN : un plan avec s1 et s2 pending
    // WHEN  : fail_plan(plan_id, "STEP_BUDGET_EXCEEDED")
    // THEN  : plan.status=failed, tous les steps skipped ou failed (AC-4)
    #[test]
    fn test_ac4_fail_plan_skippe_pending() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-003");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.fail_plan(&plan.plan_id, "STEP_BUDGET_EXCEEDED")
            .unwrap();

        let result = repo.get_plan_with_steps("task-003").unwrap();
        assert_eq!(result.status, "failed");
        for step in &result.steps {
            assert!(
                step.status == "skipped" || step.status == "failed",
                "unexpected step status: {}",
                step.status,
            );
        }
    }

    // GIVEN / WHEN : get_plan_with_steps sur un task_id inexistant
    // THEN  : PlanRepositoryError::NotFound retourné
    #[test]
    fn test_not_found() {
        let (repo, _f) = make_repo();
        let result = repo.get_plan_with_steps("inexistant");
        assert!(matches!(result, Err(PlanRepositoryError::NotFound(_))));
    }

    // ── STORY-127 : observabilité step ────────────────────────────────────

    // GIVEN un step dans un plan
    // WHEN  save_step_input avec un texte court
    // THEN  input_rendered persisté, input_truncated = false (AC-1)
    #[test]
    fn test_step_input_rendered_persisted() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-1");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig::default();
        repo.save_step_input("s1", &plan.plan_id, "lire /tmp/data.json", &config)
            .unwrap();

        let result = repo.get_plan_with_steps("task-obs-1").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.input_rendered.as_deref(), Some("lire /tmp/data.json"));
        assert!(!s1.input_truncated);
    }

    // GIVEN un step complété
    // WHEN  save_step_output + save_step_tool + save_step_duration
    // THEN  output_text, tool_used, duration_ms persistés (AC-2)
    #[test]
    fn test_step_output_on_success() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-2");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig::default();
        repo.save_step_output("s1", &plan.plan_id, "contenu du fichier", &config)
            .unwrap();
        repo.save_step_tool("s1", &plan.plan_id, "file_io").unwrap();
        repo.save_step_duration("s1", &plan.plan_id, 42).unwrap();

        let result = repo.get_plan_with_steps("task-obs-2").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.output_text.as_deref(), Some("contenu du fichier"));
        assert!(!s1.output_truncated);
        assert_eq!(s1.tool_used.as_deref(), Some("file_io"));
        assert_eq!(s1.duration_ms, Some(42));
    }

    // GIVEN un step échoué
    // WHEN  save_step_error + save_step_tool + save_step_duration
    // THEN  error_detail, tool_used, duration_ms persistés (AC-3)
    #[test]
    fn test_step_error_detail_on_failure() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-3");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.save_step_error("s1", &plan.plan_id, "Permission denied: /etc/shadow")
            .unwrap();
        repo.save_step_tool("s1", &plan.plan_id, "file_io").unwrap();
        repo.save_step_duration("s1", &plan.plan_id, 5).unwrap();

        let result = repo.get_plan_with_steps("task-obs-3").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(
            s1.error_detail.as_deref(),
            Some("Permission denied: /etc/shadow")
        );
        assert_eq!(s1.tool_used.as_deref(), Some("file_io"));
        assert_eq!(s1.duration_ms, Some(5));
    }

    // GIVEN un step
    // WHEN  save_step_duration avec une valeur
    // THEN  duration_ms persisté (AC-4)
    #[test]
    fn test_step_duration_measured() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-4");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.save_step_duration("s1", &plan.plan_id, 150).unwrap();

        let result = repo.get_plan_with_steps("task-obs-4").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.duration_ms, Some(150));
    }

    // GIVEN un step avec un input > max_input_bytes
    // WHEN  save_step_input
    // THEN  input tronqué, input_truncated = true
    #[test]
    fn test_step_input_truncated_when_over_limit() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-5");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig {
            max_input_bytes: 50,
            ..ObservabilityConfig::default()
        };
        let long_input = "x".repeat(200);
        repo.save_step_input("s1", &plan.plan_id, &long_input, &config)
            .unwrap();

        let result = repo.get_plan_with_steps("task-obs-5").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert!(s1.input_truncated);
        assert!(s1
            .input_rendered
            .as_ref()
            .map_or(false, |t| t.contains("200 octets total")));
    }

    // GIVEN : plan avec depends_on non vide
    // WHEN  : get_plan_with_steps
    // THEN  : depends_on est correctement désérialisé (AC-5)
    #[test]
    fn test_ac5_depends_on_deserialise() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-004");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let result = repo.get_plan_with_steps("task-004").unwrap();
        assert_eq!(result.steps.len(), 2);

        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert!(s1.depends_on.is_empty());

        let s2 = result.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(s2.depends_on, vec!["s1".to_string()]);
    }
}
