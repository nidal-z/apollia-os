//! Ollama's native `/api/chat` wire format, built and read without a server.
//!
//! Kept apart from the client so every shape is tested offline: the request
//! body, the non-streamed answer, and each line of the streamed one.

use serde_json::{json, Map, Value};

use crate::types::{
    ChatMessage, CompletionRequest, FinishReason, LlmError, MessageContent, Role, StreamChunk,
    TokenUsage, ToolCall,
};

/// Opening tag reasoning is re-inlined under, the shape the chat pipeline parses.
const THINK_OPEN: &str = "<think>";
/// Closing counterpart of [`THINK_OPEN`].
const THINK_CLOSE: &str = "</think>";

/// The request body for one chat call.
///
/// `options.num_ctx` is the reason this client exists. Ollama's
/// OpenAI-compatible endpoint accepts no way to choose the context window, and
/// a call through it reloads the model at the server default, which Ollama
/// derives from the machine's video memory and sets to 4096 tokens below
/// 24 GB. Measured on 2026-09-22 against Ollama 0.34.2 on a 16 GB card: a
/// native call at 16384 loaded the model at 16384, and the next
/// OpenAI-compatible call reloaded it at 4096. At 4096 the system prompt and the
/// tool schemas alone overflow the window, and Ollama drops the front of the
/// prompt without an error, so the model answered with neither its tools nor
/// the conversation.
#[must_use]
pub fn request_body(req: &CompletionRequest, model: &str, num_ctx: u32, stream: bool) -> Value {
    let mut options = Map::new();
    options.insert("num_ctx".into(), json!(num_ctx));
    if let Some(t) = req.temperature {
        options.insert("temperature".into(), json!(t));
    }
    if let Some(n) = req.max_tokens {
        options.insert("num_predict".into(), json!(n));
    }
    if let Some(seed) = req.seed {
        options.insert("seed".into(), json!(seed));
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(req.model.as_deref().unwrap_or(model)));
    body.insert("messages".into(), Value::Array(messages(&req.messages)));
    body.insert("stream".into(), json!(stream));
    body.insert("options".into(), Value::Object(options));

    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|spec| {
                json!({
                    "type": "function",
                    "function": {
                        "name": spec.name,
                        "description": spec.description,
                        "parameters": crate::schema_sanitize::grammar_safe_schema(&spec.parameters),
                    }
                })
            })
            .collect();
        body.insert("tools".into(), Value::Array(tools));
    }

    // Ollama constrains decoding to a JSON Schema through `format`. A GBNF
    // grammar has no equivalent here and is left out; the answer is still
    // validated against the schema on the way back by the caller.
    if let Some(schema) = &req.response_schema {
        body.insert("format".into(), schema.clone());
    }

    Value::Object(body)
}

/// The conversation in Ollama's message shape.
///
/// A tool result carries the name of the tool that produced it, which Ollama
/// renders into the model's template; the name is recovered from the call that
/// asked for it, since the pipeline's own result message holds only its id.
fn messages(history: &[ChatMessage]) -> Vec<Value> {
    let mut names_by_id: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(history.len());

    for msg in history {
        match &msg.content {
            MessageContent::Text(text) => out.push(json!({
                "role": role(&msg.role),
                "content": text,
            })),
            MessageContent::WithToolCalls { text, tool_calls } => {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        names_by_id.insert(tc.id.as_str(), tc.name.as_str());
                        json!({
                            "id": tc.id,
                            "function": { "name": tc.name, "arguments": tc.arguments },
                        })
                    })
                    .collect();
                out.push(json!({
                    "role": "assistant",
                    "content": text,
                    "tool_calls": calls,
                }));
            }
            MessageContent::ToolResult {
                tool_call_id,
                content,
            } => {
                let mut m = json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                });
                if let Some(name) = names_by_id.get(tool_call_id.as_str()) {
                    m["tool_name"] = json!(name);
                }
                out.push(m);
            }
        }
    }
    out
}

