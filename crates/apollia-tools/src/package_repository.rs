//! Repository SQLite pour la persistance des packages d'agents installés.
//!
//! Un **package** est un dossier auto-contenu décrit par un `agent.toml`
//! (ADR-081). Il regroupe un director et ses workers, des triggers, et des
//! dépendances pip partagées.
//!
//! Ce repository gère `installed_packages` et `package_agents`. Les agents
//! individuels restent dans `installed_agents` (voir [`AgentRepository`]).
//!
//! La migration `008_package_tables.sql` est appliquée idempotentiellement
//! à l'appel de [`PackageRepository::open`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// SQL de migration embarqué — appliqué idempotentiellement à chaque ouverture.
const MIGRATION_SQL: &str = include_str!("../migrations/008_package_tables.sql");

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Métadonnées d'un package installé, persisted dans `agents.db`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Nom unique du package (clé primaire).
    pub name: String,
    /// Version semver du package.
    pub version: String,
    /// Chemin absolu du dossier package (`~/.apollia/agents/packages/<name>/`).
    pub root_path: PathBuf,
    /// Contenu du `agent.toml` sérialisé en JSON.
    pub manifest_json: String,
    /// Horodatage d'installation (RFC 3339).
    pub installed_at: String,
    /// Horodatage de dernière mise à jour (RFC 3339).
    pub updated_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs du repository packages.
#[derive(Debug, thiserror::Error)]
pub enum PackageRepositoryError {
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("package '{0}' introuvable")]
    NotFound(String),

    #[error("erreur sérialisation JSON : {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("erreur tâche async : {0}")]
    SpawnError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// Repository SQLite pour les packages d'agents installés.
///
/// Clonable via `Arc` — chaque clone partage la même connexion.
/// Le mode WAL est activé pour la concurrence lecture/écriture.
#[derive(Clone)]
pub struct PackageRepository {
    conn: Arc<Mutex<Connection>>,
}

