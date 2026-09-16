//! Check a model's answer against the JSON Schema that was asked for.
//!
//! A decoding grammar shapes generation, it does not verify it. Two gaps make
//! that distinction load-bearing rather than theoretical:
//!
//! - The grammar the local path builds is deliberately narrower than the schema
//!   (see [`crate::grammar::json_schema_to_gbnf`]), and it expresses no
//!   numeric bound, no string length and no item count. A schema whose
//!   constraints contradict each other therefore produces a grammatical answer
//!   that the schema still refuses, and only this pass says so.
//! - The remote path has no grammar at all. `response_format` is a request, and
//!   what comes back is whatever the provider decided to send.
//!
//! The subset checked here is the one [`crate::grammar::json_schema_to_gbnf`]
//! accepts, plus the validation-only keywords the grammar cannot carry. The
//! first violation wins and carries its JSON path, because an agent acting on a
//! malformed answer needs to know where, not how many.

use serde_json::Value;

/// The first place where a value departs from its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaViolation {
    /// JSON path of the offending node, e.g. `$.rows[2].label`.
    pub path: String,
    /// What was expected there, and what was found.
    pub reason: String,
}

impl std::fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

/// Validate `value` against `schema`, returning the first violation found.
///
/// Depth-first in declaration order, so the path reported is the earliest one a
/// reader would reach scanning the document.
pub fn validate_against_schema(value: &Value, schema: &Value) -> Result<(), SchemaViolation> {
    check(value, schema, "$")
}

fn violation(path: &str, reason: impl Into<String>) -> SchemaViolation {
    SchemaViolation {
        path: path.to_string(),
        reason: reason.into(),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn check(value: &Value, schema: &Value, path: &str) -> Result<(), SchemaViolation> {
    let Some(map) = schema.as_object() else {
        // A schema that is not an object constrains nothing this validator can
        // read; the grammar side refuses such a node, so nothing reaches here.
        return Ok(());
    };

    if let Some(expected) = map.get("const") {
        if value != expected {
            return Err(violation(path, format!("expected the constant {expected}")));
        }
    }

    if let Some(allowed) = map.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            let rendered: Vec<String> = allowed.iter().map(Value::to_string).collect();
            return Err(violation(
                path,
                format!("expected one of [{}], found {value}", rendered.join(", ")),
            ));
        }
    }

    match map.get("type").and_then(Value::as_str) {
        Some("object") => check_object(value, map, path)?,
        Some("array") => check_array(value, map, path)?,
        Some("string") => check_string(value, map, path)?,
        Some("integer") => check_integer(value, map, path)?,
        Some("number") => check_number(value, map, path)?,
        Some("boolean") if !value.is_boolean() => {
            return Err(violation(
                path,
                format!("expected a boolean, found {}", type_name(value)),
            ))
        }
        Some("null") if !value.is_null() => {
            return Err(violation(
                path,
                format!("expected null, found {}", type_name(value)),
            ))
        }
        // No `type`: an `enum` or `const` alone has already been checked above.
        _ => {}
    }
    Ok(())
}

fn check_object(
    value: &Value,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    let object = value.as_object().ok_or_else(|| {
        violation(
            path,
            format!("expected an object, found {}", type_name(value)),
        )
    })?;

    let empty = serde_json::Map::new();
    let props = map
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);

    if let Some(required) = map.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(violation(
                    &format!("{path}.{name}"),
                    "required property is missing",
                ));
            }
        }
    }

    if map.get("additionalProperties") == Some(&Value::Bool(false)) {
        for name in object.keys() {
            if !props.contains_key(name) {
                return Err(violation(
                    &format!("{path}.{name}"),
                    "property is not declared and `additionalProperties` is false",
                ));
            }
        }
    }

    // Declaration order, not document order: two answers with their members
    // shuffled then report the same path for the same defect.
    for (name, sub_schema) in props {
        if let Some(child) = object.get(name) {
            check(child, sub_schema, &format!("{path}.{name}"))?;
        }
    }
    Ok(())
}

