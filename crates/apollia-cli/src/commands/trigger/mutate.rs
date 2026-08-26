//! `apollia-os trigger` write verbs: `create`, `update` and `delete`.

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::handle_client_error;

/// Fields supplied to `apollia-os trigger create`.
pub(super) struct CreateArgs<'a> {
    pub(super) id: &'a str,
    pub(super) agent: &'a str,
    pub(super) kind: &'a str,
    pub(super) detail: Option<&'a str>,
    pub(super) on_busy: &'a str,
    pub(super) input: Option<&'a str>,
}

/// `apollia-os trigger create <id> --agent <agent> --kind <kind> [options]`
///
/// Creates a new trigger via `POST /api/v1/triggers`.
pub(super) async fn run_create(client: &RuntimeClient, args: CreateArgs<'_>, json: bool) -> i32 {
    let CreateArgs {
        id,
        agent,
        kind,
        detail,
        on_busy,
        input,
    } = args;
    // Build the `source` object the runtime expects (`CreateTriggerRequest`
    // in routes_triggers.rs). The CLI `--kind` is the friendly name; the
    // runtime accepts the canonical `TriggerSourceConfig` tag; most kinds
    // map 1:1, except `filewatch` which the runtime spells `file_watch`.
    let source = match build_trigger_source(kind, detail) {
        Ok(s) => s,
        Err(msg) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
        }
    };

    let mut body = serde_json::json!({
        "id": id,
        "agent": agent,
        "on_busy": on_busy,
        "source": source,
    });
    if let Some(i) = input {
        body["input_template"] = serde_json::Value::String(i.to_string());
    }

    match client.create_trigger(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Trigger '{id}' created ({kind} → {agent})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// Build the `source` JSON object expected by `CreateTriggerRequest` /
/// `UpdateTriggerRequest`.
///
/// User-facing `--kind` values map to the runtime canonical tags:
///
/// | `--kind`    | `source.type` | required `--detail`                      |
/// |-------------|---------------|------------------------------------------|
/// | `cron`      | `cron`        | cron expression (e.g. `"0 9 * * 1"`)     |
/// | `interval`  | `interval`    | duration string (e.g. `"30m"`, `"1h"`)   |
/// | `oneshot`   | `oneshot`     | RFC 3339 timestamp                       |
/// | `filewatch` | `file_watch`  | path to file or directory                |
/// | `webhook`   | `webhook`     | shared HMAC-SHA256 secret                |
pub(super) fn build_trigger_source(
    kind: &str,
    detail: Option<&str>,
) -> Result<serde_json::Value, String> {
    let detail_str = || -> Result<&str, String> {
        detail.ok_or_else(|| format!("--detail is required for kind '{kind}'"))
    };
    match kind {
        "cron" => Ok(serde_json::json!({
            "type": "cron",
            "schedule": detail_str()?,
        })),
        "interval" => Ok(serde_json::json!({
            "type": "interval",
            "every": detail_str()?,
        })),
        "oneshot" => Ok(serde_json::json!({
            "type": "oneshot",
            "fire_at": detail_str()?,
        })),
        "filewatch" | "file_watch" => Ok(serde_json::json!({
            "type": "file_watch",
            "path": detail_str()?,
            // Watch every kind of FS event by default; the runtime keeps
            // sensible defaults for `recursive`, `follow_symlinks`, and
            // `exclude_patterns` when these are absent.
            "events": ["any"],
        })),
        "webhook" => {
            let secret = detail_str()?;
            // The runtime rejects shared-secret < 32 chars with HMAC-SHA256
            // strength reasoning; validate locally so the error is actionable
            // and avoids a confusing 422 round-trip.
            if secret.len() < 32 {
                return Err(format!(
                    "--detail must be a shared secret of at least 32 characters \
                     for kind webhook (got {} chars). HMAC-SHA256 needs a key \
                     length comparable to its 32-byte output to avoid weakness.",
                    secret.len()
                ));
            }
            Ok(serde_json::json!({
                "type": "webhook",
                "secret": secret,
            }))
        }
        other => Err(format!(
            "unknown trigger kind '{other}' (expected: cron, interval, oneshot, filewatch, webhook)"
        )),
    }
}

/// `apollia-os trigger update <id> [options]`
///
/// Updates an existing trigger via `PUT /api/v1/triggers/{id}`.
///
/// The runtime expects a **complete** body (`UpdateTriggerRequest` requires
/// `source: { type, ... }` in particular, no partial patch), so we first read
/// the current definition via `GET /api/v1/triggers/{id}` to preserve the
/// unmodified fields. This keeps the merge semantics the user expects.
/// Fields supplied to `apollia-os trigger update`.
pub(super) struct UpdateArgs<'a> {
    pub(super) id: &'a str,
    pub(super) detail: Option<&'a str>,
    pub(super) on_busy: Option<&'a str>,
    pub(super) input: Option<&'a str>,
}

