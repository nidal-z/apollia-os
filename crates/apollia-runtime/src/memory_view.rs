//! The memory of the agents as the Memory screen reads it: the namespaces,
//! the entries of one namespace, a search in one, and the entries an agent
//! recalled during one turn.
//!
//! Each namespace is one SQLite file `<base>/<namespace>.db`. The desktop
//! running the runtime in its own process calls these functions directly; a
//! client of the daemon reaches the same functions through `/api/v1/memory`.
//! Both therefore answer the same view and refuse with the same words.

use std::path::{Path, PathBuf};

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::injection_tracker::{global_entries_for, InjectedEntry};
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::search::{MemorySearch, MemorySearchError, SearchQuery, SearchSource};
use apollia_memory::semantic::SemanticMemory;
use apollia_memory::store::{MemoryStore, MemoryStoreError};
use serde::{Deserialize, Serialize};

/// How many episodes the list of a namespace reads at most.
const EPISODE_LIMIT: u32 = 500;

/// How many results a search answers at most.
const SEARCH_LIMIT: u32 = 50;

/// One memory entry, as the Memory screen lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemoryEntryView {
    /// Entry UUID.
    pub id: String,
    /// Type: `"episodic"` | `"semantic"` | `"procedural"`.
    pub entry_type: String,
    /// Key for semantic, trigger for procedural, agent id for episodic.
    pub key: String,
    /// Value or text content.
    pub value: String,
    /// ISO 8601 creation date.
    pub created_at: String,
    /// ISO 8601 expiration date, `null` without a TTL.
    pub expires_at: Option<String>,
    /// BM25 score, `null` outside a search.
    pub score: Option<f64>,
}

/// One full-text search result, as the Memory screen lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemorySearchResultView {
    /// UUID of the source entry.
    pub id: String,
    /// Type: `"episodic"` | `"semantic"`.
    pub entry_type: String,
    /// Text content of the result.
    pub content: String,
    /// BM25 score, higher is more relevant.
    pub score: f64,
    /// Importance (episodic) or confidence (semantic).
    pub relevance: Option<f64>,
    /// ISO 8601 creation date.
    pub created_at: String,
}

/// One memory entry an agent recalled during a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InjectedMemoryEntryView {
    /// Source memory entry id.
    pub id: String,
    /// Short preview of the entry content.
    pub content_preview: String,
    /// Namespace of origin.
    pub namespace: String,
    /// Why the agent recalled it.
    pub injection_reason: String,
    /// Relevance score in `[0.0, 1.0]` when available.
    pub relevance_score: f32,
}

impl From<InjectedEntry> for InjectedMemoryEntryView {
    fn from(entry: InjectedEntry) -> Self {
        Self {
            id: entry.id,
            content_preview: entry.content_preview,
            namespace: entry.namespace,
            injection_reason: entry.injection_reason,
            relevance_score: entry.relevance_score,
        }
    }
}

