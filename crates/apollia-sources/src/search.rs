//! Searching several sources at once, as an agent's `source_search` asks.
//!
//! Each source has its own index ([`crate::index`]), searched on its own; the
//! hits of all of them are then ordered by their score and cut to the limit.
//! A source whose index does not exist yet is named in the answer rather than
//! created empty: the agent learns the source is checked but not indexed.

use std::path::Path;

use serde::Serialize;

use crate::index::{
    index_path, IndexError, QueryVector, SearchHit, SearchMode, SourceIndex, DEFAULT_LIMIT,
    MAX_LIMIT,
};

/// What a search over several sources is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcesQuery {
    /// Identifiers of the sources to search, each one of the registry.
    pub sources: Vec<String>,
    /// The words searched.
    pub query: String,
    /// The vector of the query, when this machine could compute one.
    pub vector: Option<QueryVector>,
    /// Most results returned; [`DEFAULT_LIMIT`] when absent, held between one
    /// and [`MAX_LIMIT`].
    pub limit: Option<usize>,
}

/// The answer of a search over several sources.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourcesSearchOutcome {
    /// How the results were ranked: by words and meaning only when every
    /// source searched could compare the query's vector.
    pub mode: SearchMode,
    /// The passages, best first.
    pub hits: Vec<SearchHit>,
    /// The sources searched that have no index yet.
    pub not_indexed: Vec<String>,
}

/// Searches the indexes of `query.sources` under `data_dir`.
///
/// # Errors
/// The [`IndexError`] of the first index that cannot be opened or searched.
pub fn search_sources(
    data_dir: &Path,
    query: &SourcesQuery,
) -> Result<SourcesSearchOutcome, IndexError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut mode = SearchMode::Hybrid;
    let mut hits: Vec<SearchHit> = Vec::new();
    let mut not_indexed: Vec<String> = Vec::new();
    let mut searched = 0usize;
    for id in &query.sources {
        if !index_path(data_dir, id)?.exists() {
            not_indexed.push(id.clone());
            continue;
        }
        let index = SourceIndex::open(data_dir, id)?;
        let outcome = index.search(&query.query, query.vector.as_ref(), Some(limit))?;
        if outcome.mode == SearchMode::KeywordsOnly {
            mode = SearchMode::KeywordsOnly;
        }
        hits.extend(outcome.hits);
        searched += 1;
    }
    if searched == 0 {
        mode = SearchMode::KeywordsOnly;
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(limit);
    Ok(SourcesSearchOutcome {
        mode,
        hits,
        not_indexed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, Location};
    use crate::index::DocumentRecord;

    fn index_with(data_dir: &Path, source: &str, document: &str, texts: &[&str]) {
        let mut index = SourceIndex::open(data_dir, source).unwrap();
        let chunks: Vec<Chunk> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| Chunk {
                text: (*text).to_owned(),
                location: Location::Lines {
                    first: u32::try_from(i).unwrap() + 1,
                    last: u32::try_from(i).unwrap() + 1,
                },
            })
            .collect();
        index
            .upsert_document(
                &DocumentRecord {
                    path: document.to_owned(),
                    fingerprint: "f".to_owned(),
                    modified_at: "2026-10-04T00:00:00Z".to_owned(),
                },
                &chunks,
                None,
            )
            .unwrap();
    }

    fn query(sources: &[&str], words: &str, limit: Option<usize>) -> SourcesQuery {
        SourcesQuery {
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            query: words.to_owned(),
            vector: None,
            limit,
        }
    }

    #[test]
    fn test_hits_of_every_source_named_are_merged_and_only_those() {
        // GIVEN three indexed sources, two of them speaking of invoices
        let dir = tempfile::tempdir().unwrap();
        index_with(dir.path(), "src-a", "a.txt", &["the invoice of March"]);
        index_with(dir.path(), "src-b", "b.txt", &["an invoice paid late"]);
        index_with(
            dir.path(),
            "src-c",
            "c.txt",
            &["an invoice nobody may read"],
        );

        // WHEN two of them are searched for the word
        let outcome =
            search_sources(dir.path(), &query(&["src-a", "src-b"], "invoice", None)).unwrap();

        // THEN both answer, the third is never read, and the search is by words
        let mut sources: Vec<&str> = outcome.hits.iter().map(|h| h.source.as_str()).collect();
        sources.sort_unstable();
        assert_eq!(sources, vec!["src-a", "src-b"]);
        assert_eq!(outcome.mode, SearchMode::KeywordsOnly);
        assert!(outcome.not_indexed.is_empty());
    }

    #[test]
    fn test_a_source_without_index_is_named_and_not_created() {
        // GIVEN one indexed source and one that was never indexed
        let dir = tempfile::tempdir().unwrap();
        index_with(dir.path(), "src-a", "a.txt", &["the invoice of March"]);

        // WHEN both are searched
        let outcome =
            search_sources(dir.path(), &query(&["src-a", "src-new"], "invoice", None)).unwrap();

        // THEN the second is named as not indexed, and no file was made for it
        assert_eq!(outcome.not_indexed, vec!["src-new".to_owned()]);
        assert_eq!(outcome.hits.len(), 1);
        assert!(!index_path(dir.path(), "src-new").unwrap().exists());
    }

    #[test]
    fn test_the_limit_holds_across_sources() {
        // GIVEN two sources of three matching passages each
        let dir = tempfile::tempdir().unwrap();
        index_with(
            dir.path(),
            "src-a",
            "a.txt",
            &["invoice one", "invoice two", "invoice three"],
        );
        index_with(
            dir.path(),
            "src-b",
            "b.txt",
            &["invoice four", "invoice five", "invoice six"],
        );

        // WHEN both are searched with a limit of four
        let outcome =
            search_sources(dir.path(), &query(&["src-a", "src-b"], "invoice", Some(4))).unwrap();

        // THEN four passages come back, best score first
        assert_eq!(outcome.hits.len(), 4);
        assert!(outcome
            .hits
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score));
    }
}
