//! Memory search tool wrapping FTS5/BM25 from `apollia-memory`.

use std::path::PathBuf;

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Default number of results returned when `limit` is not specified.
const DEFAULT_LIMIT: u32 = 10;

/// Maximum number of results the agent may request.
const MAX_LIMIT: u32 = 50;

/// Search the agent's local memory using FTS5 full-text search with BM25 ranking.
///
/// Wraps `apollia-memory`'s `MemorySearch` engine. Only the agent's own namespace
/// and explicitly declared `shared_namespaces` are reachable; namespace isolation
/// is enforced before any I/O is attempted. Memory retrieval is always
/// agent-initiated, never automatic.
pub struct MemorySearchTool {
    /// The agent's primary namespace (read-write access).
    namespace: String,
    /// Shared namespaces the agent may read (read-only access).
    shared_namespaces: Vec<String>,
    /// Base directory containing `<namespace>.db` files (e.g. `~/.apollia/memory/`).
    base_dir: PathBuf,
}

/// Errors produced by [`MemorySearchTool`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MemorySearchToolError {
    /// The search query is empty or contains only whitespace.
    #[error("empty search query")]
    EmptyQuery,

    /// The requested namespace is not accessible by this agent.
    #[error("namespace '{0}' is not allowed for this agent")]
    NamespaceNotAllowed(String),

    /// The underlying FTS5 search or SQLite operation failed.
    #[error("memory search failed: {0}")]
    SearchFailed(String),
}

/// Input for a memory search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchInput {
    /// FTS5 search query (keywords). FTS5 operators are escaped automatically.
    pub query: String,
    /// Namespace to search. Defaults to the agent's own namespace when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Maximum number of results to return. Defaults to 10, capped at 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Filter by memory source: `"episodic"` or `"semantic"`. Both are searched when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Minimum importance/confidence threshold (0.0–1.0). Entries below this value are excluded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_relevance: Option<f64>,
}

/// A single memory search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    /// BM25 relevance score (higher = more relevant).
    pub score: f64,
    /// Memory source: `"episodic"` or `"semantic"`.
    pub source: String,
    /// Textual content of the memory entry.
    pub content: String,
    /// Importance (episodic) or confidence (semantic). `None` if not applicable.
    pub relevance: Option<f64>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Output of a memory search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchOutput {
    /// Search results sorted by BM25 score descending.
    pub results: Vec<MemorySearchResult>,
    /// Total number of results returned (equal to `results.len()`).
    pub total_found: u32,
}

impl MemorySearchTool {
    /// Creates a new `MemorySearchTool` for the given agent context.
    ///
    /// - `namespace` : the agent's primary namespace (read-write access).
    /// - `shared_namespaces` : additional namespaces the agent may read.
    /// - `base_dir` : directory containing `<namespace>.db` files.
    pub fn new(namespace: String, shared_namespaces: Vec<String>, base_dir: PathBuf) -> Self {
        Self {
            namespace,
            shared_namespaces,
            base_dir,
        }
    }

    /// Returns `true` when `namespace` is the agent's own or a declared shared namespace.
    fn is_namespace_allowed(&self, namespace: &str) -> bool {
        namespace == self.namespace || self.shared_namespaces.iter().any(|ns| ns == namespace)
    }

    /// Execute a FTS5 memory search against the target namespace.
    ///
    /// Validation order: `EmptyQuery` → `NamespaceNotAllowed` → delegate to `MemorySearch`.
    ///
    /// # Errors
    ///
    /// Returns [`MemorySearchToolError::EmptyQuery`] for blank queries.
    /// Returns [`MemorySearchToolError::NamespaceNotAllowed`] for restricted namespaces.
    /// Returns [`MemorySearchToolError::SearchFailed`] on SQLite or I/O errors.
    pub async fn run(
        &self,
        input: MemorySearchInput,
    ) -> Result<MemorySearchOutput, MemorySearchToolError> {
        // Validate query first: fast, no I/O.
        if input.query.trim().is_empty() {
            return Err(MemorySearchToolError::EmptyQuery);
        }

        // Resolve the target namespace.
        let target_ns = input
            .namespace
            .clone()
            .unwrap_or_else(|| self.namespace.clone());

        // Validate namespace access before any I/O (fail fast).
        if !self.is_namespace_allowed(&target_ns) {
            return Err(MemorySearchToolError::NamespaceNotAllowed(target_ns));
        }

        let limit = input.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let source_filter = parse_source_filter(input.source.as_deref())
            .map_err(MemorySearchToolError::SearchFailed)?;
        let min_relevance = input.min_relevance;
        let query = input.query.clone();
        let base_dir = self.base_dir.clone();

        // SQLite work is offloaded to a blocking thread to avoid stalling the async executor.
        let raw_results = tokio::task::spawn_blocking(move || {
            let db_path = base_dir.join(format!("{target_ns}.db"));
            let store = apollia_memory::store::MemoryStore::open(&db_path)
                .map_err(|e| MemorySearchToolError::SearchFailed(e.to_string()))?;
            let search = apollia_memory::search::MemorySearch::new(&store);
            search
                .query(apollia_memory::search::SearchQuery {
                    namespace: &target_ns,
                    query: &query,
                    limit,
                    sources: source_filter.as_deref(),
                    min_importance: min_relevance,
                })
                .map_err(|e| MemorySearchToolError::SearchFailed(e.to_string()))
        })
        .await
        .map_err(|e| MemorySearchToolError::SearchFailed(e.to_string()))??;

        let results: Vec<MemorySearchResult> = raw_results
            .into_iter()
            .map(|r| MemorySearchResult {
                score: r.score,
                source: source_label(r.source),
                content: r.content,
                relevance: r.relevance,
                created_at: r.created_at,
            })
            .collect();

        let total_found = results.len() as u32;
        Ok(MemorySearchOutput {
            results,
            total_found,
        })
    }

