//! The stored definition of an agent and the user's overrides:
//! `agent set`, `agent reset`, `agent list --definitions`, and the
//! definition section of `agent show`.

use apollia_tools::OverrideField;

use super::{handle_error, print_compact_error_and_exit};
use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

/// The value of `field` as typed on the command line: one string for `name`
/// and `mission`, the list of the given items for `examples`, `access` and
/// `sources` (none at all for an empty list).
pub(in crate::commands::agent) fn override_value(
    field: &str,
    values: &[String],
) -> Result<serde_json::Value, String> {
    let field = OverrideField::parse(field).map_err(|e| e.to_string())?;
    if field.is_list() {
        return Ok(serde_json::json!(values));
    }
    match values {
        [one] => Ok(serde_json::Value::String(one.clone())),
        _ => Err(format!(
            "field '{}' takes exactly one value, got {}",
            field.as_str(),
            values.len()
        )),
    }
}

/// `apollia-os agent set <agent_id> <field> [<value>...]`.
pub(in crate::commands::agent) async fn run_set(
    client: &RuntimeClient,
    agent_id: &str,
    field: &str,
    values: &[String],
    json: bool,
) -> i32 {
    let value = match override_value(field, values) {
        Ok(value) => value,
        Err(msg) => return print_compact_error_and_exit(&msg, json),
    };
    match client.set_agent_override(agent_id, field, value).await {
        Ok(resolved) => {
            print_resolved(&resolved, json);
            exit_codes::SUCCESS
        }
        Err(e) => definition_error(e, agent_id, json),
    }
}

/// `apollia-os agent reset <agent_id> [--field <field>] --confirm`.
pub(in crate::commands::agent) async fn run_reset(
    client: &RuntimeClient,
    agent_id: &str,
    field: Option<&str>,
    confirm: bool,
    json: bool,
) -> i32 {
    if let Some(field) = field {
        if let Err(e) = OverrideField::parse(field) {
            return print_compact_error_and_exit(&e.to_string(), json);
        }
    }
    if let Err(e) = client.get_agent_definition(agent_id).await {
        return definition_error(e, agent_id, json);
    }
    let action = match field {
        Some(field) => format!("reset the {field} of agent '{agent_id}' to its default"),
        None => format!("reset agent '{agent_id}' to its defaults"),
    };
    if let Some(code) = crate::output::require_confirmation(confirm, json, &action) {
        return code;
    }
    let result = match field {
        Some(field) => client.remove_agent_override(agent_id, field).await,
        None => client.reset_agent_overrides(agent_id).await,
    };
    match result {
        Ok(resolved) => {
            print_resolved(&resolved, json);
            exit_codes::SUCCESS
        }
        Err(e) => definition_error(e, agent_id, json),
    }
}

/// `apollia-os agent list --definitions`.
pub(in crate::commands::agent) async fn run_list_definitions(
    client: &RuntimeClient,
    json: bool,
) -> i32 {
    match client.list_agent_definitions().await {
        Ok(list) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&list).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let agents = list.as_array().cloned().unwrap_or_default();
            if agents.is_empty() {
                crate::note!("No agent stored.");
                return exit_codes::SUCCESS;
            }
            crate::note!("{:<24} {:<10} {:<16} OVERRIDDEN", "ID", "ORIGIN", "PACKAGE");
            for agent in &agents {
                println!(
                    "{:<24} {:<10} {:<16} {}",
                    text(agent, "id"),
                    text(agent, "origin"),
                    agent.get("package").and_then(|v| v.as_str()).unwrap_or("-"),
                    joined(agent, "overridden"),
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Print a resolved agent: the JSON document, or its fields.
fn print_resolved(resolved: &serde_json::Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(resolved).unwrap_or_default()
        );
    } else {
        print_definition_section(resolved);
    }
}

/// Human view of a resolved agent, each overridden field marked.
pub(in crate::commands::agent) fn print_definition_section(resolved: &serde_json::Value) {
    let overridden: Vec<&str> = resolved
        .get("overridden")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let mark = |field: &str| {
        if overridden.contains(&field) {
            "  (yours)"
        } else {
            ""
        }
    };
    crate::note!("  Definition (package defaults, then your overrides):");
    println!("    Name     : {}{}", text(resolved, "name"), mark("name"));
    println!(
        "    Mission  : {}{}",
        text(resolved, "mission"),
        mark("mission")
    );
    println!(
        "    Examples : {}{}",
        joined(resolved, "examples"),
        mark("examples")
    );
    println!(
        "    Access   : {}{}",
        joined(resolved, "access"),
        mark("access")
    );
    println!(
        "    Sources  : {}{}",
        joined(resolved, "sources"),
        mark("sources")
    );
    let model = resolved
        .get("model")
        .and_then(|m| m.get("model_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    println!("    Model    : {model}");
}

fn text<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn joined(value: &serde_json::Value, key: &str) -> String {
    let items: Vec<&str> = value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if items.is_empty() {
        "-".to_owned()
    } else {
        items.join(", ")
    }
}

/// Map a definition-route error: an unknown agent or a refused value is a
/// usage error (exit 1); an unreachable runtime is exit 2.
fn definition_error(err: ClientError, agent_id: &str, json: bool) -> i32 {
    match err {
        ClientError::ServerError { status: 404, .. } => {
            print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json)
        }
        ClientError::ServerError { status: 422, body } => print_compact_error_and_exit(&body, json),
        other => handle_error(other, json),
    }
}
