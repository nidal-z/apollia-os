//! Exponential retry policy shared by all LLM backends.
//!
//! # Usage
//!
//! ```rust,ignore
//! let policy = RetryPolicy::default();
//! let cancel = CancellationToken::new();
//! let response = policy.execute(cancel, || async { backend.complete(req.clone()).await }).await?;
//! ```
//!
//! The delay between attempts is `base_delay_ms * 2^attempt`, capped via
//! `attempt.min(10)`. If the `CancellationToken` fires during the wait,
//! execution stops immediately with [`LlmError::Cancelled`].

use std::future::Future;

use tokio_util::sync::CancellationToken;

use crate::types::LlmError;

/// Decides whether an error warrants a new attempt.
///
/// Implemented on [`LlmError`] for the transient variants (rate limit, server
/// overload, service unavailable). Non-retryable errors (authentication,
/// invalid request) fail immediately.
pub trait IsRetryable {
    /// Returns `true` if the error is transient and warrants a retry.
    fn is_retryable(&self) -> bool;
}

/// Builds an error that represents cancellation by the caller.
///
/// Implemented on [`LlmError`] so [`RetryPolicy::execute`] can return
/// [`LlmError::Cancelled`] when the `CancellationToken` fires.
pub trait IsCancelled {
    /// Build an instance of the error that represents a cancellation.
    fn cancelled() -> Self;
}

impl IsRetryable for LlmError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimit | LlmError::Overload | LlmError::ServiceUnavailable
        )
    }
}

impl IsCancelled for LlmError {
    fn cancelled() -> Self {
        LlmError::Cancelled
    }
}

/// Exponential retry policy shared by all LLM backends.
///
/// Holds the maximum number of attempts, the base delay, and the HTTP codes
/// treated as retryable (documentation only: the retry decision is delegated
/// to the [`IsRetryable`] trait on the error).
///
/// Cloneable and `Send + Sync`: can be stored in an `Arc` or shared between
/// backends via composition.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (includes the first). Default: `10`.
    pub max_attempts: u32,
    /// Base delay in milliseconds for the first retry. Default: `500`.
    pub base_delay_ms: u64,
    /// HTTP codes that trigger a retry, documentation only. Default: `[429, 503, 529]`.
    pub retryable_codes: Vec<u16>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            base_delay_ms: 500,
            retryable_codes: vec![429, 503, 529],
        }
    }
}

impl RetryPolicy {
    /// Run `f` with exponential retry and abort on `CancellationToken`.
    ///
    /// Loops up to `max_attempts` times:
    /// - `Ok(val)`: returns `Ok(val)` immediately.
    /// - non-retryable `Err(e)`: returns `Err(e)` immediately (fail-fast).
    /// - retryable `Err(e)`: waits `base_delay_ms * 2^attempt` ms, interrupting
    ///   the wait if `cancel` fires.
    ///
    /// If `cancel` fires during the wait, returns `E::cancelled()`.
    /// If `max_attempts` is reached, returns the last retryable error.
    pub async fn execute<F, Fut, T, E>(&self, cancel: CancellationToken, f: F) -> Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: IsRetryable + IsCancelled,
    {
        for attempt in 0..self.max_attempts {
            match f().await {
                Ok(val) => return Ok(val),
                Err(e) if !e.is_retryable() => return Err(e),
                Err(e) => {
                    if attempt + 1 >= self.max_attempts {
                        return Err(e);
                    }
                    let delay_ms = self.base_delay_ms * (1u64 << attempt.min(10));
                    tracing::warn!(
                        attempt = attempt + 1,
                        delay_ms,
                        reason = "rate limited or unavailable",
                        "llm.request.retrying"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)) => {}
                        _ = cancel.cancelled() => return Err(E::cancelled()),
                    }
                }
            }
        }
        // The loop always returns before reaching this line.
        unreachable!("retry loop must always return before exhausting attempts")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::types::LlmError;

    // GIVEN a backend that returns RateLimit x3 then Ok
    // WHEN execute() is called with max_attempts = 10
    // THEN the final result is Ok and 4 attempts were made
    #[tokio::test]
    async fn test_retry_succeeds_after_transient_errors() {
        let counter = Arc::new(AtomicU32::new(0));
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay_ms: 1,
            retryable_codes: vec![429],
        };
        let cancel = CancellationToken::new();
        let c = counter.clone();

        let result: Result<&str, LlmError> = policy
            .execute(cancel, || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 3 {
                        Err(LlmError::RateLimit)
                    } else {
                        Ok("success")
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    // GIVEN a backend that always returns RateLimit
    // WHEN execute() is called with max_attempts = 3
    // THEN the error is RateLimit and exactly 3 attempts were made
    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let counter = Arc::new(AtomicU32::new(0));
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 1,
            retryable_codes: vec![429],
        };
        let cancel = CancellationToken::new();
        let c = counter.clone();

        let result: Result<(), LlmError> = policy
            .execute(cancel, || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(LlmError::RateLimit)
                }
            })
            .await;

        assert!(matches!(result, Err(LlmError::RateLimit)));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    // GIVEN a backend that returns Unauthorized (non-retryable)
    // WHEN execute() is called
    // THEN the error is returned immediately with exactly 1 attempt
    #[tokio::test]
    async fn test_non_retryable_error_fails_immediately() {
        let counter = Arc::new(AtomicU32::new(0));
        let policy = RetryPolicy::default();
        let cancel = CancellationToken::new();
        let c = counter.clone();

        let result: Result<(), LlmError> = policy
            .execute(cancel, || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(LlmError::Unauthorized)
                }
            })
            .await;

        assert!(matches!(result, Err(LlmError::Unauthorized)));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // GIVEN a retry delay of 10s and a CancellationToken fired after 5ms
    // WHEN execute() is waiting for the retry delay
    // THEN execution stops well before 10s and returns Cancelled
    #[tokio::test]
    async fn test_cancel_during_retry_delay() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay_ms: 10_000,
            retryable_codes: vec![429],
        };
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            cancel_clone.cancel();
        });

        let start = std::time::Instant::now();
        let result: Result<(), LlmError> = policy
            .execute(cancel, || async { Err(LlmError::RateLimit) })
            .await;

        assert!(matches!(result, Err(LlmError::Cancelled)));
        assert!(
            start.elapsed().as_millis() < 500,
            "should cancel well before the 10s delay, took {}ms",
            start.elapsed().as_millis()
        );
    }

    // GIVEN IsRetryable is implemented for LlmError
    // WHEN is_retryable() is called on each variant
    // THEN only transient errors return true
    #[test]
    fn test_is_retryable_variants() {
        assert!(LlmError::RateLimit.is_retryable());
        assert!(LlmError::Overload.is_retryable());
        assert!(LlmError::ServiceUnavailable.is_retryable());
        assert!(!LlmError::Unauthorized.is_retryable());
        assert!(!LlmError::Cancelled.is_retryable());
        assert!(!LlmError::BadRequest("bad".into()).is_retryable());
        assert!(!LlmError::BudgetExceeded.is_retryable());
    }
}
