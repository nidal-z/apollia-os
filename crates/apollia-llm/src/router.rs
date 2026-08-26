//! `LlmRouter`, dispatches requests to the right backend by name.
//!
//! Built at Supervisor startup (before `TaskRouter`) via
//! [`LlmRouter::from_config`]. Shareable as `Arc<LlmRouter>` thanks to
//! `Clone + Send + Sync`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use apollia_core::error_analysis::{ErrorAnalysis, ErrorCategory};
use apollia_core::events::EventBusSender;

use crate::token_budget::SessionBudgetTracker;
use apollia_core::LlmRoutingConfig;

use crate::types::{CompletionModel, LlmError};

mod backends;
mod budget;
mod completion;
mod config;
mod construct;
mod routing;

pub use config::{BackendConfig, BackendKind, LlmConfig, ObservabilityConfig};

/// Routing context for [`LlmRouter::complete_with_fallback`].
///
/// Groups the primary backend, the ordered fallback list, the optional event
/// bus and the observability config. The completion request stays passed
/// separately because it is consumed per call.
pub struct FallbackPlan<'a> {
    /// Name of the primary backend to try first.
    pub primary: &'a str,
    /// Secondary backends tried in order if the primary fails.
    pub fallbacks: &'a [&'a str],
    /// Optional event bus to emit [`RuntimeEvent::LlmFallbackTriggered`].
    pub bus: Option<&'a EventBusSender>,
    /// Observability config propagated to the underlying calls.
    pub obs: &'a ObservabilityConfig,
}

/// Single entry point for the entire Apollia OS LLM layer.
///
/// Instantiated by the Supervisor at startup via [`LlmRouter::from_config`].
/// Dispatches requests to the right backend by name via [`get`](Self::get),
/// with fallback to the `default` backend.
///
/// [`route_precise`](Self::route_precise) and [`route_fast`](Self::route_fast)
/// select the backend by the required precision level (`[llm.routing]` config).
///
/// `LlmRouter` is `Clone + Send + Sync`, shareable as `Arc<LlmRouter>` across
/// runtime components (it acts as a read-only catalog).
///
/// `Debug` is implemented manually: `Arc<dyn CompletionModel>` does not
/// implement `Debug` (the trait object does not expose it).
///
/// The session `CancellationToken` lets `ORIAEngine::abort()` cancel all
/// in-flight LLM calls and their retry delays via
/// [`cancellation_token`](Self::cancellation_token).
#[derive(Clone)]
pub struct LlmRouter {
    pub(super) backends: HashMap<String, Arc<dyn CompletionModel>>,
    pub(super) default: String,
    /// LLM routing by precision level. `None` for routers built via
    /// `from_repository` or `with_backends` (no TOML config).
    pub(super) routing: Option<LlmRoutingConfig>,
    /// Cancellation token shared by all backends of this router.
    pub(super) cancellation_token: CancellationToken,
    /// Cumulative session budget with real-time event emission.
    ///
    /// Guarded by a standard `Mutex` (short lock, never held across an async
    /// call) so the struct can `Clone` without an extra `Arc`.
    pub(super) session_budget: Arc<Mutex<SessionBudgetTracker>>,
}

/// Validate `[llm.routing]` consistency: the named backends must exist.
///
/// `[llm.routing]` is optional. When present, the `precise`/`fast` names must
/// be in the map; otherwise `route_precise/fast` fall back to
/// `config.default` at runtime.
/// Classify a failed backend call for [`RuntimeEvent::LlmCallFailed`].
///
/// The static classifier of `apollia-runtime` reads a message string; here the
/// typed error is still in hand, so the category comes from the variant rather
/// than from a substring match on its rendering.
pub(crate) fn analyse_call_failure(error: &LlmError) -> ErrorAnalysis {
    let category = match error {
        LlmError::ApiKeyMissing { .. } => ErrorCategory::PermissionDenied,
        LlmError::HttpError { status, .. } if *status == 401 || *status == 403 => {
            ErrorCategory::PermissionDenied
        }
        LlmError::HttpError { status, .. } if *status == 408 => ErrorCategory::Timeout,
        LlmError::ParseError(_) => ErrorCategory::MalformedOutput,
        LlmError::BackendUnavailable { .. } | LlmError::ModelNotFound { .. } => {
            ErrorCategory::NetworkError
        }
        _ => ErrorCategory::LlmError,
    };
    ErrorAnalysis::new(category, category.i18n_key().to_owned(), error.to_string())
}

pub(crate) fn validate_routing(
    backends: &HashMap<String, Arc<dyn CompletionModel>>,
    routing: Option<&LlmRoutingConfig>,
) -> Result<(), LlmError> {
    if let Some(routing) = routing {
        if !backends.contains_key(&routing.precise) {
            return Err(LlmError::BackendNotFound(routing.precise.clone()));
        }
        if !backends.contains_key(&routing.fast) {
            return Err(LlmError::BackendNotFound(routing.fast.clone()));
        }
        // When the hybrid section is present, the frontier backend it names must
        // exist too: a misconfiguration is caught at startup, not at runtime.
        if let Some(hybrid) = routing.hybrid.as_ref() {
            if !backends.contains_key(&hybrid.frontier) {
                return Err(LlmError::BackendNotFound(hybrid.frontier.clone()));
            }
        }
    }
    Ok(())
}

