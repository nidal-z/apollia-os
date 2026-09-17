//! GBNF grammar generation from JSON schemas.
//!
//! Two entry points, and they differ in what they do with a construct they
//! cannot express.
//!
//! [`tool_specs_to_gbnf`] constrains a local model to emit a syntactically
//! valid tool call: a top-level object with a `"name"` in the allowed tool set
//! and an `"arguments"` object following that tool's declared schema. It
//! degrades an unsupported construct to a free JSON value with a
//! `tracing::warn` naming the field, because a tool call that loses one
//! argument's shape is still a usable tool call.
//!
//! [`json_schema_to_gbnf`] constrains the answer itself, for
//! `ctx.llm(schema=...)`. A response schema has no such slack, so it answers
//! [`GrammarError`] rather than a grammar that does not say what the caller
//! asked. Its supported subset is wider (nested objects, arrays of objects) and
//! its refusals are explicit.
//!
//! # Subset shared by both
//!
//! - `"type": "object"` with `"properties"` (typed per property)
//! - property types `"string"`, `"number"`, `"integer"`, `"boolean"`
//! - `"type": "array"` with `"items"`
//! - `"enum"`
//!
//! Neither ever panics.

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

// ─── A bare JSON Schema, for `ctx.llm(schema=...)` ───────────────────────────

/// Deepest schema nesting the translator will walk.
///
/// A JSON Schema carries no cycles once `$ref` is refused, so the walk always
/// terminates; the cap exists so a pathological schema answers an error instead
/// of exhausting the stack.
const MAX_SCHEMA_DEPTH: usize = 24;

/// Why a JSON Schema could not be turned into a GBNF grammar.
///
/// Every variant names the JSON path of the offending node, so the caller can
/// point at the exact place in the schema it passed rather than at the schema
/// as a whole.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum GrammarError {
    /// A construct the translator cannot express as a decoding constraint.
    #[error("{path}: {construct} cannot be expressed as a grammar")]
    Unsupported {
        /// JSON path of the node, e.g. `$.properties.items.items`.
        path: String,
        /// What was found there.
        construct: String,
    },

    /// A node declares neither `type` nor `enum`, so nothing constrains it.
    #[error("{path}: neither a `type` nor an `enum`, so nothing constrains this node")]
    Untyped {
        /// JSON path of the node.
        path: String,
    },

    /// `required` names a property that `properties` does not declare.
    #[error("{path}: `required` names `{property}`, which `properties` does not declare")]
    RequiredUnknown {
        /// JSON path of the owning object node.
        path: String,
        /// The name that has no declaration.
        property: String,
    },

    /// The schema nests deeper than [`MAX_SCHEMA_DEPTH`].
    #[error("{path}: schema nests deeper than {max} levels")]
    TooDeep {
        /// JSON path reached when the cap was hit.
        path: String,
        /// The cap.
        max: usize,
    },
}

/// Translate a JSON Schema into a GBNF grammar constraining a model to emit one
/// value of that schema, and nothing else.
///
/// This is the `ctx.llm(schema=...)` counterpart of [`tool_specs_to_gbnf`]. The
/// difference is the contract: the tool path degrades an unsupported construct
/// to a free JSON value, because a tool call that loses one argument's shape is
/// still a usable tool call. A response schema has no such slack, so an
/// untranslatable construct answers [`GrammarError`] and the caller decides.
///
/// # Supported subset
///
/// `object` with `properties` (nested freely), `array` with `items` (nested
/// freely), `string`, `number`, `integer`, `boolean`, `null`, and `enum` on any
/// of them. `required` is honoured. `oneOf`, `anyOf`, `allOf`, `not` and `$ref`
/// are refused by name.
///
/// # The property the grammar holds
///
/// Everything the grammar admits, the schema admits. The reverse is not
/// promised, and two places make it narrower on purpose:
///
/// - Properties are emitted in declaration order. JSON object members are
///   unordered, so a valid document with them shuffled is refused by the
///   grammar while the schema accepts it.
/// - An object that declares properties but requires none emits its first
///   declared property as mandatory, because a grammar alternative that can
///   match the empty string in the middle of a member list is ambiguous. The
///   emitted member is declared, so the result stays valid.
///
/// Narrower is the safe direction: the validation pass that follows reads the
/// schema, not the grammar, so a shape the grammar cannot produce is a shape
/// that never has to be rejected.
///
/// # Errors
///
/// [`GrammarError`], naming the JSON path of the node that could not be
/// translated.
pub fn json_schema_to_gbnf(schema: &Value) -> Result<String, GrammarError> {
    refuse_combinators(schema, "$")?;
    let normalized = crate::schema_sanitize::grammar_safe_schema(schema);
    let root = render_node(&normalized, "$", 0)?;
    Ok(format!("root ::= {root}\n{SHARED_RULES}\n"))
}

