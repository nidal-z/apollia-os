//! Documentation handed to the model at the moment it misuses a tool.
//!
//! Every tool's schema is advertised on every call, and a capable model reads
//! it once and gets the arguments right. A small one does not: it writes
//! `file_path` where the parameter is `path`, passes a string where a list is
//! expected, invents a value outside an enum, or leaves a required field out.
//! The executor then fails with whatever its deserialiser says (`missing field
//! path`), which names the symptom and not the fix, and the model tries again
//! with the same guess.
//!
//! So each call is checked against the schema of the tool it names before it
//! runs. A call that cannot be right does not run. The model gets back every
//! problem at once, and under it the tool's usage card: each parameter with its
//! type, whether it is required, the values it accepts and its description.
//! That card costs tokens only on the turn that needs it, which is what makes
//! it affordable for every tool rather than only for a few.
//!
//! The check is deliberately tolerant. It blocks only what the executor would
//! refuse anyway: a missing required parameter, arguments that are not an
//! object, a value of an incompatible kind, a value outside an enum, and an
//! undeclared parameter on a schema that forbids them. A number written as a
//! string, or a boolean as `"true"`, passes, because executors accept both, and
//! refusing a call that would have worked costs a turn for nothing. Keywords
//! it does not know (`anyOf`, `$ref`, a list of types) constrain nothing here.

use serde_json::{Map, Value};

use super::helpers::levenshtein;

/// Longest slice of a description kept in a usage card, in characters.
const DESCRIPTION_CHARS: usize = 160;

/// Why a call to `tool` cannot run as written, with the tool's usage card, or
/// `None` when the arguments are acceptable.
pub(in crate::chat::builtin_agent) fn invalid_arguments_guide(
    tool: &str,
    description: &str,
    schema: &Value,
    arguments: &Value,
) -> Option<String> {
    let problems = argument_problems(arguments, schema);
    if problems.is_empty() {
        return None;
    }
    let listed: Vec<String> = problems.iter().map(|p| format!("- {p}")).collect();
    Some(format!(
        "invalid arguments for `{tool}`, the call did not run:\n{}\n\n{}\n\nCall `{tool}` again \
         with corrected arguments. Do not repeat the same arguments.",
        listed.join("\n"),
        usage_card(tool, description, schema)
    ))
}

/// Times an identical call may run in one turn before it is answered with a
/// reminder instead. Two, so a legitimate retry after a transient failure still
/// happens; the third is a loop.
const IDENTICAL_CALLS_ALLOWED: usize = 2;

/// A reminder in place of a call that already ran [`IDENTICAL_CALLS_ALLOWED`]
/// times this turn with the same arguments, or `None` when it may run.
///
/// A small model that does not know what to do next tends to call the tool it
/// just called, with the same arguments, and read the same answer: the loop the
/// operator sees as an assistant stuck on one tool. Running the call again
/// cannot change its answer, so it is not run, and the model is told what to do
/// instead. `previous` holds the name and arguments of the calls already made
/// this turn.
pub(in crate::chat::builtin_agent) fn repeated_call_reminder<'a>(
    tool: &str,
    arguments: &Value,
    previous: impl Iterator<Item = (&'a str, &'a Value)>,
) -> Option<String> {
    let runs = previous
        .filter(|(name, args)| *name == tool && *args == arguments)
        .count();
    (runs >= IDENTICAL_CALLS_ALLOWED).then(|| {
        format!(
            "not run: `{tool}` was already called {runs} times this turn with exactly these \
             arguments, and its result is above. Use that result, call it with different \
             arguments, or tell the user what is blocking."
        )
    })
}