/// Maps a runtime `source_type` to the kind-specific detail field name.
pub(super) fn detail_field_for(source_type: &str) -> Option<&'static str> {
    match source_type {
        "cron" => Some("schedule"),
        "interval" => Some("every"),
        "oneshot" => Some("fire_at"),
        "file_watch" => Some("path"),
        "webhook" => Some("secret"),
        _ => None,
    }
}

/// Report a `404 Not Found` for a trigger in the operator's preferred format.
pub(super) fn report_trigger_not_found(id: &str, _body: &str, json: bool) {
    let _ = crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("trigger '{id}' not found"),
    );
}

/// Patch the kind-specific `--detail` field onto `source_config` in place.
///
/// Returns `Err(exit_code)` when the source type is unknown (the caller must
/// return that code immediately); `Ok(())` otherwise (including no `--detail`).
pub(super) fn apply_detail_patch(
    source_config: &mut serde_json::Value,
    source_type: &str,
    detail: Option<&str>,
    json: bool,
) -> Result<(), i32> {
    let Some(d) = detail else {
        return Ok(());
    };
    let Some(field) = detail_field_for(source_type) else {
        let msg = format!("cannot patch --detail on unknown source type '{source_type}'");
        return Err(crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &msg,
        ));
    };
    if let Some(obj) = source_config.as_object_mut() {
        obj.insert(field.to_string(), serde_json::Value::String(d.to_string()));
    }
    Ok(())
}

pub(super) async fn run_update(client: &RuntimeClient, args: UpdateArgs<'_>, json: bool) -> i32 {
    let UpdateArgs {
        id,
        detail,
        on_busy,
        input,
    } = args;
    let current = match client.get_trigger(id).await {
        Ok(v) => v,
        Err(ClientError::ServerError { status: 404, body }) => {
            report_trigger_not_found(id, &body, json);
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => return handle_client_error(e, json),
    };

    let source_type = current
        .get("source_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut source_config = current
        .get("source_config")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    if let Err(code) = apply_detail_patch(&mut source_config, source_type.as_str(), detail, json) {
        return code;
    }

    let mut source = serde_json::Map::new();
    source.insert(
        "type".to_string(),
        serde_json::Value::String(source_type.clone()),
    );
    if let Some(obj) = source_config.as_object() {
        for (k, v) in obj {
            source.insert(k.clone(), v.clone());
        }
    }

    let agent = current
        .get("agent")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let enabled = current
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let effective_on_busy = on_busy
        .map(str::to_string)
        .or_else(|| {
            current
                .get("on_busy")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "skip".to_string());
    let input_template = input.map(str::to_string).or_else(|| {
        current
            .get("input_template")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });

    let mut body = serde_json::json!({
        "agent": agent,
        "enabled": enabled,
        "on_busy": effective_on_busy,
        "source": source,
    });
    if let Some(t) = input_template {
        body["input_template"] = serde_json::Value::String(t);
    }

    match client.update_trigger(id, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Trigger '{id}' updated");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            report_trigger_not_found(id, &body, json);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger delete <id> [--confirm]`
///
/// Deletes a trigger via `DELETE /api/v1/triggers/{id}`.
pub(super) async fn run_delete(client: &RuntimeClient, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to delete trigger '{id}' without prompt"),
        );
    }

    match client.delete_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Trigger '{id}' deleted");
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