/// Walk the schema as the caller wrote it and refuse the combinators by name.
///
/// Runs before normalization because the sanitizer collapses a combinator node
/// to its description alone: after it, the node reads as untyped and the error
/// would name a symptom instead of `anyOf`.
fn refuse_combinators(node: &Value, path: &str) -> Result<(), GrammarError> {
    match node {
        Value::Object(map) => {
            for key in ["oneOf", "anyOf", "allOf", "not", "$ref"] {
                if map.contains_key(key) {
                    return Err(GrammarError::Unsupported {
                        path: path.to_string(),
                        construct: key.to_string(),
                    });
                }
            }
            for (key, child) in map {
                // `enum`, `const`, `default` and `examples` hold data, not
                // sub-schemas: a member named `$ref` inside an `enum` is a
                // string the model may emit, not a reference to resolve.
                if matches!(key.as_str(), "enum" | "const" | "default" | "examples") {
                    continue;
                }
                refuse_combinators(child, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                refuse_combinators(child, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Render one schema node as a GBNF fragment.
fn render_node(node: &Value, path: &str, depth: usize) -> Result<String, GrammarError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(GrammarError::TooDeep {
            path: path.to_string(),
            max: MAX_SCHEMA_DEPTH,
        });
    }
    let map = node.as_object().ok_or_else(|| GrammarError::Untyped {
        path: path.to_string(),
    })?;

    if let Some(values) = map.get("enum").and_then(Value::as_array) {
        if values.is_empty() {
            return Err(GrammarError::Unsupported {
                path: path.to_string(),
                construct: "an empty `enum`".to_string(),
            });
        }
        let alts: Vec<String> = values.iter().map(json_literal).collect();
        return Ok(format!("( {} )", alts.join(" | ")));
    }

    match map.get("type").and_then(Value::as_str) {
        Some("string") => Ok("str".to_string()),
        Some("integer") => Ok("int".to_string()),
        Some("number") => Ok("number".to_string()),
        Some("boolean") => Ok("bool".to_string()),
        Some("null") => Ok(lit("null")),
        Some("array") => {
            let items = map.get("items").ok_or_else(|| GrammarError::Unsupported {
                path: path.to_string(),
                construct: "an `array` with no `items`".to_string(),
            })?;
            let item = render_node(items, &format!("{path}.items"), depth + 1)?;
            Ok(format!(
                "{} ws ( {item} ( ws {} ws {item} )* )? ws {}",
                lit("["),
                lit(","),
                lit("]"),
            ))
        }
        Some("object") => render_object(map, path, depth),
        Some(other) => Err(GrammarError::Unsupported {
            path: path.to_string(),
            construct: format!("the type `{other}`"),
        }),
        None => Err(GrammarError::Untyped {
            path: path.to_string(),
        }),
    }
}

/// Render an `object` node: its members in declaration order, the required ones
/// mandatory and the rest optional.
fn render_object(
    map: &serde_json::Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<String, GrammarError> {
    let props = match map.get("properties").and_then(Value::as_object) {
        Some(props) if !props.is_empty() => props,
        // An object with no declared property constrains nothing beyond being an
        // object, which the shared `object` rule already says.
        _ => return Ok("object".to_string()),
    };

    let required: Vec<&str> = map
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for name in &required {
        if !props.contains_key(*name) {
            return Err(GrammarError::RequiredUnknown {
                path: path.to_string(),
                property: (*name).to_string(),
            });
        }
    }

    // Declaration order is what the grammar emits; `required` only decides which
    // members are mandatory. When nothing is required, the first declared member
    // is promoted (see the function doc of `json_schema_to_gbnf`).
    let mut members: Vec<(String, bool)> = props
        .keys()
        .map(|name| (name.clone(), required.contains(&name.as_str())))
        .collect();
    if !members.iter().any(|(_, is_required)| *is_required) {
        if let Some(first) = members.first_mut() {
            first.1 = true;
        }
    }
    // Mandatory members first, so every optional one can carry its own leading
    // comma and still be droppable independently.
    members.sort_by_key(|(_, is_required)| !*is_required);

    let mut rendered = Vec::with_capacity(members.len());
    for (name, is_required) in &members {
        let schema = props.get(name).unwrap_or(&Value::Null);
        let value = render_node(schema, &format!("{path}.properties.{name}"), depth + 1)?;
        let member = format!("{} ws {} ws {value}", json_key(name), lit(":"));
        rendered.push((member, *is_required));
    }

    let mut out = format!("{} ws", lit("{"));
    let mut first_emitted = false;
    for (member, is_required) in rendered {
        if !first_emitted {
            out.push_str(&format!(" {member}"));
            first_emitted = true;
        } else if is_required {
            out.push_str(&format!(" ws {} ws {member}", lit(",")));
        } else {
            out.push_str(&format!(" ( ws {} ws {member} )?", lit(",")));
        }
    }
    out.push_str(&format!(" ws {}", lit("}")));
    Ok(out)
}

/// Render a JSON value as the GBNF literal matching its serialized form.
///
/// Used for `enum` members, which may be of any JSON type: a string enum needs
/// its quotes, a number enum must not gain any.
fn json_literal(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{} {} {}", lit("\""), lit(s), lit("\"")),
        other => lit(&other.to_string()),
    }
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

#[cfg(test)]
mod schema_grammar_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_flat_object_constrains_each_property() {
        // GIVEN a flat object schema with a string and an integer, both required
        let schema = json!({
            "type": "object",
            "properties": {"title": {"type": "string"}, "count": {"type": "integer"}},
            "required": ["title", "count"]
        });
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("a flat object is translatable");
        // THEN the root rule names both keys and binds each to its own scalar rule
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        assert!(root.contains("\"title\""), "title key absent: {root}");
        assert!(root.contains("\"count\""), "count key absent: {root}");
        assert!(root.contains("str"), "the string property is not typed");
        assert!(root.contains("int"), "the integer property is not typed");
    }

    #[test]
    fn test_array_of_objects_is_translated_not_degraded() {
        // GIVEN the shape the tool path degrades: an array whose items are objects
        let schema = json!({
            "type": "object",
            "properties": {
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"label": {"type": "string"}},
                        "required": ["label"]
                    }
                }
            },
            "required": ["rows"]
        });
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("an array of objects is translatable");
        // THEN the item's own key is in the grammar, so the items are typed
        // rather than collapsed to the free `value` rule
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        assert!(root.contains("\"rows\""), "rows key absent");
        assert!(root.contains("\"label\""), "the nested object was degraded");
    }

    #[test]
    fn test_string_enumeration_admits_only_its_members() {
        // GIVEN a top-level enumeration of strings
        let schema = json!({"type": "string", "enum": ["low", "medium", "high"]});
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("an enumeration is translatable");
        // THEN the three members are the alternatives, and no free string rule
        // is used at the root
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        for member in ["low", "medium", "high"] {
            assert!(root.contains(member), "enum member `{member}` absent");
        }
        assert!(
            !root.split_whitespace().any(|token| token == "str"),
            "an enumeration must not fall back to the free string rule: {root}"
        );
    }

    #[test]
    fn test_combinator_is_refused_by_name_with_its_path() {
        // GIVEN a schema whose nested property uses `anyOf`
        let schema = json!({
            "type": "object",
            "properties": {"x": {"anyOf": [{"type": "string"}, {"type": "integer"}]}},
            "required": ["x"]
        });
        // WHEN translating it
        let err = json_schema_to_gbnf(&schema).expect_err("anyOf must be refused");
        // THEN the error is typed, names the construct and points at the node
        assert_eq!(
            err,
            GrammarError::Unsupported {
                path: "$.properties.x".to_string(),
                construct: "anyOf".to_string(),
            }
        );
    }

    #[test]
    fn test_ref_is_refused_rather_than_silently_freed() {
        // GIVEN a schema using `$ref`, which the sanitizer collapses to a free value
        let schema = json!({
            "type": "object",
            "properties": {"y": {"$ref": "#/$defs/Y"}},
            "$defs": {"Y": {"type": "string"}}
        });
        // WHEN translating it
        let err = json_schema_to_gbnf(&schema).expect_err("a $ref must be refused");
        // THEN it is named rather than turned into an unconstrained property
        assert!(
            matches!(&err, GrammarError::Unsupported { construct, .. } if construct == "$ref"),
            "expected a $ref refusal, got {err:?}"
        );
    }

    #[test]
    fn test_untyped_node_is_refused() {
        // GIVEN a property with neither a type nor an enumeration
        let schema = json!({
            "type": "object",
            "properties": {"free": {"description": "anything"}},
            "required": ["free"]
        });
        // WHEN translating it
        let err = json_schema_to_gbnf(&schema).expect_err("an untyped node must be refused");
        // THEN the path names the property
        assert_eq!(
            err,
            GrammarError::Untyped {
                path: "$.properties.free".to_string()
            }
        );
    }

    #[test]
    fn test_required_naming_an_undeclared_property_is_refused() {
        // GIVEN a schema requiring a property it never declares
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a", "b"]
        });
        // WHEN translating it
        let err = json_schema_to_gbnf(&schema).expect_err("the dangling name must be refused");
        // THEN the offending name is in the error
        assert_eq!(
            err,
            GrammarError::RequiredUnknown {
                path: "$".to_string(),
                property: "b".to_string()
            }
        );
    }

    #[test]
    fn test_optional_members_stay_droppable() {
        // GIVEN one required property and one optional one
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "integer"}},
            "required": ["a"]
        });
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("translatable");
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        // THEN the optional member carries its own comma inside an optional
        // group, so an answer may omit it
        assert!(
            root.contains("( ws \",\" ws \"\\\"\" \"b\""),
            "the optional member is not droppable: {root}"
        );
    }

    #[test]
    fn test_a_schema_requiring_nothing_still_emits_one_member() {
        // GIVEN an object that declares two properties and requires neither
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "integer"}}
        });
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("translatable");
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        // THEN the first declared member is mandatory, which keeps the rule
        // unambiguous while staying valid against the schema
        assert!(root.contains("\"a\""), "the promoted member is absent");
        assert!(
            !root.contains("( ws \",\" ws \"\\\"\" \"a\""),
            "the promoted member must not stay optional: {root}"
        );
    }

    #[test]
    fn test_enum_of_numbers_keeps_them_unquoted() {
        // GIVEN an enumeration of integers
        let schema = json!({"type": "integer", "enum": [1, 2, 3]});
        // WHEN translating it
        let gbnf = json_schema_to_gbnf(&schema).expect("translatable");
        let root = gbnf
            .lines()
            .find(|l| l.starts_with("root ::="))
            .expect("a root rule");
        // THEN the members are bare literals, not quoted strings
        assert!(root.contains("\"1\""), "the literal 1 is absent: {root}");
        assert!(
            !root.contains("\"\\\"\" \"1\""),
            "a numeric member must not be quoted as a string: {root}"
        );
    }
}
