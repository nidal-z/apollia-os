//! The inputs of a workflow as a launch form shows them: one field per input,
//! in the order the workflow declares them, each with the kind of field it
//! takes.
//!
//! The kinds are read from the schema the SDK infers: the JSON type, the
//! `enum` of a choice, the `items` of a list, and the `format` of a text
//! (`date`, `long-text`, `file-path`). The order is the `x-order` list the SDK
//! writes; an input it does not list follows, in the order of the schema.

use serde::Serialize;
use serde_json::Value;

use super::inputs::{DATE_FORMAT, FILE_PATH_FORMAT, LONG_TEXT_FORMAT};

/// Key of the input schema that lists the inputs in their declared order.
pub const INPUT_ORDER_KEY: &str = "x-order";

/// The field a form shows for an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// A line of text.
    Text,
    /// Text over several lines.
    LongText,
    /// A file, given as its absolute path.
    File,
    /// A day, `YYYY-MM-DD`.
    Date,
    /// A whole number.
    Integer,
    /// A number.
    Number,
    /// Yes or no.
    Boolean,
    /// One value among `choices`.
    Choice,
    /// Several values, each of `itemKind`.
    List,
    /// A type no form field carries; the value is given as JSON.
    Other,
}

/// One input of a workflow, as its launch form shows it.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InputField {
    /// The input's name, the key of the inputs object.
    pub name: String,
    /// The field to show.
    pub kind: InputKind,
    /// `true` when a launch without it is refused.
    pub required: bool,
    /// `true` when the input accepts null.
    pub nullable: bool,
    /// What the input is, as the workflow describes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The values a choice, or each item of a list of choices, may take, as
    /// text: a launch reads each back as the type the input declares.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// The kind of each item of a list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_kind: Option<InputKind>,
}

/// The fields of the inputs `schema` declares, in their declared order; none
/// when the workflow declares no schema.
pub fn input_fields(schema: Option<&Value>) -> Vec<InputField> {
    let Some(properties) = schema
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let required: Vec<&str> = schema
        .and_then(|s| s.get("required"))
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let declared: Vec<&str> = schema
        .and_then(|s| s.get(INPUT_ORDER_KEY))
        .and_then(Value::as_array)
        .map(|o| o.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut names: Vec<&str> = declared
        .into_iter()
        .filter(|name| properties.contains_key(*name))
        .collect();
    for name in properties.keys() {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names
        .into_iter()
        .filter_map(|name| {
            let field = properties.get(name)?;
            Some(input_field(name, field, required.contains(&name)))
        })
        .collect()
}

fn input_field(name: &str, field: &Value, required: bool) -> InputField {
    let items = field
        .get("items")
        .filter(|_| type_of(field) == Some("array"));
    let choices = field
        .get("enum")
        .or_else(|| items.and_then(|i| i.get("enum")))
        .and_then(Value::as_array)
        .map(|values| values.iter().map(as_text).collect())
        .unwrap_or_default();
    InputField {
        name: name.to_string(),
        kind: kind_of(field),
        required,
        nullable: field.get("nullable") == Some(&Value::Bool(true)),
        description: field
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        choices,
        item_kind: items.map(kind_of),
    }
}

/// A choice as text: a string as written, any other value as JSON.
fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn type_of(field: &Value) -> Option<&str> {
    field.get("type").and_then(Value::as_str)
}

fn kind_of(field: &Value) -> InputKind {
    if field.get("enum").is_some() {
        return InputKind::Choice;
    }
    match type_of(field) {
        Some("string") => match field.get("format").and_then(Value::as_str) {
            None => InputKind::Text,
            Some(DATE_FORMAT) => InputKind::Date,
            Some(LONG_TEXT_FORMAT) => InputKind::LongText,
            Some(FILE_PATH_FORMAT) => InputKind::File,
            Some(_) => InputKind::Other,
        },
        Some("integer") => InputKind::Integer,
        Some("number") => InputKind::Number,
        Some("boolean") => InputKind::Boolean,
        Some("array") => InputKind::List,
        _ => InputKind::Other,
    }
}
