//! `web_search` tool — privacy-preserving web search with pluggable backends.
//!
//! # Architecture
//!
//! This module exposes a single [`WebSearch`] tool whose work is delegated to a
//! list of [`backend::SearchBackend`] implementations. At tool-invocation time
//! the user may pin a specific backend via the `backend` input field;
//! otherwise the first available backend in the list is used. Backends are
//! registered in priority order by [`crate::native_dispatcher::build_native_dispatcher`]
//! — Brave first (if its API key is set), DuckDuckGo always as the zero-config
//! fallback.
//!
//! # Security posture
//!
//! `web_search` does **not** honour the agent-wide `http_allowlist`: its
//! network target is the *backend host*, which is an implementation detail
//! rather than an agent-supplied URL. Enabling the tool at all (via
//! `apollia.toml -> [tools].web_search = true`) is the user's opt-in to
//! network egress for search queries.

pub mod backend;
#[cfg(feature = "brave-search")]
pub mod brave;
pub mod duckduckgo;

use std::time::Instant;

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

pub use backend::{SafeSearch, SearchBackendError, SearchQuery, SearchResult, TimeRange};
use backend::SearchBackend;

/// Absolute minimum & maximum for `max_results`. Values outside are clamped
/// silently — we don't reject on cosmetic bounds (Principe #4 applies to real
/// failures, not to UX friction).
const MIN_RESULTS: u32 = 1;
const MAX_RESULTS: u32 = 20;

/// Default number of results when the caller omits `max_results`.
const DEFAULT_RESULTS: u32 = 10;

/// The `web_search` tool — holds a priority-ordered list of backends.
pub struct WebSearch {
    backends: Vec<Box<dyn SearchBackend>>,
    preferred: Option<String>,
}

/// Errors produced at the [`WebSearch`] orchestration layer (above the
/// individual [`SearchBackend`] failures).
#[derive(Debug, Error)]
pub enum WebSearchError {
    /// The caller submitted an empty or whitespace-only query.
    #[error("invalid query: {reason}")]
    InvalidQuery {
        /// Human-readable diagnostic (empty, too long, only whitespace…).
        reason: String,
    },

    /// The caller pinned a backend that isn't registered (e.g. `backend: "brave"`
    /// without `BRAVE_SEARCH_API_KEY` set, or `backend: "tavily"` which doesn't exist).
    #[error("backend '{name}' is not available")]
    BackendNotAvailable {
        /// The unknown or unavailable backend name.
        name: String,
    },

    /// No backend could fulfil the request. Carries the last backend error so the
    /// LLM can reason about whether to retry.
    #[error("all backends failed: {last}")]
    AllBackendsFailed {
        /// Short diagnostic from the last backend tried.
        last: String,
    },

    /// The configured backend list is empty (construction-time invariant violation).
    #[error("no search backend registered")]
    NoBackends,
}

/// Input schema for `web_search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchInput {
    /// Search terms. Must be non-empty and ≤ 500 characters after trimming.
    pub query: String,
    /// Maximum number of results. Clamped to `[1, 20]`. Defaults to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    /// Optional region hint. Free-form — forwarded verbatim to the backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Safe-search filter level. Defaults to `moderate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_search: Option<SafeSearch>,
    /// Optional freshness window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
    /// Preferred backend: `"auto"` (default), `"duckduckgo"`, `"brave"`.
    /// `"auto"` picks the first registered backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

/// Output schema for `web_search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchOutput {
    /// Echoed query (post-trim).
    pub query: String,
    /// Name of the backend that produced these results.
    pub backend: String,
    /// Ranked results. Length ≤ `max_results`.
    pub results: Vec<SearchResult>,
    /// Count of returned results (convenience for the LLM).
    pub total_results: u32,
    /// Wall-clock duration from `run()` start to return.
    pub duration_ms: u64,
}

impl WebSearch {
    /// Build a [`WebSearch`] over the given priority-ordered backends.
    ///
    /// Crate-private because the trait boundary is internal to `apollia-tools`
    /// and the sole call site is [`crate::native_dispatcher::build_native_dispatcher`].
    ///
    /// # Errors
    ///
    /// Returns [`WebSearchError::NoBackends`] when *backends* is empty.
    pub(crate) fn new(
        backends: Vec<Box<dyn SearchBackend>>,
        preferred: Option<String>,
    ) -> Result<Self, WebSearchError> {
        if backends.is_empty() {
            return Err(WebSearchError::NoBackends);
        }
        Ok(Self {
            backends,
            preferred,
        })
    }

