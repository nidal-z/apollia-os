//! Read-only memory verbs: `inspect` and `list`.

use std::path::{Path, PathBuf};

use super::MemoryCommandError;

/// Resolve the default memory directory (`~/.apollia/memory/`).
pub(super) fn default_data_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home).join("memory")
}

/// Format a byte size into a human-readable unit (B, KB, MB, GB).
pub(super) fn format_size(bytes: u64) -> String {
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

/// Inspect a memory namespace.
///
/// Returns the formatted output (human text or JSON) on success.
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
         File        : {} ({})\n\
         Episodes    : {}\n\
         Semantic    : {} keys\n\
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

/// Execute the `memory list` command.
///
/// Scans `data_dir/*.db`, opens each database, and collects its statistics.
/// When `agent` is supplied, only the matching namespace is returned.
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
        return Ok("No memory namespace found.".to_string());
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

/// Scan `data_dir` and return the statistics of each namespace.
pub(super) fn collect_namespace_stats(
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
