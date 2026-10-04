//! The vectors of the memory: what lets a search find an entry by its meaning
//! and not only by its words.
//!
//! Each episodic or semantic entry may carry one vector in the
//! `memory_vectors` table of its namespace database, computed by an embeddings
//! model. The memory does not compute vectors itself: it states what it needs
//! in [`MemoryEmbedder`], and the runtime, which owns the local engine,
//! implements it. Vectors are not computed on the write path: a background pass
//! ([`vectorize_dir`]) fills in every entry that has none, or one from another
//! model or of another dimension, so an install or a change of model is caught
//! up the same way as a new entry.
//!
//! A search ([`crate::search::MemorySearch::query`]) given a [`QueryVector`]
//! ranks the entries both by BM25 and by cosine similarity and fuses the two
//! rankings by reciprocal rank; without one, it ranks by words alone.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use apollia_core::vector_search::{decode_vector, dot, encode_vector};
use rusqlite::params;

use crate::search::SearchSource;
use crate::store::{MemoryStore, MemoryStoreError};

/// Entries embedded in one request of a background pass.
pub const VECTORIZE_BATCH: usize = 32;

/// Characters of an entry sent to the model at most. The model reads a bounded
/// number of tokens; a longer entry is embedded by its beginning rather than
/// failing the whole batch it sits in.
pub const MAX_EMBEDDED_CHARS: usize = 2_000;

/// The embeddings model that computes the vectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingsModel {
    /// Identifier of the model, stored beside each vector it computed.
    pub id: String,
    /// Length of the vectors it produces.
    pub dimensions: u32,
}

/// What a text is embedded as: some models prefix a query and a passage
/// differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedRole {
    /// A search query.
    Query,
    /// An entry of the memory.
    Passage,
}

/// Vectors computed by one request, with the model that computed them.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedded {
    /// The model that answered.
    pub model: EmbeddingsModel,
    /// One vector of norm one per text, in order.
    pub vectors: Vec<Vec<f32>>,
}

/// Why vectors could not be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmbedError {
    /// This machine has nothing to embed with (no model chosen or installed,
    /// no engine): the memory is searched by its words alone.
    #[error("no embeddings model is available: {0}")]
    Unavailable(String),
    /// A model exists and failed.
    #[error("the embeddings model failed: {0}")]
    Failed(String),
}

/// The future an [`MemoryEmbedder`] returns.
///
/// REASON: boxed rather than `impl Future` so that the trait stays
/// dyn-compatible: the memory crate cannot name the runtime's embedder, and
/// receives it as `Arc<dyn MemoryEmbedder>`.
pub type EmbedFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, EmbedError>> + Send + 'a>>;

/// Computes the vectors of the memory. Implemented by the runtime.
pub trait MemoryEmbedder: Send + Sync {
    /// The model that would embed now.
    ///
    /// # Errors
    /// [`EmbedError::Unavailable`] when there is none.
    fn model(&self) -> EmbedFuture<'_, EmbeddingsModel>;

    /// One vector of norm one per text, in order.
    ///
    /// # Errors
    /// [`EmbedError::Unavailable`] when there is no model, and
    /// [`EmbedError::Failed`] when the model failed.
    fn embed<'a>(&'a self, texts: &'a [String], role: EmbedRole) -> EmbedFuture<'a, Embedded>;
}

/// The embedder of this process, installed once by the runtime at boot.
///
/// REASON: a set-once handle, not shared mutable state. The embedder reads its
/// setting at every call, so a change of model needs no reinstall; every
/// reader of the memory in the process (the agent bridge, the `memory_search`
/// tool, the Memory screen) ranks alike without each runner threading a handle
/// it could not build before the engine supervisor exists.
static INSTALLED: OnceLock<Arc<dyn MemoryEmbedder>> = OnceLock::new();

/// Installs the embedder of this process. Returns `false` when one already is,
/// which leaves the first in place.
pub fn install_embedder(embedder: Arc<dyn MemoryEmbedder>) -> bool {
    INSTALLED.set(embedder).is_ok()
}

/// The embedder of this process, `None` outside a runtime.
#[must_use]
pub fn installed_embedder() -> Option<Arc<dyn MemoryEmbedder>> {
    INSTALLED.get().cloned()
}

/// The vector of a search query, with the model that computed it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryVector {
    /// Identifier of the model; only vectors of the same model are compared.
    pub model: String,
    /// The vector, of norm one.
    pub vector: Vec<f32>,
}

/// The vector of `query`, or `None` when the search goes on by words alone:
/// no embedder, no model on this machine, or a model that failed. A memory
/// search never fails for want of a vector; the failure is logged.
pub async fn query_vector(
    embedder: Option<&dyn MemoryEmbedder>,
    query: &str,
) -> Option<QueryVector> {
    let embedder = embedder?;
    let texts = [query.trim().to_owned()];
    match embedder.embed(&texts, EmbedRole::Query).await {
        Ok(Embedded { model, vectors }) => {
            let vector = vectors.into_iter().next()?;
            (vector.len() == model.dimensions as usize).then_some(QueryVector {
                model: model.id,
                vector,
            })
        }
        Err(EmbedError::Unavailable(reason)) => {
            tracing::event!(tracing::Level::DEBUG, reason = %reason, "memory.search.keywords_only");
            None
        }
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "memory.search.embedding_failed");
            None
        }
    }
}

