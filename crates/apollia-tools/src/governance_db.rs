//! Initialisation et migration de la base SQLite consolidée `governance.db`.
//!
//! Cette base unique remplace l'ancienne `permissions.db` et regroupe sous
//! `~/.apollia/governance.db` toutes les tables de gouvernance du runtime :
//!
//! - `permission_rules` — règles préfixe scope-aware (session/project/global) ;
//! - `permission_audit` — log immuable des décisions, append-only via triggers ;
//! - `tools` — état enabled/disabled et configuration JSON par outil ;
//! - `tool_credentials` — secrets par outil, valeurs chiffrées AES-256-GCM.
//!
//! ## Migration depuis `permissions.db`
//!
//! Au premier démarrage avec une `permissions.db` existante, le fichier est
//! copié vers `governance.db` puis renommé `permissions.db.bak`. La sauvegarde
//! est conservée mais plus utilisée par le runtime. Les migrations de schéma
//! (ALTER TABLE) sont idempotentes : un redémarrage ultérieur ne produit
//! aucune erreur ni doublon.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// Erreur retournée par [`GovernanceDb`].
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    /// Erreur d'I/O lors de la création du dossier ou de la migration de fichier.
    #[error("governance.db I/O error at {path}: {source}")]
    Io {
        /// Chemin concerné par l'erreur d'I/O.
        path: PathBuf,
        /// Cause sous-jacente.
        #[source]
        source: std::io::Error,
    },
    /// Erreur SQLite lors de l'ouverture ou de la migration du schéma.
    #[error("governance.db SQLite error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Nom de fichier de la base consolidée.
pub const GOVERNANCE_DB_FILENAME: &str = "governance.db";

/// Nom de fichier de l'ancienne base de permissions (legacy).
pub const LEGACY_PERMISSIONS_FILENAME: &str = "permissions.db";

/// Nom de fichier du backup créé après migration depuis `permissions.db`.
pub const LEGACY_BACKUP_FILENAME: &str = "permissions.db.bak";

/// Base SQLite consolidée pour la gouvernance d'outils et de permissions.
///
/// `GovernanceDb` est responsable de :
/// - migrer une éventuelle ancienne `permissions.db` vers `governance.db` ;
/// - garantir l'existence de toutes les tables et triggers du schéma cible ;
/// - exposer la connexion et le chemin pour les composants en aval (registry,
///   credential store, prefix-rule engine, audit log).
pub struct GovernanceDb {
    path: PathBuf,
    conn: Connection,
}

impl GovernanceDb {
    /// Ouvre (ou crée) `<base_dir>/governance.db` et exécute la migration de schéma.
    ///
    /// Si `governance.db` n'existe pas mais qu'un ancien `permissions.db` est
    /// présent dans `base_dir`, ce dernier est copié vers `governance.db` puis
    /// renommé en `permissions.db.bak`. La sauvegarde est conservée.
    ///
    /// La migration de schéma est idempotente : appeler `open` plusieurs fois
    /// sur une base déjà migrée ne produit aucun changement.
    ///
    /// # Errors
    ///
    /// - [`GovernanceError::Io`] si la création du dossier ou la copie/renommage
    ///   échoue.
    /// - [`GovernanceError::Database`] si SQLite échoue à ouvrir le fichier ou
    ///   à appliquer la migration.
    pub fn open(base_dir: &Path) -> Result<Self, GovernanceError> {
        if !base_dir.exists() {
            std::fs::create_dir_all(base_dir).map_err(|e| GovernanceError::Io {
                path: base_dir.to_path_buf(),
                source: e,
            })?;
        }

        let path = base_dir.join(GOVERNANCE_DB_FILENAME);
        let legacy = base_dir.join(LEGACY_PERMISSIONS_FILENAME);
        let backup = base_dir.join(LEGACY_BACKUP_FILENAME);

        if !path.exists() && legacy.exists() {
            std::fs::copy(&legacy, &path).map_err(|e| GovernanceError::Io {
                path: path.clone(),
                source: e,
            })?;
            std::fs::rename(&legacy, &backup).map_err(|e| GovernanceError::Io {
                path: backup.clone(),
                source: e,
            })?;
            tracing::info!(
                from = %legacy.display(),
                to = %path.display(),
                backup = %backup.display(),
                "migrated permissions.db to governance.db"
            );
        }

        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        migrate_schema(&conn)?;

        Ok(Self { path, conn })
    }

    /// Chemin absolu du fichier `governance.db` ouvert.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Connexion SQLite sous-jacente, en lecture/écriture.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

fn migrate_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS permission_rules (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name    TEXT NOT NULL,
            arg_prefix   TEXT,
            action       TEXT NOT NULL,
            created_at   INTEGER NOT NULL,
            created_by   TEXT,
            scope        TEXT NOT NULL DEFAULT 'global',
            project_path TEXT,
            expires_at   INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);

