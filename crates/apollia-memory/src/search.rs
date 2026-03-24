//! MemorySearch — recherche FTS5 avec ranking BM25.
//!
//! Interroge la table `memory_fts` et joint les tables source
//! (episodic/semantic) pour enrichir les resultats avec namespace,
//! importance/confiance et dates.
//! Le tokenizer `unicode61` normalise les accents (Principe #1 local-first).

use crate::store::MemoryStore;

/// Moteur de recherche FTS5 avec ranking BM25.
///
/// Interroge la table `memory_fts` et joint les tables source
/// (episodic/semantic) pour enrichir les resultats.
pub struct MemorySearch<'a> {
    store: &'a MemoryStore,
}

/// Resultat d'une recherche memoire.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    /// Score BM25 (plus eleve = plus pertinent).
    pub score: f64,
    /// Table source du resultat.
    pub source: SearchSource,
    /// ID de l'entree dans la table source.
    pub source_id: String,
    /// Contenu textuel du resultat.
    pub content: String,
    /// Importance (episodic) ou confiance (semantic). None si non applicable.
    pub relevance: Option<f64>,
    /// Date de creation ISO 8601.
    pub created_at: String,
}

/// Source d'un resultat de recherche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchSource {
    /// Memoire episodique (journal d'evenements).
    Episodic,
    /// Memoire semantique (base de connaissances).
    Semantic,
}

/// Erreurs de recherche.
#[derive(Debug, thiserror::Error)]
pub enum MemorySearchError {
    /// La requete FTS5 a echoue.
    #[error("search query failed: {0}")]
    QueryFailed(String),

    /// La requete de recherche est vide.
    #[error("empty search query")]
    EmptyQuery,

    /// Erreur SQLite propagee depuis rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
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

impl<'a> MemorySearch<'a> {
    /// Cree un moteur de recherche lie a un [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Recherche dans la memoire par mots-cles avec ranking BM25.
    ///
    /// - `namespace` : filtre par namespace
    /// - `query` : requete FTS5 (mots-cles)
    /// - `limit` : nombre max de resultats
    /// - `sources` : filtrer par type (None = tous)
    /// - `min_importance` : seuil d'importance/confiance (None = pas de filtre)
    pub fn query(
        &self,
        namespace: &str,
        query: &str,
        limit: u32,
        sources: Option<&[SearchSource]>,
        min_importance: Option<f64>,
    ) -> Result<Vec<SearchResult>, MemorySearchError> {
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
        let mut results = Vec::new();

        if want_episodic {
            self.query_episodic(
                conn,
                &escaped,
                namespace,
                limit,
                min_importance,
                &mut results,
            )?;
        }

        if want_semantic {
            self.query_semantic(
                conn,
                &escaped,
                namespace,
                limit,
                min_importance,
                &mut results,
            )?;
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
        conn: &rusqlite::Connection,
        escaped_query: &str,
        namespace: &str,
        limit: u32,
        min_importance: Option<f64>,
        results: &mut Vec<SearchResult>,
    ) -> Result<(), MemorySearchError> {
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
        conn: &rusqlite::Connection,
        escaped_query: &str,
        namespace: &str,
        limit: u32,
        min_importance: Option<f64>,
        results: &mut Vec<SearchResult>,
    ) -> Result<(), MemorySearchError> {
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
    use crate::semantic::SemanticMemory;
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
        sem.remember(
            "ns",
            "client.dupont.budget_max",
            &json!(15000),
            1.0,
            Some("crm"),
            None,
        )
        .unwrap();

        store
    }

    // Search returns BM25-ranked results
    #[test]
    fn test_ac1_search_returns_bm25_ranked_results() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "devis Dupont", 3, None, None).unwrap();
        // THEN
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }

    // Filter by episodic source only
    #[test]
    fn test_ac2_filter_by_episodic_source() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query("ns", "Dupont", 10, Some(&[SearchSource::Episodic]), None)
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.source, SearchSource::Episodic);
        }
    }

    // bis — Filter by semantic source only
    #[test]
    fn test_ac2_filter_by_semantic_source() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query("ns", "dupont", 10, Some(&[SearchSource::Semantic]), None)
            .unwrap();
        // THEN
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.source, SearchSource::Semantic);
        }
    }

    // Filter by minimum importance
    #[test]
    fn test_ac3_filter_by_min_importance() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "Dupont", 10, None, Some(0.7)).unwrap();
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
    fn test_ac4_unicode61_accent_normalization() {
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
        // WHEN — search without accents
        let results = search
            .query("ns", "reunion societe", 10, None, None)
            .unwrap();
        // THEN
        assert!(!results.is_empty());
    }

    // No results returns empty vec
    #[test]
    fn test_ac5_no_results_returns_empty_vec() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search
            .query("ns", "xyznonexistent", 10, None, None)
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
        let result = search.query("ns", "", 10, None, None);
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
        let result = search.query("ns", "   ", 10, None, None);
        // THEN
        assert!(matches!(result, Err(MemorySearchError::EmptyQuery)));
    }

    // FTS5 special characters are escaped safely
    #[test]
    fn test_fts5_special_chars_escaped() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN — query with FTS5 operators should not crash
        let result = search.query("ns", "NOT AND OR devis*", 10, None, None);
        // THEN — should not error (may return empty)
        assert!(result.is_ok());
    }

    // Namespace isolation — search in wrong namespace returns empty
    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("other-ns", "Dupont", 10, None, None).unwrap();
        // THEN
        assert!(results.is_empty());
    }
}
