//! ProceduralMemory — workflows appris par l'agent.
//!
//! Stocke des patterns trigger->steps avec compteur de succes.
//! Quand un agent reconnait une situation similaire, il peut recuperer
//! le workflow associe plutot que de repartir de zero.
//! L'agent est seul maitre de ce qui est enregistre (Principe #6).

use crate::store::MemoryStore;

/// Backend de memoire procedurale — workflows appris par l'agent.
///
/// Stocke des patterns trigger->steps avec compteur de succes.
/// Le trigger est match exact (pas de FTS).
pub struct ProceduralMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par [`ProceduralMemory::recall`] et [`ProceduralMemory::list`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcedureEntry {
    /// UUID v4 unique de la procedure.
    pub id: String,
    /// Namespace de la procedure.
    pub namespace: String,
    /// Trigger exact qui declenche cette procedure.
    pub trigger: String,
    /// Etapes du workflow, en ordre.
    pub steps: Vec<String>,
    /// Nombre de fois que cette procedure a ete confirmee comme reussie.
    pub success_count: u32,
    /// Date de dernier usage ISO 8601.
    pub last_used_at: String,
    /// Date de creation ISO 8601.
    pub created_at: String,
}

/// Erreurs du backend procedural.
#[derive(Debug, thiserror::Error)]
pub enum ProceduralMemoryError {
    /// L'apprentissage d'une procedure a echoue.
    #[error("failed to learn procedure: {0}")]
    LearnFailed(String),

    /// Le trigger est vide.
    #[error("empty trigger")]
    EmptyTrigger,

    /// La liste de steps est vide.
    #[error("empty steps list")]
    EmptySteps,

    /// Erreur SQLite propagee depuis rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> ProceduralMemory<'a> {
    /// Cree un backend procedural lie a un [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Apprend une procedure. Si le trigger existe deja dans le namespace,
    /// incremente `success_count` et met a jour `steps` et `last_used_at`.
    ///
    /// Retourne l'UUID de la procedure (nouveau ou existant).
    pub fn learn(
        &self,
        namespace: &str,
        trigger: &str,
        steps: &[String],
    ) -> Result<String, ProceduralMemoryError> {
        if trigger.is_empty() {
            return Err(ProceduralMemoryError::EmptyTrigger);
        }
        if steps.is_empty() {
            return Err(ProceduralMemoryError::EmptySteps);
        }

        let now = chrono_now_utc();
        let steps_json = serde_json::to_string(steps)
            .map_err(|e| ProceduralMemoryError::LearnFailed(format!("steps serialization: {e}")))?;
        let conn = self.store.conn();

        // Check if procedure already exists for this namespace+trigger.
        let existing: Option<(String, u32)> = conn
            .query_row(
                "SELECT id, success_count FROM procedural_memories WHERE namespace = ?1 AND trigger_text = ?2",
                rusqlite::params![namespace, trigger],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let id = match existing {
            Some((existing_id, count)) => {
                conn.execute(
                    "UPDATE procedural_memories
                     SET steps = ?1, success_count = ?2, last_used_at = ?3
                     WHERE id = ?4",
                    rusqlite::params![steps_json, count + 1, now, existing_id],
                )
                .map_err(|e| ProceduralMemoryError::LearnFailed(e.to_string()))?;

                tracing::info!(
                    procedure_id = %existing_id,
                    namespace = %namespace,
                    trigger = %trigger,
                    success_count = count + 1,
                    "procedure reinforced"
                );

                existing_id
            }
            None => {
                let new_id = uuid::Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![new_id, namespace, trigger, steps_json, 1, now, now],
                )
                .map_err(|e| ProceduralMemoryError::LearnFailed(e.to_string()))?;

                tracing::info!(
                    procedure_id = %new_id,
                    namespace = %namespace,
                    trigger = %trigger,
                    "procedure learned"
                );

                new_id
            }
        };

        Ok(id)
    }

    /// Recupere une procedure par trigger exact. `None` si absente.
    pub fn recall(
        &self,
        namespace: &str,
        trigger: &str,
    ) -> Result<Option<ProcedureEntry>, ProceduralMemoryError> {
        let conn = self.store.conn();

        let result = conn.query_row(
            "SELECT id, namespace, trigger_text, steps, success_count, last_used_at, created_at
             FROM procedural_memories
             WHERE namespace = ?1 AND trigger_text = ?2",
            rusqlite::params![namespace, trigger],
            |row| {
                let steps_str: String = row.get(3)?;
                Ok(ProcedureEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    trigger: row.get(2)?,
                    steps: serde_json::from_str(&steps_str).unwrap_or_default(),
                    success_count: row.get(4)?,
                    last_used_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ProceduralMemoryError::Sqlite(e)),
        }
    }

    /// Liste toutes les procedures d'un namespace, triees par `success_count` DESC.
    pub fn list(&self, namespace: &str) -> Result<Vec<ProcedureEntry>, ProceduralMemoryError> {
        let conn = self.store.conn();

        let mut stmt = conn.prepare(
            "SELECT id, namespace, trigger_text, steps, success_count, last_used_at, created_at
             FROM procedural_memories
             WHERE namespace = ?1
             ORDER BY success_count DESC",
        )?;

        let rows = stmt.query_map(rusqlite::params![namespace], |row| {
            let steps_str: String = row.get(3)?;
            Ok(ProcedureEntry {
                id: row.get(0)?,
                namespace: row.get(1)?,
                trigger: row.get(2)?,
                steps: serde_json::from_str(&steps_str).unwrap_or_default(),
                success_count: row.get(4)?,
                last_used_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }
}

/// Returns the current UTC time as an ISO 8601 string.
///
/// Delegates to SQLite's `strftime` for consistency with datetime comparisons.
fn chrono_now_utc() -> String {
    let conn = rusqlite::Connection::open_in_memory()
        .expect("in-memory SQLite connection should always succeed");
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })
    .expect("strftime should always succeed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;

    fn setup() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_proc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();
        (store, path)
    }

    #[test]
    fn test_ac1_learn_new_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN
        let id = proc
            .learn(
                "ns",
                "devis grand compte",
                &["Verifier SIRET".into(), "Calculer remise".into()],
            )
            .unwrap();
        // THEN
        assert!(!id.is_empty());
    }

    #[test]
    fn test_ac2_recall_existing_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn(
            "ns",
            "devis grand compte",
            &["Verifier SIRET".into(), "Calculer remise".into()],
        )
        .unwrap();
        // WHEN
        let entry = proc.recall("ns", "devis grand compte").unwrap();
        // THEN
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.steps.len(), 2);
        assert_eq!(e.steps[0], "Verifier SIRET");
        assert_eq!(e.success_count, 1);
    }

    #[test]
    fn test_ac3_learn_again_increments_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "trigger", &["step1".into()]).unwrap();
        // WHEN
        proc.learn("ns", "trigger", &["step1".into(), "step2".into()])
            .unwrap();
        // THEN
        let entry = proc.recall("ns", "trigger").unwrap().unwrap();
        assert_eq!(entry.success_count, 2);
        assert_eq!(entry.steps.len(), 2);
    }

    #[test]
    fn test_ac4_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(proc.recall("ns", "nope").unwrap().is_none());
    }

    #[test]
    fn test_ac5_list_sorted_by_success_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "rare", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        // WHEN
        let list = proc.list("ns").unwrap();
        // THEN
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].trigger, "popular");
        assert_eq!(list[0].success_count, 3);
        assert_eq!(list[1].trigger, "rare");
        assert_eq!(list[1].success_count, 1);
    }

    #[test]
    fn test_empty_trigger_rejected() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(matches!(
            proc.learn("ns", "", &["step".into()]),
            Err(ProceduralMemoryError::EmptyTrigger)
        ));
    }

    #[test]
    fn test_empty_steps_rejected() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(matches!(
            proc.learn("ns", "trigger", &[]),
            Err(ProceduralMemoryError::EmptySteps)
        ));
    }

    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns-a", "trigger", &["step-a".into()]).unwrap();
        proc.learn("ns-b", "trigger", &["step-b".into()]).unwrap();
        // WHEN
        let a = proc.recall("ns-a", "trigger").unwrap().unwrap();
        let b = proc.recall("ns-b", "trigger").unwrap().unwrap();
        // THEN
        assert_eq!(a.steps[0], "step-a");
        assert_eq!(b.steps[0], "step-b");
    }

    #[test]
    fn test_list_empty_namespace() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(proc.list("empty").unwrap().is_empty());
    }
}
