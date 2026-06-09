//! MemorySearch: FTS5 search with BM25 ranking.
//!
//! Queries the `memory_fts` table and joins the source tables
//! (episodic/semantic) to enrich results with namespace,
//! importance/confidence and dates.
//! The `unicode61` tokenizer normalises accents (local-first).

use crate::store::MemoryStore;

/// FTS5 search engine with BM25 ranking.
///
/// Queries the `memory_fts` table and joins the source tables
/// (episodic/semantic) to enrich results.
pub struct MemorySearch<'a> {
    store: &'a MemoryStore,
}

/// A memory search result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    /// BM25 score (higher = more relevant).
    pub score: f64,
    /// Source table of the result.
    pub source: SearchSource,
    /// ID of the entry in the source table.
    pub source_id: String,
    /// Textual content of the result.
    pub content: String,
    /// Importance (episodic) or confidence (semantic). None when not applicable.
    pub relevance: Option<f64>,
    /// ISO 8601 creation date.
    pub created_at: String,
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchSource {
    /// Episodic memory (event journal).
    Episodic,
    /// Semantic memory (knowledge base).
    Semantic,
}

/// Search errors.
#[derive(Debug, thiserror::Error)]
pub enum MemorySearchError {
    /// The FTS5 query failed.
    #[error("search query failed: {0}")]
    QueryFailed(String),

    /// The search query is empty.
    #[error("empty search query")]
    EmptyQuery,

    /// SQLite error propagated from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Parameters for a memory search run by [`MemorySearch::query`].
///
/// - `namespace`: filter by namespace
/// - `query`: FTS5 query (keywords)
/// - `limit`: max number of results
/// - `sources`: filter by type (None = all)
/// - `min_importance`: importance/confidence threshold (None = no filter)
pub struct SearchQuery<'q> {
    /// Filter by namespace.
    pub namespace: &'q str,
    /// FTS5 query (keywords).
    pub query: &'q str,
    /// Max number of results.
    pub limit: u32,
    /// Filter by type (None = all).
    pub sources: Option<&'q [SearchSource]>,
    /// Importance/confidence threshold (None = no filter).
    pub min_importance: Option<f64>,
}

/// FTS5 operators and special tokens that must be escaped in user queries.
const FTS5_SPECIAL_TOKENS: &[&str] = &["AND", "OR", "NOT", "NEAR"];

