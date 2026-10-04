//! Checking the inputs of a run against the schema its workflow declares.
//!
//! The schema is the one the SDK infers from the workflow's `TypedDict`: an
//! object with `properties`, `required` and `additionalProperties: false`,
//! each property typed (`string`, `integer`, `number`, `boolean`, `array`,
//! `object`), possibly `nullable`, possibly an `enum`, an array with its
//! `items`. A value given as text for a field of another type is read as that
//! type when the text parses as one: a form and a command line hand every
//! value over as text.
//!
//! A text field may carry a `format`. A `date` is a day written `YYYY-MM-DD`
//! that exists in the calendar; a `file-path` is the absolute path of a file
//! that exists and can be opened. Both are checked here, before the run, so a
//! workflow never starts on a day that is not one or a file that is not there.

use std::path::Path;

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Map, Value};

/// JSON Schema `format` of a date input.
pub(crate) const DATE_FORMAT: &str = "date";
/// JSON Schema `format` of a long text input.
pub(crate) const LONG_TEXT_FORMAT: &str = "long-text";
/// JSON Schema `format` of a file input.
pub(crate) const FILE_PATH_FORMAT: &str = "file-path";

/// Why an input was refused, as a caller matches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputErrorCode {
    /// The inputs are not an object.
    NotAnObject,
    /// A required input is absent.
    Required,
    /// The workflow declares no such input.
    Unknown,
    /// A null for an input that does not accept one.
    Null,
    /// A value of another type than the declared one.
    WrongType,
    /// A value outside the declared choices.
    NotAChoice,
    /// A date that is not a day written `YYYY-MM-DD`.
    MalformedDate,
    /// A file path that is not absolute, or names no file that can be opened.
    MissingFile,
}

impl InputErrorCode {
    /// The code as the API writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAnObject => "not_an_object",
            Self::Required => "required",
            Self::Unknown => "unknown",
            Self::Null => "null",
            Self::WrongType => "wrong_type",
            Self::NotAChoice => "not_a_choice",
            Self::MalformedDate => "malformed_date",
            Self::MissingFile => "missing_file",
        }
    }
}

/// An input that does not fit the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputError {
    /// Why it was refused.
    pub code: InputErrorCode,
    /// The field at fault, `name` or `name[2]`.
    pub field: String,
    /// What is wrong with it.
    pub message: String,
}

impl InputError {
    fn new(code: InputErrorCode, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            field: field.into(),
            message: message.into(),
        }
    }
}

/// The inputs as the workflow receives them, read against `schema`: every
/// required field present, no field the schema does not declare, every value
/// of its declared type. `None` accepts any object.
///
/// # Errors
///
/// The first field that does not fit, with why.
pub fn validate(schema: Option<&Value>, inputs: &Value) -> Result<Value, InputError> {
    let Some(given) = inputs.as_object() else {
        return Err(InputError::new(
            InputErrorCode::NotAnObject,
            "",
            "the inputs must be an object",
        ));
    };
    let Some(schema) = schema else {
        return Ok(inputs.clone());
    };
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    for name in &required {
        if !given.contains_key(*name) {
            return Err(InputError::new(
                InputErrorCode::Required,
                *name,
                "is required",
            ));
        }
    }
    let mut checked = Map::new();
    for (name, value) in given {
        match properties.get(name) {
            Some(field) => {
                checked.insert(name.clone(), check(name, field, value)?);
            }
            None if closed => {
                return Err(InputError::new(
                    InputErrorCode::Unknown,
                    name.clone(),
                    "is not an input of this workflow",
                ))
            }
            None => {
                checked.insert(name.clone(), value.clone());
            }
        }
    }
    Ok(Value::Object(checked))
}

/// `value` read against the schema `field`, at `path`.
fn check(path: &str, field: &Value, value: &Value) -> Result<Value, InputError> {
    if value.is_null() {
        if field.get("nullable") == Some(&Value::Bool(true)) || declares(field, "null") {
            return Ok(Value::Null);
        }
        return Err(InputError::new(
            InputErrorCode::Null,
            path,
            "may not be null",
        ));
    }
    let value = match (value, declared_type(field)) {
        (Value::String(text), Some(kind)) if kind != "string" => {
            from_text(text, kind).unwrap_or_else(|| value.clone())
        }
        _ => value.clone(),
    };
    if let Some(kind) = declared_type(field) {
        if !is_of(&value, kind) {
            return Err(InputError::new(
                InputErrorCode::WrongType,
                path,
                format!("expected {}", article(kind)),
            ));
        }
    }
    if let Some(allowed) = field.get("enum").and_then(Value::as_array) {
        if !allowed.contains(&value) {
            let names: Vec<String> = allowed.iter().map(Value::to_string).collect();
            return Err(InputError::new(
                InputErrorCode::NotAChoice,
                path,
                format!("must be one of {}", names.join(", ")),
            ));
        }
    }
    if let Value::String(text) = &value {
        check_format(path, field, text)?;
    }
    if let (Value::Array(items), Some(item_schema)) = (&value, field.get("items")) {
        let mut checked = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            checked.push(check(&format!("{path}[{index}]"), item_schema, item)?);
        }
        return Ok(Value::Array(checked));
    }
    Ok(value)
}

/// `text` read against the `format` of `field`, when it declares one this
/// module checks.
fn check_format(path: &str, field: &Value, text: &str) -> Result<(), InputError> {
    match field.get("format").and_then(Value::as_str) {
        Some(DATE_FORMAT) if !is_date(text) => Err(InputError::new(
            InputErrorCode::MalformedDate,
            path,
            format!("'{text}' is not a date written YYYY-MM-DD"),
        )),
        Some(FILE_PATH_FORMAT) => check_file(path, text),
        _ => Ok(()),
    }
}

/// `true` when `text` is a day of the calendar written `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let shaped = text.len() == 10
        && text.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    shaped && NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
}

/// `text` is the absolute path of a file that can be opened.
fn check_file(path: &str, text: &str) -> Result<(), InputError> {
    let file = Path::new(text);
    if !file.is_absolute() {
        return Err(InputError::new(
            InputErrorCode::MissingFile,
            path,
            format!("'{text}' is not an absolute path"),
        ));
    }
    let opens = file.is_file() && std::fs::File::open(file).is_ok();
    if opens {
        Ok(())
    } else {
        Err(InputError::new(
            InputErrorCode::MissingFile,
            path,
            format!("no file can be opened at '{text}'"),
        ))
    }
}

/// The single type a field declares, when it declares one.
fn declared_type(field: &Value) -> Option<&str> {
    match field.get("type")? {
        Value::String(kind) => Some(kind.as_str()),
        Value::Array(kinds) => {
            let mut named = kinds
                .iter()
                .filter_map(Value::as_str)
                .filter(|k| *k != "null");
            let first = named.next()?;
            named.next().is_none().then_some(first)
        }
        _ => None,
    }
}

fn declares(field: &Value, kind: &str) -> bool {
    match field.get("type") {
        Some(Value::String(declared)) => declared == kind,
        Some(Value::Array(kinds)) => kinds.iter().any(|k| k.as_str() == Some(kind)),
        _ => false,
    }
}

fn is_of(value: &Value, kind: &str) -> bool {
    match kind {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}

/// `text` read as `kind`, when it is entirely one.
fn from_text(text: &str, kind: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(text.trim()).ok()?;
    is_of(&parsed, kind).then_some(parsed)
}

fn article(kind: &str) -> String {
    match kind {
        "integer" | "array" | "object" => format!("an {kind}"),
        other => format!("a {other}"),
    }
}
