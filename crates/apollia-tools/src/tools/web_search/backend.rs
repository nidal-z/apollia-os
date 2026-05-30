//! Pluggable backend abstraction for the `web_search` tool.
//!
//! Two concrete backends live alongside this module:
//! * [`super::duckduckgo::DuckDuckGoBackend`]: default, zero-config HTML scrape
//! * [`super::brave::BraveBackend`]: feature-gated, API-key driven
//!
//! The trait is `pub(crate)` because no external crate should implement custom
//! search backends today. Shrinking the public surface keeps refactors cheap
//! (minimal contract).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Safe-search filter applied to the backend query.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafeSearch {
    /// No filtering. Adult content may appear.
    Off,
    /// Default for every backend. Mildly filtered.
    #[default]
    Moderate,
    /// Strict filtering. Reduces result count.
    Strict,
}

/// Freshness window applied to the backend query.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    /// Results from the last 24 hours.
    Day,
    /// Results from the last 7 days.
    Week,
    /// Results from the last 30 days.
    Month,
    /// Results from the last 365 days.
    Year,
}

/// Normalised query passed to any [`SearchBackend`].
///
/// Backends map the fields to their own wire formats (e.g. DDG's `kl=fr-fr`
/// vs Brave's `country=FR`). Invalid or unknown hints are silently ignored
/// by the backend rather than raising; agents should not be blocked by
/// cosmetic mismatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Search terms. Never empty when passed to a backend.
    pub query: String,
    /// Requested number of results. Clamped to 1..=20 by [`super::WebSearch`].
    pub max_results: u32,
    /// Optional region hint ("wt-wt", "us-en", "fr-fr", "FR"…).
    pub region: Option<String>,
    /// Safe-search filter level.
    pub safe_search: SafeSearch,
    /// Optional freshness window.
    pub time_range: Option<TimeRange>,
}

/// One result row returned by a backend.
///
/// Shared across backends; richer backend-specific fields (Brave thumbnails,
/// profile metadata, …) are deliberately dropped at the trait boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Page title.
    pub title: String,
    /// Target URL. Already stripped of backend-side redirect wrappers.
    pub url: String,
    /// Snippet / description. Stripped of markup.
    pub snippet: String,
    /// 1-based position in the backend's ranking.
    pub rank: u32,
    /// Human-readable freshness ("2 days ago", "2025-04-15"). `None` for
    /// backends that do not expose freshness (DDG).
    pub age: Option<String>,
}

/// Errors returned by a [`SearchBackend`].
///
/// Each variant carries the backend name so the dispatcher can attribute
/// failures when more than one backend is configured. Error *codes* are
/// intentionally backend-agnostic so the LLM can react uniformly.
#[derive(Debug, Error)]
pub enum SearchBackendError {
    /// Low-level networking error (DNS, TCP, TLS). Usually transient.
    #[error("backend '{backend}' request failed: {message}")]
    RequestFailed {
        /// Name of the failing backend.
        backend: String,
        /// Cause extracted from `reqwest`.
        message: String,
    },

    /// Backend returned HTTP 429. Caller may retry after a backoff.
    #[error("backend '{backend}' rate-limited (HTTP {status})")]
    RateLimited {
        /// Name of the failing backend.
        backend: String,
        /// Actual HTTP status code (always 429 today, kept for future variants).
        status: u16,
    },

    /// Backend answered with another non-2xx status.
    #[error("backend '{backend}' returned HTTP {status}")]
    BadStatus {
        /// Name of the failing backend.
        backend: String,
        /// Status code returned by the backend.
        status: u16,
    },

    /// Response body could not be interpreted (HTML selector miss, JSON schema drift…).
    /// A `parse_error` suggests the backend changed its layout.
    #[error("backend '{backend}' returned unparseable output: {reason}")]
    ParseError {
        /// Name of the failing backend.
        backend: String,
        /// Short diagnostic (parser location, missing field…).
        reason: String,
    },

    /// Backend looks alive but returned a captcha or WAF challenge.
    /// Distinct from [`SearchBackendError::RateLimited`]; the right reaction is to
    /// switch backend, not to back off.
    #[error("backend '{backend}' blocked (captcha or WAF)")]
    Blocked {
        /// Name of the failing backend.
        backend: String,
    },

