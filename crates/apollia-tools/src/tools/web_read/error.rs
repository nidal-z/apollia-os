//! Error taxonomy for the `web_read` tool.

use thiserror::Error;

/// Errors produced by [`super::WebRead`].
///
/// Variants map to stable `snake_case` codes in the [`ToolExecutor`] impl,
/// the LLM uses those codes to pick the right retry / fallback strategy.
///
/// [`ToolExecutor`]: crate::executor::ToolExecutor
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WebReadError {
    /// URL could not be parsed, or has no host component.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// URL resolves to a private / loopback / link-local address. See
    /// [`apollia_core::net::assert_public`] for the full policy.
    #[error("private address: {0}")]
    PrivateAddress(String),

    /// Response `Content-Type` is not supported. The included kind
    /// (`"pdf"`, `"json"`, `"binary"`) guides the agent toward the right
    /// next step (e.g. switch to `http_fetch` for JSON).
    #[error("unsupported content type: {kind}")]
    UnsupportedContentType {
        /// Short kind identifier: `"pdf"`, `"json"`, `"binary"`, `"unknown"`.
        kind: String,
    },

    /// Low-level network or DNS failure.
    #[error("request failed: {0}")]
    RequestFailed(String),

    /// Response came back with a non-success status.
    #[error("bad status: HTTP {0}")]
    BadStatus(u16),

    /// Body exceeded the 5 MB cap.
    #[error("response too large: {size} bytes exceeds limit of {limit} bytes")]
    ResponseTooLarge {
        /// Accumulated body size at the point reading was aborted.
        size: usize,
        /// Configured cap.
        limit: usize,
    },

    /// Request timed out.
    #[error("timeout after {0}s")]
    Timeout(u64),

    /// The extraction heuristic refused the page (no article-like content).
    #[error("extraction failed: {0}")]
    ExtractionFailed(String),

    /// Extraction succeeded but returned fewer than 100 characters; the page
    /// is likely dynamic (JS-rendered) or behind a paywall.
    #[error("empty content (extracted {0} chars)")]
    EmptyContent(usize),
}
