//! Persistance SQLite des triggers — `trigger_history` et `trigger_state`.
//!
//! [`TriggerPersistence`] est une struct **synchrone** (pas d'acteur Tokio).
//! Elle est conçue pour être utilisée depuis [`crate::engine::TriggerEngine`]
//! directement (la connexion SQLite étant `Send`, elle vit dans la tâche Tokio
//! de l'acteur) ou via `spawn_blocking` si appelée depuis un contexte qui ne
//! tolère pas de blocage.
//!
//! La migration [`include_str`] est idempotente (`CREATE TABLE IF NOT EXISTS`).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

// ─── Erreurs ──────────────────────────────────────────────────────────────

/// Erreurs de persistance des triggers.
#[derive(thiserror::Error, Debug)]
pub enum TriggerPersistenceError {
    /// Erreur SQLite sous-jacente.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Échec de parsing d'un horodatage stocké en base.
    #[error("timestamp parse error: {0}")]
    TimestampParse(String),
}

// ─── Types de résultats ───────────────────────────────────────────────────

/// Entrée d'historique retournée par [`TriggerPersistence::query_history`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerHistoryEntry {
    /// Identifiant unique de l'entrée (hex randomblob généré par SQLite).
    pub id: String,
    /// Identifiant du trigger concerné.
    pub trigger_id: String,
    /// Nom de l'agent cible.
    pub agent_name: String,
    /// Horodatage du déclenchement.
    pub fired_at: DateTime<Utc>,
    /// Identifiant de la tâche soumise — `None` si `status` est `skipped` ou `error`.
    pub task_id: Option<String>,
    /// Statut : `"fired"` | `"skipped"` | `"error"`.
    pub status: String,
    /// Raison du skip ou de l'erreur — `None` si `status` est `"fired"`.
    pub reason: Option<String>,
    /// Payload JSON complet du trigger — `None` si absent (timer sans payload).
    pub payload_json: Option<String>,
    /// Temps de dispatch en millisecondes (réception trigger → soumission tâche).
    pub dispatch_ms: Option<i64>,
}

/// État courant d'un trigger dans `trigger_state`.
#[derive(Debug, Clone)]
pub struct TriggerStateRow {
    /// Identifiant du trigger.
    pub trigger_id: String,
    /// Horodatage du dernier fire réussi — `None` si jamais déclenché.
    pub last_fired: Option<DateTime<Utc>>,
    /// Nombre cumulé de fires réussis.
    pub fire_count: u64,
    /// Nombre cumulé de skips.
    pub skip_count: u64,
    /// Indique si le trigger est activé dans `trigger_state`.
    pub enabled: bool,
}

/// Statistiques d'un trigger recalculées depuis la table `trigger_history`.
#[derive(Debug, Clone)]
pub struct TriggerStats {
    /// Nombre de fires réussis.
    pub fire_count: u64,
    /// Nombre de skips.
    pub skip_count: u64,
    /// Horodatage du dernier fire réussi — `None` si jamais déclenché.
    pub last_fired: Option<DateTime<Utc>>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Ajoute des colonnes à une table si elles n'existent pas encore.
///
/// Utilise `PRAGMA table_info` pour lister les colonnes existantes,
/// puis exécute `ALTER TABLE ADD COLUMN` uniquement pour les manquantes.
fn add_columns_if_missing(
    conn: &Connection,
    table: &str,
    columns: &[(&str, &str)],
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (col_name, col_type) in columns {
        if !existing.iter().any(|c| c == col_name) {
            conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {col_name} {col_type};"
            ))?;
        }
    }
    Ok(())
}

// ─── TriggerPersistence ───────────────────────────────────────────────────

/// Interface de persistance pour les triggers — wraps `rusqlite::Connection`.
///
/// Conçue pour être détenue par [`crate::engine::TriggerEngine`]. La connexion
/// SQLite implémente `Send`, ce qui autorise son stockage dans un `tokio::spawn`.
pub struct TriggerPersistence {
    conn: Connection,
}