/// An entry whose vector is missing or stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEntry {
    /// Its table.
    pub source: SearchSource,
    /// Its identifier.
    pub id: String,
    /// The text its vector is computed from: the content of an episode, the
    /// key and value of a fact, as the full-text index holds them.
    pub text: String,
}

fn table_name(source: SearchSource) -> &'static str {
    match source {
        SearchSource::Episodic => "episodic",
        SearchSource::Semantic => "semantic",
    }
}

fn source_of(table: &str) -> Option<SearchSource> {
    match table {
        "episodic" => Some(SearchSource::Episodic),
        "semantic" => Some(SearchSource::Semantic),
        _ => None,
    }
}

/// Up to `limit` entries of `store` with no vector of `model`: none at all, one
/// of another model, or one of another dimension.
///
/// # Errors
/// [`MemoryStoreError::Sqlite`] when the query fails.
pub fn pending_entries(
    store: &MemoryStore,
    model: &EmbeddingsModel,
    limit: usize,
) -> Result<Vec<PendingEntry>, MemoryStoreError> {
    let sql = "\
        SELECT 'episodic', e.id, e.content FROM episodic_memories e \
        LEFT JOIN memory_vectors v ON v.source_table = 'episodic' AND v.source_id = e.id \
        WHERE v.source_id IS NULL OR v.model <> ?1 OR v.dimension <> ?2 \
        UNION ALL \
        SELECT 'semantic', s.id, s.key || ' ' || s.value FROM semantic_memories s \
        LEFT JOIN memory_vectors v ON v.source_table = 'semantic' AND v.source_id = s.id \
        WHERE v.source_id IS NULL OR v.model <> ?1 OR v.dimension <> ?2 \
        LIMIT ?3";
    let mut stmt = store.conn().prepare(sql)?;
    let rows = stmt.query_map(params![model.id, model.dimensions, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut pending = Vec::new();
    for row in rows {
        let (table, id, text) = row?;
        if let Some(source) = source_of(&table) {
            pending.push(PendingEntry { source, id, text });
        }
    }
    Ok(pending)
}

/// Stores the vector of each entry, replacing any older one. A vector whose
/// length is not `model.dimensions` is skipped. Returns how many were stored.
///
/// # Errors
/// [`MemoryStoreError::Sqlite`] when a write fails.
pub fn save_vectors(
    store: &MemoryStore,
    model: &EmbeddingsModel,
    entries: &[(PendingEntry, Vec<f32>)],
) -> Result<usize, MemoryStoreError> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;
    let mut saved = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO memory_vectors (source_table, source_id, model, dimension, vector) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (entry, vector) in entries {
            if vector.len() != model.dimensions as usize {
                continue;
            }
            saved += stmt.execute(params![
                table_name(entry.source),
                entry.id,
                model.id,
                model.dimensions,
                encode_vector(vector),
            ])?;
        }
    }
    tx.commit()?;
    Ok(saved)
}

/// The filters of a ranking by meaning.
pub(crate) struct MeaningFilter<'q> {
    pub(crate) namespace: &'q str,
    pub(crate) episodic: bool,
    pub(crate) semantic: bool,
    pub(crate) min_importance: f64,
    pub(crate) candidates: usize,
}

