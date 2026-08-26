//! The router's budget, context, and inventory accessors.
//!
//! Split out of `router.rs`: the cost ceiling, the context window, and the
//! backend inventory readers live here.

use tokio_util::sync::CancellationToken;

use apollia_core::token_budget::TokenBudget;
use apollia_core::CeilingAction;

use crate::router::LlmRouter;
use crate::types::{message_char_len, BackendInfo, ChatMessage};

impl LlmRouter {
    /// Return the configured LLM cost alert threshold in USD, or `None`.
    ///
    /// Maps to `[llm] cost_alert_threshold_usd` in `apollia.toml`.
    pub fn cost_alert_threshold_usd(&self) -> Option<f64> {
        self.session_budget.lock().ok()?.threshold_usd()
    }
    /// Return `true` when hybrid routing is configured and the per-session cost
    /// has reached or exceeded the configured ceiling.
    ///
    /// Returns `false` when no `[llm.routing.hybrid]` section is configured, so
    /// a caller can treat the absence of hybrid routing as "ceiling never hit".
    /// Reads the same `session_cost_usd` as [`Self::route_with_escalation`]
    /// under a short lock (a poisoned mutex reads as `0.0`), so a caller that
    /// invokes both with no intervening `await` observes a consistent decision.
    pub fn is_ceiling_reached(&self) -> bool {
        let Some(routing) = self.routing.as_ref() else {
            return false;
        };
        let Some(hybrid) = routing.hybrid.as_ref() else {
            return false;
        };
        let session_cost = self
            .session_budget
            .lock()
            .map(|t| t.session_cost_usd)
            .unwrap_or(0.0);
        session_cost >= hybrid.cost_ceiling_usd
    }
    /// Return the configured [`CeilingAction`] for the hybrid routing section,
    /// or [`CeilingAction::StayLocal`] when no hybrid section is configured.
    ///
    /// Used by the chat loop to decide, once the cost ceiling is reached,
    /// whether to stop the run cleanly or to keep going on the local backend.
    pub fn ceiling_action(&self) -> CeilingAction {
        self.routing
            .as_ref()
            .and_then(|r| r.hybrid.as_ref())
            .map(|h| h.ceiling_action)
            .unwrap_or_default()
    }
    /// Return the accumulated session cost in USD (0.0 if the budget lock is
    /// poisoned). Mirrors the value [`Self::is_ceiling_reached`] compares.
    pub fn session_cost_usd(&self) -> f64 {
        self.session_budget
            .lock()
            .map(|t| t.session_cost_usd)
            .unwrap_or(0.0)
    }
    /// Return the configured hybrid cost ceiling in USD, or `None` when no
    /// `[llm.routing.hybrid]` section is configured.
    pub fn cost_ceiling_usd(&self) -> Option<f64> {
        self.routing
            .as_ref()
            .and_then(|r| r.hybrid.as_ref())
            .map(|h| h.cost_ceiling_usd)
    }
    /// Seed the accumulated session cost in USD.
    ///
    /// Lets integration tests drive the hybrid cost ceiling deterministically
    /// when backends are injected directly via [`Self::with_backends`], without
    /// real token billing. A no-op if the budget lock is poisoned.
    pub fn seed_session_cost_usd(&self, usd: f64) {
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.session_cost_usd = usd;
        }
    }
    /// Return the context window size in tokens used to size compaction.
    ///
    /// Prefers the active backend's reported window (e.g. a local model's
    /// trained context, once loaded) so compaction fires before the model
    /// overflows. Falls back to `200_000` (the `claude-sonnet` window,
    /// conservative for cloud backends and before the local model has loaded).
    pub fn context_limit(&self) -> usize {
        self.backends
            .get(&self.default)
            .and_then(|backend| backend.context_window())
            .unwrap_or(200_000)
    }
    /// Return the active backend's context window in tokens, when it reports one.
    ///
    /// Unlike [`Self::context_limit`], this does not substitute a generic
    /// fallback: it returns `None` when the backend cannot report its window, so
    /// a caller can render an "unknown" state (e.g. a context gauge left at zero)
    /// instead of a misleading value.
    pub fn context_window_tokens(&self) -> Option<usize> {
        self.backends
            .get(&self.default)
            .and_then(|backend| backend.context_window())
    }
    /// Estimate the token count of `messages` using the default backend.
    ///
    /// Delegates to [`crate::types::CompletionModel::count_tokens`] on the default backend, so
    /// a local backend returns its real tokenizer count while cloud backends
    /// keep the `(chars / 4) * 1.2` proxy. When no backend is registered (empty
    /// router, degraded startup), applies the proxy inline so the caller never
    /// panics.
    pub fn count_tokens(&self, messages: &[ChatMessage]) -> usize {
        match self.backends.get(&self.default) {
            Some(backend) => backend.count_tokens(messages),
            None => {
                let total_chars: usize = messages.iter().map(message_char_len).sum();
                ((total_chars as f32) / 4.0 * 1.2) as usize
            }
        }
    }
    /// Return the session `CancellationToken` to cancel in-flight calls.
    ///
    /// Called by `ORIAEngine::abort()` to interrupt all in-flight LLM calls
    /// and retry delays across every backend of the router.
    ///
    /// The token is `Clone`: each backend holds a clone, all cancelled at once
    /// by `token.cancel()`.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }
    /// Return a snapshot of the token budget accumulated since the last reset.
    ///
    /// Called by `ORIAEngine` at task end to persist the cost in
    /// `~/.apollia/session_costs.jsonl`.
    pub fn session_budget(&self) -> TokenBudget {
        self.session_budget
            .lock()
            .map(|t| t.to_token_budget())
            .unwrap_or_default()
    }
    /// Reset the session counters.
    ///
    /// Called by `ORIAEngine` at the start of each task to isolate counters
    /// per run. The tracker configuration (bus, threshold) is preserved.
    pub fn reset_session_budget(&self) {
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.reset();
        }
    }
    /// List all available backends with their summary information.
    pub fn list(&self) -> Vec<BackendInfo> {
        self.backends
            .values()
            .map(|b| BackendInfo {
                name: b.backend_name().to_string(),
                model_id: b.model_id().to_string(),
                available: b.is_available(),
            })
            .collect()
    }
}