impl PackageRepository {
    /// Ouvre (ou crée) la base SQLite et applique la migration 008.
    ///
    /// Compatible avec un `agents.db` existant (migration 007 déjà appliquée) :
    /// la migration 008 ajoute uniquement de nouvelles tables (`IF NOT EXISTS`).
    pub fn open(path: &Path) -> Result<Self, PackageRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insère ou met à jour un package installé (UPSERT idempotent).
    pub fn save(&self, pkg: &InstalledPackage) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO installed_packages \
                 (name, version, root_path, manifest_json, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &pkg.name,
                &pkg.version,
                pkg.root_path.to_string_lossy().as_ref(),
                &pkg.manifest_json,
                &pkg.installed_at,
                &pkg.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Enregistre le lien entre un package et un de ses agents.
    ///
    /// Idempotent : utilise `INSERT OR IGNORE`.
    pub fn link_agent(
        &self,
        package_name: &str,
        agent_name: &str,
    ) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO package_agents (package_name, agent_name) VALUES (?1, ?2)",
            params![package_name, agent_name],
        )?;
        Ok(())
    }

    /// Récupère un package par son nom.
    pub fn get(&self, name: &str) -> Result<Option<InstalledPackage>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, root_path, manifest_json, installed_at, updated_at \
             FROM installed_packages WHERE name = ?1",
        )?;
        let mut rows = stmt.query(params![name])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_package(row)?)),
            None => Ok(None),
        }
    }

    /// Liste tous les packages installés.
    pub fn list(&self) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, root_path, manifest_json, installed_at, updated_at \
             FROM installed_packages ORDER BY name",
        )?;
        collect_packages(&mut stmt, [])
    }

    /// Retourne les noms des agents appartenant à un package.
    pub fn list_agents_for_package(
        &self,
        package_name: &str,
    ) -> Result<Vec<String>, PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT agent_name FROM package_agents WHERE package_name = ?1 ORDER BY agent_name",
        )?;
        let rows = stmt.query_map(params![package_name], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for r in rows {
            names.push(r?);
        }
        Ok(names)
    }

    /// Supprime un package et ses liens agents (CASCADE sur `package_agents`).
    ///
    /// Les entrées dans `installed_agents` doivent être supprimées séparément
    /// via [`AgentRepository::delete`].
    pub fn delete(&self, name: &str) -> Result<(), PackageRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM installed_packages WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

    /// Wrapper async pour [`save`].
    pub async fn save_async(&self, pkg: InstalledPackage) -> Result<(), PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.save(&pkg))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`get`].
    pub async fn get_async(
        &self,
        name: String,
    ) -> Result<Option<InstalledPackage>, PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.get(&name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`list`].
    pub async fn list_async(&self) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list())
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`list_agents_for_package`].
    pub async fn list_agents_for_package_async(
        &self,
        package_name: String,
    ) -> Result<Vec<String>, PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list_agents_for_package(&package_name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`delete`].
    pub async fn delete_async(&self, name: String) -> Result<(), PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.delete(&name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`link_agent`].
    pub async fn link_agent_async(
        &self,
        package_name: String,
        agent_name: String,
    ) -> Result<(), PackageRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.link_agent(&package_name, &agent_name))
            .await
            .map_err(|e| PackageRepositoryError::SpawnError(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers internes
// ─────────────────────────────────────────────────────────────────────────────

fn row_to_package(row: &rusqlite::Row<'_>) -> Result<InstalledPackage, PackageRepositoryError> {
    let root_path_str: String = row.get(2)?;
    Ok(InstalledPackage {
        name: row.get(0)?,
        version: row.get(1)?,
        root_path: PathBuf::from(root_path_str),
        manifest_json: row.get(3)?,
        installed_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn collect_packages<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<InstalledPackage>, PackageRepositoryError> {
    let rows = stmt.query_map(params, |row| {
        let root_path_str: String = row.get(2)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            root_path_str,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut pkgs = Vec::new();
    for row_result in rows {
        let (name, version, root_path, manifest_json, installed_at, updated_at) = row_result?;
        pkgs.push(InstalledPackage {
            name,
            version,
            root_path: PathBuf::from(root_path),
            manifest_json,
            installed_at,
            updated_at,
        });
    }
    Ok(pkgs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_package(name: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            root_path: PathBuf::from(format!("/home/user/.apollia/agents/packages/{name}")),
            manifest_json: r#"{"package":{"name":"test"}}"#.to_string(),
            installed_at: "2026-04-24T07:00:00Z".to_string(),
            updated_at: "2026-04-24T07:00:00Z".to_string(),
        }
    }

    fn open_test_repo() -> PackageRepository {
        PackageRepository::open(Path::new(":memory:")).expect("open in-memory repo")
    }

    #[test]
    fn test_open_creates_tables() {
        // GIVEN une base vide
        let repo = open_test_repo();
        // WHEN on ouvre le repo
        // THEN les tables existent (requête sans erreur)
        let conn = repo.conn.lock().expect("lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM installed_packages", [], |r| r.get(0))
            .expect("table installed_packages should exist");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_save_and_get_roundtrip() {
        // GIVEN un package
        let repo = open_test_repo();
        let pkg = test_package("veille-ia");
        // WHEN on sauvegarde et récupère
        repo.save(&pkg).expect("save");
        let loaded = repo.get("veille-ia").expect("get").expect("exists");
        // THEN les champs sont identiques
        assert_eq!(loaded.name, "veille-ia");
        assert_eq!(loaded.version, "1.0.0");
        assert_eq!(loaded.root_path, pkg.root_path);
    }

    #[test]
    fn test_link_agent_and_list() {
        // GIVEN un package avec 2 agents liés
        let repo = open_test_repo();
        repo.save(&test_package("my-pkg")).expect("save pkg");
        // Note : pas de FK vers installed_agents en :memory: (pas de migration 007)
        // → on désactive PRAGMA foreign_keys pour ce test
        {
            let conn = repo.conn.lock().expect("lock");
            conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        }
        repo.link_agent("my-pkg", "director-agent")
            .expect("link director");
        repo.link_agent("my-pkg", "worker-agent")
            .expect("link worker");
        // WHEN on liste les agents du package
        let agents = repo.list_agents_for_package("my-pkg").expect("list agents");
        // THEN on obtient les 2 agents
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"director-agent".to_string()));
        assert!(agents.contains(&"worker-agent".to_string()));
    }

    #[test]
    fn test_save_upsert_existing() {
        // GIVEN un package déjà sauvegardé
        let repo = open_test_repo();
        repo.save(&test_package("pkg-v1")).expect("save v1");
        // WHEN on sauvegarde avec une nouvelle version (upsert)
        let mut pkg_v2 = test_package("pkg-v1");
        pkg_v2.version = "2.0.0".to_string();
        repo.save(&pkg_v2).expect("save v2");
        // THEN la version est mise à jour, un seul enregistrement
        let loaded = repo.get("pkg-v1").expect("get").expect("exists");
        assert_eq!(loaded.version, "2.0.0");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn test_delete_removes_package() {
        // GIVEN un package sauvegardé
        let repo = open_test_repo();
        repo.save(&test_package("to-delete")).expect("save");
        // WHEN on supprime
        repo.delete("to-delete").expect("delete");
        // THEN le package n'existe plus
        assert!(repo.get("to-delete").expect("get").is_none());
    }

    #[tokio::test]
    async fn test_list_async_returns_saved_packages() {
        // GIVEN 2 packages sauvegardés
        let repo = open_test_repo();
        repo.save(&test_package("pkg-a")).expect("save a");
        repo.save(&test_package("pkg-b")).expect("save b");
        // WHEN on liste en async
        let pkgs = repo.list_async().await.expect("list_async");
        // THEN on obtient les 2 packages
        assert_eq!(pkgs.len(), 2);
    }
}
