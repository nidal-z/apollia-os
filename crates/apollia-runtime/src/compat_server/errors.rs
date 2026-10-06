//! The error bodies of the compatible server, in the format of the family an
//! endpoint belongs to.
//!
//! An OpenAI client reads `{"error": {"message", "type", "code"}}`, an
//! Anthropic client `{"type": "error", "error": {"type", "message"}}`. Each
//! endpoint answers its errors in its own family's format, so the client shows
//! its usual message instead of failing to parse ours.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{Map, Value};

/// The API family whose format an endpoint follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    /// `/v1/chat/completions`, `/v1/responses`, `/v1/embeddings`,
    /// `/v1/models`, and any unknown path.
    OpenAi,
    /// `/v1/messages` and `/v1/messages/count_tokens`.
    Anthropic,
}

impl Family {
    /// The family of the endpoint at `path`.
    pub(crate) fn of_path(path: &str) -> Self {
        if path == "/v1/messages" || path.starts_with("/v1/messages/") {
            Self::Anthropic
        } else {
            Self::OpenAi
        }
    }
}

/// The error envelope of the OpenAI API.
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiErrorBody {
    pub(crate) error: OpenAiError,
}

/// The inner object of [`OpenAiErrorBody`].
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiError {
    /// What went wrong, for a person.
    pub(crate) message: String,
    /// The class of the error, as OpenAI names it (`invalid_request_error`).
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    /// A stable identifier of the error.
    pub(crate) code: &'static str,
}

/// The error envelope of the Anthropic API.
#[derive(Debug, Serialize)]
struct AnthropicErrorBody {
    #[serde(rename = "type")]
    kind: &'static str,
    error: AnthropicError,
}

#[derive(Debug, Serialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    kind: &'static str,
    message: String,
}

/// What went wrong, independent of the format it is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// The body cannot be read as the request the endpoint takes.
    InvalidRequest,
    /// No token in use was presented.
    Unauthorized,
    /// The model named in `model` is not loaded.
    ModelNotLoaded,
    /// The engine serving the model does not serve this endpoint.
    EndpointNotServed,
    /// No endpoint lives at this path.
    UnknownEndpoint,
    /// The engine serving the model did not answer.
    EngineUnavailable,
    /// The `apollia/...` model names no agent or workflow marked exposed.
    NotExposed,
    /// The run of an agent stopped on an access nobody granted, and waits
    /// for a decision.
    ApprovalPending,
    /// The run of an agent or a workflow failed or was stopped.
    RunFailed,
    /// This runtime cannot run agents or workflows.
    RunUnavailable,
    /// The organisation policy refuses what the request asks for.
    RefusedByPolicy,
    /// An MCP server the request brought did not answer.
    McpServerUnavailable,
}

