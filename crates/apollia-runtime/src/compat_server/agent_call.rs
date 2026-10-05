//! A call on an `apollia/...` model: an exposed agent or workflow answers as
//! a model would, on `POST /v1/chat/completions`, `POST /v1/responses` and
//! `POST /v1/messages` alike.
//!
//! The call is stateless. For an agent, the last user message is the request
//! and the messages before it are the conversation it comes from; for a
//! workflow, the last user message is a JSON object that fits the workflow's
//! input schema. Each endpoint reads that turn from its own body
//! ([`super::agent_turn`]) and writes the answer in its own format
//! ([`super::agent_reply`]); everything between the two is one path. Tools sent by the client are refused: the agent has its own
//! accesses. A call from the local network needs a token in use, whatever
//! `[server] require_auth` says.

use std::collections::BTreeMap;

use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Map, Value};

use super::agent_reply::{answer, AnswerHeader, ChatAnswer};
use super::agent_reply_messages::MessagesAnswer;
use super::agent_reply_responses::ResponsesAnswer;
use super::agent_run::{start_agent, start_workflow, StartError};
use super::agent_turn::read_turn;
use super::agents::{ExposedAgents, ExposedTarget};
use super::auth::{presented_token, unauthorized_with};
use super::errors::{error_response, error_with, ErrorKind, Family};
use super::journal::LaunchedRun;
use super::origin::{admit_agent_call, admits_agent_call, CallOrigin, AGENT_TOKEN_REQUIRED};
use super::relay::Endpoint;
use super::tokens::ServerTokensHandle;
use crate::workflows::inputs::validate;

/// The field of the item an agent reads its request from.
pub(crate) const REQUEST_FIELD: &str = "request";
/// The field of the item holding the conversation before the request.
pub(crate) const CONVERSATION_FIELD: &str = "conversation";

/// Whether a caller from `origin` presenting `headers` may reach an agent or
/// a workflow: always from the machine, from the local network with a token
/// in use only.
pub(crate) async fn admitted(
    origin: CallOrigin,
    headers: &HeaderMap,
    tokens: Option<&ServerTokensHandle>,
) -> bool {
    let token_in_use = match (origin, presented_token(headers), tokens) {
        (CallOrigin::LocalNetwork, Some(token), Some(tokens)) => tokens.verify(token).await,
        _ => false,
    };
    admits_agent_call(origin, token_in_use)
}

/// Whether the client sent tools of its own.
fn carries_tools(body: &Value) -> bool {
    ["tools", "functions"]
        .iter()
        .any(|field| match body.get(*field) {
            None | Some(Value::Null) => false,
            Some(Value::Array(tools)) => !tools.is_empty(),
            Some(_) => true,
        })
}

/// The 400 of an input that does not fit the workflow's schema, carrying the
/// schema so the client can build a valid one.
fn invalid_input(family: Family, message: String, schema: Option<&Value>) -> Response {
    let mut extra = Map::new();
    extra.insert(
        "schema".to_owned(),
        schema
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
    );
    error_with(family, ErrorKind::InvalidRequest, message, extra)
}

/// Who calls: where from, and the headers that may carry a token.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caller<'a> {
    pub(crate) origin: CallOrigin,
    pub(crate) headers: &'a HeaderMap,
}

/// What is called: the endpoint, the body it received, and the `apollia/...`
/// model the body names.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExposedCall<'a> {
    pub(crate) endpoint: Endpoint,
    pub(crate) body: &'a Value,
    pub(crate) model: &'a str,
}

