//! Memory verbs that write: clear, purge, learn, export and import.

use std::io::IsTerminal as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::{MemoryCommandError, MemoryType};

/// Execute the `memory clear` command.
///
/// Prompts for interactive confirmation when `--confirm` is absent and stdin is a TTY.
/// Returns a description of the number of rows deleted.
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
        print!("Clear memory for '{agent}' [{type_label}]? (y/N) ");
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
        "memory.cleared"
    );

    if json {
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": agent,
            "type": type_label,
            "deleted": deleted,
        }))?;
        return Ok(output);
    }

    Ok(format!("{deleted} entry(ies) deleted ({type_label})."))
}

/// Execute the `memory purge` command.
///
/// Purges entries older than `older_than` days.
/// When `memory_type` is `None`, all three types are targeted.
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
        "memory.purged"
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
        "{total} entry(ies) purged (episodic: {}, semantic: {}, procedural: {}).",
        report.episodic_deleted, report.semantic_deleted, report.procedural_deleted
    ))
}

/// Execute the `memory learn-procedure` command.
///
/// Records a procedure in a namespace's procedural memory.
/// When the trigger already exists, increments success_count and updates the steps.
pub fn execute_learn_procedure(
    namespace: &str,
    trigger: &str,
    steps: &[String],
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    if steps.is_empty() {
        return Err(MemoryCommandError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "steps must not be empty",
        )));
    }

    let db_path = data_dir.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: namespace.to_string(),
            path: db_path.display().to_string(),
        });
    }

    let store = apollia_memory::store::MemoryStore::open(&db_path)?;
    let proc = apollia_memory::procedural::ProceduralMemory::new(&store);
    let id = proc
        .learn(namespace, trigger, steps)
        .map_err(|e| MemoryCommandError::Io(std::io::Error::other(e.to_string())))?;

    tracing::info!(
        namespace = %namespace,
        trigger = %trigger,
        steps = steps.len(),
        id = %id,
        "memory.procedure.learned"
    );

    if json {
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "trigger": trigger,
            "steps": steps,
            "id": id,
        }))?;
        return Ok(output);
    }

    Ok(format!("Procedure recorded (id: {id})."))
}

/// Execute the `memory export` command.
pub fn execute_export(
    namespace: &str,
    output: Option<&Path>,
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

    let export = apollia_memory::export::export_namespace(data_dir, namespace)?;

    let out_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(format!("{namespace}.apollia-memory")));

    let json_str = serde_json::to_string_pretty(&export)?;
    std::fs::write(&out_path, &json_str)?;

    tracing::info!(
        namespace = %namespace,
        path = %out_path.display(),
        episodic = export.episodic.len(),
        semantic = export.semantic.len(),
        procedural = export.procedural.len(),
        "memory.exported"
    );

    if json {
        let output_json = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "path": out_path.display().to_string(),
            "episodic": export.episodic.len(),
            "semantic": export.semantic.len(),
            "procedural": export.procedural.len(),
        }))?;
        return Ok(output_json);
    }

    Ok(format!(
        "Memory exported to {} ({} episodic, {} semantic, {} procedural).",
        out_path.display(),
        export.episodic.len(),
        export.semantic.len(),
        export.procedural.len(),
    ))
}

/// Execute the `memory import` command.
pub fn execute_import(
    namespace: &str,
    input: &Path,
    replace: bool,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let content = std::fs::read_to_string(input)?;
    let export: apollia_memory::export::MemoryExport = serde_json::from_str(&content)?;

    let mode = if replace {
        apollia_memory::export::ImportMode::Replace
    } else {
        apollia_memory::export::ImportMode::Merge
    };

    let count = apollia_memory::export::import_namespace(data_dir, namespace, &export, mode)?;

    tracing::info!(
        namespace = %namespace,
        mode = if replace { "replace" } else { "merge" },
        imported = count,
        "memory.imported"
    );

    if json {
        let output_json = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "mode": if replace { "replace" } else { "merge" },
            "imported": count,
        }))?;
        return Ok(output_json);
    }

    Ok(format!(
        "{count} entry(ies) imported into namespace '{namespace}' (mode: {}).",
        if replace { "replace" } else { "merge" }
    ))
}
