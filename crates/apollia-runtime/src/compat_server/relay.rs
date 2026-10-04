//! The endpoints that relay a request to the engine of a local model.
//!
//! The request names a model; the relay finds the process serving it, puts in
//! `model` the value that engine expects, and forwards the body unchanged. The
//! engine's answer comes back as it gave it: a JSON body read under a cap, or
//! an event stream passed on chunk by chunk, in order, as the engine sends it.
//! Nothing is translated from one format into another: an endpoint the engine
//! of the model does not serve answers 404, never an approximation. A chat
//! completion on an `apollia/...` model is not relayed: the runtime runs the
//! exposed agent or workflow it names (see [`super::agent_call`]).

use std::sync::Arc;
use std::time::Duration;

use apollia_core::net::{read_capped_bytes, ReadCappedError};
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::StreamExt;
use serde::Serialize;
use serde_json::Value;

use super::agent_call::{admitted, chat_with_exposed, Caller};
use super::agents::{ExposedAgents, MODEL_PREFIX};
use super::errors::{error_response, ErrorKind, Family};
use super::logs::RequestedModel;
use super::models::{HeldModel, LocalModels, ServedModel, Unheld};
use super::origin::CallOrigin;
use super::tokens::ServerTokensHandle;
use crate::engine_store::Engine;
use crate::llama_server::ModelKind;

/// Largest answer read whole from an engine: a batch of embeddings is the
/// largest body an engine gives outside a stream.
const MAX_ENGINE_ANSWER: u64 = 64 * 1024 * 1024;

/// How long the relay waits for the engine to accept the connection. The
/// answer itself has no deadline: a long generation is a legitimate one.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// An endpoint the compatible server relays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endpoint {
    ChatCompletions,
    Responses,
    Embeddings,
    Messages,
    CountTokens,
}

impl Endpoint {
    /// Its path on the compatible server, the same on the engine.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/v1/chat/completions",
            Self::Responses => "/v1/responses",
            Self::Embeddings => "/v1/embeddings",
            Self::Messages => "/v1/messages",
            Self::CountTokens => "/v1/messages/count_tokens",
        }
    }

    /// Whether the engine serving a model of `kind` serves this endpoint.
    ///
    /// `llama-server` at the pinned release answers the four chat endpoints
    /// for a chat model, and only `/v1/embeddings` for the instance launched
    /// with `--embeddings`. `vllm-mlx` serves chat completions, responses and
    /// messages for a chat model; it is not asked to count tokens.
    pub(crate) const fn served_by(self, engine: Engine, kind: ModelKind) -> bool {
        match (engine, kind) {
            (Engine::LlamaServer, ModelKind::Chat) => !matches!(self, Self::Embeddings),
            (Engine::LlamaServer, ModelKind::Embeddings) => matches!(self, Self::Embeddings),
            (Engine::VllmMlx, ModelKind::Chat) => matches!(
                self,
                Self::ChatCompletions | Self::Responses | Self::Messages
            ),
            _ => false,
        }
    }

    fn family(self) -> Family {
        Family::of_path(self.path())
    }
}

/// What the routes share: the local models and the client that reaches their
/// engines, the exposed agents and workflows, and the tokens a call to one of
/// them from the local network is checked against.
#[derive(Debug, Clone)]
pub(crate) struct RelayState {
    models: LocalModels,
    client: reqwest::Client,
    agents: ExposedAgents,
    tokens: Option<ServerTokensHandle>,
}

impl RelayState {
    /// The client reaches only the engines' processes on `127.0.0.1`, which
    /// the runtime spawned: an endpoint it configured, never through a proxy.
    /// No agent nor workflow is served.
    #[cfg(test)]
    pub(crate) fn new(models: LocalModels) -> Result<Arc<Self>, reqwest::Error> {
        Self::with_agents(models, ExposedAgents::default(), None)
    }

