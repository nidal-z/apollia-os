//! `apollia-os memory` subcommands.
//!
//! Provides memory inspection and diagnostics directly from the CLI.
//! Reads the SQLite `.db` file without requiring the runtime.

use std::io::IsTerminal as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::{Subcommand, ValueEnum};

/// Type de mémoire à cibler pour les opérations de vidage.
#[derive(Debug, Clone, ValueEnum)]
pub enum MemoryType {
    /// Mémoires épisodiques.
    Episodic,
    /// Mémoires sémantiques.
    Semantic,
    /// Mémoires procédurales.
    Procedural,
    /// Tous les types de mémoire.
    All,
}

/// Commandes de gestion de la memoire.
#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// Inspecter l'etat d'un namespace memoire.
    Inspect {
        /// Nom du namespace a inspecter.
        namespace: String,

        /// Repertoire des fichiers memoire (defaut: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,

        /// Sortie JSON.
        #[arg(long)]
        json: bool,
    },

    /// Lister tous les namespaces memoire presents sur le disque.
    List {
        /// Filtrer par nom d'agent/namespace.
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Repertoire des fichiers memoire (defaut: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Vider la memoire d'un agent.
    Clear {
        /// Nom du namespace/agent a vider.
        #[arg(long, value_name = "NAME")]
        agent: String,

        /// Type de memoire a vider.
        #[arg(long, value_enum, default_value = "all")]
        r#type: MemoryType,

        /// Confirmer sans invite interactive.
        #[arg(long)]
        confirm: bool,

        /// Repertoire des fichiers memoire (defaut: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Purger les entrees memoire plus anciennes qu'un seuil en jours.
    ///
    /// Exemple : `apollia memory purge --namespace mon-agent --older-than 30`
    /// Exemple avec filtre : `apollia memory purge --namespace mon-agent --type episodic --older-than 7`
    Purge {
        /// Namespace cible.
        #[arg(long, value_name = "NAME")]
        namespace: String,

        /// Supprimer les entrees creees il y a plus de N jours.
        #[arg(long, value_name = "DAYS")]
        older_than: u32,

        /// Limiter la purge a un seul type (defaut: tous les types).
        #[arg(long, value_enum)]
        r#type: Option<MemoryType>,

        /// Repertoire des fichiers memoire (defaut: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },
}

/// Erreurs de la commande memory.
#[derive(Debug, thiserror::Error)]
pub enum MemoryCommandError {
    /// Le namespace demande n'existe pas.
    #[error("namespace '{namespace}' not found ({path} does not exist)")]
    NamespaceNotFound {
        /// Nom du namespace.
        namespace: String,
        /// Chemin attendu du fichier .db.
        path: String,
    },

    /// Erreur du MemoryStore.
    #[error("memory store error: {0}")]
    Store(#[from] apollia_memory::store::MemoryStoreError),

    /// Erreur du MemoryManager.
    #[error("memory manager error: {0}")]
    Manager(#[from] apollia_memory::manager::MemoryManagerError),

    /// Erreur de serialisation JSON.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Erreur d'entree/sortie filesystem ou stdin.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Contexte non-interactif sans --confirm.
    #[error("use --confirm for non-interactive clear")]
    NonInteractive,
}

/// Resout le repertoire memoire par defaut (`~/.apollia/memory/`).
fn default_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia").join("memory")
}

/// Formate une taille en octets en unite lisible (B, KB, MB, GB).
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Execute la logique d'inspection d'un namespace memoire.
///
/// Retourne la sortie formatee (texte humain ou JSON) en cas de succes.
pub fn execute_inspect(
    namespace: &str,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let db_path = data_dir.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: namespace.to_string(),
            path: db_path.display().to_string(),
        });
    }

    let store = apollia_memory::store::MemoryStore::open(&db_path)?;
    let stats = store.stats(namespace, &db_path)?;

    if json {
        let output = serde_json::to_string_pretty(&stats)?;
        return Ok(output);
    }

    let size_display = format_size(stats.db_size_bytes);
    let output = format!(
        "Namespace   : {}\n\
         Fichier     : {} ({})\n\
         Episodes    : {}\n\
         Semantique  : {} cles\n\
         Procedures  : {}",
        stats.namespace,
        db_path.display(),
        size_display,
        stats.episodic_count,
        stats.semantic_count,
        stats.procedural_count,
    );

    Ok(output)
}

/// Execute la commande `memory list`.
///
/// Scanne `data_dir/*.db`, ouvre chaque base et collecte les statistiques.
/// Si `agent` est fourni, seul le namespace correspondant est retourne.
pub fn execute_list(
    agent: Option<&str>,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let entries = collect_namespace_stats(data_dir, agent)?;

    if json {
        let output = serde_json::to_string_pretty(&entries)?;
        return Ok(output);
    }

    if entries.is_empty() {
        return Ok("Aucun namespace memoire trouve.".to_string());
    }

    let ns_width = entries
        .iter()
        .map(|s| s.namespace.len())
        .max()
        .unwrap_or(9)
        .max(9);

    let mut lines = vec![format!(
        "{:<ns_width$}  {:>8}  {:>8}  {:>10}  {:>8}",
        "NAMESPACE",
        "EPISODIC",
        "SEMANTIC",
        "PROCEDURAL",
        "SIZE",
        ns_width = ns_width
    )];

    for s in &entries {
        lines.push(format!(
            "{:<ns_width$}  {:>8}  {:>8}  {:>10}  {:>8}",
            s.namespace,
            s.episodic_count,
            s.semantic_count,
            s.procedural_count,
            format_size(s.db_size_bytes),
            ns_width = ns_width
        ));
    }

    Ok(lines.join("\n"))
}

/// Scanne `data_dir` et retourne les statistiques de chaque namespace.
fn collect_namespace_stats(
    data_dir: &Path,
    filter: Option<&str>,
) -> Result<Vec<apollia_memory::store::MemoryStats>, MemoryCommandError> {
    if !data_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in std::fs::read_dir(data_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }

        let namespace = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_owned(),
            None => continue,
        };

        if let Some(f) = filter {
            if namespace != f {
                continue;
            }
        }

        let store = apollia_memory::store::MemoryStore::open(&path)?;
        let stats = store.stats(&namespace, &path)?;
        results.push(stats);
    }

    results.sort_by(|a, b| a.namespace.cmp(&b.namespace));
    Ok(results)
}