    /// Returns the [`ToolDescriptor`] for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "memory_search".to_string(),
            version: "1.0.0".to_string(),
            description: "Search the agent's local memory using FTS5 full-text search with \
                BM25 ranking. Only the agent's own namespace and declared shared namespaces \
                are accessible. FTS5 operators in the query are escaped automatically. \
                Memory retrieval is always agent-initiated."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "FTS5 search query (keywords). Operators are escaped automatically."
                    },
                    "namespace": {
                        "type": "string",
                        "description": "Namespace to search. Defaults to the agent's own namespace."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum results to return. Defaults to 10, capped at 50."
                    },
                    "source": {
                        "type": "string",
                        "enum": ["episodic", "semantic"],
                        "description": "Filter by memory source. Both sources are searched when absent."
                    },
                    "min_relevance": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Minimum importance/confidence threshold (0.0–1.0)."
                    }
                },
                "required": ["query"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "score":      { "type": "number", "description": "BM25 relevance score (higher = more relevant)" },
                                "source":     { "type": "string", "description": "Memory source: episodic or semantic" },
                                "content":    { "type": "string", "description": "Textual content of the memory entry" },
                                "relevance":  { "type": "number", "description": "Importance (episodic) or confidence (semantic)" },
                                "created_at": { "type": "string", "description": "ISO 8601 creation timestamp" }
                            },
                            "required": ["score", "source", "content", "created_at"]
                        },
                        "description": "Search results sorted by BM25 score descending."
                    },
                    "total_found": {
                        "type": "integer",
                        "description": "Total number of results returned."
                    }
                },
                "required": ["results", "total_found"]
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec![
                "memory".to_string(),
                "search".to_string(),
                "fts5".to_string(),
                "bm25".to_string(),
            ],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// Converts a [`apollia_memory::search::SearchSource`] to its string label.
fn source_label(source: apollia_memory::search::SearchSource) -> String {
    match source {
        apollia_memory::search::SearchSource::Episodic => "episodic".to_string(),
        apollia_memory::search::SearchSource::Semantic => "semantic".to_string(),
    }
}