    /// Request exceeded the configured timeout.
    #[error("backend '{backend}' timed out after {timeout_secs}s")]
    Timeout {
        /// Name of the failing backend.
        backend: String,
        /// Timeout value that was in effect.
        timeout_secs: u64,
    },

    /// Required credential is missing (Brave without API key).
    #[error("backend '{backend}' missing credential: {what}")]
    MissingCredential {
        /// Name of the failing backend.
        backend: String,
        /// Name of the missing credential (env var, keychain key…).
        what: String,
    },
}

/// A pluggable search engine backend.
///
/// Implementations are stateless HTTP clients; one instance is created per
/// agent invocation by the dispatcher factory. Do not store mutable state here;
/// per-invocation rebuild makes it wasted work.
#[async_trait]
pub(crate) trait SearchBackend: Send + Sync {
    /// Stable identifier for this backend. Echoed in
    /// [`super::WebSearchOutput::backend`] and log spans.
    fn name(&self) -> &str;

    /// Perform one search.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchBackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_serde_roundtrip() {
        // GIVEN a query with every optional field set
        let q = SearchQuery {
            query: "apollia runtime".to_string(),
            max_results: 10,
            region: Some("fr-fr".to_string()),
            safe_search: SafeSearch::Strict,
            time_range: Some(TimeRange::Week),
        };
        // WHEN round-tripping through JSON
        let json = serde_json::to_string(&q).expect("serialize");
        let back: SearchQuery = serde_json::from_str(&json).expect("deserialize");
        // THEN every field is preserved
        assert_eq!(back.query, q.query);
        assert_eq!(back.max_results, q.max_results);
        assert_eq!(back.region, q.region);
        assert_eq!(back.safe_search, q.safe_search);
        assert_eq!(back.time_range, q.time_range);
    }

    #[test]
    fn safe_search_default_is_moderate() {
        assert_eq!(SafeSearch::default(), SafeSearch::Moderate);
    }

    #[test]
    fn safe_search_serializes_as_snake_case() {
        // GIVEN each variant
        let off = serde_json::to_string(&SafeSearch::Off).unwrap();
        let moderate = serde_json::to_string(&SafeSearch::Moderate).unwrap();
        let strict = serde_json::to_string(&SafeSearch::Strict).unwrap();
        // THEN
        assert_eq!(off, "\"off\"");
        assert_eq!(moderate, "\"moderate\"");
        assert_eq!(strict, "\"strict\"");
    }

    #[test]
    fn time_range_serializes_as_snake_case() {
        // GIVEN each variant
        let json = serde_json::to_string(&TimeRange::Day).unwrap();
        // THEN
        assert_eq!(json, "\"day\"");
    }

    #[test]
    fn search_result_without_age_serializes_null() {
        // GIVEN a result without freshness metadata (DDG case)
        let r = SearchResult {
            title: "t".to_string(),
            url: "https://example.com".to_string(),
            snippet: "s".to_string(),
            rank: 1,
            age: None,
        };
        // WHEN
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        // THEN age is explicitly null (not omitted) so downstream schemas stay stable
        assert!(v["age"].is_null());
    }

    #[test]
    fn error_display_includes_backend_name() {
        // GIVEN every error variant
        let errors = vec![
            SearchBackendError::RequestFailed {
                backend: "ddg".to_string(),
                message: "dns".to_string(),
            },
            SearchBackendError::RateLimited {
                backend: "ddg".to_string(),
                status: 429,
            },
            SearchBackendError::BadStatus {
                backend: "ddg".to_string(),
                status: 500,
            },
            SearchBackendError::ParseError {
                backend: "ddg".to_string(),
                reason: "selector".to_string(),
            },
            SearchBackendError::Blocked {
                backend: "ddg".to_string(),
            },
            SearchBackendError::Timeout {
                backend: "ddg".to_string(),
                timeout_secs: 15,
            },
            SearchBackendError::MissingCredential {
                backend: "brave".to_string(),
                what: "BRAVE_SEARCH_API_KEY".to_string(),
            },
        ];
        // THEN every message mentions the backend name (needed for tracing)
        for e in errors {
            let s = e.to_string();
            assert!(
                s.contains("ddg") || s.contains("brave"),
                "error message missing backend: {s}"
            );
        }
    }
}
