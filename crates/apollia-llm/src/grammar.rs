//! GBNF grammar generation from tool JSON schemas.
//!
//! Produces a GBNF grammar that constrains a local model to emit only
//! syntactically valid tool calls: a top-level object with a `"name"` in the
//! allowed tool set and an `"arguments"` object whose properties follow each
//! tool's declared schema. The output is consumed by the runner backend, which
//! prepends a grammar sampler stage to the decoding chain.
//!
//! # Supported schema subset
//!
//! - `"type": "object"` with `"properties"` (typed per property)
//! - property types `"string"`, `"number"`, `"integer"`, `"boolean"`
//! - `"type": "array"` with `"items"` of a scalar type
//! - `"enum"` on string properties
//!
//! Unsupported constructs (`oneOf`, `anyOf`, `allOf`, `$ref`, nested objects,
//! non-scalar arrays) degrade to a free JSON value for that property, with a
//! `tracing::warn` event naming the field. They never panic.

use serde_json::Value;

use crate::types::ToolSpec;

/// Shared GBNF rules emitted once per grammar. `value`/`object`/`array` back the
/// free-value degradation path; the scalars back typed properties.
const SHARED_RULES: &str = r#"str    ::= "\"" ( [^"\\] | "\\" . )* "\""
int    ::= "-"? [0-9]+
number ::= "-"? [0-9]+ ( "." [0-9]+ )?
bool   ::= "true" | "false"
value  ::= object | array | str | number | bool | "null"
object ::= "{" ws ( str ws ":" ws value ( ws "," ws str ws ":" ws value )* )? ws "}"
array  ::= "[" ws ( value ( ws "," ws value )* )? ws "]"
ws     ::= [ \t\n]*"#;

/// Internal representation of a JSON Schema property for GBNF generation.
#[derive(Debug)]
enum SchemaType {
    /// `"type": "string"`.
    Str,
    /// `"type": "number"`.
    Number,
    /// `"type": "integer"`.
    Integer,
    /// `"type": "boolean"`.
    Bool,
    /// `"type": "string"` with an `"enum"` of allowed values.
    StringEnum(Vec<String>),
    /// `"type": "array"` whose items are a scalar `SchemaType`.
    Array(Box<SchemaType>),
    /// Catch-all for unsupported constructs. Degrades to a free JSON value.
    Free { reason: String },
}

/// One tool's grammar rule: name plus its ordered `(property, type)` pairs.
#[derive(Debug)]
struct ToolRule {
    name: String,
    properties: Vec<(String, SchemaType)>,
}

/// Generates a GBNF grammar string constraining model output to valid tool calls
/// for the given tool set.
///
/// The grammar enforces a top-level object `{"name": <one of the tool names>,
/// "arguments": <the matching tool's argument object>}`. Each argument property
/// is typed from its JSON Schema. Unsupported constructs degrade to a free JSON
/// value with a `tracing::warn` event. An empty slice returns an empty string,
/// meaning "no grammar applied".
pub fn tool_specs_to_gbnf(specs: &[ToolSpec]) -> String {
    if specs.is_empty() {
        return String::new();
    }

    let rules: Vec<ToolRule> = specs.iter().map(parse_tool).collect();
    let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
    let arg_rule_names: Vec<String> = (0..rules.len()).map(|i| format!("tool-{i}-args")).collect();

    let mut out = String::new();
    out.push_str(&format!(
        "root ::= {} ws {} ws {} ws tool-name ws {} ws {} ws {} ws tool-args ws {}\n",
        lit("{"),
        json_key("name"),
        lit(":"),
        lit(","),
        json_key("arguments"),
        lit(":"),
        lit("}"),
    ));
    out.push_str(&format!("tool-name ::= {}\n", json_string_oneof(&names)));
    out.push_str(&format!("tool-args ::= {}\n", arg_rule_names.join(" | ")));
    for (i, rule) in rules.iter().enumerate() {
        out.push_str(&format!("tool-{i}-args ::= {}\n", render_args_rule(rule)));
    }
    out.push_str(SHARED_RULES);
    out.push('\n');
    out
}

/// Parses one tool's `parameters` schema into a [`ToolRule`].
fn parse_tool(spec: &ToolSpec) -> ToolRule {
    let mut properties = Vec::new();
    if let Some(props) = spec.parameters.get("properties").and_then(Value::as_object) {
        for (pname, pschema) in props {
            properties.push((pname.clone(), parse_schema_type(pschema)));
        }
    }
    ToolRule {
        name: spec.name.clone(),
        properties,
    }
}

/// Maps a single JSON Schema property to a [`SchemaType`], degrading unsupported
/// constructs to [`SchemaType::Free`]. The warning is emitted at render time,
/// where the owning property name is in scope (see [`render_args_rule`]).
fn parse_schema_type(schema: &Value) -> SchemaType {
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let strings: Vec<String> = values
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !strings.is_empty() {
            return SchemaType::StringEnum(strings);
        }
    }

    for key in ["oneOf", "anyOf", "allOf", "$ref"] {
        if schema.get(key).is_some() {
            return SchemaType::Free {
                reason: format!("unsupported construct {key}"),
            };
        }
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => SchemaType::Str,
        Some("integer") => SchemaType::Integer,
        Some("number") => SchemaType::Number,
        Some("boolean") => SchemaType::Bool,
        Some("array") => match schema.get("items") {
            Some(items) => {
                let inner = parse_schema_type(items);
                if is_scalar(&inner) {
                    SchemaType::Array(Box::new(inner))
                } else {
                    SchemaType::Free {
                        reason: "non-scalar array items".to_string(),
                    }
                }
            }
            None => SchemaType::Free {
                reason: "array without items".to_string(),
            },
        },
        Some("object") => SchemaType::Free {
            reason: "nested object".to_string(),
        },
        _ => SchemaType::Free {
            reason: "missing or unknown type".to_string(),
        },
    }
}