fn check_array(
    value: &Value,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    let items = value.as_array().ok_or_else(|| {
        violation(
            path,
            format!("expected an array, found {}", type_name(value)),
        )
    })?;

    if let Some(min) = map.get("minItems").and_then(Value::as_u64) {
        if (items.len() as u64) < min {
            return Err(violation(
                path,
                format!("expected at least {min} item(s), found {}", items.len()),
            ));
        }
    }
    if let Some(max) = map.get("maxItems").and_then(Value::as_u64) {
        if (items.len() as u64) > max {
            return Err(violation(
                path,
                format!("expected at most {max} item(s), found {}", items.len()),
            ));
        }
    }

    if let Some(item_schema) = map.get("items") {
        for (index, item) in items.iter().enumerate() {
            check(item, item_schema, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

fn check_string(
    value: &Value,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    let text = value.as_str().ok_or_else(|| {
        violation(
            path,
            format!("expected a string, found {}", type_name(value)),
        )
    })?;
    // Characters, not bytes: the bound a schema author writes is about the text.
    let length = text.chars().count() as u64;
    if let Some(min) = map.get("minLength").and_then(Value::as_u64) {
        if length < min {
            return Err(violation(
                path,
                format!("expected at least {min} character(s), found {length}"),
            ));
        }
    }
    if let Some(max) = map.get("maxLength").and_then(Value::as_u64) {
        if length > max {
            return Err(violation(
                path,
                format!("expected at most {max} character(s), found {length}"),
            ));
        }
    }
    Ok(())
}

fn check_integer(
    value: &Value,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    let number = value.as_f64().filter(|n| n.fract() == 0.0).ok_or_else(|| {
        violation(
            path,
            format!("expected an integer, found {}", type_name(value)),
        )
    })?;
    check_bounds(number, map, path)
}

fn check_number(
    value: &Value,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    let number = value.as_f64().ok_or_else(|| {
        violation(
            path,
            format!("expected a number, found {}", type_name(value)),
        )
    })?;
    check_bounds(number, map, path)
}

fn check_bounds(
    number: f64,
    map: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaViolation> {
    if let Some(min) = map.get("minimum").and_then(Value::as_f64) {
        if number < min {
            return Err(violation(
                path,
                format!("expected at least {min}, found {number}"),
            ));
        }
    }
    if let Some(max) = map.get("maximum").and_then(Value::as_f64) {
        if number > max {
            return Err(violation(
                path,
                format!("expected at most {max}, found {number}"),
            ));
        }
    }
    Ok(())
}

/// Read a model's answer as JSON and check it against `schema`.
///
/// The single entry point a caller of a structured-output call needs: it turns
/// the raw completion text into the validated value, or into the typed error
/// that says where the answer went wrong.
///
/// Leading and trailing whitespace is tolerated, and so is a fenced code block:
/// a grammar-constrained answer carries neither, but a `response_format` answer
/// from a remote provider sometimes does, and refusing a well-formed value over
/// three backticks would be a defect of this layer rather than of the model.
///
/// # Errors
///
/// - [`LlmError::StructuredOutputInvalid`] with the path `$` when the answer is
///   not JSON at all.
/// - [`LlmError::StructuredOutputInvalid`] with the path of the first violation
///   when it is JSON that the schema refuses.
pub fn parse_and_validate(content: &str, schema: &Value) -> Result<Value, crate::LlmError> {
    let text = strip_code_fence(content.trim());
    let value: Value =
        serde_json::from_str(text).map_err(|e| crate::LlmError::StructuredOutputInvalid {
            path: "$".to_string(),
            reason: format!("the answer is not JSON: {e}"),
        })?;
    validate_against_schema(&value, schema)?;
    Ok(value)
}

/// Return the body of a fenced code block, or the text unchanged.
fn strip_code_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    // The opening fence may carry a language tag (```json), which ends at the
    // first newline.
    let body = match rest.split_once('\n') {
        Some((_tag, body)) => body,
        None => return text,
    };
    body.trim_end().strip_suffix("```").unwrap_or(text).trim()
}

/// Length of a schema fingerprint, in hexadecimal characters.
///
/// Sixteen characters is 64 bits of SHA-256. The fingerprint identifies a
/// schema inside one machine's audit trail; it guards against nothing, so the
/// full digest would only make the journal wider.
const FINGERPRINT_HEX_LEN: usize = 16;

/// A short, stable identifier for a schema, for the audit trail.
///
/// Two calls constrained by the same schema answer the same fingerprint, which
/// is what lets a reader of the journal tell one constrained run from another
/// without the journal ever holding the schema itself. `serde_json` orders an
/// object's keys, so the same schema written with its members shuffled
/// fingerprints identically.
pub fn schema_fingerprint(schema: &Value) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(schema.to_string().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex.chars().take(FINGERPRINT_HEX_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_a_flat_object_that_matches() {
        // GIVEN a flat object schema and an answer that satisfies it
        let schema = json!({
            "type": "object",
            "properties": {"title": {"type": "string"}, "count": {"type": "integer"}},
            "required": ["title", "count"]
        });
        let value = json!({"title": "a report", "count": 3});
        // WHEN validating
        let result = validate_against_schema(&value, &schema);
        // THEN it passes
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn names_the_path_of_a_missing_required_property() {
        // GIVEN a schema requiring two properties and an answer carrying one
        let schema = json!({
            "type": "object",
            "properties": {"title": {"type": "string"}, "count": {"type": "integer"}},
            "required": ["title", "count"]
        });
        let value = json!({"title": "a report"});
        // WHEN validating
        let err = validate_against_schema(&value, &schema).expect_err("must refuse");
        // THEN the path points at the missing property, not at the document
        assert_eq!(err.path, "$.count");
        assert!(err.reason.contains("required"), "reason: {}", err.reason);
    }

    #[test]
    fn names_the_path_inside_an_array_of_objects() {
        // GIVEN an object holding an array of objects
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
        // WHEN the second row carries a number where a string was declared
        let value = json!({"rows": [{"label": "ok"}, {"label": 7}]});
        let err = validate_against_schema(&value, &schema).expect_err("must refuse");
        // THEN the path indexes the offending row
        assert_eq!(err.path, "$.rows[1].label");
    }

    #[test]
    fn refuses_a_value_outside_a_string_enumeration() {
        // GIVEN an enumeration of three strings
        let schema = json!({"type": "string", "enum": ["low", "medium", "high"]});
        // WHEN the answer is none of them
        let err = validate_against_schema(&json!("urgent"), &schema).expect_err("must refuse");
        // THEN the violation sits at the root and names the allowed set
        assert_eq!(err.path, "$");
        assert!(err.reason.contains("medium"), "reason: {}", err.reason);
    }

    #[test]
    fn catches_what_a_grammar_cannot_express() {
        // GIVEN a schema whose constraints contradict each other: the only
        // enumerated value is shorter than the minimum length demanded. No
        // decoding grammar can express that, so generation succeeds and the
        // answer is still wrong.
        let schema = json!({"type": "string", "enum": ["a"], "minLength": 5});
        // WHEN the model emits the only value the grammar allows
        let err = validate_against_schema(&json!("a"), &schema).expect_err("must refuse");
        // THEN validation is what catches it
        assert_eq!(err.path, "$");
        assert!(err.reason.contains("at least 5"), "reason: {}", err.reason);
    }

    #[test]
    fn refuses_an_undeclared_property_when_additional_are_closed() {
        // GIVEN a closed object schema
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a"],
            "additionalProperties": false
        });
        // WHEN the answer carries one more member
        let err =
            validate_against_schema(&json!({"a": "x", "b": 1}), &schema).expect_err("must refuse");
        // THEN the extra member is named
        assert_eq!(err.path, "$.b");
    }

    #[test]
    fn refuses_a_fractional_value_where_an_integer_was_declared() {
        // GIVEN an integer property
        let schema = json!({"type": "object", "properties": {"n": {"type": "integer"}}});
        // WHEN the answer carries a fraction
        let err = validate_against_schema(&json!({"n": 1.5}), &schema).expect_err("must refuse");
        // THEN it is refused at that property
        assert_eq!(err.path, "$.n");
    }

    #[test]
    fn a_numeric_bound_is_enforced() {
        // GIVEN a bounded integer
        let schema = json!({"type": "integer", "minimum": 10, "maximum": 20});
        // WHEN the answer sits below the floor
        let err = validate_against_schema(&json!(4), &schema).expect_err("must refuse");
        // THEN the bound is named
        assert!(err.reason.contains("at least 10"), "reason: {}", err.reason);
        // AND a value inside the range passes
        assert_eq!(validate_against_schema(&json!(15), &schema), Ok(()));
    }

    #[test]
    fn an_optional_property_absent_is_not_a_violation() {
        // GIVEN a schema declaring two properties and requiring one
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "integer"}},
            "required": ["a"]
        });
        // WHEN the optional one is absent
        // THEN the answer is accepted
        assert_eq!(validate_against_schema(&json!({"a": "x"}), &schema), Ok(()));
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_plain_json_answer() {
        // GIVEN a schema and a grammar-constrained answer, which carries no
        // decoration
        let schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        // WHEN it is parsed and validated
        let value = parse_and_validate("{\"a\": \"x\"}", &schema).expect("a valid answer");
        // THEN the caller gets the value itself, not the text
        assert_eq!(value, json!({"a": "x"}));
    }

    #[test]
    fn reads_an_answer_wrapped_in_a_code_fence() {
        // GIVEN the shape a remote provider sometimes returns under
        // `response_format`
        let schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        // WHEN it is parsed and validated
        let value = parse_and_validate("```json\n{\"a\": \"x\"}\n```", &schema)
            .expect("a fenced answer is still an answer");
        // THEN the fence is not mistaken for a defect of the model
        assert_eq!(value, json!({"a": "x"}));
    }

    #[test]
    fn a_non_json_answer_is_a_typed_error_at_the_root() {
        // GIVEN a schema and an answer that is prose
        let schema = json!({"type": "object"});
        // WHEN it is parsed
        let err = parse_and_validate("I am afraid I cannot do that", &schema)
            .expect_err("prose is not an object");
        // THEN the error is typed and points at the document itself
        assert!(
            matches!(&err, crate::LlmError::StructuredOutputInvalid { path, .. } if path == "$"),
            "expected a root violation, got {err:?}"
        );
    }

    #[test]
    fn a_violation_carries_the_path_rather_than_a_sentence_to_parse() {
        // GIVEN a schema requiring a property the answer omits
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "integer"}},
            "required": ["a", "b"]
        });
        // WHEN the answer is parsed
        let err = parse_and_validate("{\"a\": \"x\"}", &schema).expect_err("b is missing");
        // THEN the path is a field of the error, so a caller branches on it
        match err {
            crate::LlmError::StructuredOutputInvalid { path, .. } => assert_eq!(path, "$.b"),
            other => panic!("expected StructuredOutputInvalid, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_same_schema_fingerprints_the_same_way_whatever_the_key_order() {
        // GIVEN one schema written twice with its members in different orders
        let a = json!({"type": "object", "properties": {"b": {"type": "integer"}, "a": {"type": "string"}}});
        let b = json!({"properties": {"a": {"type": "string"}, "b": {"type": "integer"}}, "type": "object"});
        // WHEN both are fingerprinted
        // THEN the journal sees one schema, not two
        assert_eq!(schema_fingerprint(&a), schema_fingerprint(&b));
    }

    #[test]
    fn a_different_schema_fingerprints_differently() {
        // GIVEN two schemas that differ by one property type
        let a = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let b = json!({"type": "object", "properties": {"a": {"type": "integer"}}});
        // WHEN both are fingerprinted
        // THEN they are told apart
        assert_ne!(schema_fingerprint(&a), schema_fingerprint(&b));
    }

    #[test]
    fn the_fingerprint_never_carries_the_schema() {
        // GIVEN a schema holding a property name a reader could recognise
        let schema = json!({"type": "object", "properties": {"salary": {"type": "integer"}}});
        // WHEN it is fingerprinted
        let fingerprint = schema_fingerprint(&schema);
        // THEN the result is a short hexadecimal digest and nothing of the input
        assert_eq!(fingerprint.len(), FINGERPRINT_HEX_LEN);
        assert!(!fingerprint.contains("salary"));
        assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
