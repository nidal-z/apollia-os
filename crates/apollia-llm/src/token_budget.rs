//! LLM session budget tracking with real-time event emission.
//!
//! [`SessionBudgetTracker`] accumulates the tokens and cost of each LLM call
//! and emits [`RuntimeEvent::TokenBudgetUpdated`] on the bus after each record.
//! Emission is non-blocking: send errors are silently ignored.

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::token_budget::TokenBudget;

use crate::types::TokenUsage;

/// Token budget tracking for an LLM session with real-time emission.
///
/// Built by [`LlmRouter`](crate::router::LlmRouter) at startup via
/// [`SessionBudgetTracker::new`]. Guarded by a `Mutex`: the lock is held only
/// while updating counters, never across an async call.
///
/// Emits [`RuntimeEvent::TokenBudgetUpdated`] after each
/// [`record_usage`](Self::record_usage). The desktop widget subscribes to this
/// event to display the cost in real time.
#[derive(Debug, Clone)]
pub struct SessionBudgetTracker {
    /// Total session cost in USD accumulated since the last reset.
    pub session_cost_usd: f64,
    /// Input tokens accumulated since the last reset.
    pub total_input_tokens: u64,
    /// Output tokens accumulated since the last reset.
    pub total_output_tokens: u64,
    /// Anthropic cache-read tokens accumulated since the last reset.
    pub total_cache_read_tokens: u64,
    /// Anthropic cache-write tokens accumulated since the last reset.
    pub total_cache_write_tokens: u64,
    /// Cumulative API call latency in milliseconds.
    pub api_duration_ms: u64,
    /// Time to first token for the session's first streaming call.
    pub ttft_ms: Option<u64>,
    /// Operator-configured cost threshold in USD. `f64::MAX` if not configured.
    threshold_usd: f64,
    /// Event bus for real-time emission. `None` if not configured.
    event_tx: Option<EventBusSender>,
}

impl Default for SessionBudgetTracker {
    fn default() -> Self {
        Self {
            session_cost_usd: 0.0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read_tokens: 0,
            total_cache_write_tokens: 0,
            api_duration_ms: 0,
            ttft_ms: None,
            threshold_usd: f64::MAX,
            event_tx: None,
        }
    }
}

impl SessionBudgetTracker {
    /// Build a tracker with an event bus and a cost threshold.
    ///
    /// `event_tx: None` disables emission without changing accumulation.
    /// `threshold_usd: None` disables threshold alerts (`threshold_exceeded`
    /// is always `false`).
    pub fn new(event_tx: Option<EventBusSender>, threshold_usd: Option<f64>) -> Self {
        Self {
            threshold_usd: threshold_usd.unwrap_or(f64::MAX),
            event_tx,
            ..Default::default()
        }
    }

    /// Accumulate the counters of an LLM call and emit [`RuntimeEvent::TokenBudgetUpdated`].
    ///
    /// The lock on this tracker should be held only for the duration of this
    /// call: emission on the bus goes through `send()`, non-blocking (broadcast
    /// channel).
    pub fn record_usage(&mut self, usage: &TokenUsage, api_ms: u64, ttft_ms: Option<u64>) {
        self.total_input_tokens += u64::from(usage.prompt_tokens);
        self.total_output_tokens += u64::from(usage.completion_tokens);
        self.total_cache_read_tokens += u64::from(usage.cache_read_input_tokens);
        self.total_cache_write_tokens += u64::from(usage.cache_write_input_tokens);
        self.session_cost_usd += usage.cost_usd.unwrap_or(0.0);
        self.api_duration_ms += api_ms;
        if self.ttft_ms.is_none() {
            self.ttft_ms = ttft_ms;
        }
        self.emit_update_event();
    }

    /// Reset all counters while preserving the configuration (bus, threshold).
    pub fn reset(&mut self) {
        self.session_cost_usd = 0.0;
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
        self.total_cache_read_tokens = 0;
        self.total_cache_write_tokens = 0;
        self.api_duration_ms = 0;
        self.ttft_ms = None;
    }

    /// Return the configured cost threshold in USD, or `None` if not configured.
    ///
    /// Returns `None` when the internal value is `f64::MAX` (threshold disabled).
    pub fn threshold_usd(&self) -> Option<f64> {
        if self.threshold_usd < f64::MAX {
            Some(self.threshold_usd)
        } else {
            None
        }
    }

    /// Convert the current snapshot into a [`TokenBudget`] for the REST API.
    pub fn to_token_budget(&self) -> TokenBudget {
        TokenBudget {
            input_tokens: self.total_input_tokens,
            output_tokens: self.total_output_tokens,
            cache_read_tokens: self.total_cache_read_tokens,
            cache_write_tokens: self.total_cache_write_tokens,
            cost_usd: self.session_cost_usd,
            api_duration_ms: self.api_duration_ms,
            ttft_ms: self.ttft_ms,
            ..Default::default()
        }
    }

