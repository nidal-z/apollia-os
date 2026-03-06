//! `apollia-os memory` subcommands.
//!
//! Provides memory inspection and diagnostics directly from the CLI.
//! Reads the SQLite `.db` file without requiring the runtime.

use std::path::{Path, PathBuf};

use clap::Subcommand;

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

    /// Erreur de serialisation JSON.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
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

/// Execute une sous-commande `memory`.
pub fn run(cmd: &MemoryCommand) -> Result<String, MemoryCommandError> {
    match cmd {
        MemoryCommand::Inspect {
            namespace,
            data_dir,
            json,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_inspect(namespace, &dir, *json)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_cli_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test-ns.db");
        let _ = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        (dir, db_path)
    }

    // AC-1 -- inspect existing namespace returns Ok
    #[test]
    fn test_ac1_inspect_existing_namespace() {
        // GIVEN
        let (dir, _) = setup_test_db();
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

    // AC-2 -- JSON output is valid and has expected fields
    #[test]
    fn test_ac2_inspect_json_output() {
        // GIVEN
        let (dir, _) = setup_test_db();
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

    // AC-3 -- nonexistent namespace returns error
    #[test]
    fn test_ac3_nonexistent_namespace_error() {
        // GIVEN
        let dir = std::env::temp_dir().join(format!("apollia_cli_{}", uuid::Uuid::new_v4()));
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

    // AC-4 -- custom data_dir works
    #[test]
    fn test_ac4_custom_data_dir() {
        // GIVEN
        let custom_dir =
            std::env::temp_dir().join(format!("apollia_cli_custom_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&custom_dir).unwrap();
        let db_path = custom_dir.join("my-ns.db");
        let _ = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        // WHEN
        let result = execute_inspect("my-ns", &custom_dir, false);
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
        let (dir, _) = setup_test_db();
        let cmd = MemoryCommand::Inspect {
            namespace: "test-ns".to_string(),
            data_dir: Some(dir),
            json: false,
        };
        // WHEN
        let result = run(&cmd);
        // THEN
        assert!(result.is_ok());
    }
}
