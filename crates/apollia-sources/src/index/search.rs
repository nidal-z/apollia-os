//! Searching a source index by its words and by their meaning.
//!
//! Two rankings are drawn: the chunks whose words match the query, ordered by
//! BM25 (FTS5), and the chunks whose vector is closest to the query's, by
//! cosine similarity over every stored vector (an exhaustive scan). Their
//! scores live on unrelated scales, so they are merged by reciprocal rank
//! ([`apollia_core::vector_search::reciprocal_rank_fusion`], k = 60).
//!
//! Without a query vector (no embeddings model on this machine, or an index
//! built without one) the search is by words alone, and says so in
//! [`SearchOutcome::mode`].

use std::future::Future;

use apollia_core::vector_search::{decode_vector, dot, reciprocal_rank_fusion, RRF_K};
use apollia_llm::embeddings_client::EmbeddingError;
use rusqlite::params;
use serde::Serialize;

use super::{IndexError, SourceIndex};
use crate::chunk::Location;

/// Results returned when the caller names no limit.
pub const DEFAULT_LIMIT: usize = 10;

/// Most results one search returns.
pub const MAX_LIMIT: usize = 50;

/// Candidates drawn from each ranking before the fusion, raised to the limit
/// asked when it is larger, so that a search for fifty can return fifty.
pub const CANDIDATES_PER_RANKING: usize = 20;

/// How a search ranked its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// By words and by meaning, fused.
    Hybrid,
    /// By words alone: no vector could be compared.
    KeywordsOnly,
}

/// One passage found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    /// The passage.
    pub excerpt: String,
    /// Path of the document it comes from.
    pub document: String,
    /// Where it sits in the document.
    pub location: Location,
    /// Its fused reciprocal-rank score; higher ranks first.
    pub score: f64,
    /// The source searched.
    pub source: String,
}

/// The answer of a search.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchOutcome {
    /// How the results were ranked.
    pub mode: SearchMode,
    /// The passages, best first.
    pub hits: Vec<SearchHit>,
}

/// The vector of a query, with the model that computed it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryVector {
    /// Identifier of the embeddings model.
    pub model: String,
    /// The vector, with a norm of one.
    pub vector: Vec<f32>,
}

/// Computes the vector of a search query.
pub trait QueryEmbedder {
    /// The vector of `query`, embedded as a query, with the model's
    /// identifier.
    fn embed_query(
        &self,
        query: &str,
    ) -> impl Future<Output = Result<QueryVector, EmbeddingError>> + Send;
}

/// The vector of `query`, or `None` when this machine has nothing to embed it
/// with ([`EmbeddingError::falls_back_to_keywords`]): the search then goes on
/// with keywords alone.
///
/// # Errors
/// Any other [`EmbeddingError`]: an engine that exists and failed is
/// reported, not hidden behind a weaker search.
pub async fn query_vector<E: QueryEmbedder>(
    embedder: &E,
    query: &str,
) -> Result<Option<QueryVector>, EmbeddingError> {
    match embedder.embed_query(query).await {
        Ok(vector) => Ok(Some(vector)),
        Err(e) if e.falls_back_to_keywords() => {
            tracing::event!(
                tracing::Level::INFO,
                reason = %e,
                "sources.search.keywords_only"
            );
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// The FTS5 expression matching any word of `query`: each word quoted, so no
/// character of the query is read as FTS5 syntax. `None` without a word.
fn match_expression(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

impl SourceIndex {
    /// Searches the index for `query`.
    ///
    /// `vector` is the query's vector ([`query_vector`]); `None`, or a vector
    /// of another model than the index's, searches by words alone. `limit`
    /// defaults to [`DEFAULT_LIMIT`] and is held between one and
    /// [`MAX_LIMIT`].
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure, [`IndexError::Location`] or
    /// [`IndexError::Vector`] for a stored row that cannot be read.
    pub fn search(
        &self,
        query: &str,
        vector: Option<&QueryVector>,
        limit: Option<usize>,
    ) -> Result<SearchOutcome, IndexError> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let candidates = CANDIDATES_PER_RANKING.max(limit);
        let keywords = self.keyword_ranking(query, candidates)?;
        let comparable = match vector {
            Some(v) => self.embedding_model()?.is_some_and(|m| m == v.model),
            None => false,
        };
        let (mode, rankings) = match vector.filter(|_| comparable) {
            Some(v) => (
                SearchMode::Hybrid,
                vec![keywords, self.meaning_ranking(&v.vector, candidates)?],
            ),
            None => (SearchMode::KeywordsOnly, vec![keywords]),
        };
        let mut fused = reciprocal_rank_fusion(&rankings, RRF_K);
        fused.truncate(limit);
        let hits = self.hits(&fused)?;
        tracing::event!(
            tracing::Level::DEBUG,
            source = %self.source_id,
            mode = ?mode,
            hits = hits.len(),
            "sources.search.done"
        );
        Ok(SearchOutcome { mode, hits })
    }

    /// Chunk ids matching a word of `query`, best BM25 first.
    fn keyword_ranking(&self, query: &str, candidates: usize) -> Result<Vec<i64>, IndexError> {
        let Some(expression) = match_expression(query) else {
            return Ok(Vec::new());
        };
        let mut statement = self.conn.prepare_cached(
            "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let ids = statement
            .query_map(
                params![expression, i64::try_from(candidates).unwrap_or(i64::MAX)],
                |row| row.get(0),
            )?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    }

    /// Chunk ids by descending cosine with `vector`, over every stored vector.
    fn meaning_ranking(&self, vector: &[f32], candidates: usize) -> Result<Vec<i64>, IndexError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, vector FROM chunks WHERE vector IS NOT NULL")?;
        let mut rows = statement.query([])?;
        let mut scored: Vec<(i64, f32)> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let blob = row.get_ref(1)?.as_blob().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Blob,
                    Box::new(e),
                )
            })?;
            scored.push((id, dot(&decode_vector(blob)?, vector)));
        }
        let by_score = |a: &(i64, f32), b: &(i64, f32)| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
        if scored.len() > candidates {
            scored.select_nth_unstable_by(candidates, by_score);
            scored.truncate(candidates);
        }
        scored.sort_by(by_score);
        Ok(scored.into_iter().map(|(id, _)| id).collect())
    }

    /// The rows of the fused chunk ids, in fused order.
    fn hits(&self, fused: &[(i64, f64)]) -> Result<Vec<SearchHit>, IndexError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT c.text, c.location, d.path
             FROM chunks c JOIN documents d ON d.id = c.document_id
             WHERE c.id = ?1",
        )?;
        let mut hits = Vec::with_capacity(fused.len());
        for (id, score) in fused {
            let (excerpt, location, document): (String, String, String) = statement
                .query_row(params![id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            hits.push(SearchHit {
                excerpt,
                document,
                location: serde_json::from_str(&location)?,
                score: *score,
                source: self.source_id.clone(),
            });
        }
        Ok(hits)
    }
}