    fn emit_update_event(&self) {
        if let Some(tx) = &self.event_tx {
            let threshold_exceeded = self.session_cost_usd > self.threshold_usd;
            let _ = tx.send(RuntimeEvent::TokenBudgetUpdated {
                session_cost_usd: self.session_cost_usd,
                total_input_tokens: self.total_input_tokens,
                total_output_tokens: self.total_output_tokens,
                total_cache_read_tokens: self.total_cache_read_tokens,
                threshold_usd: self.threshold_usd,
                threshold_exceeded,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    fn usage(cost: f64) -> TokenUsage {
        TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            cost_usd: Some(cost),
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 0,
        }
    }

    #[tokio::test]
    async fn token_budget_emits_event_after_record() {
        // GIVEN tracker with event bus and pricing
        let (tx, mut rx) = broadcast::channel(16);
        let mut tracker = SessionBudgetTracker::new(Some(tx), Some(1.0));

        // WHEN record_usage called once
        tracker.record_usage(&usage(0.01), 300, None);

        // THEN TokenBudgetUpdated received on rx
        let event = rx.try_recv().expect("event should be emitted");
        assert!(
            matches!(event, RuntimeEvent::TokenBudgetUpdated { session_cost_usd, .. } if (session_cost_usd - 0.01).abs() < 1e-9)
        );
    }

    #[tokio::test]
    async fn session_cost_increments_correctly() {
        // GIVEN tracker at $0.000
        let (tx, mut rx) = broadcast::channel(16);
        let mut tracker = SessionBudgetTracker::new(Some(tx), Some(1.0));

        // WHEN 3x record_usage with different costs
        tracker.record_usage(&usage(0.010), 100, None);
        tracker.record_usage(&usage(0.020), 150, None);
        tracker.record_usage(&usage(0.015), 120, None);

        // THEN session_cost_usd = sum of 3 costs
        let expected = 0.010 + 0.020 + 0.015;
        assert!((tracker.session_cost_usd - expected).abs() < 1e-9);
        // AND 3 events emitted
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn threshold_exceeded_when_over_limit() {
        // GIVEN threshold = 0.50 and cost will be 0.51
        let (tx, mut rx) = broadcast::channel(16);
        let mut tracker = SessionBudgetTracker::new(Some(tx), Some(0.50));

        // WHEN record_usage pushes cost above threshold
        tracker.record_usage(&usage(0.51), 0, None);

        // THEN threshold_exceeded = true
        let event = rx.try_recv().expect("event emitted");
        assert!(matches!(
            event,
            RuntimeEvent::TokenBudgetUpdated {
                threshold_exceeded: true,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn threshold_not_exceeded_when_under_limit() {
        // GIVEN threshold = 0.50 and cost will be 0.10
        let (tx, mut rx) = broadcast::channel(16);
        let mut tracker = SessionBudgetTracker::new(Some(tx), Some(0.50));

        // WHEN record_usage with cost below threshold
        tracker.record_usage(&usage(0.10), 0, None);

        // THEN threshold_exceeded = false
        let event = rx.try_recv().expect("event emitted");
        assert!(matches!(
            event,
            RuntimeEvent::TokenBudgetUpdated {
                threshold_exceeded: false,
                ..
            }
        ));
    }

    #[test]
    fn reset_clears_counters_preserves_config() {
        // GIVEN tracker with accumulated usage
        let (tx, _rx) = broadcast::channel(16);
        let mut tracker = SessionBudgetTracker::new(Some(tx.clone()), Some(0.5));
        tracker.record_usage(&usage(0.01), 100, Some(50));

        // WHEN reset
        tracker.reset();

        // THEN counters are zeroed
        assert_eq!(tracker.session_cost_usd, 0.0);
        assert_eq!(tracker.total_input_tokens, 0);
        assert_eq!(tracker.total_output_tokens, 0);
        assert_eq!(tracker.ttft_ms, None);
        // AND threshold preserved
        assert!((tracker.threshold_usd - 0.5).abs() < 1e-9);
    }

    #[test]
    fn to_token_budget_maps_fields_correctly() {
        // GIVEN tracker with known counters
        let tracker = SessionBudgetTracker {
            session_cost_usd: 0.05,
            total_input_tokens: 200,
            total_output_tokens: 100,
            total_cache_read_tokens: 50,
            ..Default::default()
        };

        // WHEN converting to TokenBudget
        let budget = tracker.to_token_budget();

        // THEN fields map correctly
        assert_eq!(budget.input_tokens, 200);
        assert_eq!(budget.output_tokens, 100);
        assert_eq!(budget.cache_read_tokens, 50);
        assert!((budget.cost_usd - 0.05).abs() < 1e-9);
    }
}
