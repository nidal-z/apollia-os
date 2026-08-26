//! Transport-retry policy for an MCP session.
//!
//! Split out of `session.rs`: the session stays in the parent, the backoff
//! schedule and the wrapper that replays a request on a transport error live
//! here.

use crate::session::McpSessionError;

/// Retry policy applied to tool calls that fail with a transport-level error.
///
/// Backoff formula: `delay = min(base_delay_secs × 2^attempt, max_delay_secs)`.
pub(super) struct McpRetryConfig {
    /// Maximum number of additional attempts after the first failure.
    pub(super) max_retries: u32,
    /// Base delay in seconds for the first retry.
    pub(super) base_delay_secs: u64,
    /// Upper bound on the computed delay.
    pub(super) max_delay_secs: u64,
}
impl McpRetryConfig {
    /// Default retry policy: 3 retries, 1s base, 8s cap.
    pub(super) const DEFAULT: Self = Self {
        max_retries: 3,
        base_delay_secs: 1,
        max_delay_secs: 8,
    };
}
/// Returns `true` for errors that originate from a transport failure and are
/// therefore candidates for a retry.
pub(super) fn is_transport_error(err: &McpSessionError) -> bool {
    matches!(
        err,
        McpSessionError::StdinClosed { .. } | McpSessionError::ServerExited { .. }
    )
}
/// Compute the exponential backoff delay for a given attempt index.
///
/// `attempt` is zero-based: attempt 0 → `base`, attempt 1 → `base × 2`, etc.
pub(super) fn compute_backoff_delay(
    attempt: u32,
    base_delay_secs: u64,
    max_delay_secs: u64,
) -> u64 {
    let factor = 2u64.saturating_pow(attempt);
    base_delay_secs.saturating_mul(factor).min(max_delay_secs)
}

/// Run `f` up to `retry_cfg.max_retries + 1` times, sleeping between retries on
/// transport errors.  Non-transport errors are propagated immediately.
pub(super) async fn with_transport_retry<F, Fut, T>(
    retry_cfg: &McpRetryConfig,
    server_name: &str,
    mut f: F,
) -> Result<T, McpSessionError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, McpSessionError>>,
{
    for attempt in 0..=retry_cfg.max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(err) if is_transport_error(&err) && attempt < retry_cfg.max_retries => {
                let delay_secs = compute_backoff_delay(
                    attempt,
                    retry_cfg.base_delay_secs,
                    retry_cfg.max_delay_secs,
                );
                tracing::warn!(
                    attempt = attempt + 1,
                    max = retry_cfg.max_retries,
                    delay_ms = delay_secs * 1000,
                    server = %server_name,
                    reason = "transport error",
                    "mcp.call.retrying"
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            }
            Err(err) => return Err(err),
        }
    }
    // Unreachable: the loop always returns on the last attempt.
    unreachable!("retry loop must return within max_retries + 1 iterations")
}
