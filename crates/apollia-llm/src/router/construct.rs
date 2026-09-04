//! The router's constructors.
//!
//! Split out of `router.rs`: the routing and completion paths stay elsewhere,
//! every way of building an `LlmRouter` lives here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use apollia_core::events::EventBusSender;
use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmRoutingConfig};

use crate::router::backends::{
    build_backend, insert_vertex_backend, insert_vertex_backend_with_bus, instantiate_from_config,
    load_backend_with_bus, resolve_default_backend,
};
use crate::router::config::LlmConfig;
use crate::router::validate_routing;
use crate::router::LlmRouter;
use crate::token_budget::SessionBudgetTracker;
use crate::types::{CompletionModel, LlmError};

impl LlmRouter {
    /// Build the router from an `apollia.toml` [`LlmConfig`].
    ///
    /// No production path calls this: the daemon builds its router from
    /// `system.db` through
    /// [`from_repository_with_override`](Self::from_repository_with_override).
    /// Everything this constructor reads and that one does not, `[llm.vertex]`
    /// and `[llm.pricing_overrides]`, is therefore inert on a running daemon.
    ///
    /// Iterates over `config.backends` and tries to instantiate each backend.
    /// For `Api`: resolves the API key; if missing, logs `tracing::warn!` and
    /// skips the backend.
    ///
    /// After the loop, checks that `config.default` is present in the map.
    /// If absent (unconfigured or skipped) returns [`LlmError::BackendUnavailable`].
    ///
    /// # Errors
    ///
    /// - [`LlmError::ModelNotFound`] / [`LlmError::InferenceError`]: `.gguf` load failed.
    /// - [`LlmError::BackendUnavailable`]: the default backend is missing or unavailable.
    pub async fn from_config(config: &LlmConfig) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            let name = backend_cfg.name().to_owned();

            #[cfg(feature = "cloud")]
            let result = build_backend(backend_cfg, config, &cancellation_token);
            #[cfg(not(feature = "cloud"))]
            let result: Result<Arc<dyn CompletionModel>, LlmError> = match &backend_cfg.kind {};

