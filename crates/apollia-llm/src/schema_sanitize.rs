//! Normalize JSON-Schema tool parameters into a shape that a remote
//! json-schema-to-grammar converter (llama.cpp / llama-server) can turn into a
//! valid GBNF grammar.
//!
//! When an OpenAI-compatible chat request carries `tools`, a llama.cpp server
//! builds a constraining grammar from each tool's `parameters`. Several
//! JSON-Schema constructs drive that converter into an oversized or
//! unresolvable expansion and it answers `400 "failed to parse grammar"`. The
//! observed trigger is a large `maxLength` on a string nested inside an array
//! item: the converter unrolls it into a bounded character repetition that the
//! PEG grammar parser rejects. Schema combinators (`oneOf`/`anyOf`/`allOf`) and
//! `$ref` are also unsupported by the converter.
//!
//! Cloud APIs treat these keywords as advisory, so dropping them from tool
//! schemas is safe and keeps native tool-calling working against a local
//! llama-server. The runtime still validates real arguments; the grammar only
//! shapes generation.

use serde_json::{Map, Value};

/// Validation-only keywords that push the grammar builder into oversized or
/// unsupported expansions, plus reference machinery it cannot resolve. Removed
/// at every depth.
const STRIP_KEYS: &[&str] = &[
    "maxLength",
    "minLength",
    "pattern",
    "format",
    "$schema",
    "$id",
    "$ref",
    "$defs",
    "definitions",
];

/// Schema combinators the converter cannot fold into a single grammar rule.
/// Their presence collapses the node to a free JSON value.
const COMBINATORS: &[&str] = &["oneOf", "anyOf", "allOf", "not"];

/// Return a grammar-safe copy of a tool `parameters` schema.
pub(crate) fn grammar_safe_schema(schema: &Value) -> Value {
    sanitize(schema)
}

fn sanitize(node: &Value) -> Value {
    match node {
        Value::Object(map) => sanitize_object(map),
        Value::Array(items) => Value::Array(items.iter().map(sanitize).collect()),
        other => other.clone(),
    }
}

fn sanitize_object(map: &Map<String, Value>) -> Value {
    // A node carrying an unresolvable combinator or a `$ref` becomes a free
    // value, keeping only its description so the model retains the human hint.
    if map.contains_key("$ref") || COMBINATORS.iter().any(|k| map.contains_key(*k)) {
        let mut out = Map::new();
        if let Some(desc) = map.get("description") {
            out.insert("description".to_string(), desc.clone());
        }
        return Value::Object(out);
    }

    let mut out = Map::new();
    for (key, value) in map {
        if STRIP_KEYS.contains(&key.as_str()) {
            continue;
        }
        out.insert(key.clone(), sanitize_field(key, value));
    }

    // Collapse a union `type` array (e.g. `["string", "null"]`) to the first
    // concrete type; the converter needs a single type per node.
    if let Some(Value::Array(types)) = out.get("type").cloned() {
        let picked = types
            .iter()
            .find_map(|t| t.as_str().filter(|s| *s != "null"))
            .or_else(|| types.iter().find_map(|t| t.as_str()));
        match picked {
            Some(t) => {
                out.insert("type".to_string(), Value::String(t.to_string()));
            }
            None => {
                out.remove("type");
            }
        }
    }

    // An `array` with no `items` yields an incomplete grammar rule; give it a
    // free-value item schema.
    if out.get("type").and_then(Value::as_str) == Some("array") && !out.contains_key("items") {
        out.insert("items".to_string(), Value::Object(Map::new()));
    }

    Value::Object(out)
}

fn sanitize_field(key: &str, value: &Value) -> Value {
    match key {
        // Maps of named sub-schemas: recurse into each value.
        "properties" | "patternProperties" => match value {
            Value::Object(props) => Value::Object(
                props
                    .iter()
                    .map(|(k, v)| (k.clone(), sanitize(v)))
                    .collect(),
            ),
            other => sanitize(other),
        },
        // Leaf data that must survive verbatim (values, not schemas).
        "enum" | "required" | "default" | "const" | "examples" => value.clone(),
        // Sub-schemas (`items`, `additionalProperties`, `contains`, ...) and any
        // remaining scalar keywords.
        _ => sanitize(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn contains_key_deep(v: &Value, key: &str) -> bool {
        match v {
            Value::Object(map) => {
                map.contains_key(key) || map.values().any(|c| contains_key_deep(c, key))
            }
            Value::Array(items) => items.iter().any(|c| contains_key_deep(c, key)),
            _ => false,
        }
    }

    #[test]
    fn strips_nested_max_length_from_array_items() {
        // GIVEN the exact construct that made llama-server reject the grammar:
        // a maxLength string nested inside an array's object items.
        let schema = json!({
            "type": "object",
            "properties": {
                "proposals": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "justification": {"type": "string", "maxLength": 2000}
                        }
                    }
                }
            }
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN no length keyword survives, and the string keeps its type.
        assert!(!contains_key_deep(&out, "maxLength"));
        assert_eq!(
            out["properties"]["proposals"]["items"]["properties"]["justification"]["type"],
            json!("string")
        );
    }

    #[test]
    fn collapses_union_type_to_first_concrete() {
        // GIVEN a nullable union type.
        let schema = json!({
            "type": "object",
            "properties": {"title": {"type": ["string", "null"]}}
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN the union collapses to the first non-null type.
        assert_eq!(out["properties"]["title"]["type"], json!("string"));
    }

    #[test]
    fn gives_free_items_to_array_without_items() {
        // GIVEN an array property with no `items`.
        let schema = json!({
            "type": "object",
            "properties": {"values": {"type": "array"}}
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN it gains a free-value item schema.
        assert_eq!(out["properties"]["values"]["items"], json!({}));
    }

    #[test]
    fn collapses_combinator_to_free_value_keeping_description() {
        // GIVEN a property expressed as an `anyOf` combinator with a hint.
        let schema = json!({
            "type": "object",
            "properties": {
                "x": {"description": "id or name", "anyOf": [{"type": "string"}, {"type": "integer"}]}
            }
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN the combinator is gone and only the description remains.
        assert_eq!(out["properties"]["x"], json!({"description": "id or name"}));
    }

    #[test]
    fn drops_ref_and_schema_machinery() {
        // GIVEN a schema using `$ref`, `$defs` and a draft `$schema` marker.
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"y": {"$ref": "#/$defs/Y"}},
            "$defs": {"Y": {"type": "string"}}
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN no reference machinery survives and `$ref` collapses to a free value.
        assert!(!contains_key_deep(&out, "$ref"));
        assert!(!contains_key_deep(&out, "$defs"));
        assert!(!contains_key_deep(&out, "$schema"));
        assert_eq!(out["properties"]["y"], json!({}));
    }

    #[test]
    fn preserves_plain_schema_and_numeric_bounds() {
        // GIVEN a plain schema with an enum and numeric bounds.
        let schema = json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["read", "write"]},
                "count": {"type": "integer", "minimum": 0, "maximum": 10}
            },
            "required": ["mode"]
        });

        // WHEN sanitized.
        let out = grammar_safe_schema(&schema);

        // THEN the useful structure is preserved unchanged.
        assert_eq!(out, schema);
    }
}
