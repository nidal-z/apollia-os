//! Arguments of a tool call a model wrote as text where the schema of the
//! tool asks for a structure.
//!
//! A local model, and some engines on its behalf, sends a nested argument as
//! the JSON text of the value rather than as the value: `"items": "[{...}]"`
//! where the tool declares an array. The call is well meant and the content is
//! right; a tool that deserialises its arguments strictly refuses it with
//! "invalid type: string, expected a sequence", and the model rarely finds the
//! way out on its own. Measured on an MLX model served by vllm-mlx
//! (2026-10-13), on the task list tool.
//!
//! The repair is narrow on purpose. A string is decoded only where the schema
//! of the tool declares an array or an object, and only when the text parses
//! to exactly that kind of value. A string the schema declares a string, or
//! declares nothing about, is never touched: text that merely looks like JSON
//! stays text.

use serde_json::Value;

use crate::types::{ToolCall, ToolSpec};

/// Decode, in the arguments of each call, the values written as JSON text
/// where the schema of its tool declares an array or an object. Returns how
/// many values were decoded.
pub fn decode_stringified_arguments(calls: &mut [ToolCall], specs: &[ToolSpec]) -> usize {
    let mut decoded = 0;
    for call in calls {
        let Some(spec) = specs.iter().find(|spec| spec.name == call.name) else {
            continue;
        };
        decoded += decode(&mut call.arguments, &spec.parameters);
    }
    decoded
}

/// The JSON kind a schema declares for a node, when it declares a structure.
fn declared_structure(schema: &Value) -> Option<&str> {
    match schema.get("type").and_then(Value::as_str) {
        Some(kind @ ("array" | "object")) => Some(kind),
        _ => None,
    }
}

fn is_kind(value: &Value, kind: &str) -> bool {
    match kind {
        "array" => value.is_array(),
        _ => value.is_object(),
    }
}

fn decode(value: &mut Value, schema: &Value) -> usize {
    let mut decoded = 0;
    if let (Some(kind), Value::String(text)) = (declared_structure(schema), &*value) {
        match serde_json::from_str::<Value>(text) {
            Ok(parsed) if is_kind(&parsed, kind) => {
                *value = parsed;
                decoded += 1;
            }
            _ => return 0,
        }
    }
    match value {
        Value::Object(fields) => {
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, field) in fields.iter_mut() {
                    if let Some(field_schema) = properties.get(name) {
                        decoded += decode(field, field_schema);
                    }
                }
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for item in items.iter_mut() {
                    decoded += decode(item, item_schema);
                }
            }
        }
        _ => {}
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task_list() -> ToolSpec {
        ToolSpec {
            name: "todo_write".to_owned(),
            description: String::new(),
            parameters: json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {"type": "string"},
                                "tags": {"type": "array", "items": {"type": "string"}}
                            }
                        }
                    },
                    "note": {"type": "string"}
                }
            }),
        }
    }

    fn call(arguments: Value) -> ToolCall {
        ToolCall {
            id: "c1".to_owned(),
            name: "todo_write".to_owned(),
            arguments,
        }
    }

    #[test]
    fn test_an_array_written_as_text_is_decoded_where_the_schema_asks_for_one() {
        // GIVEN the call a local model made: the list as the text of a list
        let mut calls = vec![call(json!({
            "items": "[{\"content\": \"Open the connections screen\", \"tags\": \"[\\\"nav\\\"]\"}]"
        }))];

        // WHEN the arguments are read against the schema of the tool
        let decoded = decode_stringified_arguments(&mut calls, &[task_list()]);

        // THEN the list is a list, and the one nested in its item too
        assert_eq!(decoded, 2);
        assert_eq!(
            calls[0].arguments,
            json!({"items": [{"content": "Open the connections screen", "tags": ["nav"]}]})
        );
    }

    #[test]
    fn test_text_the_schema_declares_text_is_left_alone() {
        // GIVEN a note that happens to read as JSON, and a well-formed list
        let arguments = json!({"items": [{"content": "[1, 2]"}], "note": "{\"a\": 1}"});
        let mut calls = vec![call(arguments.clone())];

        // WHEN the arguments are read against the schema
        let decoded = decode_stringified_arguments(&mut calls, &[task_list()]);

        // THEN nothing is decoded: text stays text where text was asked for
        assert_eq!(decoded, 0);
        assert_eq!(calls[0].arguments, arguments);
    }

    #[test]
    fn test_text_that_is_not_the_declared_structure_is_left_for_the_tool_to_refuse() {
        // GIVEN a list written as text that is not JSON, one that is an object
        // where a list is asked, and a call to a tool no schema describes
        let broken = json!({"items": "first, second"});
        let wrong_kind = json!({"items": "{\"content\": \"x\"}"});
        let mut calls = vec![call(broken.clone()), call(wrong_kind.clone())];
        let mut unknown = vec![ToolCall {
            id: "c2".to_owned(),
            name: "elsewhere".to_owned(),
            arguments: json!({"items": "[1]"}),
        }];

        // WHEN each is read against the schema
        let decoded = decode_stringified_arguments(&mut calls, &[task_list()]);
        let decoded_unknown = decode_stringified_arguments(&mut unknown, &[task_list()]);

        // THEN none is touched: the tool says what is wrong with it
        assert_eq!(decoded + decoded_unknown, 0);
        assert_eq!(calls[0].arguments, broken);
        assert_eq!(calls[1].arguments, wrong_kind);
        assert_eq!(unknown[0].arguments, json!({"items": "[1]"}));
    }
}
