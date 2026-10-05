//! `POST /v1/chat/completions` on an `apollia/...` model: an exposed agent or
//! workflow answers as a model would.
//!
//! The call is stateless. For an agent, the last user message is the request
//! and the messages before it are the conversation it comes from; for a
//! workflow, the last user message is a JSON object that fits the workflow's
//! input schema. Tools sent by the client are refused: the agent has its own
//! accesses. A call from the local network needs a token in use, whatever
//! `[server] require_auth` says.

use std::collections::BTreeMap;

use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Map, Value};

use super::agent_reply::{answer_streamed, answer_whole, AnswerHeader};
use super::agent_run::{start_agent, start_workflow, StartError};
use super::agents::{ExposedAgents, ExposedTarget};
use super::auth::{presented_token, unauthorized_with};
use super::errors::{error_response, openai_error_with, ErrorKind, Family};
use super::journal::LaunchedRun;
use super::origin::{admit_agent_call, admits_agent_call, CallOrigin, AGENT_TOKEN_REQUIRED};
use super::tokens::ServerTokensHandle;
use crate::workflows::inputs::validate;

/// The path of the endpoint, named in a refusal.
const PATH: &str = "/v1/chat/completions";

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

/// The text of a message's `content`: a string, or the text parts of an
/// array of parts.
fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The request and the conversation before it, read from `messages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) request: String,
    pub(crate) conversation: Option<String>,
}

/// Reads the last user message of `body` as the request, and every message
/// before it as the conversation, one `role: text` line each.
///
/// # Errors
///
/// What is wrong, when `messages` is missing or holds no user message.
pub(crate) fn read_turn(body: &Value) -> Result<Turn, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("The body must carry `messages`, an array of messages.")?;
    let last_user = messages
        .iter()
        .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        .ok_or("`messages` must hold at least one message whose role is `user`.")?;
    let request = text_of(messages[last_user].get("content"));
    let conversation = messages[..last_user]
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            format!("{role}: {}", text_of(m.get("content")))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Turn {
        request,
        conversation: (!conversation.is_empty()).then_some(conversation),
    })
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

fn invalid(message: impl Into<String>) -> Response {
    error_response(Family::OpenAi, ErrorKind::InvalidRequest, message.into())
}

/// The 400 of an input that does not fit the workflow's schema, carrying the
/// schema so the client can build a valid one.
fn invalid_input(message: String, schema: Option<&Value>) -> Response {
    let mut extra = Map::new();
    extra.insert(
        "schema".to_owned(),
        schema
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
    );
    openai_error_with(ErrorKind::InvalidRequest, message, extra)
}

/// Who calls: where from, and the headers that may carry a token.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caller<'a> {
    pub(crate) origin: CallOrigin,
    pub(crate) headers: &'a HeaderMap,
}

/// Answers a chat completion on `model`, an `apollia/...` model, for
/// `caller`.
pub(crate) async fn chat_with_exposed(
    agents: &ExposedAgents,
    tokens: Option<&ServerTokensHandle>,
    caller: Caller<'_>,
    body: &Value,
    model: &str,
) -> Response {
    let admission = match tokens {
        Some(tokens) => admit_agent_call(caller.origin, PATH, caller.headers, tokens).await,
        None if caller.origin == CallOrigin::Loopback => Ok(()),
        None => Err(unauthorized_with(PATH, AGENT_TOKEN_REQUIRED)),
    };
    if let Err(refusal) = admission {
        return refusal;
    }
    if carries_tools(body) {
        return invalid(format!(
            "'{model}' is an Apollia agent or workflow: it works with the accesses it was \
             given in Apollia, so a request cannot bring tools. Remove `tools`."
        ));
    }
    let Some(target) = agents.find(model).await else {
        return error_response(
            Family::OpenAi,
            ErrorKind::NotExposed,
            format!(
                "No agent or workflow available to other applications is named '{model}'. \
                 GET /v1/models lists them."
            ),
        );
    };
    let turn = match read_turn(body) {
        Ok(turn) => turn,
        Err(message) => return invalid(message),
    };
    let Some(engines) = agents.engines() else {
        return error_response(
            Family::OpenAi,
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
            return error_response(Family::OpenAi, ErrorKind::RunUnavailable, message)
        }
        Err(StartError::Refused(message)) => {
            return error_response(Family::OpenAi, ErrorKind::RunFailed, message)
        }
    };
    let header = AnswerHeader::new(&run.run_id, model);
    let launched = LaunchedRun {
        task_id: run.task_id.clone(),
        run_id: run.journal_run.clone(),
    };
    let mut response = if body.get("stream") == Some(&Value::Bool(true)) {
        answer_streamed(run, header)
    } else {
        answer_whole(run, header).await
    };
    response.extensions_mut().insert(launched);
    response
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_the_last_user_message_is_the_request_and_the_rest_its_context() {
        // GIVEN a system message, a first exchange, and a last user message in
        // parts
        let body = json!({"messages": [
            {"role": "system", "content": "Be brief."},
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi."},
            {"role": "user", "content": [
                {"type": "text", "text": "Qualify"},
                {"type": "text", "text": "this demand."}
            ]}
        ]});

        // WHEN the turn is read
        let turn = read_turn(&body).unwrap();

        // THEN the last user message is the request, the others its context
        assert_eq!(turn.request, "Qualify\nthis demand.");
        assert_eq!(
            turn.conversation.as_deref(),
            Some("system: Be brief.\nuser: Hello\nassistant: Hi.")
        );
    }

    #[test]
    fn test_a_body_without_a_user_message_is_refused() {
        // GIVEN a body with only an assistant message, and one without messages
        let assistant = json!({"messages": [{"role": "assistant", "content": "Hi."}]});
        let empty = json!({"model": "apollia/clerk"});

        // WHEN each is read
        // THEN both are refused with what is missing
        assert!(read_turn(&assistant).unwrap_err().contains("`user`"));
        assert!(read_turn(&empty).unwrap_err().contains("`messages`"));
    }

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