fn role(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// Reject a body that carries Ollama's own error field.
///
/// # Errors
/// [`LlmError::InferenceError`] with the server's message.
pub fn check_error(value: &Value) -> Result<(), LlmError> {
    match value.get("error").and_then(Value::as_str) {
        Some(message) => Err(LlmError::InferenceError(format!("ollama: {message}"))),
        None => Ok(()),
    }
}

/// Tool calls out of one `message` object.
///
/// Ollama sends arguments as an object; a string is accepted too, parsed, in
/// case a template or a proxy flattened them. A call without an id is given one,
/// since the pipeline pairs every result with the call that asked for it.
pub fn tool_calls(message: &Value, first_index: usize) -> Vec<ToolCall> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .enumerate()
                .filter_map(|(i, call)| {
                    let function = call.get("function")?;
                    let name = function.get("name")?.as_str()?.to_owned();
                    let arguments = match function.get("arguments") {
                        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                        Some(v) => v.clone(),
                        None => Value::Object(Map::new()),
                    };
                    let id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map_or_else(|| format!("call_{}", first_index + i), str::to_owned);
                    Some(ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Token counts from a final (`done: true`) body.
pub fn usage(value: &Value) -> TokenUsage {
    let count = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    TokenUsage {
        prompt_tokens: count("prompt_eval_count"),
        completion_tokens: count("eval_count"),
        cost_usd: Some(0.0),
        ..Default::default()
    }
}

/// The answer text of a non-streamed call, reasoning re-inlined.
///
/// Ollama separates a thinking model's reasoning into `message.thinking`. It is
/// put back inline as `<think>` so this backend reaches the pipeline in the
/// same shape as every other one. A call whose decoding was constrained reads
/// the two as one, since a constrained value in the reasoning channel is still
/// the value and tags would make it unparseable.
pub fn answer_text(message: &Value, constrained: bool) -> String {
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let thinking = message
        .get("thinking")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if constrained {
        return if content.trim().is_empty() {
            thinking.to_owned()
        } else {
            content.to_owned()
        };
    }
    if thinking.is_empty() {
        content.to_owned()
    } else {
        format!("{THINK_OPEN}{thinking}{THINK_CLOSE}{content}")
    }
}

/// How a non-streamed call ended.
pub fn finish_reason(value: &Value, has_tool_calls: bool) -> FinishReason {
    if has_tool_calls {
        return FinishReason::ToolCalls;
    }
    match value.get("done_reason").and_then(Value::as_str) {
        Some("length") => FinishReason::Length,
        _ => FinishReason::Stop,
    }
}

/// State carried from one streamed line to the next.
#[derive(Debug, Default)]
pub struct StreamState {
    /// Whether a `<think>` block opened for reasoning is still open.
    in_think: bool,
    /// Tool calls emitted so far, to number the ones that arrive without an id.
    tool_calls_seen: usize,
    /// Whether the final line has been read.
    pub done: bool,
}

impl StreamState {
    /// Turn one NDJSON line into the chunks it carries.
    ///
    /// # Errors
    /// [`LlmError::ParseError`] for a line that is not JSON, and
    /// [`LlmError::InferenceError`] for a line carrying Ollama's error field.
    pub fn line(&mut self, line: &str) -> Result<Vec<StreamChunk>, LlmError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|e| LlmError::ParseError(format!("ollama stream line: {e}")))?;
        check_error(&value)?;

        let mut out = Vec::new();
        if let Some(message) = value.get("message") {
            if let Some(thinking) = message
                .get("thinking")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                if !self.in_think {
                    out.push(StreamChunk::Text(THINK_OPEN.to_owned()));
                    self.in_think = true;
                }
                out.push(StreamChunk::Text(thinking.to_owned()));
            }
            if let Some(content) = message
                .get("content")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                self.close_think(&mut out);
                out.push(StreamChunk::Text(content.to_owned()));
            }
            let calls = tool_calls(message, self.tool_calls_seen);
            if !calls.is_empty() {
                self.close_think(&mut out);
                self.tool_calls_seen += calls.len();
                out.extend(calls.into_iter().map(StreamChunk::ToolCall));
            }
        }

        if value.get("done").and_then(Value::as_bool) == Some(true) {
            self.close_think(&mut out);
            out.push(StreamChunk::Usage(usage(&value)));
            self.done = true;
        }
        Ok(out)
    }

    /// Chunks owed when the body ends without a final line.
    pub fn finish(&mut self) -> Vec<StreamChunk> {
        let mut out = Vec::new();
        self.close_think(&mut out);
        out
    }

    fn close_think(&mut self, out: &mut Vec<StreamChunk>) {
        if self.in_think {
            out.push(StreamChunk::Text(THINK_CLOSE.to_owned()));
            self.in_think = false;
        }
    }
}

