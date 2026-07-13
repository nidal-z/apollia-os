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
/// The grammar enforces a top-level object `{"name": <a tool name>,
/// "arguments": <that same tool's argument object>}`. Each tool gets its own
/// `root` alternative binding its name to its own argument rule, so a model
/// cannot pair one tool's name with another tool's arguments. Each argument
/// property is typed from its JSON Schema. Unsupported constructs degrade to a
/// free JSON value with a `tracing::warn` event. An empty slice returns an
/// empty string, meaning "no grammar applied".
pub fn tool_specs_to_gbnf(specs: &[ToolSpec]) -> String {
    if specs.is_empty() {
        return String::new();
    }

    let rules: Vec<ToolRule> = specs.iter().map(parse_tool).collect();
    let tool_rule_names: Vec<String> = (0..rules.len()).map(|i| format!("tool-{i}")).collect();

    let mut out = String::new();
    out.push_str(&format!("root ::= {}\n", tool_rule_names.join(" | ")));
    for (i, rule) in rules.iter().enumerate() {
        out.push_str(&format!(
            "tool-{i} ::= {} ws {} ws {} ws {} ws {} ws {} ws {} ws tool-{i}-args ws {}\n",
            lit("{"),
            json_key("name"),
            lit(":"),
            json_key(&rule.name),
            lit(","),
            json_key("arguments"),
            lit(":"),
            lit("}"),
        ));
    }
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
/// Escapes every character that is significant inside a GBNF literal: the
/// backslash, the double quote, and the whitespace/control characters that
/// would otherwise be emitted verbatim. A raw newline is the important case,
/// since it terminates a GBNF rule and would corrupt the whole grammar when a
/// tool name or enum value happens to contain one. `\n`/`\r`/`\t` use their
/// named escapes; any other C0 control or DEL uses a `\xNN` hex escape.
fn lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02X}", c as u32));
            }
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
    fn test_two_tools_both_names_present() {
        // GIVEN two tools
        let specs = vec![
            make_spec("search_web", json!({ "type": "object", "properties": {} })),
            make_spec("read_file", json!({ "type": "object", "properties": {} })),
        ];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN both quoted names appear, each in its own per-tool alternative
        assert!(gbnf.contains("\"search_web\""), "search_web missing");
        assert!(gbnf.contains("\"read_file\""), "read_file missing");
        assert!(
            gbnf.contains("root ::= tool-0 | tool-1"),
            "root must alternate per-tool rules"
        );
    }

    #[test]
    fn test_name_bound_to_own_args() {
        // GIVEN two tools with DISTINCT argument shapes
        let specs = vec![
            make_spec(
                "alpha",
                json!({ "type": "object", "properties": { "a": { "type": "string" } } }),
            ),
            make_spec(
                "beta",
                json!({ "type": "object", "properties": { "b": { "type": "integer" } } }),
            ),
        ];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN the old independent name/args alternations are gone
        assert!(
            !gbnf.contains("tool-name ::="),
            "independent tool-name rule must be gone"
        );
        assert!(
            !gbnf.contains("tool-args ::="),
            "independent tool-args rule must be gone"
        );
        // AND each tool alternative fixes its own name and references only its own args
        let tool0 = gbnf
            .lines()
            .find(|l| l.starts_with("tool-0 ::="))
            .expect("tool-0 rule present");
        assert!(
            tool0.contains("\"alpha\""),
            "tool-0 must fix the alpha name"
        );
        assert!(
            tool0.contains("tool-0-args"),
            "tool-0 must reference its own args rule"
        );
        assert!(
            !tool0.contains("tool-1-args"),
            "tool-0 must NOT be able to use beta's args"
        );
    }

    #[test]
    fn test_lit_escapes_control_characters() {
        // GIVEN a string carrying a quote, backslash, newline, tab, CR and a raw control char
        // WHEN rendering it as a GBNF literal
        let out = lit("a\"b\\c\nd\te\rf\u{07}");
        // THEN no raw control byte leaks into the literal (a raw newline would break the rule)
        assert!(!out.contains('\n'), "raw newline must not appear");
        assert!(!out.contains('\t'), "raw tab must not appear");
        assert!(!out.contains('\r'), "raw CR must not appear");
        // AND each significant character is represented by its GBNF escape
        assert!(out.contains("\\\""), "quote escape missing");
        assert!(out.contains("\\\\"), "backslash escape missing");
        assert!(out.contains("\\n"), "newline escape missing");
        assert!(out.contains("\\t"), "tab escape missing");
        assert!(out.contains("\\r"), "CR escape missing");
        assert!(out.contains("\\x07"), "control char hex escape missing");
    }

    #[test]
    fn test_tool_name_with_newline_stays_single_line() {
        // GIVEN a (pathological) tool name containing a newline
        let specs = vec![make_spec(
            "bad\nname",
            json!({ "type": "object", "properties": {} }),
        )];
        // WHEN generating the grammar
        let gbnf = tool_specs_to_gbnf(&specs);
        // THEN every grammar rule line is intact: no rule body is split by a raw newline
        for line in gbnf.lines() {
            if line.starts_with("tool-0 ::=") {
                assert!(
                    line.contains("bad\\nname"),
                    "the newline in the tool name must be escaped inside the rule"
                );
            }
        }
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