/// Parses an optional source filter string into a typed source list.
///
/// Returns `None` when `source` is `None` (search all sources).
/// Returns `Err` when the string is not `"episodic"` or `"semantic"`.
fn parse_source_filter(
    source: Option<&str>,
) -> Result<Option<Vec<apollia_memory::search::SearchSource>>, String> {
    match source {
        None => Ok(None),
        Some("episodic") => Ok(Some(vec![apollia_memory::search::SearchSource::Episodic])),
        Some("semantic") => Ok(Some(vec![apollia_memory::search::SearchSource::Semantic])),
        Some(other) => Err(format!(
            "unknown memory source: '{other}', expected 'episodic' or 'semantic'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::{episodic::EpisodicMemory, semantic::SemanticMemory, store::MemoryStore};
    use serde_json::json;
    use tempfile::TempDir;

    /// Opens a fresh store in `dir` for `namespace`, inserts episodic and semantic fixtures,
    /// then drops the connection so the tool can open its own connection cleanly.
    fn setup_store_with_data(dir: &TempDir, namespace: &str) {
        let db_path = dir.path().join(format!("{namespace}.db"));
        let store = MemoryStore::open(&db_path).expect("store open");

        let ep = EpisodicMemory::new(&store);
        ep.record(
            namespace,
            "agent-1",
            "budget client Dupont cinq mille euros",
            "budget client Dupont five thousand euros",
            None,
            None,
            None,
        )
        .expect("episodic record");
        ep.record(
            namespace,
            "agent-1",
            "meeting planned with client Martin",
            0.4,
            None,
            None,
            None,
        )
        .expect("episodic record");

        let sem = SemanticMemory::new(&store);
        sem.remember(apollia_memory::semantic::RememberInput {
            namespace,
            key: "client.dupont.budget",
            value: &json!(5000),
            confidence: 0.85,
            source: Some("crm"),
            expires_at: None,
        })
        .expect("semantic remember");
        // store dropped here: connection closed before tool opens its own
    }

    #[tokio::test]
    async fn search_returns_results_sorted_by_bm25() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        setup_store_with_data(&dir, "agent-1");
        let tool = MemorySearchTool::new("agent-1".to_string(), vec![], dir.path().to_path_buf());

        // WHEN
        let output = tool
            .run(MemorySearchInput {
                query: "budget client".to_string(),
                namespace: None,
                limit: None,
                source: None,
                min_relevance: None,
            })
            .await
            .expect("search should succeed");

        // THEN: results are non-empty and sorted by BM25 score descending
        assert!(!output.results.is_empty());
        for i in 1..output.results.len() {
            assert!(
                output.results[i - 1].score >= output.results[i].score,
                "results must be sorted by score descending"
            );
        }
        assert_eq!(output.total_found as usize, output.results.len());
    }

    #[tokio::test]
    async fn search_filters_by_source() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        setup_store_with_data(&dir, "agent-1");
        let tool = MemorySearchTool::new("agent-1".to_string(), vec![], dir.path().to_path_buf());

        // WHEN
        let output = tool
            .run(MemorySearchInput {
                query: "client".to_string(),
                namespace: None,
                limit: None,
                source: Some("episodic".to_string()),
                min_relevance: None,
            })
            .await
            .expect("search should succeed");

        // THEN: only episodic entries are returned
        assert!(!output.results.is_empty());
        for result in &output.results {
            assert_eq!(result.source, "episodic");
        }
    }

    #[tokio::test]
    async fn search_filters_by_min_relevance() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        setup_store_with_data(&dir, "agent-1");
        let tool = MemorySearchTool::new("agent-1".to_string(), vec![], dir.path().to_path_buf());

        // WHEN
        let output = tool
            .run(MemorySearchInput {
                query: "client".to_string(),
                namespace: None,
                limit: None,
                source: None,
                min_relevance: Some(0.8),
            })
            .await
            .expect("search should succeed");

        // THEN: all returned entries have relevance >= 0.8
        for result in &output.results {
            if let Some(rel) = result.relevance {
                assert!(
                    rel >= 0.8,
                    "relevance {rel} should be >= 0.8 (min_relevance filter)"
                );
            }
        }
    }

    #[tokio::test]
    async fn search_unauthorized_namespace_fails() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let tool = MemorySearchTool::new(
            "agent-1".to_string(),
            vec!["shared".to_string()],
            dir.path().to_path_buf(),
        );

        // WHEN
        let err = tool
            .run(MemorySearchInput {
                query: "test".to_string(),
                namespace: Some("agent-2".to_string()),
                limit: None,
                source: None,
                min_relevance: None,
            })
            .await
            .expect_err("should fail with NamespaceNotAllowed");

        // THEN
        assert!(
            matches!(&err, MemorySearchToolError::NamespaceNotAllowed(ns) if ns == "agent-2"),
            "expected NamespaceNotAllowed(agent-2), got: {err}"
        );
    }

    #[tokio::test]
    async fn search_shared_namespace_allowed() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        setup_store_with_data(&dir, "shared");
        let tool = MemorySearchTool::new(
            "agent-1".to_string(),
            vec!["shared".to_string()],
            dir.path().to_path_buf(),
        );

        // WHEN
        let result = tool
            .run(MemorySearchInput {
                query: "budget".to_string(),
                namespace: Some("shared".to_string()),
                limit: None,
                source: None,
                min_relevance: None,
            })
            .await;

        // THEN: search executes without error
        assert!(result.is_ok(), "shared namespace search should succeed");
    }

    #[tokio::test]
    async fn search_empty_query_fails() {
        // GIVEN
        let dir = TempDir::new().expect("tempdir");
        let tool = MemorySearchTool::new("agent-1".to_string(), vec![], dir.path().to_path_buf());

        // WHEN
        let err = tool
            .run(MemorySearchInput {
                query: "   ".to_string(),
                namespace: None,
                limit: None,
                source: None,
                min_relevance: None,
            })
            .await
            .expect_err("should fail with EmptyQuery");

        // THEN
        assert!(
            matches!(err, MemorySearchToolError::EmptyQuery),
            "expected EmptyQuery, got: {err}"
        );
    }

    #[test]
    fn descriptor_passes_validation() {
        // GIVEN the descriptor the memory_search tool publishes
        let descriptor = MemorySearchTool::descriptor();
        // WHEN it is validated
        // THEN it passes, so the registry will accept it
        assert!(
            descriptor.validate().is_ok(),
            "descriptor must be valid for ToolRegistry registration"
        );
    }
}