/// The trained context length out of an `/api/show` body.
///
/// `model_info` carries the GGUF metadata under `<architecture>.context_length`.
#[must_use]
pub fn trained_context(show: &Value) -> Option<u32> {
    show.get("model_info")?
        .as_object()?
        .iter()
        .find(|(key, _)| key.ends_with(".context_length"))
        .and_then(|(_, v)| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolSpec;

    fn texts(chunks: &[StreamChunk]) -> String {
        chunks
            .iter()
            .filter_map(|c| match c {
                StreamChunk::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_request_asks_for_the_context_window() {
        // GIVEN a request and a planned window of 32768 tokens
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hi")],
            max_tokens: Some(64),
            ..Default::default()
        };

        // WHEN the body is built
        let body = request_body(&req, "qwen3.5", 32_768, true);

        // THEN the window rides in `options.num_ctx`, the one field the
        // OpenAI-compatible endpoint cannot carry
        assert_eq!(body["options"]["num_ctx"], 32_768);
        assert_eq!(body["options"]["num_predict"], 64);
        assert_eq!(body["model"], "qwen3.5");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn tools_and_the_whole_history_are_sent() {
        // GIVEN a turn with a tool call, its result and a tool on offer
        let call = ToolCall {
            id: "c1".into(),
            name: "fs.read_dir".into(),
            arguments: json!({ "path": "Downloads" }),
        };
        let req = CompletionRequest {
            messages: vec![
                ChatMessage::system("sys"),
                ChatMessage::user("list my downloads"),
                ChatMessage::assistant_with_calls("", std::slice::from_ref(&call)),
                ChatMessage::tool_result("c1", "report.pdf"),
            ],
            tools: vec![ToolSpec {
                name: "fs.read_dir".into(),
                description: "List a directory".into(),
                parameters: json!({ "type": "object", "properties": {} }),
            }],
            ..Default::default()
        };

        // WHEN the body is built
        let body = request_body(&req, "qwen3.5", 8192, false);

        // THEN every message is there in order, the call carries object
        // arguments, and the result names the tool that produced it
        let msgs = body["messages"].as_array().expect("messages");
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(
            msgs[2]["tool_calls"][0]["function"]["arguments"]["path"],
            "Downloads"
        );
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(msgs[3]["tool_name"], "fs.read_dir");
        assert_eq!(body["tools"][0]["function"]["name"], "fs.read_dir");
    }

    #[test]
    fn reasoning_is_reinlined_as_think_tags() {
        // GIVEN an answer whose reasoning Ollama separated out
        let message = json!({ "thinking": "weighing", "content": "the answer" });

        // WHEN the text is read
        // THEN it reaches the pipeline in the shape every backend uses
        assert_eq!(
            answer_text(&message, false),
            "<think>weighing</think>the answer"
        );
        assert_eq!(answer_text(&message, true), "the answer");
    }

    #[test]
    fn a_tool_call_is_read_with_object_or_string_arguments() {
        // GIVEN one call as Ollama sends it and one flattened to a string
        let message = json!({ "tool_calls": [
            { "id": "call_a", "function": { "name": "x", "arguments": { "k": 1 } } },
            { "function": { "name": "y", "arguments": "{\"k\": 2}" } }
        ]});

        // WHEN the calls are read
        let calls = tool_calls(&message, 0);

        // THEN both parse, and the one without an id gets one
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].arguments["k"], 1);
        assert_eq!(calls[1].id, "call_1");
        assert_eq!(calls[1].arguments["k"], 2);
    }

    #[test]
    fn a_streamed_turn_brackets_its_reasoning_and_reports_usage() {
        // GIVEN the lines of a streamed turn: reasoning, then answer, then done
        let mut state = StreamState::default();
        let mut chunks = Vec::new();
        for line in [
            r#"{"message":{"role":"assistant","content":"","thinking":"plan"},"done":false}"#,
            r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#,
            r#"{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":334,"eval_count":12}"#,
        ] {
            chunks.extend(state.line(line).expect("a valid line"));
        }

        // WHEN the chunks are read back
        // THEN the reasoning is bracketed once, the answer follows, and the
        // prompt size reaches the context gauge
        assert_eq!(texts(&chunks), "<think>plan</think>Hello");
        assert!(state.done);
        assert!(chunks
            .iter()
            .any(|c| matches!(c, StreamChunk::Usage(u) if u.prompt_tokens == 334)));
    }

    #[test]
    fn a_streamed_tool_call_closes_the_reasoning_first() {
        // GIVEN a stream that goes from reasoning straight to a tool call
        let mut state = StreamState::default();
        let mut chunks = state
            .line(r#"{"message":{"content":"","thinking":"I will list it"},"done":false}"#)
            .expect("a valid line");
        chunks.extend(
            state
                .line(r#"{"message":{"content":"","tool_calls":[{"function":{"name":"ls","arguments":{}}}]},"done":false}"#)
                .expect("a valid line"),
        );

        // WHEN the chunks are read back
        // THEN the think block is balanced before the call is emitted
        assert_eq!(texts(&chunks), "<think>I will list it</think>");
        assert!(matches!(chunks.last(), Some(StreamChunk::ToolCall(c)) if c.name == "ls"));
    }

    #[test]
    fn an_error_line_is_an_error() {
        // GIVEN the line Ollama sends when a model is missing
        let mut state = StreamState::default();

        // WHEN it is read
        // THEN it surfaces as an error rather than as an empty answer
        assert!(state.line(r#"{"error":"model 'nope' not found"}"#).is_err());
    }

    #[test]
    fn the_trained_context_is_read_from_the_model_info() {
        // GIVEN an `/api/show` body for a Qwen3.5 model
        let show = json!({ "model_info": {
            "general.architecture": "qwen35",
            "qwen35.context_length": 262_144,
            "qwen35.block_count": 32
        }});

        // WHEN the trained context is read
        // THEN the architecture-prefixed key is found
        assert_eq!(trained_context(&show), Some(262_144));
        assert_eq!(trained_context(&json!({})), None);
    }
}