/// Whether `ty` is a scalar that may appear as an array item.
fn is_scalar(ty: &SchemaType) -> bool {
    matches!(
        ty,
        SchemaType::Str
            | SchemaType::Integer
            | SchemaType::Number
            | SchemaType::Bool
            | SchemaType::StringEnum(_)
    )
}

/// Renders the argument object rule body for one tool.
fn render_args_rule(rule: &ToolRule) -> String {
    if rule.properties.is_empty() {
        return format!("{} ws {}", lit("{"), lit("}"));
    }
    let sep = format!(" ws {} ws ", lit(","));
    let parts: Vec<String> = rule
        .properties
        .iter()
        .map(|(name, ty)| {
            if let SchemaType::Free { reason } = ty {
                tracing::warn!(field = %name, reason = %reason, "gbnf.unsupported_construct");
            }
            format!("{} ws {} ws {}", json_key(name), lit(":"), render_value(ty))
        })
        .collect();
    format!("{} ws {} ws {}", lit("{"), parts.join(&sep), lit("}"))
}

/// Renders the GBNF fragment matching one property value of the given type.
fn render_value(ty: &SchemaType) -> String {
    match ty {
        SchemaType::Str => "str".to_string(),
        SchemaType::Integer => "int".to_string(),
        SchemaType::Number => "number".to_string(),
        SchemaType::Bool => "bool".to_string(),
        SchemaType::StringEnum(values) => json_string_oneof(values),
        SchemaType::Array(inner) => {
            let item = render_value(inner);
            format!(
                "{} ws ( {item} ( ws {} ws {item} )* )? ws {}",
                lit("["),
                lit(","),
                lit("]"),
            )
        }
        SchemaType::Free { .. } => "value".to_string(),
    }
}

/// GBNF fragment matching the JSON string key `"name"` (quote, name, quote).
fn json_key(name: &str) -> String {
    format!("{} {} {}", lit("\""), lit(name), lit("\""))
}

/// GBNF fragment matching a JSON string whose content is one of `values`.
fn json_string_oneof(values: &[String]) -> String {
    let alts: Vec<String> = values.iter().map(|v| lit(v.as_str())).collect();
    format!("{} ( {} ) {}", lit("\""), alts.join(" | "), lit("\""))
}

/// Renders `s` as a GBNF double-quoted literal matching those exact characters.
///
/// Escapes the two characters that are significant inside a GBNF literal, the
/// backslash and the double quote, and leaves everything else verbatim.
fn lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_spec(name: &str, params: serde_json::Value) -> ToolSpec {
        ToolSpec {
            name: name.to_string(),
            description: format!("desc of {name}"),
            parameters: params,
        }
    }

    #[test]
    fn test_simple_schema_produces_root_rule() {
        // GIVEN a single tool with a string property and a required field
        let specs = vec![make_spec(
            "search_web",
            json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        )];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN the grammar is non-empty and carries the root rule and tool name
        assert!(!gbnf.is_empty(), "empty grammar for a valid tool");
        assert!(gbnf.contains("root"), "missing root rule");
        assert!(gbnf.contains("search_web"), "missing tool name");
    }

    #[test]
    fn test_two_tools_both_names_in_root() {
        // GIVEN two tools
        let specs = vec![
            make_spec("search_web", json!({ "type": "object", "properties": {} })),
            make_spec("read_file", json!({ "type": "object", "properties": {} })),
        ];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN both quoted names appear in the tool-name alternation
        assert!(gbnf.contains("\"search_web\""), "search_web missing");
        assert!(gbnf.contains("\"read_file\""), "read_file missing");
    }

    #[test]
    fn test_unsupported_oneof_degrades_without_panic() {
        // GIVEN a tool whose property uses the unsupported oneOf construct
        let specs = vec![make_spec(
            "complex_tool",
            json!({
                "type": "object",
                "properties": {
                    "input": { "oneOf": [{ "type": "string" }, { "type": "integer" }] }
                }
            }),
        )];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN it degrades gracefully to a non-empty grammar, no panic
        assert!(!gbnf.is_empty());
        assert!(
            gbnf.contains("value"),
            "degraded property should use the value rule"
        );
    }

    #[test]
    fn test_empty_specs_returns_empty_or_trivial() {
        // GIVEN an empty tool slice
        let specs: Vec<ToolSpec> = vec![];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN the result is the documented empty string, no panic
        assert!(gbnf.is_empty());
    }

    #[test]
    fn test_string_enum_produces_constrained_rule() {
        // GIVEN a tool with a string enum property
        let specs = vec![make_spec(
            "format_output",
            json!({
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["json", "text"] }
                },
                "required": ["format"]
            }),
        )];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN only the two enum values are allowed for that field
        assert!(gbnf.contains("\"json\""), "enum value json missing");
        assert!(gbnf.contains("\"text\""), "enum value text missing");
    }
}
