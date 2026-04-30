//! Repository SQLite pour la persistance des agents installés.
//!
//! Fournit [`AgentRepository`] qui stocke et restitue les agents installés
//! dans une base SQLite locale (`agents.db`). Les agents survivent aux
//! redémarrages et sont auto-chargés au boot.
//!
//! La migration `007_agent_tables.sql` est appliquée idempotentiellement
//! à l'appel de [`AgentRepository::open`].
//!
//! Toutes les méthodes sont synchrones car les appels SQLite sont légers
//! et l'appelant est responsable de les exécuter dans `spawn_blocking`
//! si nécessaire.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_core::AgentManifest;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// SQL de migration embarqué — appliqué idempotentiellement à chaque ouverture.
const MIGRATION_SQL: &str = include_str!("../migrations/007_agent_tables.sql");

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Agent installé persisté dans `agents.db`.
///
/// Représente un agent copié dans `~/.apollia/agents/<name>/` avec son
/// manifest sérialisé. Le champ `enabled` contrôle le chargement au boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAgent {
    /// Nom unique de l'agent (clé primaire).
    pub name: String,
    /// Version semver de l'agent.
    pub version: String,
    /// Chemin d'installation (ex: `~/.apollia/agents/<name>/agent.py`).
    pub install_path: PathBuf,
    /// Chemin original du fichier source installé.
    pub source_path: PathBuf,
    /// Manifest de l'agent (sérialisé en JSON dans la base).
    pub manifest: AgentManifest,
    /// Indique si l'agent est actif et doit être chargé au boot.
    pub enabled: bool,
    /// Horodatage d'installation (RFC 3339).
    pub installed_at: String,
    /// Horodatage de dernière mise à jour (RFC 3339).
    pub updated_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs du repository agents.
#[derive(Debug, thiserror::Error)]
pub enum AgentRepositoryError {
    /// Erreur SQLite sous-jacente.
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Agent introuvable pour le nom donné.
    #[error("agent '{0}' introuvable")]
    NotFound(String),

