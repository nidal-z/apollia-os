//! Schema-guided resolution of a tool step's structured arguments.
//!
//! Shared primitive behind both argument-resolution paths of the orchestrated
//! engine:
//!
//! - **Plan time**: the engine fills each tool step's `args` right after
//!   planning, so the persisted plan is fully specified, auditable and
//!   replayable.
//! - **Execution time**: the [`crate::actor::ActorLoop`] resolves a step's
//!   arguments just in time when the plan did not carry valid ones.
//!
//! Both call [`resolve_tool_args`], which asks the model for a tool call
//! constrained (on local backends) to the target tool's JSON schema via a GBNF
//! grammar, then validates the extracted arguments with [`validate_args`]. No
//! new generation machinery: the grammar comes from
//! [`apollia_llm::grammar::tool_specs_to_gbnf`] and flows through the existing
//! `CompletionRequest.grammar` field.

use std::sync::Arc;

use apollia_llm::grammar::tool_specs_to_gbnf;
use apollia_llm::{ChatMessage, CompletionModel, CompletionRequest, ToolSpec};
use serde_json::Value;

/// Errors raised while resolving a tool step's arguments.
#[derive(Debug, thiserror::Error)]
pub enum ArgResolveError {
    /// The underlying model call failed.
    #[error("model call failed: {0}")]
    Model(String),
    /// The model output could not be parsed as a JSON tool call.
    #[error("could not parse model output as a JSON tool call")]
    Unparsable,
    /// The extracted arguments did not satisfy the tool schema.
    #[error("arguments invalid against schema: {0}")]
    Invalid(String),
}

/// System prompt steering the model to emit a single tool call for `tool_name`.
const EXTRACTION_SYSTEM_PROMPT: &str = "\
You translate a step description into a single tool call. Respond with ONLY a \
JSON object of the form {\"name\": \"<tool>\", \"arguments\": { ... }} where \
arguments match the tool's input schema exactly. No prose, no code fences.";

/// Resolves structured arguments for a tool step from its schema and description.
///
/// Builds a one-tool [`ToolSpec`] from `schema`, attaches the GBNF grammar when
/// the backend runs locally (cloud backends ignore it and rely on the prompt),
/// asks the model for a tool call, extracts the `arguments` object and validates
/// it against `schema`.
///
/// # Errors
///
/// - [`ArgResolveError::Model`] if the completion call fails.
/// - [`ArgResolveError::Unparsable`] if the output is not a JSON tool call.
/// - [`ArgResolveError::Invalid`] if the arguments violate the schema.
pub async fn resolve_tool_args(
    model: &Arc<dyn CompletionModel>,
    tool_name: &str,
    schema: &Value,
    description: &str,
    temperature: f32,
) -> Result<Value, ArgResolveError> {
    let spec = ToolSpec {
        name: tool_name.to_string(),
        description: String::new(),
        parameters: schema.clone(),
    };

    let grammar = if model.is_local() {
        let gbnf = tool_specs_to_gbnf(std::slice::from_ref(&spec));
        (!gbnf.is_empty()).then_some(gbnf)
    } else {
        None
    };

    let user = format!("Tool: {tool_name}\nStep description:\n{description}");
    let response = model
        .complete(CompletionRequest {
            messages: vec![
                ChatMessage::system(EXTRACTION_SYSTEM_PROMPT),
                ChatMessage::user(&user),
            ],
            tools: vec![spec],
            temperature: Some(temperature),
            max_tokens: Some(1024),
            grammar,
            ..Default::default()
        })
        .await
        .map_err(|e| ArgResolveError::Model(e.to_string()))?;

    let args = extract_arguments(&response.content).ok_or(ArgResolveError::Unparsable)?;
    validate_args(&args, schema).map_err(ArgResolveError::Invalid)?;
    Ok(args)
}

/// Extracts the arguments object from a model tool-call output.
///
/// Accepts the grammar-shaped `{"name": ..., "arguments": {...}}` wrapper (from
/// which it returns the `arguments` object) as well as a bare arguments object
/// (returned as-is). Tolerates surrounding whitespace and ```` ```json ````
/// fences. Returns `None` when no JSON object can be recovered.
fn extract_arguments(content: &str) -> Option<Value> {
    let trimmed = strip_fences(content.trim());
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    let obj = parsed.as_object()?;
    match obj.get("arguments") {
        Some(args) if args.is_object() => Some(args.clone()),
        _ if obj.contains_key("name") => Some(Value::Object(serde_json::Map::new())),
        _ => Some(parsed.clone()),
    }
}

