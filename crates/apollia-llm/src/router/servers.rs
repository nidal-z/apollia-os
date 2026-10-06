//! Routing to the models of a declared inference server, `<server>/<model>`.
//!
//! A server is not a backend row per model: the router knows the server's
//! address and key, and builds the OpenAI-compatible client of one of its
//! models the first time that model is addressed. The client is then kept, so
//! later calls share its connection pool.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apollia_core::inference_server::split_route_name;

use std::pin::Pin;

use futures::Stream;

use crate::router::LlmRouter;
use crate::types::{CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk};

/// What the router needs to reach a declared server.
#[derive(Clone)]
pub struct ServerEndpoint {
    /// Server name, the first segment of `<server>/<model>`.
    pub name: String,
    /// Base address, with or without the trailing `/v1`.
    pub base_url: String,
    /// Resolved key, `None` for a server that asks for none.
    pub api_key: Option<String>,
    /// Why the organisation policy refuses this server, `None` when it is
    /// allowed. A refused server stays known, so a request to one of its
    /// models fails with this reason before anything is sent to it.
    pub refusal: Option<String>,
    /// How long the server may stay silent before a request is given up.
    /// `None` keeps [`crate::http_client::DEFAULT_IDLE_TIMEOUT`].
    pub idle_timeout: Option<std::time::Duration>,
    /// Fields added to every request sent to this server. A field the request
    /// already carries is left as it is.
    pub extra_params: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Hand-written so a logged endpoint never prints its key.
impl std::fmt::Debug for ServerEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerEndpoint")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("refusal", &self.refusal)
            .field("idle_timeout", &self.idle_timeout)
            .field(
                "extra_params",
                &self
                    .extra_params
                    .as_ref()
                    .map(|params| params.keys().collect::<Vec<_>>()),
            )
            .finish()
    }
}

/// The servers a router reaches, and the clients already built for them.
#[derive(Default)]
pub(crate) struct ServerRoutes {
    endpoints: HashMap<String, ServerEndpoint>,
    clients: Mutex<HashMap<String, Arc<dyn CompletionModel>>>,
}

impl LlmRouter {
    /// Attach the declared servers, whose every model becomes routable as
    /// `<server>/<model>` without a backend row of its own.
    pub fn with_servers(mut self, servers: Vec<ServerEndpoint>) -> Self {
        self.servers = Arc::new(ServerRoutes {
            endpoints: servers.into_iter().map(|s| (s.name.clone(), s)).collect(),
            clients: Mutex::new(HashMap::new()),
        });
        self
    }

    /// The backend named `name`: a configured backend first, then a model of
    /// a declared server when `name` reads `<server>/<model>`.
    pub(crate) fn lookup(&self, name: &str) -> Option<Arc<dyn CompletionModel>> {
        if let Some(backend) = self.backends.get(name) {
            return Some(backend.clone());
        }
        let (server, model) = split_route_name(name)?;
        let endpoint = self.servers.endpoints.get(server)?;
        let mut clients = self
            .servers
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(client) = clients.get(name) {
            return Some(client.clone());
        }
        let client = match &endpoint.refusal {
            Some(reason) => Arc::new(RefusedServerModel {
                name: name.to_string(),
                model: model.to_string(),
                reason: reason.clone(),
            }) as Arc<dyn CompletionModel>,
            None => self.server_client(endpoint, name, model)?,
        };
        clients.insert(name.to_string(), client.clone());
        Some(client)
    }

    #[cfg(feature = "cloud")]
    fn server_client(
        &self,
        endpoint: &ServerEndpoint,
        name: &str,
        model: &str,
    ) -> Option<Arc<dyn CompletionModel>> {
        use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};

        let config = ApiBackendConfig {
            name: name.to_string(),
            api_url: apollia_core::inference_server::openai_api_base(&endpoint.base_url),
            api_key_env: String::new(),
            model: model.to_string(),
            context_window: None,
            llama_cpp_extensions: false,
        };
        tracing::event!(
            tracing::Level::INFO,
            server = %endpoint.name,
            model = %model,
            "llm.server.model.routed"
        );
        let client = OpenAICompatibleClient::with_idle_timeout(
            &config,
            endpoint.api_key.clone().unwrap_or_default(),
            self.cancellation_token.clone(),
            endpoint
                .idle_timeout
                .unwrap_or(crate::http_client::DEFAULT_IDLE_TIMEOUT),
        )
        .with_extra_params(endpoint.extra_params.clone().unwrap_or_default());
        Some(Arc::new(client) as Arc<dyn CompletionModel>)
    }

    #[cfg(not(feature = "cloud"))]
    fn server_client(
        &self,
        endpoint: &ServerEndpoint,
        _name: &str,
        model: &str,
    ) -> Option<Arc<dyn CompletionModel>> {
        tracing::event!(
            tracing::Level::WARN,
            server = %endpoint.name,
            model = %model,
            reason = "the cloud feature is not compiled",
            "llm.server.model.unroutable"
        );
        None
    }
}

/// A model of a server the organisation policy refuses: every call fails with
/// the policy's reason, and nothing leaves this machine.
struct RefusedServerModel {
    name: String,
    model: String,
    reason: String,
}