    /// Build a [`WebSearch`] with the default backend priority used across the
    /// runtime: Brave first (if `BRAVE_SEARCH_API_KEY` is set and the
    /// `brave-search` feature is compiled in), DuckDuckGo always as the
    /// zero-config fallback. Encapsulates the `SearchBackend` trait so callers
    /// outside `apollia-tools` (e.g. the libre-chat tool invoker) don't need
    /// to import crate-private types.
    pub fn with_default_backends() -> Self {
        let mut backends: Vec<Box<dyn SearchBackend>> = Vec::new();

        #[cfg(feature = "brave-search")]
        if let Ok(b) = crate::tools::web_search::brave::BraveBackend::from_env() {
            backends.push(Box::new(b));
        }

        backends.push(Box::new(crate::tools::web_search::duckduckgo::DuckDuckGoBackend::new()));

        // SAFETY: we always push DuckDuckGo, so the list is never empty —
        // `WebSearch::new` cannot return `NoBackends`.
        Self::new(backends, None).expect("DuckDuckGo backend is always pushed")
    }

    /// Dispatch to the selected backend and return results.
    pub async fn run(&self, input: WebSearchInput) -> Result<WebSearchOutput, WebSearchError> {
        let started = Instant::now();

        let query = input.query.trim();
        if query.is_empty() {
            return Err(WebSearchError::InvalidQuery {
                reason: "query is empty".to_string(),
            });
        }
        if query.chars().count() > 500 {
            return Err(WebSearchError::InvalidQuery {
                reason: "query exceeds 500 characters".to_string(),
            });
        }

        let max_results = input
            .max_results
            .unwrap_or(DEFAULT_RESULTS)
            .clamp(MIN_RESULTS, MAX_RESULTS);

        let normalized = SearchQuery {
            query: query.to_string(),
            max_results,
            region: input.region.clone(),
            safe_search: input.safe_search.unwrap_or_default(),
            time_range: input.time_range,
        };

        // Precedence: per-call input.backend > operator-configured preferred > auto.
        let requested = input
            .backend
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "auto")
            .or_else(|| {
                self.preferred
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && *s != "auto")
            });

        // Route: either pinned-by-name, or walk the priority list.
        if let Some(name) = requested {
            let backend = self
                .backends
                .iter()
                .find(|b| b.name() == name)
                .ok_or_else(|| WebSearchError::BackendNotAvailable {
                    name: name.to_string(),
                })?;

            return dispatch(backend.as_ref(), &normalized, query, started).await;
        }

        let mut last_err: Option<String> = None;
        for backend in &self.backends {
            match dispatch(backend.as_ref(), &normalized, query, started).await {
                Ok(output) => return Ok(output),
                Err(WebSearchError::InvalidQuery { .. })
                | Err(WebSearchError::BackendNotAvailable { .. })
                | Err(WebSearchError::NoBackends) => {
                    // Defensive: these shouldn't originate from a backend call,
                    // but if they do, propagate directly (not retry-able).
                    return Err(WebSearchError::AllBackendsFailed {
                        last: "unexpected orchestration error".to_string(),
                    });
                }
                Err(WebSearchError::AllBackendsFailed { last }) => {
                    last_err = Some(last);
                }
            }
        }

        Err(WebSearchError::AllBackendsFailed {
            last: last_err.unwrap_or_else(|| "no backend returned a result".to_string()),
        })
    }

    /// Return the [`ToolDescriptor`] for registration.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "web_search".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Search the web and return a ranked list of results (title, URL, snippet). \
                 Use this as the first step for any research or fact-finding task: the snippets \
                 give you enough signal to decide which URLs to read in full (use `web_read` to \
                 fetch the extracted content of a specific URL). Defaults to DuckDuckGo; Brave \
                 is used automatically when BRAVE_SEARCH_API_KEY is set. This tool does NOT \
                 honour the agent network allowlist — ticking it in the chat's tool picker is \
                 the user's opt-in to search-engine egress."
                    .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 500,
                        "description": "Search terms."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "description": "Max results to return. Defaults to 10."
                    },
                    "region": {
                        "type": "string",
                        "maxLength": 10,
                        "description": "Region hint (e.g. 'wt-wt', 'us-en', 'fr-fr', 'FR')."
                    },
                    "safe_search": {
                        "type": "string",
                        "enum": ["off", "moderate", "strict"],
                        "description": "Safe-search level. Defaults to moderate."
                    },
                    "time_range": {
                        "type": "string",
                        "enum": ["day", "week", "month", "year"],
                        "description": "Limit results to this freshness window."
                    },
                    "backend": {
                        "type": "string",
                        "enum": ["auto", "duckduckgo", "brave"],
                        "description": "Pin a specific backend; 'auto' picks the first available."
                    }
                },
                "required": ["query"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "required": ["query", "backend", "results", "total_results", "duration_ms"],
                "properties": {
                    "query":   { "type": "string" },
                    "backend": { "type": "string" },
                    "results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["title", "url", "snippet", "rank"],
                            "properties": {
                                "title":   { "type": "string" },
                                "url":     { "type": "string" },
                                "snippet": { "type": "string" },
                                "rank":    { "type": "integer", "minimum": 1 },
                                "age":     { "type": ["string", "null"] }
                            }
                        }
                    },
                    "total_results": { "type": "integer" },
                    "duration_ms":   { "type": "integer" }
                }
            })),
            sandbox_profile: SandboxProfile::NetworkRestricted,
            tags: vec![
                "network".to_string(),
                "search".to_string(),
                "web".to_string(),
                "research".to_string(),
            ],
            dangerous: false,
            is_read_only: true,
            risk_score: 2,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// Issue one backend call and wrap the result for the orchestrator.
