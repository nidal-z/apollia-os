//! Politique de retry exponentiel partagée par tous les backends LLM.
//!
//! # Utilisation
//!
//! ```rust,ignore
//! let policy = RetryPolicy::default();
//! let cancel = CancellationToken::new();
//! let response = policy.execute(cancel, || async { backend.complete(req.clone()).await }).await?;
//! ```
//!
//! Le délai entre les tentatives est `base_delay_ms * 2^attempt`, plafonné via
//! `attempt.min(10)`. Si le `CancellationToken` est déclenché pendant l'attente,
//! l'exécution s'arrête immédiatement avec [`LlmError::Cancelled`].

use std::future::Future;

use tokio_util::sync::CancellationToken;

use crate::types::LlmError;

// ─────────────────────────────────────────────
// Traits
// ─────────────────────────────────────────────

/// Permet de déterminer si une erreur justifie une nouvelle tentative.
///
/// Implémenté sur [`LlmError`] pour les variantes transitoires (rate limit,
/// surcharge serveur, service indisponible). Les erreurs non-retryables
/// (authentification, requête invalide) échouent immédiatement.
pub trait IsRetryable {
    /// Retourne `true` si l'erreur est transitoire et justifie un retry.
    fn is_retryable(&self) -> bool;
}

/// Permet de construire une erreur représentant une annulation par l'appelant.
///
/// Implémenté sur [`LlmError`] pour permettre à [`RetryPolicy::execute`]
/// de retourner [`LlmError::Cancelled`] quand le `CancellationToken` est déclenché.
pub trait IsCancelled {
    /// Construit une instance de l'erreur représentant une annulation.
    fn cancelled() -> Self;
}

// ─────────────────────────────────────────────
// Implémentations pour LlmError
// ─────────────────────────────────────────────

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

// ─────────────────────────────────────────────
// RetryPolicy
// ─────────────────────────────────────────────

/// Politique de retry exponentiel partagée par tous les backends LLM.
///
/// Encapsule le nombre maximum de tentatives, le délai de base, et les codes
/// HTTP considérés comme retryables (à titre documentaire — la décision de retry
/// est déléguée au trait [`IsRetryable`] sur l'erreur).
///
/// Cloneable et `Send + Sync` — peut être stockée dans un `Arc` ou partagée
/// entre backends via composition.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Nombre maximum de tentatives (inclut la première). Défaut : `10`.
    pub max_attempts: u32,
    /// Délai de base en millisecondes pour le premier retry. Défaut : `500`.
    pub base_delay_ms: u64,
    /// Codes HTTP déclenchant un retry — à titre documentaire. Défaut : `[429, 503, 529]`.
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
    /// Exécute `f` avec retry exponentiel et abort sur `CancellationToken`.
    ///
    /// Boucle jusqu'à `max_attempts` tentatives :
    /// - `Ok(val)` → retourne immédiatement `Ok(val)`.
    /// - `Err(e)` non-retryable → retourne immédiatement `Err(e)` (fail-fast).
    /// - `Err(e)` retryable → attend `base_delay_ms * 2^attempt` ms,
    ///   en interrompant l'attente si `cancel` est déclenché.
    ///
    /// Si `cancel` est déclenché pendant l'attente, retourne `E::cancelled()`.
    /// Si `max_attempts` est atteint, retourne la dernière erreur retryable.
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
                        "LLM rate limited or unavailable, retrying"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)) => {}
                        _ = cancel.cancelled() => return Err(E::cancelled()),
                    }
                }
            }
        }
        // Unreachable : la boucle retourne toujours avant d'atteindre cette ligne.
        unreachable!("retry loop must always return before exhausting attempts")
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

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
    // THEN only transitoires errors return true
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
