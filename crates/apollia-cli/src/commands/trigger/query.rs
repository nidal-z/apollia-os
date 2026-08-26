//! `apollia-os trigger` read and state verbs.
//!
//! The seven verbs that read the runtime or flip a trigger's state: `list`,
//! `status`, `fire`, `enable`, `disable`, `logs` and `reload`.

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::display::{format_trigger_detail, format_trigger_list, format_trigger_logs};
use super::handle_client_error;

/// `apollia-os trigger list`: list all triggers.
pub(super) async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_triggers().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_trigger_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger status <id>`: detailed status of a trigger.
pub(super) async fn run_status(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.get_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_trigger_detail(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger fire <id>`: fire immediately.
pub(super) async fn run_fire(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.fire_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let task_id = resp.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
                note!("✔ Trigger '{id}' fired → task {task_id}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger enable <id>`: enable a trigger.
pub(super) async fn run_enable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.enable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Trigger '{id}' enabled");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger disable <id>`: disable a trigger.
pub(super) async fn run_disable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.disable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Trigger '{id}' disabled");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger logs <id> [--last N]`: firing history.
pub(super) async fn run_logs(client: &RuntimeClient, id: &str, last: usize, json: bool) -> i32 {
    match client.get_trigger_logs(id, last).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_trigger_logs(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger reload`: hot reload triggers from `apollia.toml`.
pub(super) async fn run_reload(client: &RuntimeClient, json: bool) -> i32 {
    match client.reload_triggers().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let count = resp.get("reloaded").and_then(|v| v.as_u64()).unwrap_or(0);
                note!("✔ Triggers reloaded - {count} active");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("trigger reload failed ({status}): {body}"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}