impl TriggerPersistence {
    /// Ouvre (ou crée) la base SQLite et applique la migration 003.
    ///
    /// Utilise `CREATE TABLE IF NOT EXISTS` — idempotente.
    /// **Fail fast** : une erreur de migration est fatale (`.expect`),
    /// conformément au Principe #4 d'Apollia OS.
    pub fn open(db_path: &std::path::Path) -> Result<Self, TriggerPersistenceError> {
        let conn = Connection::open(db_path)?;
        // WAL pour de meilleures performances en écriture concurrente
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(include_str!(
            "../../apollia-tools/migrations/003_trigger_tables.sql"
        ))?;
        // Migration : ajout payload_json et dispatch_ms.
        // Utilise PRAGMA table_info pour vérifier l'existence (pattern cohérent workspace).
        add_columns_if_missing(
            &conn,
            "trigger_history",
            &[("payload_json", "TEXT"), ("dispatch_ms", "INTEGER")],
        )?;
        Ok(Self { conn })
    }

    /// Persiste un fire réussi dans `trigger_history` et incrémente `fire_count`.
    ///
    /// Met également à jour `trigger_state.last_fired` avec `fired_at`.
    /// `payload_json` et `dispatch_ms` sont optionnels pour la rétrocompatibilité.
    pub fn record_fired(
        &mut self,
        trigger_id: &str,
        agent_name: &str,
        task_id: &str,
        fired_at: DateTime<Utc>,
        payload_json: Option<&str>,
        dispatch_ms: Option<i64>,
    ) -> Result<(), TriggerPersistenceError> {
        let fired_at_str = fired_at.to_rfc3339();
        self.conn.execute(
            "INSERT INTO trigger_history \
             (trigger_id, agent_name, fired_at, task_id, status, payload_json, dispatch_ms) \
             VALUES (?1, ?2, ?3, ?4, 'fired', ?5, ?6)",
            params![
                trigger_id,
                agent_name,
                fired_at_str,
                task_id,
                payload_json,
                dispatch_ms
            ],
        )?;
        self.conn.execute(
            "INSERT INTO trigger_state (trigger_id, last_fired, fire_count, skip_count) \
             VALUES (?1, ?2, 1, 0) \
             ON CONFLICT(trigger_id) DO UPDATE SET \
               last_fired = excluded.last_fired, \
               fire_count = fire_count + 1",
            params![trigger_id, fired_at_str],
        )?;
        Ok(())
    }

    /// Persiste un skip (`on_busy=drop`) dans `trigger_history` et incrémente `skip_count`.
    ///
    /// `payload_json` est optionnel — permet de tracer le payload même en cas de skip.
    pub fn record_skipped(
        &mut self,
        trigger_id: &str,
        agent_name: &str,
        reason: &str,
        fired_at: DateTime<Utc>,
        payload_json: Option<&str>,
    ) -> Result<(), TriggerPersistenceError> {
        let fired_at_str = fired_at.to_rfc3339();
        self.conn.execute(
            "INSERT INTO trigger_history \
             (trigger_id, agent_name, fired_at, task_id, status, reason, payload_json) \
             VALUES (?1, ?2, ?3, NULL, 'skipped', ?4, ?5)",
            params![trigger_id, agent_name, fired_at_str, reason, payload_json],
        )?;
        self.conn.execute(
            "INSERT INTO trigger_state (trigger_id, fire_count, skip_count) \
             VALUES (?1, 0, 1) \
             ON CONFLICT(trigger_id) DO UPDATE SET \
               skip_count = skip_count + 1",
            params![trigger_id],
        )?;
        Ok(())
    }

    /// Persiste une erreur de soumission au `TaskRouter` dans `trigger_history`.
    ///
    /// `payload_json` est optionnel — permet de tracer le payload même en cas d'erreur.
    pub fn record_error(
        &mut self,
        trigger_id: &str,
        agent_name: &str,
        error: &str,
        fired_at: DateTime<Utc>,
        payload_json: Option<&str>,
    ) -> Result<(), TriggerPersistenceError> {
        let fired_at_str = fired_at.to_rfc3339();
        self.conn.execute(
            "INSERT INTO trigger_history \
             (trigger_id, agent_name, fired_at, task_id, status, reason, payload_json) \
             VALUES (?1, ?2, ?3, NULL, 'error', ?4, ?5)",
            params![trigger_id, agent_name, fired_at_str, error, payload_json],
        )?;
        Ok(())
    }