/// Answers `call` for `caller`, in the format of its endpoint.
///
/// The three endpoints differ in how the turn is read from the body and in
/// how the answer is written; admission, refusals and the run are the same.
pub(crate) async fn call_exposed(
    agents: &ExposedAgents,
    tokens: Option<&ServerTokensHandle>,
    caller: Caller<'_>,
    call: ExposedCall<'_>,
) -> Response {
    let ExposedCall {
        endpoint,
        body,
        model,
    } = call;
    let family = endpoint.family();
    let path = endpoint.path();
    let admission = match tokens {
        Some(tokens) => admit_agent_call(caller.origin, path, caller.headers, tokens).await,
        None if caller.origin == CallOrigin::Loopback => Ok(()),
        None => Err(unauthorized_with(path, AGENT_TOKEN_REQUIRED)),
    };
    if let Err(refusal) = admission {
        return refusal;
    }
    let invalid = |message: String| error_response(family, ErrorKind::InvalidRequest, message);
    if carries_tools(body) {
        return invalid(format!(
            "'{model}' is an Apollia agent or workflow: it works with the accesses it was \
             given in Apollia, so a request cannot bring tools. Remove `tools`."
        ));
    }
    let Some(target) = agents.find(model).await else {
        return error_response(
            family,
            ErrorKind::NotExposed,
            format!(
                "No agent or workflow available to other applications is named '{model}'. \
                 GET /v1/models lists them."
            ),
        );
    };
    let turn = match read_turn(endpoint, body) {
        Ok(turn) => turn,
        Err(message) => return invalid(message),
    };
    let Some(engines) = agents.engines() else {
        return error_response(
            family,
            ErrorKind::RunUnavailable,
            "This runtime cannot run agents or workflows.".to_owned(),
        );
    };
    let started = match target {
        ExposedTarget::Agent(agent) => {
            let mut fields = BTreeMap::from([(REQUEST_FIELD.to_owned(), turn.request)]);
            if let Some(conversation) = turn.conversation {
                fields.insert(CONVERSATION_FIELD.to_owned(), conversation);
            }
            start_agent(&engines, &agent, fields).await
        }
        ExposedTarget::Workflow(workflow) => {
            let schema = workflow.input_schema.as_ref();
            let inputs = match serde_json::from_str::<Value>(&turn.request) {
                Ok(inputs @ Value::Object(_)) => inputs,
                _ => {
                    return invalid_input(
                        family,
                        format!(
                            "'{model}' is a workflow: the last user message must be a JSON \
                             object that fits its input schema, given in `schema`."
                        ),
                        schema,
                    )
                }
            };
            if let Err(error) = validate(schema, &inputs) {
                return invalid_input(
                    family,
                    format!("Input '{}': {}", error.field, error.message),
                    schema,
                );
            }
            start_workflow(&engines, &workflow.id, inputs).await
        }
    };
    let run = match started {
        Ok(run) => run,
        Err(StartError::Unavailable(message)) => {
            return error_response(family, ErrorKind::RunUnavailable, message)
        }
        Err(StartError::Refused(message)) => {
            return error_response(family, ErrorKind::RunFailed, message)
        }
    };
    let header = AnswerHeader::new(&run.run_id, model);
    let launched = LaunchedRun {
        task_id: run.task_id.clone(),
        run_id: run.journal_run.clone(),
    };
    let streamed = body.get("stream") == Some(&Value::Bool(true));
    let mut response = match endpoint {
        Endpoint::Responses => answer(run, ResponsesAnswer::new(header), streamed).await,
        Endpoint::Messages => answer(run, MessagesAnswer::new(header), streamed).await,
        _ => answer(run, ChatAnswer::new(header), streamed).await,
    };
    response.extensions_mut().insert(launched);
    response
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_tools_sent_by_the_client_are_noticed() {
        // GIVEN bodies with tools, legacy functions, an empty list and none
        // WHEN each is checked
        // THEN only a non-empty list counts
        assert!(carries_tools(&json!({"tools": [{"type": "function"}]})));
        assert!(carries_tools(&json!({"functions": [{"name": "f"}]})));
        assert!(!carries_tools(&json!({"tools": []})));
        assert!(!carries_tools(&json!({"tools": null})));
        assert!(!carries_tools(&json!({})));
    }
}