    /// As [`Self::new`], serving `agents` too; a call to one of them from the
    /// local network presents a token `tokens` holds in use.
    pub(crate) fn with_agents(
        models: LocalModels,
        agents: ExposedAgents,
        tokens: Option<ServerTokensHandle>,
    ) -> Result<Arc<Self>, reqwest::Error> {
        let client = apollia_core::net::loopback_client_builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;
        Ok(Arc::new(Self {
            models,
            client,
            agents,
            tokens,
        }))
    }
}

/// One entry of `GET /v1/models`, in the OpenAI list format.
#[derive(Debug, Serialize)]
struct ModelEntry {
    id: String,
    object: &'static str,
    created: i64,
    owned_by: &'static str,
    /// `llm` for a chat model, `embeddings` for the instance serving vectors,
    /// `agent` or `workflow` for an exposed agent or workflow.
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct ModelList {
    object: &'static str,
    data: Vec<ModelEntry>,
}

fn entry(model: ServedModel) -> ModelEntry {
    ModelEntry {
        id: model.id,
        object: "model",
        created: model.created,
        owned_by: "apollia",
        kind: match model.kind {
            ModelKind::Chat => "llm",
            ModelKind::Embeddings => "embeddings",
        },
    }
}

/// `GET /v1/models`: the resident models and, while on-demand loading is on,
/// the language models of this machine, by their registry identifier, then
/// the exposed agents and workflows, `apollia/<id>`. A caller of the local
/// network without a token in use is not told the agents and workflows.
pub(crate) async fn list_models(
    State(state): State<Arc<RelayState>>,
    origin: Option<Extension<CallOrigin>>,
    headers: HeaderMap,
) -> Response {
    let mut data: Vec<ModelEntry> = state.models.listed().await.into_iter().map(entry).collect();
    if admitted(origin_of(origin), &headers, state.tokens.as_ref()).await {
        data.extend(
            state
                .agents
                .listed()
                .await
                .into_iter()
                .map(|model| ModelEntry {
                    id: model.id,
                    object: "model",
                    created: 0,
                    owned_by: "apollia",
                    kind: model.kind.as_str(),
                }),
        );
    }
    Json(ModelList {
        object: "list",
        data,
    })
    .into_response()
}

/// The origin of a request; one that carries none, which only a test builds,
/// is read as the local network, the stricter of the two.
fn origin_of(origin: Option<Extension<CallOrigin>>) -> CallOrigin {
    origin.map_or(CallOrigin::LocalNetwork, |Extension(origin)| origin)
}

/// Relays `body` to the engine serving the model it names, on `endpoint`.
pub(crate) async fn relay(state: &RelayState, endpoint: Endpoint, body: Bytes) -> Response {
    let family = endpoint.family();
    let request: Value = match serde_json::from_slice(&body) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => {
            return error_response(
                family,
                ErrorKind::InvalidRequest,
                "The body must be a JSON object.".to_owned(),
            )
        }
    };
    let Some(id) = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return error_response(
            family,
            ErrorKind::InvalidRequest,
            "The body must name a model in its `model` field. GET /v1/models lists them."
                .to_owned(),
        );
    };
    let mut response = relay_model(state, endpoint, request, &id).await;
    response.extensions_mut().insert(RequestedModel(id));
    response
}

/// Relays `request`, which names the model `id`, to the engine serving it.
async fn relay_model(
    state: &RelayState,
    endpoint: Endpoint,
    mut request: Value,
    id: &str,
) -> Response {
    let family = endpoint.family();
    let held = match state.models.hold(id).await {
        Ok(held) => held,
        Err(Unheld::NotLoaded) => {
            return error_response(
                family,
                ErrorKind::ModelNotLoaded,
                format!(
                    "The model '{id}' is not loaded. Load it in Apollia first; \
                     GET /v1/models lists the models it serves."
                ),
            )
        }
        Err(Unheld::LoadFailed(message)) => {
            return error_response(family, ErrorKind::EngineUnavailable, message)
        }
    };
    if !endpoint.served_by(held.served.engine, held.served.kind) {
        return error_response(
            family,
            ErrorKind::EndpointNotServed,
            format!(
                "The model '{id}' is served by {}, which does not serve {} for this model.",
                held.served.engine.as_str(),
                endpoint.path()
            ),
        );
    }
    request["model"] = Value::String(held.request_model.clone());
    forward(state, endpoint, &request, held).await
}