impl RefusedServerModel {
    fn refused(&self) -> LlmError {
        tracing::event!(
            tracing::Level::WARN,
            backend = %self.name,
            reason = %self.reason,
            "llm.server.request.refused_by_policy"
        );
        LlmError::BackendUnavailable {
            backend: self.name.clone(),
            reason: self.reason.clone(),
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for RefusedServerModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(self.refused())
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Err(self.refused())
    }

    fn is_available(&self) -> bool {
        false
    }

    fn backend_name(&self) -> &str {
        &self.name
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    // A model of a server the policy refuses fails with the policy's reason.
    #[tokio::test]
    async fn test_a_refused_server_answers_with_the_policy_reason() {
        // GIVEN a router whose one server is refused by the policy
        let router = LlmRouter::empty().with_servers(vec![ServerEndpoint {
            name: "cloud".into(),
            base_url: "https://api.example.com".into(),
            api_key: Some("sk".into()),
            refusal: Some("refused by the organisation policy".into()),
            idle_timeout: None,
            extra_params: None,
        }]);

        // WHEN a completion is sent to one of its models
        let backend = router.get(Some("cloud/gpt")).expect("the route is known");
        let outcome = backend
            .complete(CompletionRequest {
                messages: vec![ChatMessage::user("bonjour")],
                ..Default::default()
            })
            .await;

        // THEN it is refused with the reason, and the backend reads unavailable
        assert!(!backend.is_available());
        assert!(matches!(
            outcome,
            Err(LlmError::BackendUnavailable { ref reason, .. })
                if reason == "refused by the organisation policy"
        ));
    }

    #[cfg(feature = "cloud")]
    mod cloud {
        use super::super::*;
        use crate::types::{ChatMessage, CompletionRequest};
        use crate::ObservabilityConfig;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn completion_body(model: &str) -> serde_json::Value {
            serde_json::json!({
                "id": "chatcmpl-1",
                "object": "chat.completion",
                "created": 1,
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "ok" },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
            })
        }

        // GIVEN a server declared with two extra parameters, one of which the
        //       request also fixes
        // WHEN a completion is addressed to one of its models
        // THEN the body carries the extra parameter the request did not fix,
        //      and the value of the request for the one it did
        #[tokio::test]
        async fn test_the_extra_parameters_of_a_server_travel_with_each_request() {
            // GIVEN
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(completion_body("m")))
                .mount(&server)
                .await;
            let extra = serde_json::json!({ "keep_alive": "30m", "temperature": 0.9 });
            let router = LlmRouter::empty().with_servers(vec![ServerEndpoint {
                name: "corp".into(),
                base_url: server.uri(),
                api_key: None,
                refusal: None,
                idle_timeout: Some(std::time::Duration::from_secs(120)),
                extra_params: extra.as_object().cloned(),
            }]);
            let request = CompletionRequest {
                messages: vec![ChatMessage::user("bonjour")],
                temperature: Some(0.25),
                ..Default::default()
            };

            // WHEN
            router
                .complete_with_observability(
                    Some("corp/qwen3-32b"),
                    request,
                    None,
                    &ObservabilityConfig::default(),
                )
                .await
                .unwrap();

            // THEN
            let received = server.received_requests().await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
            assert_eq!(body["keep_alive"], "30m");
            assert_eq!(body["temperature"], 0.25);
            assert_eq!(body["model"], "qwen3-32b");
        }

        // GIVEN a router with no backend row at all for a declared server, which
        //       serves two models
        // WHEN a completion is addressed to each `<server>/<model>`
        // THEN both reach the server's `/v1/chat/completions`, each with `model`
        //      equal to the model identifier, and an undeclared server is not routable
        #[tokio::test]
        async fn test_each_model_of_a_server_is_routable_without_a_row() {
            // GIVEN
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(completion_body("m")))
                .mount(&server)
                .await;
            let router = LlmRouter::empty().with_servers(vec![ServerEndpoint {
                name: "corp".into(),
                base_url: server.uri(),
                api_key: Some("sk-corp".into()),
                refusal: None,
                idle_timeout: None,
                extra_params: None,
            }]);
            let request = || CompletionRequest {
                messages: vec![ChatMessage::user("bonjour")],
                ..Default::default()
            };
            let obs = ObservabilityConfig::default();

            // WHEN
            router
                .complete_with_observability(Some("corp/qwen3-32b"), request(), None, &obs)
                .await
                .unwrap();
            router
                .complete_with_observability(Some("corp/Qwen/Qwen3-8B"), request(), None, &obs)
                .await
                .unwrap();

            // THEN
            let models: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .map(|r| {
                    let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
                    body["model"].as_str().unwrap().to_string()
                })
                .collect();
            assert_eq!(
                models,
                vec!["qwen3-32b".to_string(), "Qwen/Qwen3-8B".to_string()]
            );
            assert!(
                router.backend_names().is_empty(),
                "no backend row was added"
            );
            assert!(router.get(Some("other/qwen3-32b")).is_none());
            assert!(router.get(Some("corp")).is_none());
        }
    }
}
