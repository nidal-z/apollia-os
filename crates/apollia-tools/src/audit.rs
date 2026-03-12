//! Audit trail — SQLite-persisted log of tool invocations.
//!
//! Architecture : acteur `tokio::task::spawn_blocking` avec `tokio::sync::mpsc` borné.
//! L'acteur détient la `rusqlite::Connection` (non-`Sync`) en exclusivité.
//! Le handle est clonable et expose une API async via `oneshot` pour
//! les opérations qui nécessitent une réponse.

use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Enregistrement d'une invocation d'outil pour l'audit trail.
#[derive(Debug, Clone)]
pub struct ToolInvocationRecord {
    /// Identifiant unique de l'invocation (UUID v4).
    pub id: String,
    /// Identifiant de l'agent ayant invoqué l'outil.
    pub agent_id: String,
    /// Identifiant de la tâche dans laquelle s'inscrit l'invocation.
    pub task_id: String,
    /// Nom de l'outil invoqué (ex. `bash_executor`, `file_io`).
    pub tool_name: String,
    /// SHA256 hex des paramètres sérialisés en JSON.
    pub input_hash: String,
    /// Profil de sandbox utilisé, sérialisé en string.
    pub sandbox_profile: String,
    /// Horodatage de début d'invocation au format RFC3339 UTC.
    pub started_at: String,
    /// Durée de l'invocation en millisecondes.
    pub duration_ms: Option<u64>,
    /// Code de retour du processus fils.
    pub exit_code: Option<i32>,
    /// `true` si l'invocation s'est terminée sans erreur.
    pub success: bool,
    /// Code d'erreur lisible si `success` est `false`.
    pub error_code: Option<String>,
    /// Ressources consommées (JSON brut). `null` en MVP.
    pub resources_used: Option<serde_json::Value>,
}

/// Statistiques agrégées de l'audit trail.
#[derive(Debug, Clone)]
pub struct AuditStats {
    /// Nombre total d'invocations enregistrées.
    pub total_events: u64,
    /// Nombre d'outils distincts invoqués.
    pub unique_tools: u64,
    /// Nombre d'agents distincts ayant invoqué des outils.
    pub unique_agents: u64,
}

/// Erreurs d'ouverture ou d'initialisation de l'audit trail.
#[derive(Debug, Error)]
pub enum AuditTrailError {
    /// Échec d'ouverture du fichier SQLite.
    #[error("failed to open SQLite database: {0}")]
    OpenFailed(String),
    /// Échec de création du schéma (table ou index).
    #[error("failed to initialize audit schema: {0}")]
    SchemaInitFailed(String),
}