/// Sends `request` to the engine of `held` and answers what the engine
/// answered.
async fn forward(
    state: &RelayState,
    endpoint: Endpoint,
    request: &Value,
    held: HeldModel,
) -> Response {
    let family = endpoint.family();
    let suffix = endpoint.path().trim_start_matches("/v1");
    let url = format!("{}{suffix}", held.lease.base_url());
    let sent = state.client.post(&url).json(request).send().await;
    let answer = match sent {
        Ok(answer) => answer,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                model = %held.served.id,
                endpoint = endpoint.path(),
                error = %error,
                "compat_server.relay.engine_unreachable"
            );
            return error_response(
                family,
                ErrorKind::EngineUnavailable,
                format!("The engine serving '{}' did not answer.", held.served.id),
            );
        }
    };
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = answer
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));
    tracing::event!(
        tracing::Level::DEBUG,
        model = %held.served.id,
        endpoint = endpoint.path(),
        status = status.as_u16(),
        "compat_server.relay.answered"
    );
    if content_type.as_bytes().starts_with(b"text/event-stream") {
        let lease = held.lease;
        // SAFETY: an event stream is open-ended by design; it is relayed chunk
        // by chunk as the engine sends it, never buffered, so no cap applies.
        let chunks = answer.bytes_stream().map(move |chunk| {
            let _held_for_the_stream = &lease;
            chunk
        });
        return (
            status,
            [(header::CONTENT_TYPE, content_type)],
            Body::from_stream(chunks),
        )
            .into_response();
    }
    match read_capped_bytes(answer, MAX_ENGINE_ANSWER).await {
        Ok(bytes) => (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                model = %held.served.id,
                endpoint = endpoint.path(),
                error = %error,
                "compat_server.relay.answer_unreadable"
            );
            let message = match error {
                ReadCappedError::TooLarge { limit, .. } => {
                    format!("The engine's answer exceeds {limit} bytes.")
                }
                _ => format!(
                    "The engine serving '{}' broke off its answer.",
                    held.served.id
                ),
            };
            error_response(family, ErrorKind::EngineUnavailable, message)
        }
    }
}

/// `POST /v1/chat/completions`: an `apollia/...` model is an exposed agent or
/// workflow, run by the runtime; any other is relayed to its engine.
pub(crate) async fn chat_completions(
    State(state): State<Arc<RelayState>>,
    origin: Option<Extension<CallOrigin>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(model) = request
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| model.starts_with(MODEL_PREFIX))
        .map(str::to_owned)
    else {
        return relay(&state, Endpoint::ChatCompletions, body).await;
    };
    let caller = Caller {
        origin: origin_of(origin),
        headers: &headers,
    };
    let mut response = chat_with_exposed(
        &state.agents,
        state.tokens.as_ref(),
        caller,
        &request,
        &model,
    )
    .await;
    response.extensions_mut().insert(RequestedModel(model));
    response
}

/// `POST /v1/responses`.
pub(crate) async fn responses(State(state): State<Arc<RelayState>>, body: Bytes) -> Response {
    relay(&state, Endpoint::Responses, body).await
}

/// `POST /v1/embeddings`.
pub(crate) async fn embeddings(State(state): State<Arc<RelayState>>, body: Bytes) -> Response {
    relay(&state, Endpoint::Embeddings, body).await
}

/// `POST /v1/messages`.
pub(crate) async fn messages(State(state): State<Arc<RelayState>>, body: Bytes) -> Response {
    relay(&state, Endpoint::Messages, body).await
}

/// `POST /v1/messages/count_tokens`.
pub(crate) async fn count_tokens(State(state): State<Arc<RelayState>>, body: Bytes) -> Response {
    relay(&state, Endpoint::CountTokens, body).await
}
