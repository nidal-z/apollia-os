//! Tauri IPC commands that change the agents' memory: clear, purge, delete
//! one entry, export and import a namespace.
//!
//! Embedded, each command calls [`apollia_runtime::memory_maintenance`] on the
//! memory directory of this machine. Attached, it reaches the same functions
//! through `/api/v1/memory/namespaces/{namespace}/...` on the daemon, which
//! owns the memory. An export or an import file stays on the machine of the
//! window: the daemon sends and receives its content as JSON.

use std::path::Path;

use apollia_memory::export::MemoryExport;
use apollia_runtime::api::routes_memory_maintenance::{
    ClearMemoryBody, ImportMemoryBody, MemoryEntryDeleted, MemoryImported, MemoryRemoved,
    PurgeMemoryBody,
};
use apollia_runtime::memory_maintenance;
use tauri::State;

use super::memory_base_dir;
use crate::attached::{segment, AttachedClient, AttachedError, RuntimeLink};

fn namespace_route(namespace: &str, tail: &str) -> String {
    format!("/api/v1/memory/namespaces/{}/{tail}", segment(namespace))
}

/// Clears every entry of `namespace` on the daemon.
pub(crate) async fn clear_on(
    remote: &AttachedClient,
    namespace: &str,
    memory_type: Option<String>,
) -> Result<u64, AttachedError> {
    let body = ClearMemoryBody { memory_type };
    let answer: MemoryRemoved = remote
        .post(&namespace_route(namespace, "clear"), &body)
        .await?;
    Ok(answer.removed)
}

/// Purges the entries of `namespace` older than `older_than_days` on the
/// daemon.
pub(crate) async fn purge_on(
    remote: &AttachedClient,
    namespace: &str,
    older_than_days: u32,
    memory_type: Option<String>,
) -> Result<u64, AttachedError> {
    let body = PurgeMemoryBody {
        older_than_days,
        memory_type,
    };
    let answer: MemoryRemoved = remote
        .post(&namespace_route(namespace, "purge"), &body)
        .await?;
    Ok(answer.removed)
}

/// Deletes the entry `entry_id` of `namespace` on the daemon.
pub(crate) async fn delete_entry_on(
    remote: &AttachedClient,
    namespace: &str,
    entry_id: &str,
) -> Result<bool, AttachedError> {
    let tail = format!("entries/{}", segment(entry_id));
    let answer: MemoryEntryDeleted = remote.delete(&namespace_route(namespace, &tail)).await?;
    Ok(answer.deleted)
}

/// Reads the export of `namespace` from the daemon.
pub(crate) async fn export_on(
    remote: &AttachedClient,
    namespace: &str,
) -> Result<MemoryExport, AttachedError> {
    remote.get(&namespace_route(namespace, "export")).await
}

/// Sends `content` to the daemon to import into `namespace`.
pub(crate) async fn import_on(
    remote: &AttachedClient,
    namespace: &str,
    content: MemoryExport,
    mode: String,
) -> Result<usize, AttachedError> {
    let body = ImportMemoryBody { mode, content };
    let answer: MemoryImported = remote
        .post(&namespace_route(namespace, "import"), &body)
        .await?;
    Ok(answer.imported)
}

/// Clears all memory entries of a namespace by type.
///
/// `memory_type` can be `"episodic"`, `"semantic"`, `"procedural"`, or `"all"` (default).
/// Returns the total number of removed entries.
#[tauri::command]
pub async fn clear_memory(
    link: State<'_, RuntimeLink>,
    namespace: String,
    memory_type: Option<String>,
) -> Result<u64, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            memory_maintenance::clear(memory_base_dir()?, namespace, memory_type)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(clear_on(remote, &namespace, memory_type).await?),
    }
}

/// Purges memory entries older than N days.
///
/// `memory_type` can be `"episodic"`, `"semantic"`, `"procedural"`, or `"all"` (default).
/// Returns the total number of purged entries.
#[tauri::command]
pub async fn purge_memory(
    link: State<'_, RuntimeLink>,
    namespace: String,
    older_than_days: u32,
    memory_type: Option<String>,
) -> Result<u64, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            memory_maintenance::purge(memory_base_dir()?, namespace, older_than_days, memory_type)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            Ok(purge_on(remote, &namespace, older_than_days, memory_type).await?)
        }
    }
}

/// Deletes a memory entry by its UUID.
///
/// Searches all tables (episodic, semantic, procedural) and deletes the
/// matching entry along with its FTS5 index.
#[tauri::command]
pub async fn delete_memory_entry(
    link: State<'_, RuntimeLink>,
    namespace: String,
    entry_id: String,
) -> Result<bool, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            memory_maintenance::delete_entry(memory_base_dir()?, namespace, entry_id)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(delete_entry_on(remote, &namespace, &entry_id).await?),
    }
}

/// Writes `export` to the file `output_path` of this machine and says what it
/// holds.
fn write_export(export: &MemoryExport, output_path: &Path) -> Result<String, String> {
    let json_str =
        serde_json::to_string_pretty(export).map_err(|e| format!("serialize failed: {e}"))?;
    std::fs::write(output_path, &json_str).map_err(|e| format!("write failed: {e}"))?;
    Ok(format!(
        "Exported {} episodic, {} semantic, {} procedural entries to {}",
        export.episodic.len(),
        export.semantic.len(),
        export.procedural.len(),
        output_path.display(),
    ))
}

/// Reads an export from the file `input_path` of this machine.
fn read_export(input_path: &str) -> Result<MemoryExport, String> {
    let content = std::fs::read_to_string(input_path).map_err(|e| format!("read failed: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("parse failed: {e}"))
}

/// Exports a namespace's memory to a JSON file.
///
/// `output_path`: absolute path of the output file, on the machine of the
/// window. Returns a confirmation message with the export statistics.
#[tauri::command]
pub async fn memory_export_namespace(
    link: State<'_, RuntimeLink>,
    namespace: String,
    output_path: String,
) -> Result<String, String> {
    let export = match link.inner() {
        RuntimeLink::Embedded(_) => memory_maintenance::export(memory_base_dir()?, namespace)
            .await
            .map_err(|e| e.to_string())?,
        RuntimeLink::Attached(remote) => export_on(remote, &namespace).await?,
    };
    write_export(&export, Path::new(&output_path))
}

/// Imports memory from a JSON file into a namespace.
///
/// `input_path` is read on the machine of the window. `mode` can be
/// `"replace"` or `"merge"` (default: `"merge"`). Returns the number of
/// imported entries.
#[tauri::command]
pub async fn memory_import_namespace(
    link: State<'_, RuntimeLink>,
    namespace: String,
    input_path: String,
    mode: String,
) -> Result<String, String> {
    let export = read_export(&input_path)?;
    let count = match link.inner() {
        RuntimeLink::Embedded(_) => {
            memory_maintenance::import(memory_base_dir()?, namespace.clone(), export, &mode)
                .await
                .map_err(|e| e.to_string())?
        }
        RuntimeLink::Attached(remote) => {
            import_on(remote, &namespace, export, mode.clone()).await?
        }
    };
    Ok(format!(
        "{count} entries imported into namespace '{namespace}' (mode: {mode})"
    ))
}

#[cfg(test)]
mod tests;
