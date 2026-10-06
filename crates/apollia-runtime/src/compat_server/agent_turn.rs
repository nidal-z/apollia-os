//! Reading the turn an agent or a workflow is called for, from the body of
//! each endpoint.
//!
//! Whatever the format, the rule is one: the last user message is the
//! request, and everything before it is the conversation it comes from, one
//! `role: text` line each. What the client gives as the model's instructions
//! (`system` of the Messages API, `instructions` of the Responses API) opens
//! that conversation as a `system` line: it is context the agent reads, never
//! the agent's own mission.

use serde_json::Value;

use super::relay::Endpoint;

/// The request and the conversation before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) request: String,
    pub(crate) conversation: Option<String>,
}

/// The text of a `content`: a string, or the text parts of an array of
/// parts, whatever each format names them (`text`, `input_text`,
/// `output_text`).
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

fn role_of(message: &Value) -> Option<&str> {
    message.get("role").and_then(Value::as_str)
}

/// The turn `messages` end on, `instructions` opening the conversation.
/// `field` names the body field they were read from, for the refusal.
fn turn_of(instructions: Option<String>, messages: &[&Value], field: &str) -> Result<Turn, String> {
    let last_user = messages
        .iter()
        .rposition(|m| role_of(m) == Some("user"))
        .ok_or_else(|| format!("`{field}` must hold at least one message whose role is `user`."))?;
    let request = text_of(messages[last_user].get("content"));
    let conversation = instructions
        .filter(|text| !text.is_empty())
        .map(|text| format!("system: {text}"))
        .into_iter()
        .chain(messages[..last_user].iter().map(|m| {
            let role = role_of(m).unwrap_or("user");
            format!("{role}: {}", text_of(m.get("content")))
        }))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Turn {
        request,
        conversation: (!conversation.is_empty()).then_some(conversation),
    })
}

fn present(body: &Value, field: &str) -> bool {
    !matches!(body.get(field), None | Some(Value::Null))
}

/// The turn of a chat completion: `messages`, a `system` message being one of
/// them.
fn chat_turn(body: &Value) -> Result<Turn, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("The body must carry `messages`, an array of messages.")?;
    turn_of(None, &messages.iter().collect::<Vec<_>>(), "messages")
}

/// The turn of a response: `input`, a text or a list of items of which the
/// messages are read, and `instructions`. The fields that continue a stored
/// response are refused: the call holds no state. `store` is not read.
fn responses_turn(body: &Value) -> Result<Turn, String> {
    for field in ["previous_response_id", "conversation"] {
        if present(body, field) {
            return Err(format!(
                "A call to an Apollia agent or workflow holds no state, so `{field}` cannot \
                 be honoured. Remove it and send the earlier messages in `input`."
            ));
        }
    }
    let instructions = body
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match body.get("input") {
        Some(Value::String(text)) => Ok(Turn {
            request: text.clone(),
            conversation: instructions
                .filter(|text| !text.is_empty())
                .map(|text| format!("system: {text}")),
        }),
        Some(Value::Array(items)) => {
            let messages: Vec<&Value> = items
                .iter()
                .filter(|item| {
                    role_of(item).is_some()
                        && matches!(
                            item.get("type").and_then(Value::as_str),
                            None | Some("message")
                        )
                })
                .collect();
            turn_of(instructions, &messages, "input")
        }
        _ => Err("The body must carry `input`, a text or an array of input items.".to_owned()),
    }
}

/// The turn of a message: `messages`, and `system`, a text or text blocks.
fn messages_turn(body: &Value) -> Result<Turn, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("The body must carry `messages`, an array of messages.")?;
    let system = present(body, "system").then(|| text_of(body.get("system")));
    turn_of(system, &messages.iter().collect::<Vec<_>>(), "messages")
}

