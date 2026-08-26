//! Backend selection, from a name or from an escalation level.
//!
//! Split out of `router.rs`: the router's state stays in the parent, the
//! decisions that pick which backend answers live here.

use std::sync::Arc;

use crate::router::backends::no_backend;
use crate::router::LlmRouter;
use crate::routing_level::{EscalationSignal, LlmRoutingLevel};
use crate::types::{CompletionModel, LlmError};

impl LlmRouter {
    /// Return the backend by name, or the default backend if `name` is `None`.
    ///
    /// Returns `None` if the requested backend is not in the router.
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>> {
        let key = name.unwrap_or(&self.default);
        self.backends.get(key).cloned()
    }
    /// Return the backend for `llm_backend`, or the default if `None` / unknown.
    ///
    /// Emits `tracing::warn!` if the named backend is absent from the router
    /// (silent fallback apart from the structured log).
    ///
    /// A router built via [`LlmRouter::empty()`] holds no default, and routing
    /// on it returns [`no_backend`], whose every call reports
    /// [`LlmError::BackendUnavailable`]. That case used to end in a panic
    /// asserting an invariant the empty constructor never established.
    pub fn route(&self, llm_backend: Option<&str>) -> Arc<dyn CompletionModel> {
        if let Some(name) = llm_backend {
            if let Some(backend) = self.backends.get(name) {
                return backend.clone();
            }
            tracing::warn!(
                backend = %name,
                fallback = %self.default,
                detail = "routing to the default backend",
                "llm.backend.unknown"
            );
        }
        self.backends
            .get(&self.default)
            .cloned()
            .unwrap_or_else(no_backend)
    }
    /// Return the names of all backends loaded in the router.
    pub fn backend_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.backends.keys().cloned().collect();
        names.sort();
        names
    }
    /// Return the default backend name configured in `apollia.toml`.
    pub fn default_name(&self) -> &str {
        &self.default
    }
    /// Return the backend configured for deep reasoning tasks.
    ///
    /// Selects the backend named in `[llm.routing] precise` of `apollia.toml`.
    /// Used by components that need maximum reasoning quality (ORIA planning,
    /// complex analysis, judgment).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendNotFound`]: `[llm.routing] precise` names a backend
    ///   that is not instantiated.
    /// - [`LlmError::RoutingConfigMissing`]: no `[llm.routing]` AND no resolvable
    ///   default backend (empty router or invalid default).
    ///
    /// # Fallback
    ///
    /// When `[llm.routing]` is not configured explicitly (the default case
    /// after a plain `apollia-os llm backends set-default <name>`), the router
    /// falls back to `self.default`. This is the documented promise: a
    /// single-backend setup must work for orchestrated agents without an
    /// explicit `[llm.routing]` config.
    pub fn route_precise(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        // Explicit case: `[llm.routing] precise = "<name>"`. If the named
        // backend exists, use it; otherwise a structured error (invalid config).
        if let Some(routing) = self.routing.as_ref() {
            return self
                .backends
                .get(&routing.precise)
                .cloned()
                .ok_or_else(|| LlmError::BackendNotFound(routing.precise.clone()));
        }
        // Fallback: no routing means the default backend.
        self.fallback_default("precise")
    }
    /// Return the backend configured for light extraction tasks.
    ///
    /// Selects the backend named in `[llm.routing] fast` of `apollia.toml`.
    /// Used by components doing deterministic extraction (metadata, short
    /// summaries, classification, path parsing).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendNotFound`]: `[llm.routing] fast` names a backend
    ///   that is not instantiated.
    /// - [`LlmError::RoutingConfigMissing`]: no `[llm.routing]` AND no resolvable
    ///   default backend.
    ///
    /// # Fallback
    ///
    /// See [`route_precise`](Self::route_precise): same fallback rules on
    /// `self.default` when `[llm.routing]` is not explicit.
    pub fn route_fast(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        if let Some(routing) = self.routing.as_ref() {
            return self
                .backends
                .get(&routing.fast)
                .cloned()
                .ok_or_else(|| LlmError::BackendNotFound(routing.fast.clone()));
        }
        self.fallback_default("fast")
    }
    /// Resolve the default backend when `[llm.routing]` is not configured.
    ///
    /// Consistent with the single-backend promise (`set-default` is enough).
    /// Returns the backend named by `self.default` if it exists; otherwise
    /// returns [`LlmError::RoutingConfigMissing`] with a message that guides
    /// the operator.
    ///
    /// `role` is only used for the structured log ("precise" or "fast").
    fn fallback_default(&self, role: &str) -> Result<Arc<dyn CompletionModel>, LlmError> {
        if self.default.is_empty() || self.backends.is_empty() {
            return Err(LlmError::RoutingConfigMissing);
        }
        match self.backends.get(&self.default).cloned() {
            Some(backend) => {
                tracing::debug!(
                    role = %role,
                    backend = %self.default,
                    detail = "routing to the default backend",
                    "llm.routing.missing"
                );
                Ok(backend)
            }
            None => Err(LlmError::RoutingConfigMissing),
        }
    }
    /// Return the backend to use for this step, applying the escalation policy.
    ///
    /// Decision tree, in order:
    /// 1. No `[llm.routing.hybrid]` configured: return the local backend for `level`.
    /// 2. Signal is [`EscalationSignal::None`]: return the local backend.
    /// 3. `session_cost_usd >= cost_ceiling_usd`: return local, emit `tracing::warn!`.
    /// 4. Frontier backend absent from the router: return local, emit `tracing::warn!`.
    /// 5. Otherwise: return the frontier backend.
    ///
    /// Never returns an error: degradation is always local, never a crash
    /// (Principle #1 local-first; Principle #4 fail fast at startup, not runtime).
    /// The per-session cost ceiling is consulted before every escalation and is
    /// not bypassable (Principle #7).
    pub fn route_with_escalation(
        &self,
        signal: EscalationSignal,
        level: LlmRoutingLevel,
    ) -> Arc<dyn CompletionModel> {
        // Local fallback for `level`. `route_precise`/`route_fast` only error when
        // routing is misconfigured (caught at startup); `route(None)` is the
        // infallible last resort on the default backend, avoiding any `expect` here.
        let local = || match level {
            LlmRoutingLevel::Precise => self.route_precise().unwrap_or_else(|_| self.route(None)),
            LlmRoutingLevel::Fast => self.route_fast().unwrap_or_else(|_| self.route(None)),
        };

        // 1. No hybrid configuration: stay local.
        let Some(routing) = self.routing.as_ref() else {
            return local();
        };
        let Some(hybrid) = routing.hybrid.as_ref() else {
            return local();
        };

        // 2. No escalation requested.
        if !signal.is_escalation() {
            return local();
        }

        // 3. Cost ceiling check. A poisoned mutex reads as 0.0 (cost unknown,
        // conservative: it permits the escalation rather than blocking work).
        let session_cost = self
            .session_budget
            .lock()
            .map(|t| t.session_cost_usd)
            .unwrap_or(0.0);
        if session_cost >= hybrid.cost_ceiling_usd {
            tracing::warn!(
                session_cost_usd = session_cost,
                ceiling_usd = hybrid.cost_ceiling_usd,
                signal = ?signal,
                reason = "the session cost ceiling is reached",
                "llm.hybrid.escalation.blocked"
            );
            return local();
        }

        // 4 + 5. Frontier availability.
        match self.backends.get(&hybrid.frontier) {
            Some(backend) => {
                tracing::info!(
                    frontier = %hybrid.frontier,
                    session_cost_usd = session_cost,
                    ceiling_usd = hybrid.cost_ceiling_usd,
                    signal = ?signal,
                    "llm.hybrid.escalation.routed"
                );
                backend.clone()
            }
            None => {
                tracing::warn!(
                    frontier = %hybrid.frontier,
                    signal = ?signal,
                    reason = "the frontier backend is absent from the router",
                    "llm.hybrid.escalation.blocked"
                );
                local()
            }
        }
    }
}