/// Execute la commande `memory clear`.
///
/// Demande confirmation interactive si `--confirm` est absent et que stdin est un TTY.
/// Retourne une description du nombre de lignes supprimees.
pub fn execute_clear(
    agent: &str,
    memory_type: &MemoryType,
    confirm: bool,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let db_path = data_dir.join(format!("{agent}.db"));

    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: agent.to_string(),
            path: db_path.display().to_string(),
        });
    }

    if !confirm {
        let is_interactive = std::io::stdin().is_terminal() && !json;
        if !is_interactive {
            return Err(MemoryCommandError::NonInteractive);
        }
        let type_label = match memory_type {
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
            MemoryType::Procedural => "procedural",
            MemoryType::All => "all",
        };
        print!("Vider la memoire de '{agent}' [{type_label}] ? (y/N) ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            return Ok("Annule.".to_string());
        }
    }

    let store = apollia_memory::store::MemoryStore::open(&db_path)?;

    let deleted = match memory_type {
        MemoryType::Episodic => store.clear_episodic(agent)?,
        MemoryType::Semantic => store.clear_semantic(agent)?,
        MemoryType::Procedural => store.clear_procedural(agent)?,
        MemoryType::All => {
            let e = store.clear_episodic(agent)?;
            let s = store.clear_semantic(agent)?;
            let p = store.clear_procedural(agent)?;
            e + s + p
        }
    };

    let type_label = match memory_type {
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
        MemoryType::Procedural => "procedural",
        MemoryType::All => "all",
    };

    tracing::info!(
        namespace = %agent,
        memory_type = %type_label,
        deleted = %deleted,
        "memory cleared"
    );

    if json {
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": agent,
            "type": type_label,
            "deleted": deleted,
        }))?;
        return Ok(output);
    }

    Ok(format!("{deleted} entree(s) supprimee(s) ({type_label})."))
}

/// Execute la commande `memory purge`.
///
/// Purge les entrees plus anciennes que `older_than` jours.
/// Si `memory_type` est `None`, les trois types sont cibles.
pub fn execute_purge(
    namespace: &str,
    older_than: u32,
    memory_type: Option<&MemoryType>,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let db_path = data_dir.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: namespace.to_string(),
            path: db_path.display().to_string(),
        });
    }

    let mut mgr =
        apollia_memory::manager::MemoryManager::new(data_dir, Some(namespace.to_string()), vec![]);

    let (ep_days, sem_days, proc_days) = match memory_type {
        Some(MemoryType::Episodic) => (Some(older_than), None, None),
        Some(MemoryType::Semantic) => (None, Some(older_than), None),
        Some(MemoryType::Procedural) => (None, None, Some(older_than)),
        Some(MemoryType::All) | None => (Some(older_than), Some(older_than), Some(older_than)),
    };

    let report = mgr.purge_old_entries(namespace, ep_days, sem_days, proc_days)?;

    let total = report.episodic_deleted + report.semantic_deleted + report.procedural_deleted;

    tracing::info!(
        namespace = %namespace,
        older_than_days = older_than,
        episodic = report.episodic_deleted,
        semantic = report.semantic_deleted,
        procedural = report.procedural_deleted,
        "memory purge completed"
    );

    if json {
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "older_than_days": older_than,
            "episodic_deleted": report.episodic_deleted,
            "semantic_deleted": report.semantic_deleted,
            "procedural_deleted": report.procedural_deleted,
            "total_deleted": total,
        }))?;
        return Ok(output);
    }

    Ok(format!(
        "{total} entree(s) purgee(s) (episodic: {}, semantic: {}, procedural: {}).",
        report.episodic_deleted, report.semantic_deleted, report.procedural_deleted
    ))
}