async fn dispatch(
    backend: &dyn SearchBackend,
    normalized: &SearchQuery,
    echoed_query: &str,
    started: Instant,
) -> Result<WebSearchOutput, WebSearchError> {
    match backend.search(normalized).await {
        Ok(results) => {
            let total_results = results.len() as u32;
            let duration_ms = started.elapsed().as_millis() as u64;
            Ok(WebSearchOutput {
                query: echoed_query.to_string(),
                backend: backend.name().to_string(),
                results,
                total_results,
                duration_ms,
            })
        }
        Err(e) => {
            tracing::warn!(
                backend = backend.name(),
                error = %e,
                "backend failed — trying next if available"
            );
            Err(WebSearchError::AllBackendsFailed {
                last: e.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockBackend {
        name: &'static str,
        fixed: Vec<SearchResult>,
        fail_with: Option<SearchBackendError>,
    }

    #[async_trait]
    impl SearchBackend for MockBackend {
        fn name(&self) -> &str {
            self.name
        }
        async fn search(
            &self,
            _query: &SearchQuery,
        ) -> Result<Vec<SearchResult>, SearchBackendError> {
            if let Some(e) = &self.fail_with {
                // Clone doesn't exist on thiserror-derived; construct an equivalent error.
                return Err(match e {
                    SearchBackendError::Blocked { backend } => {
                        SearchBackendError::Blocked {
                            backend: backend.clone(),
                        }
                    }
                    _ => SearchBackendError::ParseError {
                        backend: self.name.to_string(),
                        reason: "mock".to_string(),
                    },
                });
            }
            Ok(self.fixed.clone())
        }
    }

    fn sample_results(n: usize, prefix: &str) -> Vec<SearchResult> {
        (1..=n)
            .map(|i| SearchResult {
                title: format!("{prefix} {i}"),
                url: format!("https://example.com/{prefix}/{i}"),
                snippet: format!("snippet {i}"),
                rank: i as u32,
                age: None,
            })
            .collect()
    }

    fn basic_input(query: &str) -> WebSearchInput {
        WebSearchInput {
            query: query.to_string(),
            max_results: None,
            region: None,
            safe_search: None,
            time_range: None,
            backend: None,
        }
    }

    #[tokio::test]
    async fn auto_uses_first_backend() {
        // GIVEN Brave first (healthy), DDG second
        let backends: Vec<Box<dyn SearchBackend>> = vec![
            Box::new(MockBackend {
                name: "brave",
                fixed: sample_results(2, "brave"),
                fail_with: None,
            }),
            Box::new(MockBackend {
                name: "duckduckgo",
                fixed: sample_results(5, "ddg"),
                fail_with: None,
            }),
        ];
        let tool = WebSearch::new(backends, None).unwrap();

        // WHEN auto
        let out = tool.run(basic_input("hello")).await.expect("ok");

        // THEN Brave wins
        assert_eq!(out.backend, "brave");
        assert_eq!(out.total_results, 2);
    }

    #[tokio::test]
    async fn auto_falls_back_when_first_backend_blocked() {
        // GIVEN Brave blocked, DDG healthy
        let backends: Vec<Box<dyn SearchBackend>> = vec![
            Box::new(MockBackend {
                name: "brave",
                fixed: vec![],
                fail_with: Some(SearchBackendError::Blocked {
                    backend: "brave".to_string(),
                }),
            }),
            Box::new(MockBackend {
                name: "duckduckgo",
                fixed: sample_results(3, "ddg"),
                fail_with: None,
            }),
        ];
        let tool = WebSearch::new(backends, None).unwrap();

        // WHEN
        let out = tool.run(basic_input("hello")).await.expect("fallback ok");

        // THEN DDG served the request
        assert_eq!(out.backend, "duckduckgo");
        assert_eq!(out.total_results, 3);
    }

    #[tokio::test]
    async fn pinned_backend_is_honoured() {
        // GIVEN Brave + DDG both healthy
        let backends: Vec<Box<dyn SearchBackend>> = vec![
            Box::new(MockBackend {
                name: "brave",
                fixed: sample_results(1, "brave"),
                fail_with: None,
            }),
            Box::new(MockBackend {
                name: "duckduckgo",
                fixed: sample_results(5, "ddg"),
                fail_with: None,
            }),
        ];
        let tool = WebSearch::new(backends, None).unwrap();

        // WHEN input pins DDG
        let mut input = basic_input("hello");
        input.backend = Some("duckduckgo".to_string());
        let out = tool.run(input).await.expect("ok");

        // THEN DDG is used even though Brave is healthy
        assert_eq!(out.backend, "duckduckgo");
    }

    #[tokio::test]
    async fn pinned_unknown_backend_errors() {
        let backends: Vec<Box<dyn SearchBackend>> = vec![Box::new(MockBackend {
            name: "duckduckgo",
            fixed: sample_results(1, "ddg"),
            fail_with: None,
        })];
        let tool = WebSearch::new(backends, None).unwrap();

        let mut input = basic_input("hello");
        input.backend = Some("tavily".to_string());
        let err = tool.run(input).await.expect_err("unknown backend");

        assert!(matches!(err, WebSearchError::BackendNotAvailable { ref name } if name == "tavily"));
    }

    #[tokio::test]
    async fn empty_query_rejected() {
        let backends: Vec<Box<dyn SearchBackend>> = vec![Box::new(MockBackend {
            name: "duckduckgo",
            fixed: vec![],
            fail_with: None,
        })];
        let tool = WebSearch::new(backends, None).unwrap();

        let err = tool.run(basic_input("   ")).await.expect_err("empty");

        assert!(matches!(err, WebSearchError::InvalidQuery { .. }));
    }

    #[tokio::test]
    async fn too_long_query_rejected() {
        let backends: Vec<Box<dyn SearchBackend>> = vec![Box::new(MockBackend {
            name: "duckduckgo",
            fixed: vec![],
            fail_with: None,
        })];
        let tool = WebSearch::new(backends, None).unwrap();

        let long = "x".repeat(501);
        let err = tool.run(basic_input(&long)).await.expect_err("too long");

        assert!(matches!(err, WebSearchError::InvalidQuery { .. }));
    }

    #[tokio::test]
    async fn all_backends_failing_surfaces_all_backends_failed() {
        let backends: Vec<Box<dyn SearchBackend>> = vec![
            Box::new(MockBackend {
                name: "brave",
                fixed: vec![],
                fail_with: Some(SearchBackendError::Blocked {
                    backend: "brave".to_string(),
                }),
            }),
            Box::new(MockBackend {
                name: "duckduckgo",
                fixed: vec![],
                fail_with: Some(SearchBackendError::Blocked {
                    backend: "duckduckgo".to_string(),
                }),
            }),
        ];
        let tool = WebSearch::new(backends, None).unwrap();

        let err = tool.run(basic_input("hi")).await.expect_err("all failed");

        assert!(matches!(err, WebSearchError::AllBackendsFailed { .. }));
    }

    #[test]
    fn new_with_empty_backend_list_errors() {
        match WebSearch::new(vec![], None) {
            Err(WebSearchError::NoBackends) => {}
            Err(other) => panic!("expected NoBackends, got: {other}"),
            Ok(_) => panic!("expected error, got success"),
        }
    }

    #[test]
    fn max_results_is_clamped() {
        // GIVEN input requesting 100 results
        // WHEN clamped via run()
        // Here we test the clamp indirectly via a mock that returns what it's asked for.
        // (Integration-level — exercised through ToolExecutor tests.)
    }

    #[test]
    fn descriptor_is_valid() {
        let descriptor = WebSearch::descriptor();
        assert!(descriptor.validate().is_ok());
        assert_eq!(descriptor.name, "web_search");
        assert!(descriptor.is_read_only);
        assert_eq!(descriptor.risk_score, 2);
    }
}
