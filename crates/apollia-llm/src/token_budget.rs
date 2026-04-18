//! Suivi du budget de session LLM avec émission d'événements temps réel.
//!
//! [`SessionBudgetTracker`] accumule les tokens et coûts de chaque appel LLM
//! et émet [`RuntimeEvent::TokenBudgetUpdated`] sur le bus après chaque enregistrement.
//! L'émission est non-bloquante — les erreurs d'envoi sont silencieusement ignorées.

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::token_budget::TokenBudget;

use crate::types::TokenUsage;

/// Suivi du budget de tokens pour une session LLM avec émission temps réel.
///
/// Construit par [`LlmRouter`](crate::router::LlmRouter) au démarrage via
/// [`SessionBudgetTracker::new`]. Protégé par un `Mutex` — verrou tenu uniquement
/// le temps de la mise à jour des compteurs, jamais pendant un appel async.
///
/// Émet [`RuntimeEvent::TokenBudgetUpdated`] après chaque [`record_usage`](Self::record_usage).
/// Le desktop widget s'abonne à cet événement pour afficher le coût en temps réel.
#[derive(Debug, Clone)]
pub struct SessionBudgetTracker {
    /// Coût total de la session en USD cumulé depuis le dernier reset.
    pub session_cost_usd: f64,
    /// Tokens en entrée cumulés depuis le dernier reset.
    pub total_input_tokens: u64,
    /// Tokens en sortie cumulés depuis le dernier reset.
    pub total_output_tokens: u64,
    /// Tokens lus depuis le cache Anthropic cumulés depuis le dernier reset.
    pub total_cache_read_tokens: u64,
    /// Tokens écrits dans le cache Anthropic cumulés depuis le dernier reset.
    pub total_cache_write_tokens: u64,
    /// Latence cumulée des appels API en millisecondes.
    pub api_duration_ms: u64,
    /// Time to First Token pour le premier appel streaming de la session.
    pub ttft_ms: Option<u64>,
    /// Seuil de coût en USD configuré par l'opérateur. `f64::MAX` si non configuré.
    threshold_usd: f64,
    /// Bus d'événements pour l'émission temps réel. `None` si non configuré.
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
    /// Construit un tracker avec bus d'événements et seuil de coût.
    ///
    /// `event_tx: None` désactive l'émission sans changer l'accumulation.
    /// `threshold_usd: None` désactive les alertes de seuil (`threshold_exceeded` sera toujours `false`).
    pub fn new(event_tx: Option<EventBusSender>, threshold_usd: Option<f64>) -> Self {
        Self {
            threshold_usd: threshold_usd.unwrap_or(f64::MAX),
            event_tx,
            ..Default::default()
        }
    }

    /// Accumule les compteurs d'un appel LLM et émet [`RuntimeEvent::TokenBudgetUpdated`].
    ///
    /// Le verrou sur ce tracker doit être tenu le temps de cet appel uniquement —
    /// l'émission sur le bus se fait via `send()`, non-bloquant (broadcast channel).
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

    /// Remet à zéro tous les compteurs en conservant la configuration (bus, seuil).
    pub fn reset(&mut self) {
        self.session_cost_usd = 0.0;
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
        self.total_cache_read_tokens = 0;
        self.total_cache_write_tokens = 0;
        self.api_duration_ms = 0;
        self.ttft_ms = None;
    }

    /// Retourne le seuil de coût configuré en USD, ou `None` si non configuré.
    ///
    /// Retourne `None` quand la valeur interne vaut `f64::MAX` (seuil désactivé).
    pub fn threshold_usd(&self) -> Option<f64> {
        if self.threshold_usd < f64::MAX {
            Some(self.threshold_usd)
        } else {
            None
        }
    }

    /// Convertit le snapshot courant en [`TokenBudget`] pour l'API REST.
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