/// Everything wrong with `arguments` for `schema`, in parameter order.
fn argument_problems(arguments: &Value, schema: &Value) -> Vec<String> {
    let Some(schema) = schema.as_object() else {
        return Vec::new();
    };
    let empty = Map::new();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if properties.is_empty() && required.is_empty() {
        return Vec::new();
    }

    // Some backends hand over the arguments as the JSON text the model wrote,
    // and a tool without parameters is often called with none at all.
    let parsed;
    let object = match arguments {
        Value::Object(map) => map,
        Value::Null => &empty,
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => {
                parsed = map;
                &parsed
            }
            _ => return vec!["the arguments must be a JSON object of named parameters".into()],
        },
        _ => return vec!["the arguments must be a JSON object of named parameters".into()],
    };

    let undeclared: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|k| !properties.contains_key(*k))
        .collect();

    let mut problems = Vec::new();
    for name in &required {
        if object.contains_key(*name) {
            continue;
        }
        match closest(name, &undeclared) {
            Some(passed) => problems.push(format!(
                "missing required parameter `{name}` (you passed `{passed}`, which is not a \
                 parameter of this tool; use `{name}`)"
            )),
            None => problems.push(format!("missing required parameter `{name}`")),
        }
    }

    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for key in &undeclared {
            if required.iter().any(|r| closest(r, &[key]).is_some()) {
                continue; // already named in the missing-parameter line
            }
            problems.push(format!("`{key}` is not a parameter of this tool"));
        }
    }

    for (name, property) in properties {
        if let Some(value) = object.get(name) {
            if let Some(problem) = value_problem(name, value, property) {
                problems.push(problem);
            }
        }
    }
    problems
}

/// The undeclared key that most plausibly meant `wanted`, if any.
///
/// Catches the two ways a model misnames a parameter: a near spelling
/// (`pth`), and a qualified form of the right word (`file_path`, `pathName`).
fn closest<'a>(wanted: &str, passed: &[&'a str]) -> Option<&'a str> {
    let normalise = |s: &str| s.to_lowercase().replace(['_', '-'], "");
    let target = normalise(wanted);
    passed.iter().copied().find(|candidate| {
        let c = normalise(candidate);
        c == target
            || (target.len() >= 3 && (c.ends_with(&target) || c.starts_with(&target)))
            || levenshtein(&c, &target) <= (target.chars().count() / 3).clamp(1, 3)
    })
}

/// What is wrong with one parameter's value, when something is.
fn value_problem(name: &str, value: &Value, property: &Value) -> Option<String> {
    if let Some(allowed) = property.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            let listed: Vec<String> = allowed.iter().map(Value::to_string).collect();
            return Some(format!(
                "`{name}` must be one of {}, not {value}",
                listed.join(", ")
            ));
        }
        return None;
    }
    let expected = property.get("type").and_then(Value::as_str)?;
    let fits = match expected {
        "string" => value.is_string(),
        "integer" | "number" => {
            value.is_number()
                || value
                    .as_str()
                    .is_some_and(|s| s.trim().parse::<f64>().is_ok())
        }
        "boolean" => value.is_boolean() || matches!(value.as_str(), Some("true" | "false")),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    };
    (!fits).then(|| {
        format!(
            "`{name}` must be {}, not {}",
            article(expected),
            kind_of(value)
        )
    })
}