    /// Retourne les `limit` dernières entrées pour un trigger donné, triées `fired_at DESC`.
    pub fn query_history(
        &self,
        trigger_id: &str,
        limit: usize,
    ) -> Result<Vec<TriggerHistoryEntry>, TriggerPersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, trigger_id, agent_name, fired_at, task_id, status, reason, \
                    payload_json, dispatch_ms \
             FROM trigger_history \
             WHERE trigger_id = ?1 \
             ORDER BY fired_at DESC \
             LIMIT ?2",
        )?;

        let mut entries = Vec::new();
        let rows = stmt.query_map(params![trigger_id, limit as i64], |row| {
            Ok(RawHistoryRow {
                id: row.get(0)?,
                trigger_id: row.get(1)?,
                agent_name: row.get(2)?,
                fired_at: row.get(3)?,
                task_id: row.get(4)?,
                status: row.get(5)?,
                reason: row.get(6)?,
                payload_json: row.get(7)?,
                dispatch_ms: row.get(8)?,
            })
        })?;

        for row_result in rows {
            let row = row_result?;
            let fired_at = DateTime::parse_from_rfc3339(&row.fired_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| TriggerPersistenceError::TimestampParse(e.to_string()))?;
            entries.push(TriggerHistoryEntry {
                id: row.id,
                trigger_id: row.trigger_id,
                agent_name: row.agent_name,
                fired_at,
                task_id: row.task_id,
                status: row.status,
                reason: row.reason,
                payload_json: row.payload_json,
                dispatch_ms: row.dispatch_ms,
            });
        }
        Ok(entries)
    }

    /// Recalcule les compteurs de tous les triggers depuis `trigger_history`.
    ///
    /// Retourne une map `trigger_id → TriggerStats`. Les triggers sans historique
    /// n'apparaissent pas dans la map — leurs compteurs restent à zéro côté moteur.
    /// La requête utilise `SUM(CASE WHEN …)` pour une compatibilité maximale avec
    /// toutes les versions de SQLite embarquées dans rusqlite.
    pub fn load_counters(&self) -> Result<HashMap<String, TriggerStats>, TriggerPersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT trigger_id, \
                    SUM(CASE WHEN status = 'fired'   THEN 1 ELSE 0 END) AS fire_count, \
                    SUM(CASE WHEN status = 'skipped' THEN 1 ELSE 0 END) AS skip_count, \
                    MAX(CASE WHEN status = 'fired'   THEN fired_at ELSE NULL END) AS last_fired \
             FROM trigger_history \
             GROUP BY trigger_id",
        )?;

        let mut map = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok(RawCounterRow {
                trigger_id: row.get(0)?,
                fire_count: row.get::<_, i64>(1)?,
                skip_count: row.get::<_, i64>(2)?,
                last_fired: row.get(3)?,
            })
        })?;

        for row_result in rows {
            let row = row_result?;
            let last_fired = row
                .last_fired
                .map(|s: String| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| TriggerPersistenceError::TimestampParse(e.to_string()))
                })
                .transpose()?;
            map.insert(
                row.trigger_id,
                TriggerStats {
                    fire_count: row.fire_count as u64,
                    skip_count: row.skip_count as u64,
                    last_fired,
                },
            );
        }

        Ok(map)
    }

    /// Retourne l'état courant d'un trigger (`last_fired`, `fire_count`, `skip_count`).
    ///
    /// Retourne `None` si aucun état n'existe encore pour ce trigger.
    pub fn get_state(
        &self,
        trigger_id: &str,
    ) -> Result<Option<TriggerStateRow>, TriggerPersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT trigger_id, last_fired, fire_count, skip_count, enabled \
             FROM trigger_state \
             WHERE trigger_id = ?1",
        )?;

        let mut rows = stmt.query_map(params![trigger_id], |row| {
            Ok(RawStateRow {
                trigger_id: row.get(0)?,
                last_fired: row.get(1)?,
                fire_count: row.get::<_, i64>(2)?,
                skip_count: row.get::<_, i64>(3)?,
                enabled: row.get(4)?,
            })
        })?;

        match rows.next() {
            None => Ok(None),
            Some(row_result) => {
                let row = row_result?;
                let last_fired = row
                    .last_fired
                    .map(|s: String| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .map_err(|e| TriggerPersistenceError::TimestampParse(e.to_string()))
                    })
                    .transpose()?;
                Ok(Some(TriggerStateRow {
                    trigger_id: row.trigger_id,
                    last_fired,
                    fire_count: row.fire_count as u64,
                    skip_count: row.skip_count as u64,
                    enabled: row.enabled,
                }))
            }
        }
    }
}

