//! Searching the sources checked for an agent, at the agent's initiative.
//!
//! The tool reaches the index of each source through the handle of the
//! registry actor of `apollia-sources`. It is built with the sources the
//! resolved agent has checked, and searches those and no other: a source the
//! agent names outside them is refused before any index is read. Nothing a
//! source holds enters a prompt unless the agent calls this tool.

use std::future::Future;
use std::pin::Pin;

use apollia_core::SandboxProfile;
use apollia_sources::index::{SearchHit, SearchMode, MAX_LIMIT};
use apollia_sources::{SourceRegistryHandle, SourcesQuery};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::descriptor::{ActionFamily, FamilySource, ToolDescriptor, ToolKind};
use crate::executor::{ToolExecutionError, ToolExecutor};

/// Name of the tool.
pub const SOURCE_SEARCH_TOOL: &str = "source_search";

/// What an agent asks the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSearchInput {
    /// The words searched.
    pub query: String,
    /// Identifiers of the sources to search, among those checked; all of them
    /// when absent or empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    /// Most results returned, from one to fifty; ten when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// What the tool answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceSearchOutput {
    /// How the results were ranked: `hybrid` (words and meaning) or
    /// `keywords_only`.
    pub mode: SearchMode,
    /// The passages found, best first: excerpt, document, location, score,
    /// source.
    pub results: Vec<SearchHit>,
    /// The sources searched that are not indexed yet.
    pub not_indexed: Vec<String>,
}

/// Errors of a search, before or during it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceSearchError {
    /// The query has no word.
    #[error("empty search query")]
    EmptyQuery,
    /// The limit is outside one to fifty.
    #[error("limit {0} is outside 1 to {MAX_LIMIT}")]
    InvalidLimit(u32),
    /// A source named is not one the agent has checked.
    #[error("source '{0}' is not checked for this agent")]
    SourceNotChecked(String),
    /// The index could not be read.
    #[error("source search failed: {0}")]
    SearchFailed(String),
}

impl SourceSearchError {
    fn code(&self) -> &'static str {
        match self {
            Self::EmptyQuery => "empty_query",
            Self::InvalidLimit(_) => "invalid_limit",
            Self::SourceNotChecked(_) => "source_not_checked",
            Self::SearchFailed(_) => "search_failed",
        }
    }
}

/// The `source_search` tool of one agent: the registry it searches through
/// and the sources it may search.
#[derive(Debug, Clone)]
pub struct SourceSearchTool {
    registry: SourceRegistryHandle,
    sources: Vec<String>,
}

impl SourceSearchTool {
    /// The tool searching `sources`, the indexable sources the agent checked,
    /// through `registry`.
    #[must_use]
    pub fn new(registry: SourceRegistryHandle, sources: Vec<String>) -> Self {
        Self { registry, sources }
    }

    /// The sources this tool may search.
    #[must_use]
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// The sources `asked` names, each checked, or all of them when it names
    /// none.
    ///
    /// # Errors
    /// [`SourceSearchError::SourceNotChecked`] for the first source outside
    /// the checked ones.
    fn targets(&self, asked: Option<Vec<String>>) -> Result<Vec<String>, SourceSearchError> {
        let asked = asked.unwrap_or_default();
        if asked.is_empty() {
            return Ok(self.sources.clone());
        }
        let mut targets: Vec<String> = Vec::with_capacity(asked.len());
        for id in asked {
            if !self.sources.contains(&id) {
                return Err(SourceSearchError::SourceNotChecked(id));
            }
            if !targets.contains(&id) {
                targets.push(id);
            }
        }
        Ok(targets)
    }

    /// Runs one search.
    ///
    /// Validation order: empty query, limit, sources named; then the search.
    ///
    /// # Errors
    /// Every [`SourceSearchError`].
    pub async fn run(
        &self,
        input: SourceSearchInput,
    ) -> Result<SourceSearchOutput, SourceSearchError> {
        let query = input.query.trim().to_owned();
        if query.is_empty() {
            return Err(SourceSearchError::EmptyQuery);
        }
        let limit = match input.limit {
            Some(limit) if limit == 0 || limit as usize > MAX_LIMIT => {
                return Err(SourceSearchError::InvalidLimit(limit));
            }
            limit => limit.map(|l| l as usize),
        };
        let targets = self.targets(input.sources)?;
        let vector = query_vector(&query).await;
        let outcome = self
            .registry
            .search(SourcesQuery {
                sources: targets,
                query,
                vector,
                limit,
            })
            .await
            .map_err(|e| SourceSearchError::SearchFailed(e.to_string()))?;
        Ok(SourceSearchOutput {
            mode: outcome.mode,
            results: outcome.hits,
            not_indexed: outcome.not_indexed,
        })
    }

    /// The descriptor of the tool, registered at boot.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: SOURCE_SEARCH_TOOL.to_string(),
            version: "1.0.0".to_string(),
            description: "Search the documents of the sources checked for this agent (indexed \
                folders and files), by keywords and by meaning when an embeddings model is \
                installed. Only the checked sources are reachable. Nothing from a source \
                reaches the conversation unless this tool is called."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for, in plain words."
                    },
                    "sources": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Identifiers of the sources to search, among those checked. All of them when absent."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum results to return. Defaults to 10."
                    }
                },
                "required": ["query"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["hybrid", "keywords_only"],
                        "description": "How the results were ranked."
                    },
                    "results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "excerpt":  { "type": "string", "description": "The passage found" },
                                "document": { "type": "string", "description": "Path of the document it comes from" },
                                "location": { "type": "object", "description": "Page, slide, rows or lines of the passage" },
                                "score":    { "type": "number", "description": "Rank score, higher first" },
                                "source":   { "type": "string", "description": "Identifier of the source" }
                            },
                            "required": ["excerpt", "document", "location", "score", "source"]
                        }
                    },
                    "not_indexed": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Sources searched that are not indexed yet."
                    }
                },
                "required": ["mode", "results", "not_indexed"]
            })),
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec![
                "sources".to_string(),
                "search".to_string(),
                "documents".to_string(),
            ],
            family: ActionFamily::Read,
            family_source: FamilySource::Builtin,
            target_arg: None,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// The vector of `query` from the embeddings model of this process, the one
/// memory search uses, or `None`: the search then goes by keywords alone and
/// says so.
#[cfg(feature = "memory-search")]
async fn query_vector(query: &str) -> Option<apollia_sources::index::QueryVector> {
    let embedder = apollia_memory::vectors::installed_embedder();
    apollia_memory::vectors::query_vector(embedder.as_deref(), query)
        .await
        .map(|v| apollia_sources::index::QueryVector {
            model: v.model,
            vector: v.vector,
        })
}

/// Without the memory engine in this build, no vector: keywords alone.
#[cfg(not(feature = "memory-search"))]
async fn query_vector(_query: &str) -> Option<apollia_sources::index::QueryVector> {
    None
}

impl ToolExecutor for SourceSearchTool {
    fn name(&self) -> &str {
        SOURCE_SEARCH_TOOL
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: SourceSearchInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;
            let output =
                self.run(typed)
                    .await
                    .map_err(|e| ToolExecutionError::ExecutionFailed {
                        code: e.code().to_string(),
                        message: e.to_string(),
                    })?;
            serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialization_error".to_string(),
                message: e.to_string(),
            })
        })
    }
}

#[cfg(test)]
mod tests;