fn article(kind: &str) -> String {
    match kind {
        "array" => "a list (JSON array)".into(),
        "object" => "an object".into(),
        "integer" => "an integer".into(),
        other => format!("a {other}"),
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// The tool's usage card: what it does and every parameter it takes.
fn usage_card(tool: &str, description: &str, schema: &Value) -> String {
    let mut card = format!(
        "How to call `{tool}`: {}",
        clip(first_sentence(description))
    );
    let properties = schema.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let Some(properties) = properties.filter(|p| !p.is_empty()) else {
        card.push_str("\nIt takes no parameters: call it with `{}`.");
        return card;
    };
    card.push_str("\nParameters:");
    for (name, property) in properties {
        let kind = property
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("any");
        let mut facts = vec![kind.to_string()];
        facts.push(if required.contains(&name.as_str()) {
            "required".into()
        } else {
            "optional".into()
        });
        if let Some(allowed) = property.get("enum").and_then(Value::as_array) {
            let listed: Vec<String> = allowed.iter().map(Value::to_string).collect();
            facts.push(format!("one of {}", listed.join(", ")));
        }
        if let Some(default) = property.get("default") {
            facts.push(format!("default {default}"));
        }
        let line = match property.get("description").and_then(Value::as_str) {
            Some(text) if !text.trim().is_empty() => {
                format!("\n- `{name}` ({}): {}", facts.join(", "), clip(text))
            }
            _ => format!("\n- `{name}` ({})", facts.join(", ")),
        };
        card.push_str(&line);
    }
    card
}

fn first_sentence(text: &str) -> &str {
    let text = text.trim();
    match text.find(". ") {
        Some(end) => &text[..=end],
        None => text,
    }
}

fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= DESCRIPTION_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(DESCRIPTION_CHARS).collect();
    format!("{}...", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file_write_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Absolute or workspace-relative path."},
                "content": {"type": "string"},
                "mode": {"type": "string", "enum": ["overwrite", "append"], "default": "overwrite"},
                "tags": {"type": "array", "items": {"type": "string"}},
                "timeout_secs": {"type": "integer"}
            },
            "required": ["path", "content"]
        })
    }

    #[test]
    fn a_third_identical_call_is_answered_with_a_reminder() {
        // GIVEN a turn that already listed the same folder twice
        let args = json!({"path": "/tmp"});
        let other = json!({"path": "/home"});
        let history = [
            ("file_list", &args),
            ("file_list", &args),
            ("file_list", &other),
        ];

        // WHEN the model asks for the same listing again, and for another one
        let again = repeated_call_reminder("file_list", &args, history.iter().copied());
        let fresh = repeated_call_reminder("file_list", &other, history.iter().copied());

        // THEN only the loop is stopped
        assert!(again.is_some_and(|r| r.contains("already called 2 times")));
        assert!(fresh.is_none());
    }

    #[test]
    fn a_misnamed_parameter_is_named_back_with_the_right_one() {
        // GIVEN a call that says `file_path` where the tool takes `path`
        let args = json!({"file_path": "/tmp/a.txt", "content": "hi"});

        // WHEN it is checked
        let guide =
            invalid_arguments_guide("file_write", "Write a file.", &file_write_schema(), &args)
                .expect("the call cannot run");

        // THEN the model is told which name to use, and gets the usage card
        assert!(
            guide.contains("missing required parameter `path`"),
            "{guide}"
        );
        assert!(guide.contains("you passed `file_path`"), "{guide}");
        assert!(guide.contains("How to call `file_write`"), "{guide}");
        assert!(guide.contains("- `mode` (string, optional, one of \"overwrite\", \"append\", default \"overwrite\")"), "{guide}");
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        // GIVEN a call with a value outside the enum and a string for a list
        let args = json!({"path": "a", "content": "b", "mode": "replace", "tags": "x,y"});

        // WHEN it is checked
        let problems = argument_problems(&args, &file_write_schema());

        // THEN both are listed, so the model fixes them in one retry
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("`mode` must be one of"));
        assert!(problems[1].contains("`tags` must be a list"));
    }

    #[test]
    fn values_the_executors_accept_are_not_refused() {
        // GIVEN a number and a boolean written as strings, an undeclared extra
        // key on a schema that allows one, and arguments sent as JSON text
        let lenient = json!({"path": "a", "content": "b", "timeout_secs": "30", "note": "x"});
        let as_text = Value::String(r#"{"path": "a", "content": "b"}"#.into());

        // WHEN they are checked
        // THEN nothing blocks the call
        assert!(argument_problems(&lenient, &file_write_schema()).is_empty());
        assert!(argument_problems(&as_text, &file_write_schema()).is_empty());
    }

    #[test]
    fn a_schema_without_parameters_constrains_nothing() {
        // GIVEN a tool that takes no parameters, called with none
        let schema = json!({"type": "object", "properties": {}});

        // WHEN the call is checked
        // THEN it runs
        assert!(invalid_arguments_guide("list_things", "", &schema, &Value::Null).is_none());
    }

    #[test]
    fn arguments_that_are_not_an_object_are_refused() {
        // GIVEN a bare string where named parameters were expected
        let args = Value::String("/tmp/a.txt".into());

        // WHEN it is checked
        let problems = argument_problems(&args, &file_write_schema());

        // THEN the model is told the shape it must use
        assert_eq!(
            problems,
            vec!["the arguments must be a JSON object of named parameters"]
        );
    }
}