/// Reads the turn of `body`, the body of a call on `endpoint`.
///
/// # Errors
///
/// What is wrong, when the body holds no user message or asks for a state the
/// call does not keep.
pub(crate) fn read_turn(endpoint: Endpoint, body: &Value) -> Result<Turn, String> {
    match endpoint {
        Endpoint::Responses => responses_turn(body),
        Endpoint::Messages => messages_turn(body),
        _ => chat_turn(body),
    }
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
        let turn = read_turn(Endpoint::ChatCompletions, &body).unwrap();

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

        // WHEN each is read, as a chat completion and as a message
        // THEN both are refused with what is missing
        for endpoint in [Endpoint::ChatCompletions, Endpoint::Messages] {
            assert!(read_turn(endpoint, &assistant)
                .unwrap_err()
                .contains("`user`"));
            assert!(read_turn(endpoint, &empty)
                .unwrap_err()
                .contains("`messages`"));
        }
    }

    #[test]
    fn test_a_response_reads_its_input_items_and_its_instructions() {
        // GIVEN an input of items: an exchange, a tool output that is not a
        // message, and a last user message in parts, with instructions
        let body = json!({
            "instructions": "Be brief.",
            "input": [
                {"role": "user", "content": "Hello"},
                {"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": "Hi."}
                ]},
                {"type": "function_call_output", "call_id": "c1", "output": "42"},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "Qualify this demand."}
                ]}
            ]
        });

        // WHEN the turn is read
        let turn = read_turn(Endpoint::Responses, &body).unwrap();

        // THEN the last user item is the request; the instructions open the
        // conversation, and the item that is not a message is left out
        assert_eq!(turn.request, "Qualify this demand.");
        assert_eq!(
            turn.conversation.as_deref(),
            Some("system: Be brief.\nuser: Hello\nassistant: Hi.")
        );
    }

    #[test]
    fn test_a_response_takes_a_text_input_and_refuses_what_is_missing() {
        // GIVEN a text input, a body without input, and items without a user
        let text = json!({"input": "Qualify this demand."});
        let missing = json!({"model": "apollia/clerk"});
        let no_user = json!({"input": [{"role": "assistant", "content": "Hi."}]});

        // WHEN each is read
        let turn = read_turn(Endpoint::Responses, &text).unwrap();

        // THEN the text is the request with no conversation, and the two
        // others are refused with what is missing
        assert_eq!(turn.request, "Qualify this demand.");
        assert_eq!(turn.conversation, None);
        assert!(read_turn(Endpoint::Responses, &missing)
            .unwrap_err()
            .contains("`input`"));
        assert!(read_turn(Endpoint::Responses, &no_user)
            .unwrap_err()
            .contains("`user`"));
    }

    #[test]
    fn test_a_response_that_continues_a_stored_one_is_refused_and_store_is_ignored() {
        // GIVEN a body naming a previous response, one naming a conversation,
        // and one asking to store the response
        let previous = json!({"input": "Go on.", "previous_response_id": "resp_1"});
        let conversation = json!({"input": "Go on.", "conversation": {"id": "conv_1"}});
        let stored = json!({"input": "Go on.", "store": true, "previous_response_id": null});

        // WHEN each is read
        // THEN the first two are refused, naming the field and why; the third
        // is read as if `store` were absent
        let refusal = read_turn(Endpoint::Responses, &previous).unwrap_err();
        assert!(refusal.contains("`previous_response_id`"), "{refusal}");
        assert!(refusal.contains("holds no state"), "{refusal}");
        assert!(read_turn(Endpoint::Responses, &conversation)
            .unwrap_err()
            .contains("`conversation`"));
        assert_eq!(
            read_turn(Endpoint::Responses, &stored).unwrap().request,
            "Go on."
        );
    }

    #[test]
    fn test_the_system_of_a_message_opens_the_conversation() {
        // GIVEN a message body whose system is a list of text blocks, and one
        // whose system is a text
        let blocks = json!({
            "system": [{"type": "text", "text": "Be brief."}],
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": [{"type": "text", "text": "Hi."}]},
                {"role": "user", "content": [{"type": "text", "text": "Qualify."}]}
            ]
        });
        let text = json!({
            "system": "Be brief.",
            "messages": [{"role": "user", "content": "Qualify."}]
        });

        // WHEN each is read
        let (blocks, text) = (
            read_turn(Endpoint::Messages, &blocks).unwrap(),
            read_turn(Endpoint::Messages, &text).unwrap(),
        );

        // THEN the system is the first line of the conversation, never the
        // request
        assert_eq!(blocks.request, "Qualify.");
        assert_eq!(
            blocks.conversation.as_deref(),
            Some("system: Be brief.\nuser: Hello\nassistant: Hi.")
        );
        assert_eq!(text.request, "Qualify.");
        assert_eq!(text.conversation.as_deref(), Some("system: Be brief."));
    }
}