/// Escapes a user query for safe use in FTS5 MATCH.
///
/// Wraps each word in double quotes so that FTS5 operators and special
/// characters (`*`, `:`, `^`) are treated as literals.
fn escape_fts5_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|word| {
            let needs_quoting = word.contains('"')
                || word.contains('*')
                || word.contains(':')
                || word.contains('^')
                || word.contains('(')
                || word.contains(')')
                || FTS5_SPECIAL_TOKENS.contains(&word.to_uppercase().as_str());

            if needs_quoting {
                let escaped = word.replace('"', "\"\"");
                format!("\"{escaped}\"")
            } else {
                format!("\"{word}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parameters shared by the per-source-table FTS5 queries.
///
/// Groups the common arguments of [`MemorySearch::query_episodic`] and
/// [`MemorySearch::query_semantic`] to keep their signatures concise.
struct FtsQueryParams<'q> {
    conn: &'q rusqlite::Connection,
    escaped_query: &'q str,
    namespace: &'q str,
    limit: u32,
    min_importance: Option<f64>,
}

impl<'a> MemorySearch<'a> {
    /// Create a search engine bound to a [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Search memory by keywords with BM25 ranking.
    ///
    /// Parameters are grouped in [`SearchQuery`].
    pub fn query(&self, search: SearchQuery<'_>) -> Result<Vec<SearchResult>, MemorySearchError> {
        let SearchQuery {
            namespace,
            query,
            limit,
            sources,
            min_importance,
        } = search;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(MemorySearchError::EmptyQuery);
        }

        let escaped = escape_fts5_query(trimmed);

        let want_episodic = sources
            .map(|s| s.contains(&SearchSource::Episodic))
            .unwrap_or(true);
        let want_semantic = sources
            .map(|s| s.contains(&SearchSource::Semantic))
            .unwrap_or(true);

        let conn = self.store.conn();
        let params = FtsQueryParams {
            conn,
            escaped_query: &escaped,
            namespace,
            limit,
            min_importance,
        };
        let mut results = Vec::new();

        if want_episodic {
            self.query_episodic(&params, &mut results)?;
        }

        if want_semantic {
            self.query_semantic(&params, &mut results)?;
        }

        // Sort by score descending (higher = more relevant).
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit as usize);

        Ok(results)
    }

    /// Queries episodic memories via FTS5 JOIN.
    fn query_episodic(
        &self,
        params: &FtsQueryParams,
        results: &mut Vec<SearchResult>,
    ) -> Result<(), MemorySearchError> {
        let FtsQueryParams {
            conn,
            escaped_query,
            namespace,
            limit,
            min_importance,
        } = *params;
        let min_imp = min_importance.unwrap_or(0.0);

        let sql = "\
            SELECT f.content, f.source_id, -f.rank AS score, e.importance, e.created_at \
            FROM memory_fts f \
            JOIN episodic_memories e ON f.source_id = e.id \
            WHERE memory_fts MATCH ?1 \
            AND f.source_table = 'episodic' \
            AND e.namespace = ?2 \
            AND e.importance >= ?3 \
            ORDER BY f.rank \
            LIMIT ?4";

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?;

        let rows = stmt
            .query_map(
                rusqlite::params![escaped_query, namespace, min_imp, limit],
                |row| {
                    Ok(SearchResult {
                        content: row.get(0)?,
                        source_id: row.get(1)?,
                        score: row.get(2)?,
                        relevance: Some(row.get::<_, f64>(3)?),
                        created_at: row.get(4)?,
                        source: SearchSource::Episodic,
                    })
                },
            )
            .map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?;

        for row in rows {
            results.push(row.map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?);
        }

        Ok(())
    }

    /// Queries semantic memories via FTS5 JOIN.
    fn query_semantic(
        &self,
        params: &FtsQueryParams,
        results: &mut Vec<SearchResult>,
    ) -> Result<(), MemorySearchError> {
        let FtsQueryParams {
            conn,
            escaped_query,
            namespace,
            limit,
            min_importance,
        } = *params;
        let min_conf = min_importance.unwrap_or(0.0);

        let sql = "\
            SELECT f.content, f.source_id, -f.rank AS score, s.confidence, s.created_at \
            FROM memory_fts f \
            JOIN semantic_memories s ON f.source_id = s.id \
            WHERE memory_fts MATCH ?1 \
            AND f.source_table = 'semantic' \
            AND s.namespace = ?2 \
            AND s.confidence >= ?3 \
            ORDER BY f.rank \
            LIMIT ?4";

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?;

        let rows = stmt
            .query_map(
                rusqlite::params![escaped_query, namespace, min_conf, limit],
                |row| {
                    Ok(SearchResult {
                        content: row.get(0)?,
                        source_id: row.get(1)?,
                        score: row.get(2)?,
                        relevance: Some(row.get::<_, f64>(3)?),
                        created_at: row.get(4)?,
                        source: SearchSource::Semantic,
                    })
                },
            )
            .map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?;

        for row in rows {
            results.push(row.map_err(|e| MemorySearchError::QueryFailed(e.to_string()))?);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episodic::EpisodicMemory;
    use crate::semantic::{RememberInput, SemanticMemory};
    use crate::store::MemoryStore;
    use serde_json::json;

    fn setup_with_data() -> MemoryStore {
        let dir = std::env::temp_dir().join(format!("apollia_search_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();

        let ep = EpisodicMemory::new(&store);
        ep.record(
            "ns",
            "agent-1",
            "Devis envoye a Dupont SA pour 5000 euros",
            0.8,
            Some("t1"),
            None,
            None,
        )
        .unwrap();
        ep.record(
            "ns",
            "agent-1",
            "Reunion planifiee avec Martin SAS",
            0.5,
            None,
            None,
            None,
        )
        .unwrap();
        ep.record(
            "ns",
            "agent-1",
            "Devis refuse par Dupont SA budget insuffisant",
            0.9,
            None,
            None,
            None,
        )
        .unwrap();

        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "client.dupont.budget_max",
            value: &json!(15000),
            confidence: 1.0,
            source: Some("crm"),
            expires_at: None,
        })
        .unwrap();

        store
    }

    // Search returns BM25-ranked results
    #[test]
    fn test_search_returns_bm25_ranked_results() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "devis Dupont",
                limit: 3,
                sources: None,
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }

    // Filter by episodic source only
    #[test]
    fn test_filter_by_episodic_source() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "Dupont",
                limit: 10,
                sources: Some(&[SearchSource::Episodic]),
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.source, SearchSource::Episodic);
        }
    }

    // Filter by semantic source only
    #[test]
    fn test_filter_by_semantic_source() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "dupont",
                limit: 10,
                sources: Some(&[SearchSource::Semantic]),
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.source, SearchSource::Semantic);
        }
    }

    // Filter by minimum importance
    #[test]
    fn test_filter_by_min_importance() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "Dupont",
                limit: 10,
                sources: None,
                min_importance: Some(0.7),
            })
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        for r in &results {
            if let Some(rel) = r.relevance {
                assert!(rel >= 0.7);
            }
        }
    }

    // Unicode61 normalises accents
    #[test]
    fn test_unicode61_accent_normalization() {
        // GIVEN
        let dir = std::env::temp_dir().join(format!("apollia_s_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = MemoryStore::open(&dir.join("test.db")).unwrap();
        let ep = EpisodicMemory::new(&store);
        ep.record(
            "ns",
            "a",
            "reunion avec societe Dupont",
            0.5,
            None,
            None,
            None,
        )
        .unwrap();
        let search = MemorySearch::new(&store);
        // WHEN: search without accents
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "reunion societe",
                limit: 10,
                sources: None,
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(!results.is_empty());
    }

    // No results returns empty vec
    #[test]
    fn test_no_results_returns_empty_vec() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "ns",
                query: "xyznonexistent",
                limit: 10,
                sources: None,
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(results.is_empty());
    }

    // Empty query returns EmptyQuery error
    #[test]
    fn test_empty_query_returns_error() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let result = search.query(SearchQuery {
            namespace: "ns",
            query: "",
            limit: 10,
            sources: None,
            min_importance: None,
        });
        // THEN
        assert!(matches!(result, Err(MemorySearchError::EmptyQuery)));
    }

    // Whitespace-only query returns EmptyQuery error
    #[test]
    fn test_whitespace_only_query_returns_error() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let result = search.query(SearchQuery {
            namespace: "ns",
            query: "   ",
            limit: 10,
            sources: None,
            min_importance: None,
        });
        // THEN
        assert!(matches!(result, Err(MemorySearchError::EmptyQuery)));
    }

    // FTS5 special characters are escaped safely
    #[test]
    fn test_fts5_special_chars_escaped() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN: query with FTS5 operators should not crash
        let result = search.query(SearchQuery {
            namespace: "ns",
            query: "NOT AND OR devis*",
            limit: 10,
            sources: None,
            min_importance: None,
        });
        // THEN: should not error (may return empty)
        assert!(result.is_ok());
    }

    // Namespace isolation: search in wrong namespace returns empty
    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query(SearchQuery {
                namespace: "other-ns",
                query: "Dupont",
                limit: 10,
                sources: None,
                min_importance: None,
            })
            .unwrap();
        // THEN
        assert!(results.is_empty());
    }
}