/// Why a memory read did not complete. The display text is what the
/// interface shows, the same whichever process ran the read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MemoryViewError {
    /// The namespace is empty or names a path rather than a file.
    #[error("VALIDATION_ERROR: invalid memory namespace '{0}'")]
    InvalidNamespace(String),
    /// The memory directory could not be listed.
    #[error("failed to read memory directory: {0}")]
    ReadDir(#[source] std::io::Error),
    /// The namespace file did not open.
    #[error("failed to open store: {0}")]
    Open(#[source] MemoryStoreError),
    /// The full-text search failed.
    #[error("search failed: {0}")]
    Search(#[source] MemorySearchError),
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

/// The file of `namespace` under `base`, refusing a name that would leave it.
fn namespace_file(base: &Path, namespace: &str) -> Result<PathBuf, MemoryViewError> {
    if namespace.is_empty() || namespace.contains(['/', '\\', '\0']) || namespace == ".." {
        return Err(MemoryViewError::InvalidNamespace(namespace.to_owned()));
    }
    Ok(base.join(format!("{namespace}.db")))
}

/// The namespaces under `base`, one per `.db` file, sorted.
///
/// # Errors
///
/// [`MemoryViewError::ReadDir`] when the directory exists and cannot be read.
pub async fn list_namespaces(base: PathBuf) -> Result<Vec<String>, MemoryViewError> {
    tokio::task::spawn_blocking(move || {
        if !base.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&base).map_err(MemoryViewError::ReadDir)?;
        let mut namespaces: Vec<String> = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    path.file_stem().and_then(|s| s.to_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        namespaces.sort();
        Ok(namespaces)
    })
    .await
    .map_err(MemoryViewError::Join)?
}

/// Every entry of `namespace` (episodic, semantic, procedural), newest first.
/// A namespace without a file has no entry.
///
/// # Errors
///
/// [`MemoryViewError::InvalidNamespace`] or [`MemoryViewError::Open`].
pub async fn namespace_entries(
    base: PathBuf,
    namespace: String,
) -> Result<Vec<MemoryEntryView>, MemoryViewError> {
    tokio::task::spawn_blocking(move || {
        let db_path = namespace_file(&base, &namespace)?;
        if !db_path.exists() {
            return Ok(Vec::new());
        }
        let store = MemoryStore::open(&db_path).map_err(MemoryViewError::Open)?;
        Ok(entries_of(&store, &namespace))
    })
    .await
    .map_err(MemoryViewError::Join)?
}

fn entries_of(store: &MemoryStore, namespace: &str) -> Vec<MemoryEntryView> {
    let mut entries = Vec::new();
    if let Ok(episodes) = EpisodicMemory::new(store).history(namespace, EPISODE_LIMIT, None) {
        entries.extend(episodes.into_iter().map(|ep| MemoryEntryView {
            id: ep.id,
            entry_type: "episodic".to_owned(),
            key: ep.agent_id,
            value: ep.content,
            created_at: ep.created_at,
            expires_at: ep.expires_at,
            score: None,
        }));
    }
    if let Ok(facts) = SemanticMemory::new(store).recall_all(namespace, None) {
        entries.extend(facts.into_iter().map(|fact| MemoryEntryView {
            id: fact.id,
            entry_type: "semantic".to_owned(),
            key: fact.key,
            value: serde_json::to_string(&fact.value).unwrap_or_default(),
            created_at: fact.created_at,
            expires_at: fact.expires_at,
            score: None,
        }));
    }
    if let Ok(procedures) = ProceduralMemory::new(store).list(namespace) {
        entries.extend(procedures.into_iter().map(|proc| MemoryEntryView {
            id: proc.id,
            entry_type: "procedural".to_owned(),
            key: proc.trigger,
            value: proc.steps.join(" -> "),
            created_at: proc.created_at,
            expires_at: None,
            score: None,
        }));
    }
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries
}

/// Full-text search of `query` in `namespace`, best BM25 score first. A
/// namespace without a file has no result.
///
/// # Errors
///
/// [`MemoryViewError::InvalidNamespace`], [`MemoryViewError::Open`] or
/// [`MemoryViewError::Search`].
pub async fn search(
    base: PathBuf,
    namespace: String,
    query: String,
) -> Result<Vec<MemorySearchResultView>, MemoryViewError> {
    tokio::task::spawn_blocking(move || {
        let db_path = namespace_file(&base, &namespace)?;
        if !db_path.exists() {
            return Ok(Vec::new());
        }
        let store = MemoryStore::open(&db_path).map_err(MemoryViewError::Open)?;
        let results = MemorySearch::new(&store)
            .query(SearchQuery {
                namespace: &namespace,
                query: &query,
                limit: SEARCH_LIMIT,
                sources: None,
                min_importance: None,
            })
            .map_err(MemoryViewError::Search)?;
        Ok(results
            .into_iter()
            .map(|r| MemorySearchResultView {
                id: r.source_id,
                entry_type: match r.source {
                    SearchSource::Episodic => "episodic".to_owned(),
                    SearchSource::Semantic => "semantic".to_owned(),
                },
                content: r.content,
                score: r.score,
                relevance: r.relevance,
                created_at: r.created_at,
            })
            .collect())
    })
    .await
    .map_err(MemoryViewError::Join)?
}

/// The entries an agent of this process recalled during `turn_id`, read from
/// the injection tracker the agent bridge fills. A turn never tracked has none.
pub fn injected_entries(turn_id: &str) -> Vec<InjectedMemoryEntryView> {
    global_entries_for(turn_id)
        .into_iter()
        .map(InjectedMemoryEntryView::from)
        .collect()
}

#[cfg(test)]
mod tests;