impl LlmRouter {}

// ─────────────────────────────────────────────
// Backend instantiation helpers
// ─────────────────────────────────────────────

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{CeilingAction, LlmBackendConfig, LlmProvider};

    use super::backends::{
        extract_base_url, ollama_context_from_ps, resolve_context_window, resolve_default_backend,
    };
    use super::config::infer_api_provider_from_url;
    use crate::routing_level::{EscalationSignal, LlmRoutingLevel};
    use crate::types::ChatMessage;

    use std::pin::Pin;

    use futures::Stream;

    use crate::types::{
        CompletionRequest, CompletionResponse, FinishReason, StreamChunk, TokenUsage,
    };

    // ── Mock ─────────────────────────────────────────────────────────────────

    struct MockCompletionModel {
        name: String,
    }

    impl Default for MockCompletionModel {
        fn default() -> Self {
            Self {
                name: "mock".to_owned(),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                engine_timings: None,
                content: "mock response".to_owned(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::once(async {
                Ok(StreamChunk::Text("mock chunk".to_owned()))
            })))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            &self.name
        }

        fn model_id(&self) -> &str {
            &self.name
        }
    }

    /// A backend whose every call fails, for the failure-path emission test.
    struct FailingCompletionModel;

    #[async_trait::async_trait]
    impl CompletionModel for FailingCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::ApiKeyMissing {
                var: "APOLLIA_TEST_KEY".to_owned(),
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::ApiKeyMissing {
                var: "APOLLIA_TEST_KEY".to_owned(),
            })
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "mock"
        }

        fn model_id(&self) -> &str {
            "mock"
        }
    }

    fn make_mock_backend(name: &str) -> Arc<dyn CompletionModel> {
        Arc::new(MockCompletionModel {
            name: name.to_owned(),
        })
    }

    fn make_test_router(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: &str,
    ) -> LlmRouter {
        LlmRouter {
            backends,
            default: default.into(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    fn make_routing_router(precise: &str, fast: &str) -> LlmRouter {
        let mut backends = HashMap::new();
        backends.insert(precise.to_owned(), make_mock_backend(precise));
        if fast != precise {
            backends.insert(fast.to_owned(), make_mock_backend(fast));
        }
        let routing = Some(LlmRoutingConfig {
            format_version: 1,
            precise: precise.to_owned(),
            fast: fast.to_owned(),
            hybrid: None,
        });
        LlmRouter {
            default: precise.to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Backend overriding `count_tokens` with a fixed value, for the delegate
    /// test. Inference methods are stubbed.
    struct FixedCountModel(usize);

    #[async_trait::async_trait]
    impl CompletionModel for FixedCountModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError("stub".into()))
        }
        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::InferenceError("stub".into()))
        }
        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "fixed"
        }
        fn model_id(&self) -> &str {
            "fixed-model"
        }
        fn count_tokens(&self, _messages: &[ChatMessage]) -> usize {
            self.0
        }
    }

    // ── Tests count_tokens() ─────────────────────────────────────────────────

    // GIVEN a router whose default backend overrides count_tokens to return 42
    // WHEN count_tokens is called
    // THEN the router delegates and returns 42
    #[test]
    fn test_router_count_tokens_delegates() {
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("fixed".into(), Arc::new(FixedCountModel(42)));
        let router = make_test_router(backends, "fixed");

        let tokens = router.count_tokens(&[ChatMessage::user("anything")]);
        assert_eq!(tokens, 42);
    }

    // GIVEN an empty router (no backend)
    // WHEN count_tokens is called
    // THEN the inline proxy is returned (> 0) without panicking
    #[test]
    fn test_router_empty_count_tokens_no_panic() {
        let router = LlmRouter::empty();
        let tokens = router.count_tokens(&[ChatMessage::user("hello")]);
        assert!(tokens > 0);
    }

    // ── Tests route() ────────────────────────────────────────────────────────

    // GIVEN router with "local-code" and "mistral-small", default = "mistral-small"
    // WHEN route(Some("local-code"))
    // THEN the "local-code" backend is returned
    #[test]
    fn test_route_to_explicit_backend() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        backends.insert("mistral-small".into(), make_mock_backend("mistral-small"));
        let router = make_test_router(backends, "mistral-small");

        let backend = router.route(Some("local-code"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router with default = "local-code"
    // WHEN route(None)
    // THEN the default backend is returned
    #[test]
    fn test_route_none_returns_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(None);
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router without "phantom"
    // WHEN route(Some("phantom"))
    // THEN the default backend is returned (warning emitted)
    #[test]
    fn test_unknown_backend_falls_back_to_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(Some("phantom"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN a router built by `empty()`, which holds no backend at all
    // WHEN route(None), then a completion on what it returned
    // THEN the call reports BackendUnavailable, where routing used to panic
    #[tokio::test]
    async fn test_empty_router_routes_to_an_unavailable_backend() {
        let router = LlmRouter::empty();

        let backend = router.route(None);

        assert!(!backend.is_available());
        let outcome = backend
            .complete(CompletionRequest {
                messages: vec![ChatMessage::user("anything")],
                ..Default::default()
            })
            .await;
        assert!(matches!(outcome, Err(LlmError::BackendUnavailable { .. })));
    }

    // GIVEN router with 2 backends
    // WHEN backend_names()
    // THEN sorted list of names returned
    #[test]
    fn test_backend_names_sorted() {
        let mut backends = HashMap::new();
        backends.insert("z-backend".into(), make_mock_backend("z-backend"));
        backends.insert("a-backend".into(), make_mock_backend("a-backend"));
        let router = make_test_router(backends, "a-backend");

        let names = router.backend_names();
        assert_eq!(names, vec!["a-backend", "z-backend"]);
    }

    // GIVEN a LlmBackendRepository with 2 enabled + 1 disabled Ollama backend
    // WHEN from_repository(&repo).await
    // THEN the router contains exactly 2 backends (the disabled one is excluded)
    #[cfg(feature = "cloud")]
    #[tokio::test]
    async fn test_from_repository_loads_only_enabled() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(
            &dir.path()
                .join(apollia_core::paths::DataFile::System.file_name()),
        )
        .unwrap();

        let make_ollama = |name: &str, enabled: bool, is_default: bool| LlmBackendConfig {
            name: name.to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({ "base_url": "http://localhost:11434/v1" }),
            enabled,
            is_default,
        };

        repo.save(&make_ollama("ollama-default", true, true))
            .unwrap();
        repo.save(&make_ollama("ollama-extra", true, false))
            .unwrap();
        repo.save(&make_ollama("ollama-disabled", false, false))
            .unwrap();

        let router = LlmRouter::from_repository(&repo)
            .await
            .expect("from_repository should succeed");

        let names = router.backend_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"ollama-default".to_string()));
        assert!(names.contains(&"ollama-extra".to_string()));
        assert!(!names.contains(&"ollama-disabled".to_string()));
    }

    // GIVEN backends whose URL is stored under each of the three historical keys
    // (canonical `base_url`, desktop `endpoint`, legacy `api_url`), plus one with none
    // WHEN  extract_base_url() resolves the URL
    // THEN  each stored key is honored and the empty one falls back to the default
    #[cfg(feature = "cloud")]
    #[test]
    fn test_extract_base_url_reads_all_key_variants() {
        use apollia_core::{LlmBackendConfig, LlmProvider};

        let make = |json: serde_json::Value| LlmBackendConfig {
            name: "b".to_string(),
            provider: LlmProvider::OpenAi,
            model: "m".to_string(),
            config_json: json,
            enabled: true,
            is_default: false,
        };
        let default = "https://api.openai.com/v1";

        // WHEN/THEN each writer's key resolves.
        assert_eq!(
            extract_base_url(
                &make(serde_json::json!({"base_url": "http://a/v1"})),
                default
            ),
            "http://a/v1"
        );
        assert_eq!(
            extract_base_url(
                &make(serde_json::json!({"endpoint": "http://b/v1"})),
                default
            ),
            "http://b/v1"
        );
        assert_eq!(
            extract_base_url(
                &make(serde_json::json!({"api_url": "http://c/v1"})),
                default
            ),
            "http://c/v1"
        );
        // Canonical key wins when several are present.
        assert_eq!(
            extract_base_url(
                &make(
                    serde_json::json!({"base_url": "http://win/v1", "endpoint": "http://lose/v1"})
                ),
                default
            ),
            "http://win/v1"
        );
        // No key (or empty) falls back to the provider default.
        assert_eq!(
            extract_base_url(&make(serde_json::json!({})), default),
            default
        );
        assert_eq!(
            extract_base_url(&make(serde_json::json!({"base_url": ""})), default),
            default
        );
    }

    // GIVEN a repository holding no default backend, either empty or written
    //       behind `save` so its auto-promotion does not apply
    // WHEN from_repository(&repo).await
    // THEN BackendUnavailable is returned, never a silent fallback to an
    //      arbitrary backend
    #[tokio::test]
    async fn test_from_repository_no_default_returns_error() {
        use apollia_core::LlmBackendRepository;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let db_path = dir
            .path()
            .join(apollia_core::paths::DataFile::System.file_name());
        let repo = LlmBackendRepository::open(&db_path).unwrap();

        // GIVEN an empty repository
        // THEN there is nothing to route to
        let result = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result, Err(LlmError::BackendUnavailable { .. })));

        // GIVEN an enabled backend inserted directly, bypassing `save` so that
        // its auto-promotion to default does not fire. This is the shape a
        // hand-edited or partially migrated database can take.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO llm_backends (name, provider, model, config_json, enabled, is_default) \
             VALUES ('orphan', 'ollama', 'llama3', '{}', 1, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        // THEN the router still refuses to start rather than picking one
        let result2 = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result2, Err(LlmError::BackendUnavailable { .. })));
    }

    // GIVEN an empty repository and a backend saved with is_default = false
    // WHEN from_repository(&repo).await
    // THEN the save promoted it to default, so the router builds. This guards
    //      the invariant that the table never holds backends without a default,
    //      which is what produced the "no default LLM backend configured" state.
    #[tokio::test]
    async fn test_first_saved_backend_is_promoted_to_default() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(
            &dir.path()
                .join(apollia_core::paths::DataFile::System.file_name()),
        )
        .unwrap();

        repo.save(&LlmBackendConfig {
            name: "orphan".to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: false,
        })
        .unwrap();

        assert_eq!(
            repo.find_default().unwrap().map(|c| c.name).as_deref(),
            Some("orphan")
        );
        assert!(LlmRouter::from_repository(&repo).await.is_ok());
    }

    // ── Tests: get, list, clone, error cases ─────────────────────────────────

    // GIVEN an LlmRouter with default = "local" and a "local" backend
    // WHEN get(None) is called
    // THEN Some(backend) with backend_name() == "local" is returned
    #[tokio::test]
    async fn test_get_none_returns_default() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let result = router.get(None);

        // THEN
        assert!(
            result.is_some(),
            "get(None) doit retourner Some pour le backend défaut"
        );
        assert_eq!(
            result.unwrap().backend_name(),
            "local",
            "le backend retourné doit être le backend défaut"
        );
    }

    // GIVEN an LlmRouter with an "anthropic" backend
    // WHEN get(Some("anthropic")) is called
    // THEN Some(arc) with backend_name() == "anthropic" is returned
    #[tokio::test]
    async fn test_get_named_backend() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("anthropic".into(), make_mock_backend("anthropic"));
        let router = make_test_router(backends, "anthropic");

        // WHEN
        let result = router.get(Some("anthropic"));

        // THEN
        assert!(
            result.is_some(),
            "get(Some(\"anthropic\")) doit retourner Some"
        );
        assert_eq!(result.unwrap().backend_name(), "anthropic");
    }

    // GIVEN an LlmRouter without an "inexistant" backend
    // WHEN get(Some("inexistant")) is called
    // THEN None is returned
    #[tokio::test]
    async fn test_get_unknown_returns_none() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN / THEN
        assert!(
            router.get(Some("inexistant")).is_none(),
            "get(Some(\"inexistant\")) doit retourner None pour un backend inconnu"
        );
    }

    // GIVEN an LlmRouter with 2 backends ("a" and "b")
    // WHEN list() is called
    // THEN a Vec of length 2 is returned
    #[tokio::test]
    async fn test_router_list_returns_all_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("a".into(), make_mock_backend("a"));
        backends.insert("b".into(), make_mock_backend("b"));
        let router = make_test_router(backends, "a");

        // WHEN
        let list = router.list();

        // THEN
        assert_eq!(
            list.len(),
            2,
            "list() doit retourner autant d'entrées que de backends"
        );
    }

    // GIVEN a cloned LlmRouter
    // WHEN the clone is queried
    // THEN it shares the same backends via Arc (refcount)
    #[tokio::test]
    async fn test_router_clone_shares_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let cloned = router.clone();

        // THEN
        assert!(
            cloned.get(None).is_some(),
            "le clone doit avoir accès aux mêmes backends"
        );
        assert_eq!(cloned.list().len(), 1);
    }

    // GIVEN an LlmConfig with default = "local" but an empty backends list
    // WHEN LlmRouter::from_config(&config).await is called
    // THEN Err(LlmError::BackendUnavailable { backend: "local", .. }) is returned
    #[tokio::test]
    async fn test_from_config_errors_if_default_missing() {
        // GIVEN
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
            runner: Default::default(),
        };

        // WHEN
        let result = LlmRouter::from_config(&config).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config doit retourner BackendUnavailable si le backend défaut est absent"
        );
    }

    // GIVEN an instantiated backend set that does NOT contain the configured default
    // WHEN resolving the default backend
    // THEN it fails fast with BackendUnavailable rather than substituting another backend
    #[test]
    fn test_resolve_default_backend_fails_when_default_absent() {
        // GIVEN
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("cloud".to_string(), make_mock_backend("cloud"));
        backends.insert("other".to_string(), make_mock_backend("other"));

        // WHEN
        let result = resolve_default_backend("local".to_string(), &backends);

        // THEN
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "a missing configured default must fail fast, not silently route to another backend"
        );
    }

    // GIVEN an instantiated backend set that DOES contain the configured default
    // WHEN resolving the default backend
    // THEN it returns that exact name
    #[test]
    fn test_resolve_default_backend_returns_present_default() {
        // GIVEN
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("local".to_string(), make_mock_backend("local"));

        // WHEN
        let result = resolve_default_backend("local".to_string(), &backends);

        // THEN
        assert_eq!(result.expect("present default resolves"), "local");
    }

    // ── Observability tests ──────────────────────────────────────────────────

    // GIVEN an LlmRouter with a mock backend and an EventBusSender
    // WHEN complete_with_observability(None, req, Some(&tx), &obs) is called
    // THEN an LlmCallCompleted event is received on the bus with backend == "mock"
    #[tokio::test]
    async fn test_llm_call_completed_emitted() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("test")],
            ..Default::default()
        };
        let obs = ObservabilityConfig::default();

        // WHEN
        router
            .complete_with_observability(None, req, Some(&tx), &obs)
            .await
            .expect("complete_with_observability ne doit pas échouer avec un mock valide");

        // THEN
        let event = rx
            .try_recv()
            .expect("un événement doit être présent dans le bus");
        assert!(
            matches!(
                event,
                RuntimeEvent::LlmCallCompleted { ref backend, .. } if backend == "mock"
            ),
            "l'événement reçu doit être LlmCallCompleted avec backend == \"mock\", obtenu: {event:?}"
        );
    }

    // GIVEN a router whose backend returns an error and an EventBusSender
    // WHEN complete_with_observability() is called
    // THEN LlmCallFailed reaches the bus, as the variant's own contract states
    #[tokio::test]
    async fn test_llm_call_failed_emitted() {
        use apollia_core::error_analysis::ErrorCategory;
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(FailingCompletionModel) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("test")],
            ..Default::default()
        };
        let obs = ObservabilityConfig::default();

        // WHEN
        let result = router
            .complete_with_observability(None, req, Some(&tx), &obs)
            .await;

        // THEN
        assert!(
            result.is_err(),
            "the failing backend must propagate its error"
        );
        let event = rx.try_recv().expect("the failure must reach the bus");
        match event {
            RuntimeEvent::LlmCallFailed {
                backend, analysis, ..
            } => {
                assert_eq!(backend, "mock");
                assert_eq!(analysis.category, ErrorCategory::PermissionDenied);
            }
            other => panic!("expected LlmCallFailed, got: {other:?}"),
        }
    }

    // GIVEN a router with debug_log_prompt = false
    // WHEN complete_with_observability() is called with a "secret_payload_xyz" message
    // THEN the function does not panic and returns Ok; the prompt is not logged at INFO
    #[tokio::test]
    async fn test_prompt_not_logged_at_info_without_debug_flag() {
        // GIVEN
        let obs = ObservabilityConfig {
            debug_log_prompt: false,
            ..Default::default()
        };
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("secret_payload_xyz")],
            ..Default::default()
        };
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");

        // WHEN: must not panic; an absent bus is acceptable (Option::None)
        let result = router
            .complete_with_observability(None, req, None, &obs)
            .await;

        // THEN
        assert!(
            result.is_ok(),
            "complete_with_observability doit retourner Ok même sans bus : {result:?}"
        );
    }

    // GIVEN an LlmRouter with an EventBusSender and an empty backends list (default absent)
    // WHEN from_config_with_bus is called
    // THEN Err(LlmError::BackendUnavailable) is returned without a crash
    // (variant without the "local" feature: checks the router does not crash)
    #[tokio::test]
    async fn test_from_config_with_bus_no_backends_returns_error() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
            runner: Default::default(),
        };

        // WHEN
        let result = LlmRouter::from_config_with_bus(&config, Some(tx)).await;

        // THEN: clean error, no crash
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config_with_bus doit retourner BackendUnavailable si aucun backend n'est disponible"
        );
    }

    // ── Routing tests ────────────────────────────────────────────────────────

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_precise()
    // THEN backend "claude-opus-4-6" is selected
    #[tokio::test]
    async fn router_precise_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router
            .route_precise()
            .expect("route_precise should succeed");
        assert_eq!(backend.backend_name(), "claude-opus-4-6");
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_fast()
    // THEN backend "claude-haiku-4-5-20251001" is selected
    #[tokio::test]
    async fn router_fast_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router.route_fast().expect("route_fast should succeed");
        assert_eq!(backend.backend_name(), "claude-haiku-4-5-20251001");
    }

    // GIVEN a router with a "default" backend but no [llm.routing] (routing: None)
    // WHEN route_precise() / route_fast() are called
    // THEN both fall back to the `default` backend (documented single-backend
    //      case: `apollia-os llm backends set-default <name>` is enough).
    #[tokio::test]
    async fn router_falls_back_to_default_when_routing_missing() {
        let mut backends = HashMap::new();
        backends.insert("default".to_owned(), make_mock_backend("default"));
        let router = make_test_router(backends, "default");

        let precise = router
            .route_precise()
            .expect("route_precise should fallback to default backend");
        assert_eq!(precise.backend_name(), "default");

        let fast = router
            .route_fast()
            .expect("route_fast should fallback to default backend");
        assert_eq!(fast.backend_name(), "default");
    }

    // GIVEN no backend at all (empty router) and no [llm.routing]
    // WHEN route_precise() is called
    // THEN Err(RoutingConfigMissing): no fallback possible, the operator must
    //      declare at least one backend.
    #[tokio::test]
    async fn router_errors_when_no_backend_and_no_routing() {
        let backends = HashMap::new();
        let router = make_test_router(backends, "");

        assert!(
            matches!(router.route_precise(), Err(LlmError::RoutingConfigMissing)),
            "route_precise() must error when no backend is registered"
        );
        assert!(
            matches!(router.route_fast(), Err(LlmError::RoutingConfigMissing)),
            "route_fast() must error when no backend is registered"
        );
    }

    // GIVEN a router built via `from_repository` (so routing=None) then enriched
    //       via with_routing(LlmRoutingConfig { precise, fast })
    // WHEN route_precise() / route_fast() are called
    // THEN the chained routing is respected.
    #[tokio::test]
    async fn router_with_routing_attaches_routing_post_construction() {
        let mut backends = HashMap::new();
        backends.insert("opus".to_owned(), make_mock_backend("opus"));
        backends.insert("haiku".to_owned(), make_mock_backend("haiku"));
        let router = make_test_router(backends, "haiku").with_routing(LlmRoutingConfig {
            format_version: 1,
            precise: "opus".to_owned(),
            fast: "haiku".to_owned(),
            hybrid: None,
        });

        assert_eq!(
            router
                .route_precise()
                .expect("route_precise should resolve via attached routing")
                .backend_name(),
            "opus"
        );
        assert_eq!(
            router
                .route_fast()
                .expect("route_fast should resolve via attached routing")
                .backend_name(),
            "haiku"
        );
    }

    // primary fails, secondary succeeds, LlmFallbackTriggered emitted
    #[tokio::test]
    async fn router_emits_fallback_event_on_primary_failure() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        struct FailingBackend {
            name: String,
        }
        #[async_trait::async_trait]
        impl CompletionModel for FailingBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                &self.name
            }
            fn model_id(&self) -> &str {
                &self.name
            }
        }

        // GIVEN a router with a failing primary and a healthy secondary
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert(
            "primary".into(),
            Arc::new(FailingBackend {
                name: "primary".into(),
            }),
        );
        backends.insert("secondary".into(), make_mock_backend("secondary"));
        let router = make_test_router(backends, "primary");
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hi")],
            ..Default::default()
        };

        // WHEN complete_with_fallback
        let obs = ObservabilityConfig::default();
        let response = router
            .complete_with_fallback(
                FallbackPlan {
                    primary: "primary",
                    fallbacks: &["secondary"],
                    bus: Some(&tx),
                    obs: &obs,
                },
                req,
            )
            .await
            .expect("fallback should succeed");

        // THEN response comes from secondary
        assert_eq!(response.content, "mock response");

        // AND LlmFallbackTriggered was emitted
        let mut saw_fallback = false;
        while let Ok(evt) = rx.try_recv() {
            if let RuntimeEvent::LlmFallbackTriggered {
                from_provider,
                to_provider,
                ..
            } = evt
            {
                assert_eq!(from_provider, "primary");
                assert_eq!(to_provider, "secondary");
                saw_fallback = true;
            }
        }
        assert!(
            saw_fallback,
            "LlmFallbackTriggered should have been emitted"
        );
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-opus-4-6" }
    // WHEN route_precise() and route_fast()
    // THEN the same backend "claude-opus-4-6" is returned in both cases
    #[tokio::test]
    async fn router_same_backend_for_precise_and_fast_when_identical() {
        let router = make_routing_router("claude-opus-4-6", "claude-opus-4-6");

        let precise = router
            .route_precise()
            .expect("route_precise should succeed");
        let fast = router.route_fast().expect("route_fast should succeed");

        assert_eq!(precise.backend_name(), "claude-opus-4-6");
        assert_eq!(fast.backend_name(), "claude-opus-4-6");
        assert_eq!(precise.backend_name(), fast.backend_name());
    }

    // ── Hybrid escalation policy ──────────────────────────────

    /// Build a router with `precise = fast = "local"`, a `"frontier-model"`
    /// backend, an `[llm.routing.hybrid]` section with the given ceiling, and a
    /// seeded session cost.
    fn make_hybrid_router(ceiling: f64, session_cost: f64) -> LlmRouter {
        let mut backends = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        backends.insert(
            "frontier-model".to_owned(),
            make_mock_backend("frontier-model"),
        );
        let routing = Some(LlmRoutingConfig {
            format_version: 1,
            precise: "local".to_owned(),
            fast: "local".to_owned(),
            hybrid: Some(apollia_core::HybridRoutingConfig {
                format_version: 1,
                frontier: "frontier-model".to_owned(),
                cost_ceiling_usd: ceiling,
                ceiling_action: apollia_core::CeilingAction::StayLocal,
            }),
        });
        let session_budget = Arc::new(Mutex::new(SessionBudgetTracker::default()));
        session_budget.lock().unwrap().session_cost_usd = session_cost;
        LlmRouter {
            default: "local".to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget,
        }
    }

    #[test]
    fn test_ceiling_action_default_stay_local() {
        // GIVEN a router with no hybrid routing section
        let mut backends = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        let router = LlmRouter {
            default: "local".to_owned(),
            backends,
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        };

        // WHEN reading the ceiling action and ceiling
        // THEN they default to StayLocal and no ceiling
        assert_eq!(router.ceiling_action(), CeilingAction::StayLocal);
        assert_eq!(router.cost_ceiling_usd(), None);
    }

    #[test]
    fn test_ceiling_action_hard_stop_when_configured() {
        // GIVEN a hybrid router configured with HardStop
        let mut backends = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        backends.insert(
            "frontier-model".to_owned(),
            make_mock_backend("frontier-model"),
        );
        let routing = Some(LlmRoutingConfig {
            format_version: 1,
            precise: "local".to_owned(),
            fast: "local".to_owned(),
            hybrid: Some(apollia_core::HybridRoutingConfig {
                format_version: 1,
                frontier: "frontier-model".to_owned(),
                cost_ceiling_usd: 2.0,
                ceiling_action: CeilingAction::HardStop,
            }),
        });
        let router = LlmRouter {
            default: "local".to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        };

        // WHEN reading the ceiling action and ceiling
        // THEN HardStop and the configured ceiling are surfaced
        assert_eq!(router.ceiling_action(), CeilingAction::HardStop);
        assert_eq!(router.cost_ceiling_usd(), Some(2.0));
    }

    // escalation accepted when the frontier is available and under ceiling.
    #[test]
    fn test_escalation_accepted_under_ceiling() {
        // GIVEN a hybrid router, session cost 0.50, ceiling 2.00
        let router = make_hybrid_router(2.00, 0.50);

        // WHEN a failure signal escalates a precise step
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 3,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the frontier backend is returned
        assert_eq!(backend.backend_name(), "frontier-model");
    }

    // ceiling reached keeps the router local.
    #[test]
    fn test_escalation_blocked_by_cost_ceiling() {
        // GIVEN a hybrid router, session cost 1.05, ceiling 1.00
        let router = make_hybrid_router(1.00, 1.05);

        // WHEN a failure signal escalates a precise step
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 2,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the local precise backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // no hybrid section means no escalation, no error.
    #[test]
    fn test_no_hybrid_config_returns_local() {
        // GIVEN a router without a hybrid section
        let router = make_routing_router("local", "local");

        // WHEN any escalation signal is applied
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 1,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the local backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // a frontier absent from the router is rejected at construction.
    #[test]
    fn test_frontier_absent_fails_at_construction() {
        // GIVEN a routing whose hybrid frontier is not in the backend map
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        let routing = LlmRoutingConfig {
            format_version: 1,
            precise: "local".to_owned(),
            fast: "local".to_owned(),
            hybrid: Some(apollia_core::HybridRoutingConfig {
                format_version: 1,
                frontier: "phantom".to_owned(),
                cost_ceiling_usd: 1.00,
                ceiling_action: apollia_core::CeilingAction::StayLocal,
            }),
        };

        // WHEN validate_routing runs
        let result = validate_routing(&backends, Some(&routing));

        // THEN it reports the missing frontier backend
        assert!(matches!(
            result,
            Err(LlmError::BackendNotFound(name)) if name == "phantom"
        ));
    }

    // an absent signal keeps the router local even under the ceiling.
    #[test]
    fn test_signal_none_returns_local() {
        // GIVEN a hybrid router well under the ceiling
        let router = make_hybrid_router(2.00, 0.10);

        // WHEN the signal is None
        let backend =
            router.route_with_escalation(EscalationSignal::None, LlmRoutingLevel::Precise);

        // THEN the local backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // Truth table for EscalationSignal::is_escalation.
    #[test]
    fn test_escalation_signal_is_escalation() {
        // GIVEN the three escalation signals the router can raise
        // WHEN each is asked whether it escalates
        // THEN the none signal does not, and the two real ones do
        assert!(!EscalationSignal::None.is_escalation());
        assert!(EscalationSignal::RepeatedStepFailure {
            consecutive_failures: 1
        }
        .is_escalation());
        assert!(EscalationSignal::AutonomyTierRequest.is_escalation());
    }

    // is_ceiling_reached: false when no hybrid section is configured.
    #[test]
    fn test_is_ceiling_reached_false_without_hybrid() {
        // GIVEN a router with routing but no hybrid section
        let router = make_routing_router("local", "local");

        // WHEN the ceiling is queried
        // THEN it reports not reached
        assert!(!router.is_ceiling_reached());
    }

    // is_ceiling_reached: false when the session cost is below the ceiling.
    #[test]
    fn test_is_ceiling_reached_false_below_ceiling() {
        // GIVEN a hybrid router, session cost 0.50, ceiling 2.00
        let router = make_hybrid_router(2.00, 0.50);

        // WHEN the ceiling is queried
        // THEN it reports not reached
        assert!(!router.is_ceiling_reached());
    }

    // is_ceiling_reached: true at or above the ceiling.
    #[test]
    fn test_is_ceiling_reached_true_at_or_above_ceiling() {
        // GIVEN a hybrid router exactly at the ceiling
        let at = make_hybrid_router(1.00, 1.00);
        // AND one above the ceiling
        let above = make_hybrid_router(1.00, 1.50);

        // WHEN the ceiling is queried
        // THEN both report reached
        assert!(at.is_ceiling_reached());
        assert!(above.is_ceiling_reached());
    }

    // seed_session_cost_usd drives the ceiling decision deterministically.
    #[test]
    fn test_seed_session_cost_usd_crosses_ceiling() {
        // GIVEN a hybrid router below the ceiling
        let router = make_hybrid_router(1.00, 0.10);
        assert!(!router.is_ceiling_reached());

        // WHEN the session cost is seeded above the ceiling
        router.seed_session_cost_usd(2.00);

        // THEN the ceiling is reported reached
        assert!(router.is_ceiling_reached());
    }

    // GIVEN an Ollama `/api/ps` body listing a loaded model
    // WHEN its context window is read
    // THEN the loaded figure is returned, which is the only authoritative one:
    //      Ollama sizes the window from available memory, so neither the
    //      model's trained length nor any local default predicts it
    #[test]
    fn test_ollama_context_is_read_from_the_loaded_model() {
        let body = serde_json::json!({
            "models": [
                {"name": "other:8b", "context_length": 4096},
                {"name": "qwen3:8b", "context_length": 32768}
            ]
        });

        assert_eq!(ollama_context_from_ps(&body, "qwen3:8b"), Some(32768));
    }

    // GIVEN a backend configured without a tag, against a server that always
    // reports one
    // WHEN the window is read
    // THEN `:latest` is matched, so the common shorthand is not a silent miss
    #[test]
    fn test_ollama_context_matches_an_implicit_latest_tag() {
        let body = serde_json::json!({
            "models": [{"name": "qwen3:latest", "context_length": 8192}]
        });

        assert_eq!(ollama_context_from_ps(&body, "qwen3"), Some(8192));
    }

    // GIVEN a server with the model not currently loaded
    // WHEN the window is read
    // THEN nothing is returned rather than a guess: reporting the trained
    //      length would over-state the window on exactly the small machines
    //      where overflowing it is a real risk
    #[test]
    fn test_ollama_context_is_unknown_when_the_model_is_not_loaded() {
        let body = serde_json::json!({ "models": [] });

        assert_eq!(ollama_context_from_ps(&body, "qwen3:8b"), None);
    }

    // GIVEN a backend whose operator pinned a context window
    // WHEN the window is resolved
    // THEN the configured value wins, because it is the only one that survives
    //      the server being unreachable
    #[tokio::test]
    async fn test_configured_context_window_wins_over_any_probe() {
        let cfg = LlmBackendConfig {
            name: "ollama".into(),
            provider: LlmProvider::Ollama,
            model: "qwen3:8b".into(),
            config_json: serde_json::json!({ "context_window": 16384 }),
            enabled: true,
            is_default: true,
        };

        let resolved =
            resolve_context_window(&cfg, &LlmProvider::Ollama, "http://127.0.0.1:1/v1").await;

        assert_eq!(resolved, Some(16384));
    }

    // GIVEN a cloud backend with no configured window
    // WHEN the window is resolved
    // THEN it stays unknown and no provider-specific probe is attempted
    #[tokio::test]
    async fn test_cloud_backend_without_configured_window_stays_unknown() {
        let cfg = LlmBackendConfig {
            name: "openai".into(),
            provider: LlmProvider::OpenAi,
            model: "gpt-4o".into(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: false,
        };

        let resolved =
            resolve_context_window(&cfg, &LlmProvider::OpenAi, "https://api.openai.com/v1").await;

        assert_eq!(resolved, None);
    }

    // GIVEN a legacy TOML backend with no explicit provider
    // WHEN the provider is inferred from its base URL
    // THEN an Ollama server is recognised wherever it runs, not only on
    //      localhost, so offloading inference to a second machine does not
    //      silently relabel the backend as OpenAI
    #[test]
    fn test_infer_provider_recognises_ollama_on_any_host() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:11434/v1",
            "http://192.168.1.20:11434/v1",
            "http://mac-studio.local:11434/v1",
        ] {
            assert_eq!(
                infer_api_provider_from_url(url),
                LlmProvider::Ollama,
                "expected Ollama for {url}"
            );
        }
    }

    #[test]
    fn test_infer_provider_falls_back_to_openai_for_unknown_hosts() {
        // GIVEN a self-hosted OpenAI-compatible gateway on a custom port
        // WHEN the provider is inferred
        // THEN it defaults to the OpenAI-compatible client, which is the one
        //      that serves every such endpoint
        assert_eq!(
            infer_api_provider_from_url("https://gateway.internal:8443/v1"),
            LlmProvider::OpenAi
        );
        assert_eq!(
            infer_api_provider_from_url("https://api.anthropic.com"),
            LlmProvider::Anthropic
        );
    }
}