        CREATE TABLE IF NOT EXISTS permission_audit (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name   TEXT NOT NULL,
            first_arg   TEXT,
            decision    TEXT NOT NULL,
            decided_at  INTEGER NOT NULL,
            scope       TEXT,
            rule_id     INTEGER,
            agent       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_tool ON permission_audit(tool_name, decided_at);

        CREATE TABLE IF NOT EXISTS tools (
            name        TEXT PRIMARY KEY,
            enabled     BOOLEAN NOT NULL DEFAULT TRUE,
            config_json TEXT,
            updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS tool_credentials (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name       TEXT NOT NULL,
            key_name        TEXT NOT NULL,
            value_encrypted BLOB NOT NULL,
            created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used_at    INTEGER,
            UNIQUE(tool_name, key_name)
        );",
    )?;

    add_column_if_missing(
        conn,
        "permission_rules",
        "scope",
        "TEXT NOT NULL DEFAULT 'global'",
    )?;
    add_column_if_missing(conn, "permission_rules", "project_path", "TEXT")?;
    add_column_if_missing(conn, "permission_rules", "expires_at", "INTEGER")?;

    add_column_if_missing(conn, "permission_audit", "scope", "TEXT")?;
    add_column_if_missing(conn, "permission_audit", "rule_id", "INTEGER")?;
    add_column_if_missing(conn, "permission_audit", "agent", "TEXT")?;

    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS no_update_audit
         BEFORE UPDATE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS no_delete_audit
         BEFORE DELETE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;",
    )?;

    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let sql = format!("PRAGMA table_info(\"{table}\")");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    type_def: &str,
) -> Result<(), rusqlite::Error> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {type_def}");
    conn.execute_batch(&sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    fn count_tables(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            params![name],
            |row| row.get(0),
        )
        .expect("query sqlite_master")
    }

    #[test]
    fn test_fresh_create_all_tables() {
        // GIVEN un dossier vide sans aucune base existante.
        let dir = TempDir::new().expect("tempdir");
        // WHEN on ouvre une GovernanceDb pour la première fois.
        let db = GovernanceDb::open(dir.path()).expect("open governance.db");
        // THEN governance.db est créée avec les quatre tables cibles.
        let conn = db.connection();
        assert_eq!(count_tables(conn, "permission_rules"), 1);
        assert_eq!(count_tables(conn, "permission_audit"), 1);
        assert_eq!(count_tables(conn, "tools"), 1);
        assert_eq!(count_tables(conn, "tool_credentials"), 1);
        assert!(db.path().ends_with(GOVERNANCE_DB_FILENAME));
        assert!(db.path().exists());
    }

    #[test]
    fn test_migration_from_permissions_db() {
        // GIVEN une ancienne permissions.db remplie avec une règle existante.
        let dir = TempDir::new().expect("tempdir");
        let legacy = dir.path().join(LEGACY_PERMISSIONS_FILENAME);
        {
            let conn = Connection::open(&legacy).expect("create legacy");
            conn.execute_batch(
                "CREATE TABLE permission_rules (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    tool_name   TEXT NOT NULL,
                    arg_prefix  TEXT,
                    action      TEXT NOT NULL,
                    created_at  INTEGER NOT NULL,
                    created_by  TEXT
                );",
            )
            .expect("legacy schema");
            conn.execute(
                "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, created_by) \
                 VALUES ('bash_executor', 'git', 'allow', 1700000000, 'operator')",
                [],
            )
            .expect("seed legacy rule");
        }

        // WHEN GovernanceDb::open migre cette base.
        let db = GovernanceDb::open(dir.path()).expect("migrate");

        // THEN governance.db existe, permissions.db a été renommée en .bak,
        //      la règle existante est présente avec scope='global'.
        let governance = dir.path().join(GOVERNANCE_DB_FILENAME);
        let backup = dir.path().join(LEGACY_BACKUP_FILENAME);
        assert!(governance.exists(), "governance.db must exist");
        assert!(backup.exists(), "permissions.db.bak must exist");
        assert!(!legacy.exists(), "permissions.db must have been renamed");

        let (tool, scope, project_path, expires_at): (String, String, Option<String>, Option<i64>) =
            db.connection()
                .query_row(
                    "SELECT tool_name, scope, project_path, expires_at FROM permission_rules",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("query migrated rule");
        assert_eq!(tool, "bash_executor");
        assert_eq!(scope, "global");
        assert!(project_path.is_none());
        assert!(expires_at.is_none());
    }

    #[test]
    fn test_idempotent_migration() {
        // GIVEN un dossier qui a déjà été migré une fois.
        let dir = TempDir::new().expect("tempdir");
        {
            let _first = GovernanceDb::open(dir.path()).expect("first open");
        }
        // WHEN on rouvre la GovernanceDb.
        let second = GovernanceDb::open(dir.path()).expect("second open");
        // THEN aucune erreur, schéma identique, pas de tables dupliquées.
        let conn = second.connection();
        for table in [
            "permission_rules",
            "permission_audit",
            "tools",
            "tool_credentials",
        ] {
            assert_eq!(count_tables(conn, table), 1, "table {table} must be unique");
        }
    }

    #[test]
    fn test_audit_trigger_blocks_update() {
        // GIVEN une GovernanceDb fraîche avec une entrée d'audit insérée.
        let dir = TempDir::new().expect("tempdir");
        let db = GovernanceDb::open(dir.path()).expect("open");
        db.connection()
            .execute(
                "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
                 VALUES ('bash_executor', 'git status', 'AutoAllowedSafeList', 1700000000)",
                [],
            )
            .expect("insert audit row");

        // WHEN on tente de modifier la décision...
        let update_result = db
            .connection()
            .execute("UPDATE permission_audit SET decision = 'NeedsApproval'", []);
        // ...ou de la supprimer.
        let delete_result = db.connection().execute("DELETE FROM permission_audit", []);

        // THEN les deux opérations sont bloquées par les triggers d'append-only.
        assert!(update_result.is_err(), "UPDATE must be blocked");
        assert!(delete_result.is_err(), "DELETE must be blocked");
    }
}