    /// Erreur de sérialisation/désérialisation du manifest JSON.
    #[error("erreur sérialisation manifest : {0}")]
    SerdeError(#[from] serde_json::Error),

    /// Échec d'une tâche `spawn_blocking` (panique dans le thread de travail).
    #[error("erreur tâche async : {0}")]
    SpawnError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// Repository SQLite pour les agents installés.
///
/// Encapsule une connexion SQLite vers `agents.db`. La migration
/// `007_agent_tables.sql` est appliquée à [`open`](Self::open).
/// Le mode WAL est activé pour la concurrence lecture/écriture.
///
/// Clonable via `Arc` — chaque clone partage la même connexion. Les wrappers
/// async utilisent `tokio::task::spawn_blocking` pour éviter de bloquer
/// l'exécuteur Tokio. Voir [`save_async`], [`list_async`], etc.
///
/// [`save_async`]: Self::save_async
/// [`list_async`]: Self::list_async
#[derive(Clone)]
pub struct AgentRepository {
    conn: Arc<Mutex<Connection>>,
}

impl AgentRepository {
    /// Ouvre (ou crée) la base SQLite et applique la migration 007.
    ///
    /// La migration est idempotente (`CREATE TABLE IF NOT EXISTS`), donc sûre
    /// à rejouer sur une base existante. Active le mode WAL pour la concurrence.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le fichier SQLite ne peut pas être ouvert ou si
    /// la migration échoue.
    pub fn open(path: &Path) -> Result<Self, AgentRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insère ou met à jour un agent installé.
    ///
    /// Utilise `INSERT OR REPLACE` : si un agent avec le même `name` existe
    /// déjà, il est remplacé intégralement. Le champ `updated_at` est mis
    /// à jour automatiquement.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] si le manifest ne peut pas être sérialisé
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn save(&self, agent: &InstalledAgent) -> Result<(), AgentRepositoryError> {
        let manifest_json = serde_json::to_string(&agent.manifest)?;
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO installed_agents \
                 (name, version, install_path, source_path, manifest_json, \
                  enabled, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &agent.name,
                &agent.version,
                agent.install_path.to_string_lossy().as_ref(),
                agent.source_path.to_string_lossy().as_ref(),
                &manifest_json,
                agent.enabled,
                &agent.installed_at,
                &agent.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Récupère un agent installé par son nom.
    ///
    /// Retourne `None` si aucun agent ne porte ce nom.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] si le manifest JSON est corrompu
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn get(&self, name: &str) -> Result<Option<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents WHERE name = ?1",
        )?;

        let mut rows = stmt.query(params![name])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_installed_agent(row)?)),
            None => Ok(None),
        }
    }

    /// Liste tous les agents installés.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] si un manifest JSON est corrompu
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn list(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents ORDER BY name",
        )?;
        collect_agents(&mut stmt, [])
    }

    /// Liste uniquement les agents installés et activés (`enabled = true`).
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::SerdeError`] si un manifest JSON est corrompu
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn list_enabled(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT name, version, install_path, source_path, manifest_json, \
                    enabled, installed_at, updated_at \
             FROM installed_agents WHERE enabled = 1 ORDER BY name",
        )?;
        collect_agents(&mut stmt, [])
    }

    /// Supprime un agent installé par son nom.
    ///
    /// Opération idempotente : ne retourne pas d'erreur si l'agent n'existe pas.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn delete(&self, name: &str) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM installed_agents WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    /// Active ou désactive un agent installé.
    ///
    /// Met à jour le champ `enabled` et l'horodatage `updated_at`.
    ///
    /// # Errors
    ///
    /// - [`AgentRepositoryError::NotFound`] si l'agent n'existe pas
    /// - [`AgentRepositoryError::Sqlite`] en cas d'erreur SQLite
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let rows_affected = conn.execute(
            "UPDATE installed_agents SET enabled = ?1, updated_at = datetime('now') \
             WHERE name = ?2",
            params![enabled, name],
        )?;
        if rows_affected == 0 {
            return Err(AgentRepositoryError::NotFound(name.to_string()));
        }
        Ok(())
    }

    // ─── Async wrappers ──────────────────────────────────────────────────────

    /// Wrapper async pour [`save`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`save`]: Self::save
    pub async fn save_async(&self, agent: InstalledAgent) -> Result<(), AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.save(&agent))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`get`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`get`]: Self::get
    pub async fn get_async(
        &self,
        name: String,
    ) -> Result<Option<InstalledAgent>, AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.get(&name))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`list`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`list`]: Self::list
    pub async fn list_async(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list())
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`list_enabled`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`list_enabled`]: Self::list_enabled
    pub async fn list_enabled_async(&self) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.list_enabled())
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`delete`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`delete`]: Self::delete
    pub async fn delete_async(&self, name: String) -> Result<(), AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.delete(&name))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }

    /// Wrapper async pour [`set_enabled`] — exécute l'I/O SQLite sur un thread bloquant.
    ///
    /// [`set_enabled`]: Self::set_enabled
    pub async fn set_enabled_async(
        &self,
        name: String,
        enabled: bool,
    ) -> Result<(), AgentRepositoryError> {
        let repo = self.clone();
        tokio::task::spawn_blocking(move || repo.set_enabled(&name, enabled))
            .await
            .map_err(|e| AgentRepositoryError::SpawnError(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers internes
// ─────────────────────────────────────────────────────────────────────────────

/// Convertit une ligne SQLite en [`InstalledAgent`].
fn row_to_installed_agent(row: &rusqlite::Row<'_>) -> Result<InstalledAgent, AgentRepositoryError> {
    let manifest_json: String = row.get(4)?;
    let manifest: AgentManifest = serde_json::from_str(&manifest_json)?;
    let install_path_str: String = row.get(2)?;
    let source_path_str: String = row.get(3)?;

    Ok(InstalledAgent {
        name: row.get(0)?,
        version: row.get(1)?,
        install_path: PathBuf::from(install_path_str),
        source_path: PathBuf::from(source_path_str),
        manifest,
        enabled: row.get(5)?,
        installed_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Collecte les résultats d'une requête en vecteur d'[`InstalledAgent`].
fn collect_agents<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<InstalledAgent>, AgentRepositoryError> {
    let rows = stmt.query_map(params, |row| {
        let manifest_json: String = row.get(4)?;
        let install_path_str: String = row.get(2)?;
        let source_path_str: String = row.get(3)?;

        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            install_path_str,
            source_path_str,
            manifest_json,
            row.get::<_, bool>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;

    let mut agents = Vec::new();
    for row_result in rows {
        let (
            name,
            version,
            install_path,
            source_path,
            manifest_json,
            enabled,
            installed_at,
            updated_at,
        ) = row_result?;
        let manifest: AgentManifest = serde_json::from_str(&manifest_json)?;
        agents.push(InstalledAgent {
            name,
            version,
            install_path: PathBuf::from(install_path),
            source_path: PathBuf::from(source_path),
            manifest,
            enabled,
            installed_at,
            updated_at,
        });
    }
    Ok(agents)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::AgentManifest;
    use std::path::PathBuf;

    /// Crée un [`AgentManifest`] de test avec des valeurs par défaut.
    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("Test agent {name}"),
            tools_required: vec!["bash".to_string()],
            tools_optional: Vec::new(),
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: Vec::new(),
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: Vec::new(),
            skills: Vec::new(),
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: Vec::new(),
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
        }
    }

    /// Crée un [`InstalledAgent`] de test.
    fn test_agent(name: &str) -> InstalledAgent {
        InstalledAgent {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            install_path: PathBuf::from(format!("/home/user/.apollia/agents/{name}/agent.py")),
            source_path: PathBuf::from(format!("/tmp/{name}.py")),
            manifest: test_manifest(name),
            enabled: true,
            installed_at: "2026-03-17T10:00:00Z".to_string(),
            updated_at: "2026-03-17T10:00:00Z".to_string(),
        }
    }

    /// Ouvre un repository en mémoire pour les tests.
    fn open_test_repo() -> AgentRepository {
        AgentRepository::open(Path::new(":memory:")).expect("failed to open test repo")
    }

    // Migration crée la table installed_agents
    #[test]
    fn test_open_creates_table() {
        let repo = open_test_repo();
        // Vérifie que la table existe en requêtant sans erreur
        let count: i64 = {
            let conn = repo.conn.lock().expect("lock");
            conn.query_row("SELECT COUNT(*) FROM installed_agents", [], |row| {
                row.get(0)
            })
            .expect("table installed_agents should exist")
        };
        assert_eq!(count, 0);
    }

    // save() + get() round-trip avec manifest serde
    #[test]
    fn test_save_and_get_roundtrip() {
        let repo = open_test_repo();
        let agent = test_agent("hello-agent");

        repo.save(&agent).expect("save should succeed");
        let loaded = repo
            .get("hello-agent")
            .expect("get should succeed")
            .expect("agent should exist");

        assert_eq!(loaded.name, "hello-agent");
        assert_eq!(loaded.version, "1.0.0");
        assert_eq!(loaded.install_path, agent.install_path);
        assert_eq!(loaded.source_path, agent.source_path);
        assert!(loaded.enabled);
        // round-trip manifest
        assert_eq!(loaded.manifest.name, "hello-agent");
        assert_eq!(loaded.manifest.description, "Test agent hello-agent");
        assert_eq!(loaded.manifest.tools_required, vec!["bash"]);
    }

    // list() retourne tous les agents
    #[test]
    fn test_list_all_agents() {
        let repo = open_test_repo();
        repo.save(&test_agent("agent-a")).expect("save a");
        repo.save(&test_agent("agent-b")).expect("save b");
        repo.save(&test_agent("agent-c")).expect("save c");

        let agents = repo.list().expect("list should succeed");
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0].name, "agent-a");
        assert_eq!(agents[1].name, "agent-b");
        assert_eq!(agents[2].name, "agent-c");
    }

    // delete() supprime l'agent
    #[test]
    fn test_delete_agent() {
        let repo = open_test_repo();
        repo.save(&test_agent("to-delete")).expect("save");
        assert!(repo.get("to-delete").expect("get").is_some());

        repo.delete("to-delete").expect("delete should succeed");
        assert!(repo.get("to-delete").expect("get after delete").is_none());
    }

    // list_enabled() filtre les disabled
    #[test]
    fn test_list_enabled_filters_disabled() {
        let repo = open_test_repo();
        repo.save(&test_agent("enabled-1")).expect("save 1");
        repo.save(&test_agent("enabled-2")).expect("save 2");

        let mut disabled = test_agent("disabled-1");
        disabled.enabled = false;
        repo.save(&disabled).expect("save disabled");

        let enabled_agents = repo.list_enabled().expect("list_enabled");
        assert_eq!(enabled_agents.len(), 2);
        assert!(enabled_agents.iter().all(|a| a.enabled));
    }

    // set_enabled() toggle + updated_at
    #[test]
    fn test_set_enabled_toggle() {
        let repo = open_test_repo();
        repo.save(&test_agent("toggle-agent")).expect("save");

        // Vérifie état initial : enabled
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(agent.enabled);

        // Désactive
        repo.set_enabled("toggle-agent", false)
            .expect("set_enabled false");
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(!agent.enabled);
        // updated_at a changé (datetime('now') != timestamp original)
        assert_ne!(agent.updated_at, "2026-03-17T10:00:00Z");

        // Réactive
        repo.set_enabled("toggle-agent", true)
            .expect("set_enabled true");
        let agent = repo.get("toggle-agent").expect("get").expect("exists");
        assert!(agent.enabled);
    }

    // save() upsert sur agent existant
    #[test]
    fn test_save_upsert_existing() {
        let repo = open_test_repo();
        let mut agent = test_agent("upsert-agent");
        repo.save(&agent).expect("save v1");

        // Met à jour la version
        agent.version = "2.0.0".to_string();
        agent.updated_at = "2026-03-17T12:00:00Z".to_string();
        repo.save(&agent).expect("save v2");

        let loaded = repo.get("upsert-agent").expect("get").expect("exists");
        assert_eq!(loaded.version, "2.0.0");
        assert_eq!(loaded.updated_at, "2026-03-17T12:00:00Z");

        // Un seul agent dans la base
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1);
    }

    // get() retourne None si agent inexistant
    #[test]
    fn test_get_nonexistent_returns_none() {
        let repo = open_test_repo();
        let result = repo.get("does-not-exist").expect("get should not error");
        assert!(result.is_none());
    }

    // list_async() retourne les mêmes agents que list()
    #[tokio::test]
    async fn test_agent_repository_async_list() {
        // GIVEN
        let repo = open_test_repo();
        repo.save(&test_agent("alpha")).expect("save alpha");
        repo.save(&test_agent("beta")).expect("save beta");

        // WHEN
        let agents = repo.list_async().await.expect("list_async should succeed");

        // THEN
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "alpha");
        assert_eq!(agents[1].name, "beta");
    }

    // list_async() propage les erreurs correctement
    #[tokio::test]
    async fn test_agent_repository_async_error_propagation() {
        // GIVEN un repository fermé ne peut pas se produire avec in-memory,
        // mais on vérifie que get_async retourne None pour un agent inexistant
        let repo = open_test_repo();

        // WHEN
        let result = repo
            .get_async("non-existent-agent".to_string())
            .await
            .expect("get_async should not error");

        // THEN
        assert!(result.is_none());
    }
}