/// The entries of `filter.namespace` closest to `query`, best first: an
/// exhaustive scan of the vectors of the query's model, compared by dot
/// product (both sides have a norm of one).
pub(crate) fn rank_by_meaning(
    conn: &rusqlite::Connection,
    query: &QueryVector,
    filter: &MeaningFilter<'_>,
) -> Result<Vec<(SearchSource, String)>, rusqlite::Error> {
    let mut scored: Vec<(f32, SearchSource, String)> = Vec::new();
    let tables: [(bool, SearchSource, &str); 2] = [
        (
            filter.episodic,
            SearchSource::Episodic,
            "SELECT v.source_id, v.vector FROM memory_vectors v \
             JOIN episodic_memories e ON e.id = v.source_id \
             WHERE v.source_table = 'episodic' AND v.model = ?1 AND v.dimension = ?2 \
             AND e.namespace = ?3 AND e.importance >= ?4",
        ),
        (
            filter.semantic,
            SearchSource::Semantic,
            "SELECT v.source_id, v.vector FROM memory_vectors v \
             JOIN semantic_memories s ON s.id = v.source_id \
             WHERE v.source_table = 'semantic' AND v.model = ?1 AND v.dimension = ?2 \
             AND s.namespace = ?3 AND s.confidence >= ?4",
        ),
    ];
    for (wanted, source, sql) in tables {
        if !wanted {
            continue;
        }
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(
            params![
                query.model,
                query.vector.len() as i64,
                filter.namespace,
                filter.min_importance
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;
        for row in rows {
            let (id, bytes) = row?;
            let Ok(vector) = decode_vector(&bytes) else {
                continue;
            };
            scored.push((dot(&query.vector, &vector), source, id));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(filter.candidates);
    Ok(scored
        .into_iter()
        .map(|(_, source, id)| (source, id))
        .collect())
}

/// Why a background pass stopped.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VectorizeError {
    /// The namespace database could not be read or written.
    #[error("memory database {path}: {source}")]
    Store {
        /// The database.
        path: PathBuf,
        /// The cause.
        #[source]
        source: MemoryStoreError,
    },
    /// The embeddings model failed.
    #[error(transparent)]
    Embed(#[from] EmbedError),
    /// The blocking task of a database step did not complete.
    #[error("a database step did not complete: {0}")]
    Join(String),
    /// The model changed or answered vectors of another shape mid-pass.
    #[error("the embeddings model answered {got} for {expected}")]
    ModelChanged {
        /// The model the pass started with.
        expected: String,
        /// What answered.
        got: String,
    },
}

fn excerpt(text: &str) -> String {
    match text.char_indices().nth(MAX_EMBEDDED_CHARS) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

/// Runs `step` on `store` off the async workers and hands the store back.
async fn on_store<T, F>(
    store: MemoryStore,
    path: &Path,
    step: F,
) -> Result<(MemoryStore, T), VectorizeError>
where
    T: Send + 'static,
    F: FnOnce(&MemoryStore) -> Result<T, MemoryStoreError> + Send + 'static,
{
    let (store, outcome) = tokio::task::spawn_blocking(move || {
        let outcome = step(&store);
        (store, outcome)
    })
    .await
    .map_err(|e| VectorizeError::Join(e.to_string()))?;
    let value = outcome.map_err(|source| VectorizeError::Store {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((store, value))
}

/// Computes the missing vectors of the namespace database `path` with
/// `model`, batch by batch, until none is missing. Returns how many were
/// stored.
///
/// # Errors
/// [`VectorizeError`] when the database or the model fails, or when the model
/// answering is no longer `model`.
pub async fn vectorize_database(
    path: &Path,
    embedder: &dyn MemoryEmbedder,
    model: &EmbeddingsModel,
) -> Result<usize, VectorizeError> {
    let owned = path.to_path_buf();
    let store = tokio::task::spawn_blocking(move || MemoryStore::open(&owned))
        .await
        .map_err(|e| VectorizeError::Join(e.to_string()))?
        .map_err(|source| VectorizeError::Store {
            path: path.to_path_buf(),
            source,
        })?;
    let mut store = store;
    let mut stored = 0;
    loop {
        let wanted = model.clone();
        let (back, pending) = on_store(store, path, move |s| {
            pending_entries(s, &wanted, VECTORIZE_BATCH)
        })
        .await?;
        store = back;
        if pending.is_empty() {
            break;
        }
        let texts: Vec<String> = pending.iter().map(|p| excerpt(&p.text)).collect();
        let embedded = embedder.embed(&texts, EmbedRole::Passage).await?;
        if embedded.model != *model || embedded.vectors.len() != pending.len() {
            return Err(VectorizeError::ModelChanged {
                expected: format!("{} ({}-d)", model.id, model.dimensions),
                got: format!(
                    "{} ({}-d, {} vectors)",
                    embedded.model.id,
                    embedded.model.dimensions,
                    embedded.vectors.len()
                ),
            });
        }
        let entries: Vec<(PendingEntry, Vec<f32>)> =
            pending.into_iter().zip(embedded.vectors).collect();
        let wanted = model.clone();
        let (back, saved) =
            on_store(store, path, move |s| save_vectors(s, &wanted, &entries)).await?;
        store = back;
        if saved == 0 {
            // Every vector had the wrong length: stop rather than ask again.
            break;
        }
        stored += saved;
    }
    Ok(stored)
}

/// One background pass over every namespace database of `dir`: each entry
/// without a vector of the current model gets one. Without a model the pass
/// does nothing. A database that fails is logged and the pass goes on with
/// the next. Returns how many vectors were stored.
pub async fn vectorize_dir(dir: &Path, embedder: &dyn MemoryEmbedder) -> usize {
    let model = match embedder.model().await {
        Ok(model) => model,
        Err(EmbedError::Unavailable(_)) => return 0,
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "memory.vectors.model_failed");
            return 0;
        }
    };
    let owned = dir.to_path_buf();
    let databases = tokio::task::spawn_blocking(move || namespace_databases(&owned))
        .await
        .unwrap_or_default();
    let mut total = 0;
    for path in databases {
        match vectorize_database(&path, embedder, &model).await {
            Ok(0) => {}
            Ok(stored) => {
                tracing::event!(
                    tracing::Level::INFO,
                    path = %path.display(),
                    model = %model.id,
                    stored,
                    "memory.vectors.stored"
                );
                total += stored;
            }
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %path.display(),
                    error = %e,
                    "memory.vectors.failed"
                );
            }
        }
    }
    total
}

/// The `.db` files directly under `dir`, sorted; none when it cannot be read.
fn namespace_databases(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "db"))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests;