// ─── Types internes (raw rows) ────────────────────────────────────────────

/// Ligne brute de `trigger_history` telle que lue depuis SQLite.
struct RawHistoryRow {
    id: String,
    trigger_id: String,
    agent_name: String,
    fired_at: String,
    task_id: Option<String>,
    status: String,
    reason: Option<String>,
    payload_json: Option<String>,
    dispatch_ms: Option<i64>,
}

/// Ligne brute de `trigger_history` agrégée par `load_counters`.
struct RawCounterRow {
    trigger_id: String,
    fire_count: i64,
    skip_count: i64,
    last_fired: Option<String>,
}

/// Ligne brute de `trigger_state` telle que lue depuis SQLite.
struct RawStateRow {
    trigger_id: String,
    last_fired: Option<String>,
    fire_count: i64,
    skip_count: i64,
    enabled: bool,
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Ouvre une base de test dans un répertoire temporaire.
    fn open_test_db() -> (TempDir, TriggerPersistence) {
        let dir = TempDir::new().unwrap();
        let db = TriggerPersistence::open(&dir.path().join("test.db")).unwrap();
        (dir, db)
    }

    #[test]
    fn test_ac1_migration_creates_tables() {
        // GIVEN / WHEN — open_test_db applique la migration
        let (_dir, persistence) = open_test_db();
        // THEN — les tables existent (query ne panique pas, retourne vec vide)
        let entries = persistence.query_history("any", 10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_ac2_record_fired_persists_entry() {
        // GIVEN
        let (_dir, mut persistence) = open_test_db();
        // WHEN
        persistence
            .record_fired(
                "rapport-hebdo",
                "rapport-agent",
                "task-001",
                Utc::now(),
                None,
                None,
            )
            .unwrap();
        // THEN — entrée dans trigger_history
        let entries = persistence.query_history("rapport-hebdo", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "fired");
        assert_eq!(entries[0].task_id, Some("task-001".into()));
        // ET trigger_state.fire_count = 1
        let state = persistence.get_state("rapport-hebdo").unwrap().unwrap();
        assert_eq!(state.fire_count, 1);
        assert!(state.last_fired.is_some());
    }

    #[test]
    fn test_ac3_record_skipped_persists_reason() {
        // GIVEN
        let (_dir, mut persistence) = open_test_db();
        // WHEN
        persistence
            .record_skipped("crm-sync", "crm-agent", "agent busy", Utc::now(), None)
            .unwrap();
        // THEN
        let entries = persistence.query_history("crm-sync", 10).unwrap();
        assert_eq!(entries[0].status, "skipped");
        assert!(entries[0].task_id.is_none());
        assert_eq!(entries[0].reason, Some("agent busy".into()));
        let state = persistence.get_state("crm-sync").unwrap().unwrap();
        assert_eq!(state.skip_count, 1);
        assert_eq!(state.fire_count, 0);
    }

    #[test]
    fn test_ac4_record_error_persists_error_message() {
        // GIVEN
        let (_dir, mut persistence) = open_test_db();
        // WHEN
        persistence
            .record_error(
                "factures",
                "facture-agent",
                "submit failed: agent not found",
                Utc::now(),
                None,
            )
            .unwrap();
        // THEN
        let entries = persistence.query_history("factures", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "error");
        assert!(entries[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("submit failed"));
    }

    #[test]
    fn test_ac5_query_history_returns_n_last_entries_sorted_desc() {
        // GIVEN — 5 fires espacés de 1 seconde
        let (_dir, mut persistence) = open_test_db();
        let base = Utc::now();
        for i in 0..5u64 {
            let t = base + chrono::Duration::seconds(i as i64);
            persistence
                .record_fired(
                    "rapport-hebdo",
                    "agent",
                    &format!("task-{i}"),
                    t,
                    None,
                    None,
                )
                .unwrap();
        }
        // WHEN — limit = 3
        let entries = persistence.query_history("rapport-hebdo", 3).unwrap();
        // THEN — 3 entrées, triées DESC (la plus récente en premier)
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].task_id, Some("task-4".into()));
        assert_eq!(entries[1].task_id, Some("task-3".into()));
        assert_eq!(entries[2].task_id, Some("task-2".into()));
    }

    // ── Extra ──────────────────────────────────────────────────────────────

    #[test]
    fn test_get_state_returns_none_for_unknown_trigger() {
        // GIVEN une base vide
        let (_dir, persistence) = open_test_db();
        // WHEN
        let state = persistence.get_state("unknown-trigger").unwrap();
        // THEN
        assert!(state.is_none());
    }

    #[test]
    fn test_fire_count_accumulates_across_multiple_fires() {
        // GIVEN
        let (_dir, mut persistence) = open_test_db();
        // WHEN — 3 fires
        for i in 0..3u64 {
            persistence
                .record_fired(
                    "my-trigger",
                    "my-agent",
                    &format!("task-{i}"),
                    Utc::now(),
                    None,
                    None,
                )
                .unwrap();
        }
        // THEN — fire_count = 3
        let state = persistence.get_state("my-trigger").unwrap().unwrap();
        assert_eq!(state.fire_count, 3);
        assert_eq!(state.skip_count, 0);
    }

    #[test]
    fn test_idempotent_migration_can_open_twice() {
        // GIVEN une base déjà migrée
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        {
            let _p = TriggerPersistence::open(&path).unwrap();
        }
        // WHEN on ouvre de nouveau (IF NOT EXISTS → pas d'erreur)
        let result = TriggerPersistence::open(&path);
        // THEN
        assert!(result.is_ok());
    }

    // ── payload_json + dispatch_ms ──────────────────────────────────────────

    #[test]
    fn test_trigger_payload_persisted() {
        // GIVEN un TriggerPersistence in-memory
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_fired avec payload_json
        persistence
            .record_fired(
                "trg-1",
                "agent-x",
                "t-123",
                Utc::now(),
                Some(r#"{"key":"val"}"#),
                Some(45),
            )
            .unwrap();
        // THEN payload_json est persisté
        let entries = persistence.query_history("trg-1", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].payload_json.as_deref(), Some(r#"{"key":"val"}"#));
    }

    #[test]
    fn test_trigger_task_id_recorded() {
        // GIVEN un fire réussi
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_fired avec task_id = "t-abc"
        persistence
            .record_fired("trg-2", "agent-y", "t-abc", Utc::now(), None, None)
            .unwrap();
        // THEN task_id est persisté
        let entries = persistence.query_history("trg-2", 10).unwrap();
        assert_eq!(entries[0].task_id, Some("t-abc".into()));
    }

    #[test]
    fn test_trigger_dispatch_ms_measured() {
        // GIVEN un fire
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_fired avec dispatch_ms = 45
        persistence
            .record_fired("trg-3", "agent-z", "t-456", Utc::now(), None, Some(45))
            .unwrap();
        // THEN dispatch_ms est persisté
        let entries = persistence.query_history("trg-3", 10).unwrap();
        assert_eq!(entries[0].dispatch_ms, Some(45));
    }

    #[test]
    fn test_trigger_payload_none_when_absent() {
        // GIVEN un fire sans payload
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_fired sans payload_json ni dispatch_ms
        persistence
            .record_fired("trg-4", "agent-w", "t-789", Utc::now(), None, None)
            .unwrap();
        // THEN les champs sont None
        let entries = persistence.query_history("trg-4", 10).unwrap();
        assert!(entries[0].payload_json.is_none());
        assert!(entries[0].dispatch_ms.is_none());
    }

    #[test]
    fn test_skipped_with_payload() {
        // GIVEN un skip avec payload
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_skipped avec payload_json
        persistence
            .record_skipped(
                "trg-5",
                "agent-v",
                "agent busy",
                Utc::now(),
                Some(r#"{"action":"sync"}"#),
            )
            .unwrap();
        // THEN payload_json est persisté, dispatch_ms est None (pas de dispatch pour un skip)
        let entries = persistence.query_history("trg-5", 10).unwrap();
        assert_eq!(
            entries[0].payload_json.as_deref(),
            Some(r#"{"action":"sync"}"#)
        );
        assert!(entries[0].dispatch_ms.is_none());
    }

    // ── load_counters ──────────────────────────────────────────────────────

    #[test]
    fn test_load_counters_empty_history_table() {
        // GIVEN une base vide
        let (_dir, persistence) = open_test_db();
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN aucune entrée, pas d'erreur
        assert!(counters.is_empty());
    }

    #[test]
    fn test_load_counters_after_3_fires() {
        // GIVEN 3 fires insérés
        let (_dir, mut persistence) = open_test_db();
        let base = Utc::now();
        for i in 0..3u64 {
            persistence
                .record_fired(
                    "t-fires",
                    "agent",
                    &format!("task-{i}"),
                    base + chrono::Duration::seconds(i as i64),
                    None,
                    None,
                )
                .unwrap();
        }
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN fire_count = 3
        let stats = counters.get("t-fires").expect("trigger absent de la map");
        assert_eq!(stats.fire_count, 3);
        assert_eq!(stats.skip_count, 0);
    }

    #[test]
    fn test_load_counters_after_2_skips() {
        // GIVEN 2 skips insérés
        let (_dir, mut persistence) = open_test_db();
        for _ in 0..2 {
            persistence
                .record_skipped("t-skips", "agent", "busy", Utc::now(), None)
                .unwrap();
        }
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN skip_count = 2
        let stats = counters.get("t-skips").expect("trigger absent de la map");
        assert_eq!(stats.skip_count, 2);
        assert_eq!(stats.fire_count, 0);
    }

    #[test]
    fn test_load_counters_preserves_last_fired() {
        // GIVEN des fires avec timestamps distincts
        let (_dir, mut persistence) = open_test_db();
        let base = Utc::now();
        let t0 = base;
        let t1 = base + chrono::Duration::seconds(10);
        persistence
            .record_fired("t-last", "agent", "task-a", t0, None, None)
            .unwrap();
        persistence
            .record_fired("t-last", "agent", "task-b", t1, None, None)
            .unwrap();
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN last_fired = t1 (le plus récent)
        let stats = counters.get("t-last").expect("trigger absent de la map");
        let last = stats.last_fired.expect("last_fired doit être Some");
        // Comparer à la seconde près (RFC3339 ne préserve pas les sub-secondes de Utc::now)
        assert_eq!(last.timestamp(), t1.timestamp());
    }

    #[test]
    fn test_load_counters_new_trigger_zero() {
        // GIVEN une base sans entrée pour ce trigger
        let (_dir, persistence) = open_test_db();
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN le trigger inconnu n'est pas dans la map (compteurs à zéro côté moteur)
        assert!(counters.get("non-existant").is_none());
    }

    #[test]
    fn test_load_counters_mixed_triggers() {
        // GIVEN deux triggers avec des historiques différents
        let (_dir, mut persistence) = open_test_db();
        let now = Utc::now();
        // trigger-a : 5 fires, 1 skip
        for i in 0..5u64 {
            persistence
                .record_fired(
                    "trigger-a",
                    "agent",
                    &format!("t{i}"),
                    now + chrono::Duration::seconds(i as i64),
                    None,
                    None,
                )
                .unwrap();
        }
        persistence
            .record_skipped("trigger-a", "agent", "busy", now, None)
            .unwrap();
        // trigger-b : 0 fires, 3 skips
        for _ in 0..3 {
            persistence
                .record_skipped("trigger-b", "agent", "busy", now, None)
                .unwrap();
        }
        // WHEN
        let counters = persistence.load_counters().unwrap();
        // THEN compteurs indépendants
        let a = counters.get("trigger-a").expect("trigger-a absent");
        assert_eq!(a.fire_count, 5);
        assert_eq!(a.skip_count, 1);
        let b = counters.get("trigger-b").expect("trigger-b absent");
        assert_eq!(b.fire_count, 0);
        assert_eq!(b.skip_count, 3);
    }

    #[test]
    fn test_error_with_payload() {
        // GIVEN une erreur avec payload
        let (_dir, mut persistence) = open_test_db();
        // WHEN record_error avec payload_json
        persistence
            .record_error(
                "trg-6",
                "agent-u",
                "submit failed",
                Utc::now(),
                Some(r#"{"data":"test"}"#),
            )
            .unwrap();
        // THEN payload_json est persisté
        let entries = persistence.query_history("trg-6", 10).unwrap();
        assert_eq!(
            entries[0].payload_json.as_deref(),
            Some(r#"{"data":"test"}"#)
        );
        assert!(entries[0].dispatch_ms.is_none());
    }
}