/// Calcule le SHA256 d'un objet JSON sérialisé.
///
/// Retourne la représentation hexadécimale lowercase du hash.
/// Deux appels avec la même valeur JSON produisent toujours le même résultat.
pub fn compute_input_hash(params: &serde_json::Value) -> String {
    let serialized = serde_json::to_string(params).unwrap_or_default();
    let hash = Sha256::digest(serialized.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Schéma SQL
// ---------------------------------------------------------------------------

/// Schéma SQL de la table `tool_invocations` et de ses index.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS tool_invocations (
        id              TEXT PRIMARY KEY,
        agent_id        TEXT NOT NULL,
        task_id         TEXT NOT NULL,
        tool_name       TEXT NOT NULL,
        input_hash      TEXT NOT NULL,
        sandbox_profile TEXT NOT NULL,
        started_at      TEXT NOT NULL,
        duration_ms     INTEGER,
        exit_code       INTEGER,
        success         INTEGER NOT NULL,
        error_code      TEXT,
        resources_used  TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_tool_invocations_agent_id
        ON tool_invocations(agent_id);
    CREATE INDEX IF NOT EXISTS idx_tool_invocations_started_at
        ON tool_invocations(started_at);
";

// ---------------------------------------------------------------------------
// Messages internes
// ---------------------------------------------------------------------------

/// Messages envoyés à l'acteur AuditTrail.
enum AuditMessage {
    /// Insère un enregistrement (fire-and-forget).
    Record(Box<ToolInvocationRecord>),
    /// Retourne les N dernières invocations, triées par date décroissante.
    QueryLast {
        n: usize,
        reply: tokio::sync::oneshot::Sender<Vec<ToolInvocationRecord>>,
    },
    /// Retourne les statistiques agrégées de l'audit trail.
    QueryStats {
        reply: tokio::sync::oneshot::Sender<AuditStats>,
    },
    /// Arrête l'acteur proprement après avoir vidé la file.
    Shutdown,
}

// ---------------------------------------------------------------------------
// Acteur interne
// ---------------------------------------------------------------------------

/// Acteur interne — jamais exposé directement.
///
/// Détient la `rusqlite::Connection` et traite les messages de manière séquentielle,
/// garantissant l'absence de contention sur la base de données.
struct AuditTrail {
    conn: rusqlite::Connection,
    receiver: tokio::sync::mpsc::Receiver<AuditMessage>,
}

impl AuditTrail {
    /// Boucle principale de l'acteur.
    fn run(mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            match msg {
                AuditMessage::Record(record) => {
                    if let Err(e) = Self::insert(&self.conn, &record) {
                        tracing::error!(
                            error = %e,
                            tool = %record.tool_name,
                            id   = %record.id,
                            "audit insert failed"
                        );
                    }
                }
                AuditMessage::QueryLast { n, reply } => {
                    let results = Self::query_last_n(&self.conn, n).unwrap_or_default();
                    let _ = reply.send(results);
                }
                AuditMessage::QueryStats { reply } => {
                    let stats = Self::query_stats(&self.conn).unwrap_or(AuditStats {
                        total_events: 0,
                        unique_tools: 0,
                        unique_agents: 0,
                    });
                    let _ = reply.send(stats);
                }
                AuditMessage::Shutdown => break,
            }
        }
    }

    /// Insère un enregistrement dans `tool_invocations`.
    fn insert(conn: &rusqlite::Connection, r: &ToolInvocationRecord) -> rusqlite::Result<()> {
        let resources_json = r.resources_used.as_ref().map(|v| v.to_string());
        conn.execute(
            "INSERT INTO tool_invocations \
             (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
              started_at, duration_ms, exit_code, success, error_code, resources_used) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                r.id,
                r.agent_id,
                r.task_id,
                r.tool_name,
                r.input_hash,
                r.sandbox_profile,
                r.started_at,
                r.duration_ms.map(|v| v as i64),
                r.exit_code,
                r.success as i32,
                r.error_code,
                resources_json,
            ],
        )?;
        Ok(())
    }

    /// Retourne les statistiques agrégées de l'audit trail via une requête SQL agrégée.
    fn query_stats(conn: &rusqlite::Connection) -> rusqlite::Result<AuditStats> {
        conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT tool_name), COUNT(DISTINCT agent_id) \
             FROM tool_invocations",
            [],
            |row| {
                Ok(AuditStats {
                    total_events: row.get::<_, i64>(0)? as u64,
                    unique_tools: row.get::<_, i64>(1)? as u64,
                    unique_agents: row.get::<_, i64>(2)? as u64,
                })
            },
        )
    }

    /// Retourne les N dernières invocations, ordonnées par `started_at` décroissant.
    fn query_last_n(
        conn: &rusqlite::Connection,
        n: usize,
    ) -> rusqlite::Result<Vec<ToolInvocationRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
             started_at, duration_ms, exit_code, success, error_code, resources_used \
             FROM tool_invocations \
             ORDER BY started_at DESC \
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![n as i64], |row| {
            let resources_str: Option<String> = row.get(11)?;
            let resources_used = resources_str.and_then(|s| serde_json::from_str(&s).ok());
            Ok(ToolInvocationRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                task_id: row.get(2)?,
                tool_name: row.get(3)?,
                input_hash: row.get(4)?,
                sandbox_profile: row.get(5)?,
                started_at: row.get(6)?,
                duration_ms: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                exit_code: row.get(8)?,
                success: row.get::<_, i32>(9)? != 0,
                error_code: row.get(10)?,
                resources_used,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }
}