/// Execute une sous-commande `memory`.
pub fn run(cmd: &MemoryCommand, json: bool) -> Result<String, MemoryCommandError> {
    match cmd {
        MemoryCommand::Inspect {
            namespace,
            data_dir,
            json: local_json,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_inspect(namespace, &dir, *local_json)
        }
        MemoryCommand::List { agent, data_dir } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_list(agent.as_deref(), &dir, json)
        }
        MemoryCommand::Clear {
            agent,
            r#type,
            confirm,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_clear(agent, r#type, *confirm, &dir, json)
        }
        MemoryCommand::Purge {
            namespace,
            older_than,
            r#type,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_purge(namespace, *older_than, r#type.as_ref(), &dir, json)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let db_path = dir.join(format!("{name}.db"));
        let _ = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        db_path
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("apollia_cli_{}", uuid::Uuid::new_v4()))
    }

    // -- inspect existing namespace returns Ok
    #[test]
    fn test_ac1_inspect_existing_namespace() {
        // GIVEN
        let dir = temp_dir();
        setup_test_db(&dir, "test-ns");
        // WHEN
        let result = execute_inspect("test-ns", &dir, false);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Namespace   : test-ns"));
        assert!(output.contains("Episodes    : 0"));
        assert!(output.contains("Semantique  : 0 cles"));
        assert!(output.contains("Procedures  : 0"));
    }

    // -- JSON output is valid and has expected fields
    #[test]
    fn test_ac2_inspect_json_output() {
        // GIVEN
        let dir = temp_dir();
        setup_test_db(&dir, "test-ns");
        // WHEN
        let result = execute_inspect("test-ns", &dir, true);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["namespace"], "test-ns");
        assert!(parsed["db_size_bytes"].is_u64());
        assert_eq!(parsed["episodic_count"], 0);
        assert_eq!(parsed["semantic_count"], 0);
        assert_eq!(parsed["procedural_count"], 0);
    }

    // -- nonexistent namespace returns error
    #[test]
    fn test_ac3_nonexistent_namespace_error() {
        // GIVEN
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN
        let result = execute_inspect("nonexistent", &dir, false);
        // THEN
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nonexistent"));
        assert!(msg.contains("not found"));
    }

    // -- custom data_dir works
    #[test]
    fn test_ac4_custom_data_dir() {
        // GIVEN
        let dir = temp_dir();
        setup_test_db(&dir, "my-ns");
        // WHEN
        let result = execute_inspect("my-ns", &dir, false);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Namespace   : my-ns"));
    }

    // format_size produces human-readable output
    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    // run() dispatches to execute_inspect correctly
    #[test]
    fn test_run_dispatches_inspect() {
        // GIVEN
        let dir = temp_dir();
        setup_test_db(&dir, "test-ns");
        let cmd = MemoryCommand::Inspect {
            namespace: "test-ns".to_string(),
            data_dir: Some(dir),
            json: false,
        };
        // WHEN
        let result = run(&cmd, false);
        // THEN
        assert!(result.is_ok());
    }

    // list returns all namespaces
    #[test]
    fn test_list_returns_all_namespaces() {
        // GIVEN two db files
        let dir = temp_dir();
        setup_test_db(&dir, "agent-a");
        setup_test_db(&dir, "agent-b");
        // WHEN
        let result = execute_list(None, &dir, false);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("agent-a"));
        assert!(output.contains("agent-b"));
    }

    // list with --agent filters results
    #[test]
    fn test_list_filters_by_agent() {
        // GIVEN two namespaces
        let dir = temp_dir();
        setup_test_db(&dir, "agent-a");
        setup_test_db(&dir, "agent-b");
        // WHEN filter to agent-a only
        let result = execute_list(Some("agent-a"), &dir, false);
        // THEN only agent-a appears
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("agent-a"));
        assert!(!output.contains("agent-b"));
    }

    // list JSON output is a valid array
    #[test]
    fn test_list_json_output() {
        // GIVEN one namespace
        let dir = temp_dir();
        setup_test_db(&dir, "ns-x");
        // WHEN
        let result = execute_list(None, &dir, true);
        // THEN
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["namespace"], "ns-x");
    }

    // list on nonexistent dir returns empty
    #[test]
    fn test_list_empty_dir_returns_empty() {
        // GIVEN a dir that does not exist
        let dir = temp_dir();
        // WHEN
        let result = execute_list(None, &dir, false);
        // THEN no error, empty output
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Aucun"));
    }

    // clear with --confirm deletes episodic entries and returns count
    #[test]
    fn test_clear_with_confirm_deletes_episodic() {
        // GIVEN a namespace with 3 episodic entries
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-a");
        // Initialise schema via MemoryStore then seed via raw connection
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        for i in 0..3u32 {
            conn.execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    format!("ep-{i}"),
                    "agent-a",
                    "agent-a",
                    format!("content {i}"),
                    0.5_f64,
                    "2026-01-01T00:00:00Z"
                ],
            )
            .unwrap();
        }
        drop(conn);
        // WHEN clear with --confirm
        let result = execute_clear("agent-a", &MemoryType::Episodic, true, &dir, false);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("3"));
        // AND entries are gone
        let store = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        let stats = store.stats("agent-a", &db_path).unwrap();
        assert_eq!(stats.episodic_count, 0);
    }

    // clear with --confirm and type all deletes all types
    #[test]
    fn test_clear_all_types() {
        // GIVEN entries of each type
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-b");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at) VALUES ('e1', 'agent-b', 'agent-b', 'c', 0.5, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at) VALUES ('s1', 'agent-b', 'k', 'v', 1.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, last_used_at, created_at) VALUES ('p1', 'agent-b', 't', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);
        // WHEN clear all
        let result = execute_clear("agent-b", &MemoryType::All, true, &dir, false);
        // THEN total deleted = 3
        assert!(result.is_ok());
        assert!(result.unwrap().contains("3"));
    }

    // clear non-interactive without --confirm returns NonInteractive error
    #[test]
    fn test_clear_non_interactive_requires_confirm() {
        // GIVEN a namespace db
        let dir = temp_dir();
        setup_test_db(&dir, "agent-c");
        // WHEN clear in json (non-TTY) mode without --confirm
        let result = execute_clear("agent-c", &MemoryType::All, false, &dir, true);
        // THEN NonInteractive error
        assert!(matches!(result, Err(MemoryCommandError::NonInteractive)));
    }

    // clear missing namespace returns NamespaceNotFound
    #[test]
    fn test_clear_missing_namespace_returns_error() {
        // GIVEN dir exists but no db for the namespace
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN
        let result = execute_clear("ghost", &MemoryType::All, true, &dir, false);
        // THEN
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    // purge -- older-than filters episodic only when --type episodic is set
    #[test]
    fn test_purge_episodic_type_only() {
        // GIVEN a namespace with one old episodic entry and one semantic entry
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-p");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'agent-p', 'a', 'old content', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at)
             VALUES ('s1', 'agent-p', 'k', '\"v\"', 1.0, '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);

        // WHEN purge episodic only with threshold of 7 days
        let result = execute_purge("agent-p", 7, Some(&MemoryType::Episodic), &dir, false);

        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("episodic: 1"));
        assert!(output.contains("semantic: 0"));

        // AND semantic entry is intact
        let store = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        let stats = store.stats("agent-p", &db_path).unwrap();
        assert_eq!(stats.episodic_count, 0);
        assert_eq!(stats.semantic_count, 1);
    }

    // purge -- nonexistent namespace returns NamespaceNotFound
    #[test]
    fn test_purge_missing_namespace_returns_error() {
        // GIVEN
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN
        let result = execute_purge("ghost", 30, None, &dir, false);
        // THEN
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    // purge -- JSON output is valid
    #[test]
    fn test_purge_json_output() {
        // GIVEN an empty namespace
        let dir = temp_dir();
        setup_test_db(&dir, "agent-q");
        // WHEN purge with JSON output
        let result = execute_purge("agent-q", 30, None, &dir, true);
        // THEN valid JSON with expected fields
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["namespace"], "agent-q");
        assert_eq!(parsed["older_than_days"], 30);
        assert_eq!(parsed["total_deleted"], 0);
    }

    // purge -- all types deleted when no --type filter
    #[test]
    fn test_purge_all_types_no_filter() {
        // GIVEN old entries of each type
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-r");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'agent-r', 'a', 'c', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at)
             VALUES ('s1', 'agent-r', 'k', '\"v\"', 1.0, '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, last_used_at, created_at)
             VALUES ('p1', 'agent-r', 't', '[]', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);
        // WHEN purge all (no --type) older than 7 days
        let result = execute_purge("agent-r", 7, None, &dir, false);
        // THEN all three types deleted
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("3 entree(s)"));
    }
}