impl ErrorKind {
    fn status(self) -> StatusCode {
        match self {
            Self::InvalidRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::ModelNotLoaded | Self::EndpointNotServed | Self::UnknownEndpoint => {
                StatusCode::NOT_FOUND
            }
            Self::EngineUnavailable | Self::McpServerUnavailable => StatusCode::BAD_GATEWAY,
            Self::RefusedByPolicy => StatusCode::FORBIDDEN,
            Self::NotExposed => StatusCode::NOT_FOUND,
            Self::ApprovalPending => StatusCode::CONFLICT,
            Self::RunFailed => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RunUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The OpenAI `type` and `code` of the error.
    fn openai(self) -> (&'static str, &'static str) {
        match self {
            Self::InvalidRequest => ("invalid_request_error", "invalid_request"),
            Self::Unauthorized => ("invalid_request_error", "invalid_api_key"),
            Self::ModelNotLoaded => ("invalid_request_error", "model_not_loaded"),
            Self::EndpointNotServed => ("invalid_request_error", "endpoint_not_served"),
            Self::UnknownEndpoint => ("invalid_request_error", "unknown_url"),
            Self::EngineUnavailable => ("server_error", "engine_unavailable"),
            Self::NotExposed => ("invalid_request_error", "model_not_found"),
            Self::ApprovalPending => ("approval_pending", "approval_pending"),
            Self::RunFailed => ("server_error", "run_failed"),
            Self::RunUnavailable => ("server_error", "run_unavailable"),
            Self::RefusedByPolicy => ("invalid_request_error", "refused_by_policy"),
            Self::McpServerUnavailable => ("server_error", "mcp_server_unavailable"),
        }
    }

    /// The Anthropic `type` of the error.
    fn anthropic(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request_error",
            Self::Unauthorized => "authentication_error",
            Self::ModelNotLoaded
            | Self::EndpointNotServed
            | Self::UnknownEndpoint
            | Self::NotExposed => "not_found_error",
            Self::ApprovalPending => "approval_pending",
            Self::RefusedByPolicy => "permission_error",
            Self::EngineUnavailable
            | Self::RunFailed
            | Self::RunUnavailable
            | Self::McpServerUnavailable => "api_error",
        }
    }
}

/// The body of an OpenAI error of `kind`, with `extra` fields beside its
/// `message`, `type` and `code`: what a client needs to act on it, such as the
/// schema an input must follow or the decision a run waits for.
fn openai_error_body(kind: ErrorKind, message: String, extra: Map<String, Value>) -> Value {
    let (openai_kind, code) = kind.openai();
    let mut error = Map::new();
    error.insert("message".to_owned(), Value::String(message));
    error.insert("type".to_owned(), Value::String(openai_kind.to_owned()));
    error.insert("code".to_owned(), Value::String(code.to_owned()));
    error.extend(extra);
    let mut body = Map::new();
    body.insert("error".to_owned(), Value::Object(error));
    Value::Object(body)
}

/// The body of an Anthropic error of `kind`, with `extra` fields beside its
/// `type` and `message`.
fn anthropic_error_body(kind: ErrorKind, message: String, extra: Map<String, Value>) -> Value {
    let mut error = Map::new();
    error.insert(
        "type".to_owned(),
        Value::String(kind.anthropic().to_owned()),
    );
    error.insert("message".to_owned(), Value::String(message));
    error.extend(extra);
    let mut body = Map::new();
    body.insert("type".to_owned(), Value::String("error".to_owned()));
    body.insert("error".to_owned(), Value::Object(error));
    Value::Object(body)
}

/// The body of an error of `kind` in the format of `family`, with `extra`
/// fields in its error object (see [`openai_error_body`]).
pub(crate) fn error_body(
    family: Family,
    kind: ErrorKind,
    message: String,
    extra: Map<String, Value>,
) -> Value {
    match family {
        Family::OpenAi => openai_error_body(kind, message, extra),
        Family::Anthropic => anthropic_error_body(kind, message, extra),
    }
}

/// The answer to an error of `kind` in the format of `family`, carrying
/// `extra` fields (see [`error_body`]).
pub(crate) fn error_with(
    family: Family,
    kind: ErrorKind,
    message: String,
    extra: Map<String, Value>,
) -> Response {
    (
        kind.status(),
        Json(error_body(family, kind, message, extra)),
    )
        .into_response()
}

/// The answer to an error of `kind`, in the format of `family`.
pub(crate) fn error_response(family: Family, kind: ErrorKind, message: String) -> Response {
    let status = kind.status();
    match family {
        Family::OpenAi => {
            let (openai_kind, code) = kind.openai();
            (
                status,
                Json(OpenAiErrorBody {
                    error: OpenAiError {
                        message,
                        kind: openai_kind,
                        code,
                    },
                }),
            )
                .into_response()
        }
        Family::Anthropic => (
            status,
            Json(AnthropicErrorBody {
                kind: "error",
                error: AnthropicError {
                    kind: kind.anthropic(),
                    message,
                },
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_extra_fields_sit_in_the_error_object_of_each_family() {
        // GIVEN an error that carries the decision a run waits for
        let extra = || {
            let mut extra = Map::new();
            extra.insert("pending_id".to_owned(), Value::String("batch:b:0".into()));
            extra
        };

        // WHEN it is written for each family
        let openai = error_body(
            Family::OpenAi,
            ErrorKind::ApprovalPending,
            "wait".into(),
            extra(),
        );
        let anthropic = error_body(
            Family::Anthropic,
            ErrorKind::ApprovalPending,
            "wait".into(),
            extra(),
        );

        // THEN each keeps its envelope, and the field is beside the message
        assert_eq!(openai["error"]["code"], "approval_pending");
        assert_eq!(openai["error"]["pending_id"], "batch:b:0");
        assert_eq!(anthropic["type"], "error");
        assert_eq!(anthropic["error"]["type"], "approval_pending");
        assert_eq!(anthropic["error"]["message"], "wait");
        assert_eq!(anthropic["error"]["pending_id"], "batch:b:0");
    }

    #[test]
    fn test_only_the_messages_endpoints_read_as_anthropic() {
        // GIVEN the Anthropic endpoints and OpenAI ones
        // WHEN each is classified
        // THEN only the first are Anthropic's
        assert_eq!(Family::of_path("/v1/messages"), Family::Anthropic);
        assert_eq!(
            Family::of_path("/v1/messages/count_tokens"),
            Family::Anthropic
        );
        assert_eq!(Family::of_path("/v1/chat/completions"), Family::OpenAi);
        assert_eq!(Family::of_path("/v1/messagesx"), Family::OpenAi);
    }
}