// ---------------------------------------------------------------------------
// Handle public
// ---------------------------------------------------------------------------

/// Capacité du channel interne entre le handle et l'acteur.
const CHANNEL_CAPACITY: usize = 1024;

/// Handle clonable vers l'acteur [`AuditTrail`].
///
/// Créé via [`AuditTrailHandle::open`]. Toutes les méthodes sont thread-safe ;
/// plusieurs handles peuvent coexister et émettre des messages vers le même acteur.
#[derive(Clone)]
pub struct AuditTrailHandle {
    sender: tokio::sync::mpsc::Sender<AuditMessage>,
}

impl AuditTrailHandle {
    /// Ouvre la base SQLite et démarre l'acteur en arrière-plan.
    ///
    /// Crée le fichier si absent. Crée la table `tool_invocations` et ses index
    /// si ils n'existent pas (`CREATE … IF NOT EXISTS`). Active le mode WAL.
    pub async fn open(db_path: &Path) -> Result<Self, AuditTrailError> {
        let db_path = db_path.to_path_buf();
        let (sender, receiver) = tokio::sync::mpsc::channel::<AuditMessage>(CHANNEL_CAPACITY);

        // Canal de signalisation pour l'initialisation : le thread informe l'appelant
        // dès que la connexion et le schéma sont prêts (ou en cas d'erreur).
        let (init_tx, init_rx) = tokio::sync::oneshot::channel::<Result<(), AuditTrailError>>();

        tokio::task::spawn_blocking(move || {
            // Ouverture de la connexion SQLite.
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    let _ = init_tx.send(Err(AuditTrailError::OpenFailed(e.to_string())));
                    return;
                }
            };

            // Mode WAL pour la concurrence lecture/écriture.
            if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL;") {
                let _ = init_tx.send(Err(AuditTrailError::SchemaInitFailed(e.to_string())));
                return;
            }

            // Création du schéma (idempotente).
            if let Err(e) = conn.execute_batch(SCHEMA) {
                let _ = init_tx.send(Err(AuditTrailError::SchemaInitFailed(e.to_string())));
                return;
            }

            // Signale le succès de l'initialisation avant d'entrer dans la boucle.
            let _ = init_tx.send(Ok(()));

