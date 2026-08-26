//! Entry deletion and semantic search over the memory namespaces.

use std::path::Path;

use super::MemoryCommandError;

/// Delete a single memory entry by UUID.
pub fn execute_forget(
    namespace: &str,
    entry_id: &str,
    data_dir: &Path,
    json: bool,
) -> Result<String, MemoryCommandError> {
    let db_path = data_dir.join(format!("{namespace}.db"));
    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: namespace.to_string(),
            path: db_path.to_string_lossy().into_owned(),
        });
    }
    let store = apollia_memory::store::MemoryStore::open(&db_path)?;
    let removed = store.delete_entry_by_id(entry_id)?;
    if !removed {
        return Err(MemoryCommandError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("entry '{entry_id}' not found in namespace '{namespace}'"),
        )));
    }
    if json {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "entry_id": entry_id,
            "removed": true,
        }))
        .unwrap_or_default())
    } else {
        Ok(format!("* {namespace} / {entry_id} removed"))
    }
}

/// Parameters for [`execute_search`].
pub struct SearchArgs<'a> {
    /// Memory namespace (maps to a `<namespace>.db` file).
    pub namespace: &'a str,
    /// Full-text query string.
    pub query: &'a str,
    /// Maximum number of hits to return.
    pub limit: u32,
    /// Optional source filter (`episodic` / `semantic`).
    pub source: Option<&'a str>,
    /// Directory containing the namespace database.
    pub data_dir: &'a Path,
    /// Emit machine-readable JSON.
    pub json: bool,
}

/// Full-text search across episodic + semantic memory.
pub fn execute_search(args: SearchArgs<'_>) -> Result<String, MemoryCommandError> {
    let SearchArgs {
        namespace,
        query,
        limit,
        source,
        data_dir,
        json,
    } = args;
    let db_path = data_dir.join(format!("{namespace}.db"));
    if !db_path.exists() {
        return Err(MemoryCommandError::NamespaceNotFound {
            namespace: namespace.to_string(),
            path: db_path.to_string_lossy().into_owned(),
        });
    }
    let store = apollia_memory::store::MemoryStore::open(&db_path)?;
    let search_engine = apollia_memory::search::MemorySearch::new(&store);

    let sources_vec = source.map(|s| match s {
        "episodic" => vec![apollia_memory::search::SearchSource::Episodic],
        "semantic" => vec![apollia_memory::search::SearchSource::Semantic],
        _ => Vec::new(),
    });
    let sources_slice = sources_vec.as_deref();

    let results = search_engine
        .query(apollia_memory::search::SearchQuery {
            namespace,
            query,
            limit,
            sources: sources_slice,
            min_importance: None,
        })
        .map_err(|e| MemoryCommandError::Io(std::io::Error::other(e.to_string())))?;

    if json {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "namespace": namespace,
            "query": query,
            "limit": limit,
            "source": source,
            "results": results,
        }))
        .unwrap_or_default());
    }

    if results.is_empty() {
        return Ok(format!(
            "No matches for '{query}' in {namespace} (limit {limit})."
        ));
    }

    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "  Search results for '{query}' in {namespace} ({} hits):",
        results.len()
    );
    let _ = writeln!(
        out,
        "  {:<6} {:<10} {:<10} ID                                   CONTENT",
        "SCORE", "SOURCE", "RELEVANCE"
    );
    for r in &results {
        let source_str = match r.source {
            apollia_memory::search::SearchSource::Episodic => "episodic",
            apollia_memory::search::SearchSource::Semantic => "semantic",
        };
        let relevance = r
            .relevance
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string());
        let content_preview: String = r.content.chars().take(60).collect();
        let _ = writeln!(
            out,
            "  {:<6.2} {:<10} {:<10} {:<36} {}",
            r.score, source_str, relevance, r.source_id, content_preview
        );
    }
    Ok(out)
}