            match result {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        reason = "API key missing",
                        "llm.backend.skipped"
                    );
                    continue;
                }
            }
        }

        // Vertex AI is instantiated separately from [llm.vertex] when enabled = true.
        #[cfg(feature = "cloud")]
        insert_vertex_backend(&mut backends, config, &cancellation_token);

        // The default backend must be available after the loop.
        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        validate_routing(&backends, config.routing.as_ref())?;

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }
    /// Build the router from configuration with EventBus observability.
    ///
    /// Variant of [`from_config`](Self::from_config) with event emission. Like
    /// `from_config`, no production path calls it, so `cost_alert_threshold_usd`
    /// (the one field it reads that `from_config` does not) never reaches a
    /// running daemon's [`SessionBudgetTracker`].
    ///
    /// Emits on the bus for each backend:
    /// - [`RuntimeEvent::LlmModelLoading`]: before loading
    /// - [`RuntimeEvent::LlmModelReady`]: when loading succeeds
    /// - [`RuntimeEvent::LlmModelFailed`]: when loading fails (backend skipped, no crash)
    ///
    /// The `EventBusSender` is optional: `None` disables event emission without
    /// changing functional behavior.
    ///
    /// Unlike [`from_config`](Self::from_config), per-backend load errors
    /// (missing `.gguf`, etc.) are logged and emitted as `LlmModelFailed` but
    /// do not propagate an error; the router continues with the available backends.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the default backend is missing after
    ///   all backends have been tried.
    pub async fn from_config_with_bus(
        config: &LlmConfig,
        bus: Option<EventBusSender>,
    ) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            load_backend_with_bus(
                &mut backends,
                backend_cfg,
                config,
                &cancellation_token,
                &bus,
            );
        }

        // Vertex AI is instantiated separately from [llm.vertex] when enabled = true.
        #[cfg(feature = "cloud")]
        insert_vertex_backend_with_bus(&mut backends, config, &cancellation_token, &bus);

        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        validate_routing(&backends, config.routing.as_ref())?;

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::new(
                bus,
                config.cost_alert_threshold_usd,
            ))),
        })
    }
    /// Build an `LlmRouter` from already-instantiated backends.
    ///
    /// Used in integration tests to inject [`CompletionModel`] mocks without
    /// going through the TOML configuration. LLM routing is not configured on
    /// this router, so `route_precise()` and `route_fast()` will return
    /// [`LlmError::RoutingConfigMissing`].
    ///
    /// # Panics
    ///
    /// Panics if `default` is not present in `backends`.
    pub fn with_backends(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: impl Into<String>,
    ) -> Self {
        let default = default.into();
        assert!(
            backends.contains_key(&default),
            "LlmRouter::with_backends - backend '{default}' must be present in backends map"
        );
        Self {
            backends,
            default,
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }
    /// Build the router from an already-loaded list of [`LlmBackendConfig`].
    ///
    /// Variant of [`from_repository`](Self::from_repository) that takes the
    /// configs directly (useful when the SQLite repository has already been
    /// read in a blocking thread, e.g. via `spawn_blocking`).
    ///
    /// Only `enabled = true` backends are instantiated. Backends that fail are
    /// logged and skipped (non-fatal degradation).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`] if `default_name` is not instantiated successfully.
    pub async fn from_backend_configs(
        all: Vec<LlmBackendConfig>,
        default_name: String,
    ) -> Result<Self, LlmError> {
        Self::from_backend_configs_with_override(all, default_name, |_| None).await
    }
    /// Variant of [`from_backend_configs`](Self::from_backend_configs) with an
    /// override factory (multi-runner).
    ///
    /// Identical to [`from_repository_with_override`](Self::from_repository_with_override)
    /// but takes already-loaded configs (useful for reloads that read the
    /// SQLite repository in a `spawn_blocking`). The factory routes `LlamaCpp`
    /// backends to a `RunnerLlmBackend`; without it, those backends are skipped
    /// (the local runner becomes unreachable).
    pub async fn from_backend_configs_with_override<F>(
        all: Vec<LlmBackendConfig>,
        default_name: String,
        override_factory: F,
    ) -> Result<Self, LlmError>
    where
        F: Fn(&LlmBackendConfig) -> Option<Arc<dyn CompletionModel>>,
    {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for cfg in all.into_iter().filter(|c| c.enabled) {
            let name = cfg.name.clone();
            if let Some(overriden) = override_factory(&cfg) {
                backends.insert(name, overriden);
                continue;
            }
            match instantiate_from_config(&cfg, cancellation_token.clone()).await {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        reason = "instantiation from the configuration failed",
                        "llm.backend.skipped"
                    );
                }
            }
        }

        let default = resolve_default_backend(default_name, &backends)?;

        Ok(Self {
            backends,
            default,
            routing: None,
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }
    /// Build the router from a SQLite [`LlmBackendRepository`].
    ///
    /// Loads every `enabled = true` backend. The `is_default = true` backend
    /// becomes the default. Backends that fail to instantiate are logged with
    /// `tracing::warn!` and skipped (non-fatal degradation).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`] if no backend is marked `is_default = true`
    ///   in `system.db`, or if the default backend fails to instantiate.
    pub async fn from_repository(repo: &LlmBackendRepository) -> Result<Self, LlmError> {
        Self::from_repository_with_override(repo, |_| None).await
    }
    /// Variant of [`from_repository`] that lets callers inject a factory to
    /// override instantiation of certain backends (multi-runner support).
    ///
    /// The closure receives the `LlmBackendConfig` and returns:
    /// - `Some(Arc<dyn CompletionModel>)` to override (typically used to
    ///   redirect `LlamaCpp` backends to a `RunnerLlmBackend`).
    /// - `None` to keep standard instantiation (the cloud backend case).
    pub async fn from_repository_with_override<F>(
        repo: &LlmBackendRepository,
        override_factory: F,
    ) -> Result<Self, LlmError>
    where
        F: Fn(&apollia_core::LlmBackendConfig) -> Option<Arc<dyn CompletionModel>>,
    {
        let all = repo.list().map_err(|e| LlmError::BackendUnavailable {
            backend: apollia_core::paths::DataFile::System
                .file_name()
                .to_string(),
            reason: e.to_string(),
        })?;

        let default_name = repo
            .find_default()
            .map_err(|e| LlmError::BackendUnavailable {
                backend: apollia_core::paths::DataFile::System
                    .file_name()
                    .to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| LlmError::BackendUnavailable {
                backend: "default".to_string(),
                reason: "no default LLM backend in system.db - configure one with is_default=true"
                    .to_string(),
            })?
            .name;

        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for cfg in all.into_iter().filter(|c| c.enabled) {
            let name = cfg.name.clone();
            // Try the override first (typically the runner proxy for LlamaCpp).
            if let Some(overriden) = override_factory(&cfg) {
                tracing::info!(
                    backend = %name,
                    detail = "instantiated through the runner proxy override",
                    "llm.backend.instantiated"
                );
                backends.insert(name, overriden);
                continue;
            }

            match instantiate_from_config(&cfg, cancellation_token.clone()).await {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        reason = "instantiation from the repository failed",
                        "llm.backend.skipped"
                    );
                }
            }
        }

        let default = resolve_default_backend(default_name, &backends)?;

        Ok(Self {
            backends,
            default,
            routing: None,
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }
    /// Attach (or replace) the router's `[llm.routing]`.
    ///
    /// Useful post-construction when the router is instantiated via
    /// [`from_repository`](Self::from_repository) (which reads `system.db`)
    /// while the `[llm.routing]` config lives in `apollia.toml`. The supervisor
    /// calls `from_repository` then chains `with_routing` if the TOML declares
    /// a `[llm.routing]` section, propagating the application routing to the
    /// router without duplicating the read.
    ///
    /// Validation: the routing is applied as-is. The backends it names are
    /// checked at invocation via [`route_precise`](Self::route_precise) /
    /// [`route_fast`](Self::route_fast), which return
    /// [`LlmError::BackendNotFound`] if a name points to an absent backend.
    pub fn with_routing(mut self, routing: LlmRoutingConfig) -> Self {
        self.routing = Some(routing);
        self
    }
    /// Create an empty `LlmRouter` with no backend, for unit tests.
    ///
    /// Used to test degradation paths: `ctx.llm = None` and `AgentDegraded`
    /// on the EventBus.
    pub fn empty() -> Self {
        Self {
            backends: HashMap::new(),
            default: String::new(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }
}