            // Boucle de l'acteur — tourne jusqu'à réception de Shutdown.
            AuditTrail { conn, receiver }.run();
        });

        // Attendre le résultat de l'initialisation.
        init_rx
            .await
            .map_err(|_| AuditTrailError::OpenFailed("init channel disconnected".to_string()))??;

        Ok(Self { sender })
    }

    /// Enregistre une invocation (fire-and-forget).
    ///
    /// Retourne immédiatement sans attendre la confirmation SQLite.
    /// Si le channel est saturé, l'enregistrement est abandonné avec un `warn!`.
    pub fn record(&self, record: ToolInvocationRecord) {
        match self.sender.try_send(AuditMessage::Record(Box::new(record))) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("audit trail channel full, record dropped (backpressure)");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("audit trail actor disconnected, record dropped");
            }
        }
    }

    /// Retourne les N dernières invocations, triées par date décroissante.
    pub async fn query_last(&self, n: usize) -> Vec<ToolInvocationRecord> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(AuditMessage::QueryLast { n, reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Retourne les statistiques agrégées (total, outils distincts, agents distincts).
    pub async fn stats(&self) -> AuditStats {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(AuditMessage::QueryStats { reply: reply_tx })
            .await
            .is_err()
        {
            return AuditStats {
                total_events: 0,
                unique_tools: 0,
                unique_agents: 0,
            };
        }
        reply_rx.await.unwrap_or(AuditStats {
            total_events: 0,
            unique_tools: 0,
            unique_agents: 0,
        })
    }

    /// Envoie le signal d'arrêt à l'acteur et attend qu'il ait traité tous les messages en attente.
    ///
    /// Après cet appel, le handle est consommé. Les handles clonés restants
    /// tenteront d'envoyer sur un channel dont le receiver est fermé.
    pub async fn shutdown(self) {
        let _ = self.sender.send(AuditMessage::Shutdown).await;
        // Laisse le temps à l'acteur de traiter les messages en file et de fermer la connexion.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_test_audit() -> AuditTrailHandle {
        let db_path =
            std::env::temp_dir().join(format!("apollia_audit_test_{}.db", uuid::Uuid::new_v4()));
        AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit trail")
    }

    fn make_record(success: bool, error_code: Option<&str>) -> ToolInvocationRecord {
        ToolInvocationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: "test-agent".to_string(),
            task_id: "task-001".to_string(),
            tool_name: "bash_executor".to_string(),
            input_hash: "abc123".to_string(),
            sandbox_profile: "file_system".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            duration_ms: Some(42),
            exit_code: if success { Some(0) } else { Some(1) },
            success,
            error_code: error_code.map(|s| s.to_string()),
            resources_used: None,
        }
    }

    // AC-1 — Enregistrement d'une invocation réussie
    #[tokio::test]
    async fn test_ac1_record_successful_invocation() {
        // GIVEN
        let handle = open_test_audit().await;
        let record = make_record(true, None);
        let tool_name = record.tool_name.clone();
        // WHEN
        handle.record(record);
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // THEN
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, tool_name);
        assert!(results[0].success);
        handle.shutdown().await;
    }

    // AC-2 — Enregistrement d'une invocation échouée
    #[tokio::test]
    async fn test_ac2_record_failed_invocation() {
        // GIVEN
        let handle = open_test_audit().await;
        let record = make_record(false, Some("Timeout"));
        // WHEN
        handle.record(record);
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // THEN
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert_eq!(results[0].error_code.as_deref(), Some("Timeout"));
        handle.shutdown().await;
    }

    // AC-3 — Schéma créé automatiquement sur une base fraîche
    #[tokio::test]
    async fn test_ac3_schema_created_on_fresh_db() {
        // GIVEN — base inexistante
        let db_path =
            std::env::temp_dir().join(format!("apollia_fresh_{}.db", uuid::Uuid::new_v4()));
        // WHEN
        let result = AuditTrailHandle::open(&db_path).await;
        // THEN
        assert!(result.is_ok());
        let handle = result.unwrap();
        handle.shutdown().await;
        tokio::fs::remove_file(&db_path).await.ok();
    }

    // AC-4 — record() ne bloque pas (fire-and-forget)
    #[tokio::test]
    async fn test_ac4_record_is_fire_and_forget() {
        // GIVEN
        let handle = open_test_audit().await;
        // WHEN — on enregistre 10 invocations sans attendre
        for i in 0..10 {
            let mut r = make_record(true, None);
            r.id = format!("id-{i}");
            r.started_at = format!("2026-01-01T00:00:{i:02}Z");
            handle.record(r);
        }
        // THEN — la méthode a retourné immédiatement ; les inserts arrivent en fond
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let results = handle.query_last(10).await;
        assert_eq!(results.len(), 10);
        handle.shutdown().await;
    }

    // AC-5 — Mêmes paramètres → même input_hash
    #[test]
    fn test_ac5_same_params_same_input_hash() {
        // GIVEN
        let params = serde_json::json!({ "command": "echo hello", "timeout_secs": 30 });
        // WHEN
        let hash1 = compute_input_hash(&params);
        let hash2 = compute_input_hash(&params);
        // THEN
        assert_eq!(hash1, hash2);
    }

    // Hash différents pour des paramètres différents
    #[test]
    fn test_different_params_different_hash() {
        // GIVEN
        let params_a = serde_json::json!({ "command": "echo hello" });
        let params_b = serde_json::json!({ "command": "echo world" });
        // WHEN / THEN
        assert_ne!(compute_input_hash(&params_a), compute_input_hash(&params_b));
    }

    // Hash est une string hexadécimale valide (64 caractères = SHA256)
    #[test]
    fn test_input_hash_is_valid_hex() {
        // GIVEN
        let params = serde_json::json!({ "x": 1 });
        // WHEN
        let hash = compute_input_hash(&params);
        // THEN
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
