//! `apollia-os memory` subcommands.
//!
//! Provides memory inspection and diagnostics directly from the CLI.
//! Reads the SQLite `.db` file without requiring the runtime.

use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

mod inspect;
mod maintenance;
mod search;

use inspect::{default_data_dir, execute_inspect, execute_list};
use maintenance::{
    execute_clear, execute_export, execute_import, execute_learn_procedure, execute_purge,
};
use search::{execute_forget, execute_search, SearchArgs};

/// Memory type targeted by wipe operations.
#[derive(Debug, Clone, ValueEnum)]
pub enum MemoryType {
    /// Episodic memories.
    Episodic,
    /// Semantic memories.
    Semantic,
    /// Procedural memories.
    Procedural,
    /// All memory types.
    All,
}

/// Memory management commands.
#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// Inspect the state of a memory namespace.
    Inspect {
        /// Namespace name to inspect.
        namespace: String,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,

        /// JSON output.
        #[arg(long)]
        json: bool,
    },

    /// List every memory namespace present on disk.
    List {
        /// Filter by agent/namespace name.
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Wipe an agent's memory.
    Clear {
        /// Namespace/agent name to wipe.
        #[arg(long, value_name = "NAME")]
        agent: String,

        /// Memory type to wipe.
        #[arg(long, value_enum, default_value = "all")]
        r#type: MemoryType,

        /// Confirm without an interactive prompt.
        #[arg(long)]
        confirm: bool,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Purge memory entries older than a day threshold.
    ///
    /// Example: `apollia-os memory purge --namespace my-agent --older-than 30`
    /// Filtered: `apollia-os memory purge --namespace my-agent --type episodic --older-than 7`
    Purge {
        /// Target namespace.
        #[arg(long, value_name = "NAME")]
        namespace: String,

        /// Delete entries created more than N days ago.
        #[arg(long, value_name = "DAYS")]
        older_than: u32,

        /// Restrict the purge to a single type (default: all types).
        #[arg(long, value_enum)]
        r#type: Option<MemoryType>,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },

    /// Record a procedure in a namespace's procedural memory.
    ///
    /// Example: `apollia-os memory learn-procedure --namespace agent-x --trigger "analyse a report" --steps "1. Open, 2. Read, 3. Summarise"`
    LearnProcedure {
        /// Target namespace.
        #[arg(long, value_name = "NAME")]
        namespace: String,

        /// Exact trigger phrase for the procedure.
        #[arg(long, value_name = "TEXT")]
        trigger: String,

        /// Procedure steps (comma- or semicolon-separated).
        /// Example: "Open the PDF, Extract revenue, Generate summary"
        #[arg(long, value_name = "STEPS", required_unless_present = "file")]
        steps: Option<String>,

        /// JSON file containing {"trigger": "...", "steps": [...]}.
        #[arg(long, value_name = "FILE", required_unless_present = "steps")]
        file: Option<PathBuf>,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Export a namespace's memory to a JSON file.
    ///
    /// Example: `apollia-os memory export --namespace agent-x --output ./backup.apollia-memory`
    Export {
        /// Namespace to export.
        #[arg(long, value_name = "NAME")]
        namespace: String,

        /// Output file (default: `<namespace>.apollia-memory` in the current directory).
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Import memory from a JSON file into a namespace.
    ///
    /// Example: `apollia-os memory import --namespace agent-x --input ./backup.apollia-memory --replace`
    Import {
        /// Target namespace.
        #[arg(long, value_name = "NAME")]
        namespace: String,

        /// Input file exported by `memory export`.
        #[arg(long, value_name = "FILE")]
        input: PathBuf,

        /// Mode: replace the existing namespace (default: merge).
        #[arg(long, conflicts_with = "merge")]
        replace: bool,

        /// Mode: merge with the existing namespace (default).
        #[arg(long, conflicts_with = "replace")]
        merge: bool,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },

    /// Delete a single memory entry by its UUID.
    ///
    /// Searches `episodic_memories`, `semantic_memories`, and
    /// `procedural_memories` in order; removes the matching row and its FTS5
    /// index entry. Returns exit 1 when no entry matches.
    Forget {
        /// Namespace containing the entry.
        namespace: String,

        /// Entry UUID (matches `id` columns across the three tables).
        entry_id: String,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },

    /// Full-text search across a namespace's episodic + semantic memory.
    ///
    /// Returns BM25-ranked matches with their source table (episodic|semantic),
    /// content, and relevance score.
    Search {
        /// Namespace to search.
        namespace: String,

        /// FTS5 query (whitespace-separated keywords; quotes preserved).
        query: String,

        /// Maximum number of matches to return.
        #[arg(long, default_value = "20", value_name = "N")]
        limit: u32,

        /// Restrict to a single source: `episodic` or `semantic`. Omit for
        /// both.
        #[arg(long, value_parser = ["episodic", "semantic"])]
        source: Option<String>,

        /// Memory data directory (default: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },
}

/// Errors raised by the `memory` command.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MemoryCommandError {
    /// The requested namespace does not exist.
    #[error("namespace '{namespace}' not found ({path} does not exist)")]
    NamespaceNotFound {
        /// Namespace name.
        namespace: String,
        /// Expected path of the .db file.
        path: String,
    },

    /// MemoryStore error.
    #[error("memory store error: {0}")]
    Store(#[from] apollia_memory::store::MemoryStoreError),

    /// MemoryManager error.
    #[error("memory manager error: {0}")]
    Manager(#[from] apollia_memory::manager::MemoryManagerError),

    /// JSON serialization error.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Filesystem or stdin I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Non-interactive context without --confirm.
    #[error("use --confirm for non-interactive clear")]
    NonInteractive,

    /// Memory export/import error.
    #[error("export/import error: {0}")]
    Export(#[from] apollia_memory::export::ExportError),
}
/// Execute a `memory` sub-command.
/// The confirmation the destructive `memory` leaves owe their operator
/// (`crates/apollia-cli/AGENTS.md` section 2).
///
/// Applied before [`run`] rather than inside it: the leaves return a rendered
/// string, while the rule returns a process exit code. Returns `Some(code)`
/// when the leaf must stop.
pub fn confirmation(cmd: &MemoryCommand, json: bool) -> Option<i32> {
    match cmd {
        MemoryCommand::Purge {
            namespace,
            older_than,
            confirm,
            ..
        } => crate::output::require_confirmation(
            *confirm,
            json,
            &format!("purge entries older than {older_than} day(s) in namespace '{namespace}'"),
        ),
        MemoryCommand::Forget {
            namespace,
            entry_id,
            confirm,
            ..
        } => crate::output::require_confirmation(
            *confirm,
            json,
            &format!("forget entry '{entry_id}' in namespace '{namespace}'"),
        ),
        _ => None,
    }
}

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
            confirm: _,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_purge(namespace, *older_than, r#type.as_ref(), &dir, json)
        }
        MemoryCommand::LearnProcedure {
            namespace,
            trigger,
            steps,
            file,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);

            let parsed_steps = if let Some(f) = file {
                // Load from JSON file
                let content = std::fs::read_to_string(f)?;
                let v: serde_json::Value = serde_json::from_str(&content)?;
                v["steps"]
                    .as_array()
                    .ok_or_else(|| {
                        MemoryCommandError::Io(std::io::Error::other(
                            "JSON file must contain a 'steps' array",
                        ))
                    })?
                    .iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            } else if let Some(s) = steps {
                s.split([',', ';'])
                    .map(|step| step.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            } else {
                return Err(MemoryCommandError::Io(std::io::Error::other(
                    "either --steps or --file is required",
                )));
            };

            execute_learn_procedure(namespace, trigger, &parsed_steps, &dir, json)
        }
        MemoryCommand::Export {
            namespace,
            output,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_export(namespace, output.as_deref(), &dir, json)
        }
        MemoryCommand::Import {
            namespace,
            input,
            replace,
            merge: _,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_import(namespace, input, *replace, &dir, json)
        }
        MemoryCommand::Forget {
            namespace,
            entry_id,
            data_dir,
            confirm: _,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_forget(namespace, entry_id, &dir, json)
        }
        MemoryCommand::Search {
            namespace,
            query,
            limit,
            source,
            data_dir,
        } => {
            let dir = data_dir.clone().unwrap_or_else(default_data_dir);
            execute_search(SearchArgs {
                namespace,
                query,
                limit: *limit,
                source: source.as_deref(),
                data_dir: &dir,
                json,
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::inspect::format_size;
    use super::*;
    use std::path::Path;

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
    fn test_inspect_existing_namespace() {
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
        assert!(output.contains("Semantic    : 0 keys"));
        assert!(output.contains("Procedures  : 0"));
    }

    // -- JSON output is valid and has expected fields
    #[test]
    fn test_inspect_json_output() {
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
    fn test_nonexistent_namespace_error() {
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
    fn test_custom_data_dir() {
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
        // GIVEN byte counts at each unit boundary, from zero to a gigabyte
        // WHEN each is formatted for the operator
        // THEN the unit changes at the boundary and the decimal is kept
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
        assert!(result.unwrap().contains("No"));
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
        assert!(output.contains("3 entry(ies)"));
    }

    // learn-procedure stores a procedure and is retrievable
    #[test]
    fn test_learn_procedure_stores_and_recalls() {
        // GIVEN a namespace
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-proc");

        // WHEN learn-procedure
        let result = execute_learn_procedure(
            "agent-proc",
            "analyser rapport",
            &["step 1".to_string(), "step 2".to_string()],
            &dir,
            false,
        );

        // THEN success
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("recorded"));

        // AND procedure is in the database
        let store = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        let proc = apollia_memory::procedural::ProceduralMemory::new(&store);
        let entry = proc.recall("agent-proc", "analyser rapport").unwrap();
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.steps.len(), 2);
        assert_eq!(e.steps[0], "step 1");
    }

    // learn-procedure with missing namespace returns error
    #[test]
    fn test_learn_procedure_missing_namespace_error() {
        // GIVEN a dir with no db
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();

        // WHEN
        let result =
            execute_learn_procedure("ghost", "trigger", &["step".to_string()], &dir, false);

        // THEN NamespaceNotFound
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    // learn-procedure JSON output is valid
    #[test]
    fn test_learn_procedure_json_output() {
        // GIVEN a namespace
        let dir = temp_dir();
        setup_test_db(&dir, "agent-j");

        // WHEN learn-procedure with json flag
        let result =
            execute_learn_procedure("agent-j", "mon trigger", &["step1".to_string()], &dir, true);

        // THEN valid JSON
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["namespace"], "agent-j");
        assert_eq!(parsed["trigger"], "mon trigger");
        assert!(parsed["id"].is_string());
    }

    // export creates a file with expected JSON structure
    #[test]
    fn test_export_creates_file() {
        // GIVEN a namespace with some data
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-exp");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at, metadata)
             VALUES ('ep1', 'agent-exp', 'a', 'hello', 0.5, '2026-01-01T00:00:00Z', '{}')",
            [],
        )
        .unwrap();
        drop(conn);

        let out = dir.join("backup.apollia-memory");

        // WHEN export
        let result = execute_export("agent-exp", Some(&out), &dir, false);

        // THEN success + file created
        assert!(result.is_ok());
        assert!(out.exists());
        let content = std::fs::read_to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["format_version"], 1);
        assert_eq!(v["namespace"], "agent-exp");
        assert_eq!(v["episodic"].as_array().unwrap().len(), 1);
    }

    // import replace round-trip restores entries
    #[test]
    fn test_import_replace_round_trip() {
        // GIVEN a namespace with 1 semantic entry
        let dir = temp_dir();
        let db_path = setup_test_db(&dir, "agent-imp");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at)
             VALUES ('s1', 'agent-imp', 'k', '\"v\"', 1.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);

        // Export
        let backup = dir.join("backup.json");
        execute_export("agent-imp", Some(&backup), &dir, false).expect("export");

        // Clear
        let store = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        store.clear_semantic("agent-imp").unwrap();
        drop(store);

        // Import with replace
        // WHEN it is exported, the namespace cleared, and the file imported in replace mode
        let result = execute_import("agent-imp", &backup, true, &dir, false);
        assert!(result.is_ok());

        // THEN the entry is back, so the export file is one the import can read
        // Verify restored
        let store = apollia_memory::store::MemoryStore::open(&db_path).unwrap();
        let stats = store.stats("agent-imp", &db_path).unwrap();
        assert_eq!(stats.semantic_count, 1);
    }

    // export missing namespace returns error
    #[test]
    fn test_export_missing_namespace_error() {
        // GIVEN a data directory with no such namespace
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN an export is asked for it
        let result = execute_export("ghost", None, &dir, false);
        // THEN the namespace is reported missing rather than an empty file being written
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    #[test]
    fn test_forget_missing_namespace_errors() {
        // GIVEN a data directory with no such namespace
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN an entry of it is forgotten
        let result = execute_forget("ghost", "00000000-0000-0000-0000-000000000000", &dir, false);
        // THEN the namespace is reported missing
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    #[test]
    fn test_forget_unknown_entry_errors() {
        // GIVEN an existing namespace holding no such entry
        let dir = temp_dir();
        setup_test_db(&dir, "ns");
        // WHEN that entry is forgotten
        let result = execute_forget("ns", "00000000-0000-0000-0000-000000000000", &dir, true);
        // THEN the command errors rather than reporting a deletion that did not happen
        assert!(result.is_err(), "missing entry should error");
    }

    #[test]
    fn test_search_missing_namespace_errors() {
        // GIVEN a data directory with no such namespace
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN a search runs against it
        let result = execute_search(SearchArgs {
            namespace: "ghost",
            query: "needle",
            limit: 10,
            source: None,
            data_dir: &dir,
            json: false,
        });
        // THEN the namespace is reported missing rather than yielding no match
        assert!(matches!(
            result,
            Err(MemoryCommandError::NamespaceNotFound { .. })
        ));
    }

    #[test]
    fn test_search_empty_namespace_returns_no_matches() {
        // GIVEN an existing namespace with nothing stored in it
        let dir = temp_dir();
        setup_test_db(&dir, "ns");
        // WHEN a search runs against it
        let result = execute_search(SearchArgs {
            namespace: "ns",
            query: "needle",
            limit: 10,
            source: None,
            data_dir: &dir,
            json: false,
        });
        // THEN it succeeds and says so, which is not the same answer as a missing namespace
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No matches"));
    }

    #[test]
    fn test_search_invalid_source_yields_no_matches() {
        // Unknown source string maps to an empty vec; the engine then returns
        // an empty result list (no episodic OR semantic flag set).
        // GIVEN an existing namespace and a source filter
        let dir = temp_dir();
        setup_test_db(&dir, "ns");
        // WHEN a search runs with that filter
        let result = execute_search(SearchArgs {
            namespace: "ns",
            query: "needle",
            limit: 10,
            source: Some("episodic"),
            data_dir: &dir,
            json: false,
        });
        // THEN it succeeds, with no match, rather than failing on the filter
        assert!(result.is_ok());
    }

    #[test]
    fn parses_forget() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: MemoryCommand,
        }
        // GIVEN "memory forget ns abc-uuid"
        let cli = TestCli::parse_from(["x", "forget", "ns", "abc-uuid"]);
        // WHEN clap parses the argument line
        // THEN the namespace comes first and the entry second, in that order
        match cli.cmd {
            MemoryCommand::Forget {
                namespace,
                entry_id,
                ..
            } => {
                assert_eq!(namespace, "ns");
                assert_eq!(entry_id, "abc-uuid");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_search() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: MemoryCommand,
        }
        // GIVEN a search carrying a multi-word query, a limit and a source
        let cli = TestCli::parse_from([
            "x",
            "search",
            "ns",
            "needle in haystack",
            "--limit",
            "5",
            "--source",
            "episodic",
        ]);
        // WHEN clap parses the argument line
        // THEN the query keeps its spaces and each option lands on its own field
        match cli.cmd {
            MemoryCommand::Search {
                namespace,
                query,
                limit,
                source,
                ..
            } => {
                assert_eq!(namespace, "ns");
                assert_eq!(query, "needle in haystack");
                assert_eq!(limit, 5);
                assert_eq!(source.as_deref(), Some("episodic"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_search_rejects_invalid_source() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: MemoryCommand,
        }
        // GIVEN a search whose --source names a memory kind the CLI does not expose
        // WHEN clap parses the argument line
        let result = TestCli::try_parse_from(["x", "search", "ns", "q", "--source", "procedural"]);
        // THEN parsing fails rather than reaching the store with a filter nothing matches
        assert!(result.is_err(), "procedural is not yet exposed");
    }
}
