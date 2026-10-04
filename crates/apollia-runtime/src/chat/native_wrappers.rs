//! Wrapper [`ToolExecutor`] implementation that lets the chat dispatcher
//! host `http_fetch` without keeping a parallel `invoke_*` fast path on
//! `NativeChatToolInvoker`.
//!
//! [`DynamicAllowlistHttpFetch`] preserves the conversation's habit of adding
//! the requested URL's host to the allowlist on the fly. The stock
//! `HttpFetch` is constructed with a static allowlist; this wrapper builds a
//! fresh one per call with `[host_from_url]`.
//!
//! Writing a file, editing one or running code needs no wrapper: the consent
//! is decided before the call, by action family, on the same card as every
//! other tool, so the executors are registered as they are.

use std::future::Future;
use std::pin::Pin;

use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use apollia_tools::tools::http_fetch::{HttpFetch, HttpFetchInput};
use serde_json::Value;

// ─── DynamicAllowlistHttpFetch ──────────────────────────────────────────────

/// Wraps `http_fetch` to preserve the Chat Libre behaviour where the host
/// of the requested URL is injected into the allowlist for that single
/// call. The stock [`HttpFetch`] executor is built once with a static
/// allowlist, in Chat Libre, every tool call is HITL-approved upstream,
/// so allowing the call-specific host is the right default.
pub struct DynamicAllowlistHttpFetch;

impl DynamicAllowlistHttpFetch {
    pub fn new() -> Self {
        Self
    }
}

impl ToolExecutor for DynamicAllowlistHttpFetch {
    fn name(&self) -> &str {
        "http_fetch"
    }

    fn is_read_only(&self) -> bool {
        // GET-style fetch is read-only from the agent's perspective even
        // though network I/O happens, keeps batching semantics intact.
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let parsed: HttpFetchInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: format!("http_fetch: invalid arguments: {e}"),
                })?;
            let hostname =
                extract_hostname(&parsed.url).ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "http_fetch: cannot parse hostname from URL".into(),
                })?;

            let tool = HttpFetch::new(Some(vec![hostname]));
            let output =
                tool.run(parsed)
                    .await
                    .map_err(|e| ToolExecutionError::ExecutionFailed {
                        code: "http_fetch".into(),
                        message: e.to_string(),
                    })?;
            serde_json::to_value(&output).map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialise".into(),
                message: e.to_string(),
            })
        })
    }
}

impl Default for DynamicAllowlistHttpFetch {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the hostname from a URL string. Minimal best-effort parser -
/// strips `scheme://`, takes everything before the next `/` or `:`.
fn extract_hostname(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_with_port = after_scheme.split('/').next()?;
    let host = host_with_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_hostname_strips_scheme() {
        // GIVEN URLs with a scheme, with a port, bare, and an empty string
        // WHEN the hostname is extracted from each
        // THEN only the host survives, and the empty string yields nothing
        assert_eq!(
            extract_hostname("https://api.example.com/foo").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            extract_hostname("http://localhost:8080/x").as_deref(),
            Some("localhost")
        );
        assert_eq!(
            extract_hostname("api.example.com").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(extract_hostname("").as_deref(), None);
    }
}