/// Strips a leading/trailing Markdown code fence, if present.
fn strip_fences(s: &str) -> &str {
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

/// Validates an arguments object against a tool JSON schema.
///
/// Dependency-free safety net complementing the GBNF constraint. Checks that
/// `args` is an object, that every `required` property is present, and that the
/// declared properties carry a compatible base type
/// (string/number/integer/boolean/array/object). Unknown or unconstrained
/// properties are left untouched.
///
/// # Errors
///
/// Returns a human-readable message describing the first violation found.
pub fn validate_args(args: &Value, schema: &Value) -> Result<(), String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for req in required {
            if let Some(key) = req.as_str() {
                if !obj.contains_key(key) {
                    return Err(format!("missing required property '{key}'"));
                }
            }
        }
    }

    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (key, val) in obj {
            if let Some(prop_schema) = props.get(key) {
                if let Some(expected) = prop_schema.get("type").and_then(Value::as_str) {
                    if !type_matches(expected, val) {
                        return Err(format!(
                            "property '{key}' should be {expected}, got {}",
                            json_type_name(val)
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Whether `val` satisfies the JSON Schema base `type` name.
fn type_matches(expected: &str, val: &Value) -> bool {
    match expected {
        "string" => val.is_string(),
        "integer" => val.is_i64() || val.is_u64(),
        "number" => val.is_number(),
        "boolean" => val.is_boolean(),
        "array" => val.is_array(),
        "object" => val.is_object(),
        "null" => val.is_null(),
        _ => true,
    }
}

/// Human-readable JSON type name for error messages.
fn json_type_name(val: &Value) -> &'static str {
    match val {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::types::{CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage};
    use futures::Stream;
    use std::pin::Pin;

    fn schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }

    // A model that returns a fixed content string, reporting a chosen locality.
    struct FixedModel {
        content: String,
        local: bool,
    }

    #[async_trait::async_trait]
    impl CompletionModel for FixedModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                engine_timings: None,
                content: self.content.clone(),
                tool_calls: vec![],
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }
        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }
        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "fixed"
        }
        fn model_id(&self) -> &str {
            "fixed"
        }
        fn is_local(&self) -> bool {
            self.local
        }
    }

    #[tokio::test]
    async fn test_resolve_extracts_and_validates_wrapped_call() {
        // GIVEN a model returning a grammar-shaped tool call
        let model: Arc<dyn CompletionModel> = Arc::new(FixedModel {
            content: r#"{"name":"file_write","arguments":{"path":"/tmp/x","content":"hi"}}"#.into(),
            local: true,
        });

        // WHEN resolving arguments against the tool schema
        let args = resolve_tool_args(&model, "file_write", &schema(), "write hi to /tmp/x", 0.0)
            .await
            .expect("resolution should succeed");

        // THEN the extracted arguments match the schema
        assert_eq!(args, serde_json::json!({"path": "/tmp/x", "content": "hi"}));
    }

    #[tokio::test]
    async fn test_resolve_rejects_arguments_missing_required() {
        // GIVEN a model returning a tool call missing the required 'content'
        let model: Arc<dyn CompletionModel> = Arc::new(FixedModel {
            content: r#"{"name":"file_write","arguments":{"path":"/tmp/x"}}"#.into(),
            local: false,
        });

        // WHEN resolving arguments
        let result =
            resolve_tool_args(&model, "file_write", &schema(), "write to /tmp/x", 0.0).await;

        // THEN validation fails on the missing required property
        assert!(matches!(result, Err(ArgResolveError::Invalid(_))));
    }

    #[test]
    fn test_validate_args_missing_required() {
        // GIVEN an object missing a required key
        let args = serde_json::json!({"path": "/tmp/x"});
        // WHEN validating against the schema
        let result = validate_args(&args, &schema());
        // THEN it reports the missing property
        assert!(result.unwrap_err().contains("content"));
    }

    #[test]
    fn test_validate_args_type_mismatch() {
        // GIVEN a property with the wrong type
        let args = serde_json::json!({"path": 42, "content": "hi"});
        // WHEN validating
        let result = validate_args(&args, &schema());
        // THEN it reports the type mismatch
        assert!(result.unwrap_err().contains("path"));
    }

    #[test]
    fn test_validate_args_accepts_valid_object() {
        // GIVEN a well-formed arguments object
        let args = serde_json::json!({"path": "/tmp/x", "content": "hi"});
        // WHEN validating
        // THEN it passes
        assert!(validate_args(&args, &schema()).is_ok());
    }
}
