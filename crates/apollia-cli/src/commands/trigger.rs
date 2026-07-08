//! `apollia-os trigger` subcommands: trigger management.
//!
//! Provides the `list`, `status`, `fire`, `enable`, `disable`, `logs`, and
//! `reload` subcommands to manage, debug, and audit automatic agent triggers
//! from the terminal without editing `apollia.toml`.
//!
//! Noun-verb pattern consistent with `agent` and `task`.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

// ─── Subcommands ──────────────────────────────────────────────────────────

/// Trigger subcommands: `apollia-os trigger <verb>`.
#[derive(Debug, Subcommand)]
pub enum TriggerCommand {
    /// List all triggers with their status.
    List,
    /// Show the detailed status of a trigger.
    Status {
        /// Trigger identifier.
        id: String,
    },
    /// Fire a trigger immediately (debug/test).
    Fire {
        /// Trigger identifier.
        id: String,
    },
    /// Enable a disabled trigger.
    Enable {
        /// Trigger identifier.
        id: String,
    },
    /// Disable a trigger without editing apollia.toml.
    Disable {
        /// Trigger identifier.
        id: String,
    },
    /// Show the firing history from SQLite.
    Logs {
        /// Trigger identifier.
        id: String,
        /// Maximum number of entries to display.
        #[arg(long, default_value = "20")]
        last: usize,
    },
    /// Reload trigger config from apollia.toml (hot reload).
    ///
    /// Rereads `[[triggers]]` from `apollia.toml`, validates the new definitions,
    /// and restarts modified sources. Invalid TOML or invalid trigger configuration
    /// returns an error without interrupting the currently-running triggers.
    Reload,
    /// Create a new trigger (CRUD, complements hot-reload via apollia.toml).
    Create {
        /// Unique trigger identifier.
        id: String,
        /// Target agent.
        #[arg(long)]
        agent: String,
        /// Source type: cron, interval, oneshot, filewatch, webhook.
        #[arg(long, value_name = "TYPE")]
        kind: String,
        /// Source-specific detail:
        ///   cron      → cron expression (e.g. `"0 9 * * 1"`)
        ///   interval  → duration string (`30m`, `1h`, `6h`, `1d`)
        ///   oneshot   → RFC 3339 timestamp
        ///   filewatch → path to a file or directory
        ///   webhook   → shared HMAC-SHA256 secret of at least 32 chars
        #[arg(long)]
        detail: Option<String>,
        /// Policy when the agent is busy when a fire arrives.
        /// `queue` enqueues the fire (default), `drop` discards it.
        #[arg(long, value_parser = ["queue", "drop"], default_value = "queue")]
        on_busy: String,
        /// Input template sent to the agent when fired.
        #[arg(long)]
        input: Option<String>,
    },
    /// Update an existing trigger.
    Update {
        /// Trigger identifier.
        id: String,
        /// New source detail (kind is read from the existing definition).
        #[arg(long)]
        detail: Option<String>,
        /// New on-busy policy (`queue` or `drop`).
        #[arg(long, value_parser = ["queue", "drop"])]
        on_busy: Option<String>,
        /// New input template.
        #[arg(long)]
        input: Option<String>,
    },
    /// Delete a trigger.
    Delete {
        /// Trigger identifier.
        id: String,
        /// Confirm deletion without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────

/// Execute a `trigger` subcommand.
///
/// Returns the process exit code (0 = success, 1 = error, 2 = runtime offline).
pub async fn run(cmd: &TriggerCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        TriggerCommand::List => run_list(&client, json).await,
        TriggerCommand::Status { id } => run_status(&client, id, json).await,
        TriggerCommand::Fire { id } => run_fire(&client, id, json).await,
        TriggerCommand::Enable { id } => run_enable(&client, id, json).await,
        TriggerCommand::Disable { id } => run_disable(&client, id, json).await,
        TriggerCommand::Logs { id, last } => run_logs(&client, id, *last, json).await,
        TriggerCommand::Reload => run_reload(&client, json).await,
        TriggerCommand::Create {
            id,
            agent,
            kind,
            detail,
            on_busy,
            input,
        } => {
            run_create(
                &client,
                CreateArgs {
                    id,
                    agent,
                    kind,
                    detail: detail.as_deref(),
                    on_busy,
                    input: input.as_deref(),
                },
                json,
            )
            .await
        }
        TriggerCommand::Update {
            id,
            detail,
            on_busy,
            input,
        } => {
            run_update(
                &client,
                UpdateArgs {
                    id,
                    detail: detail.as_deref(),
                    on_busy: on_busy.as_deref(),
                    input: input.as_deref(),
                },
                json,
            )
            .await
        }
        TriggerCommand::Delete { id, confirm } => run_delete(&client, id, *confirm, json).await,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────

/// `apollia-os trigger list`: list all triggers.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
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
async fn run_status(client: &RuntimeClient, id: &str, json: bool) -> i32 {
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
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger fire <id>`: fire immediately.
async fn run_fire(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.fire_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let task_id = resp.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
                println!("✔ Trigger '{id}' fired → task {task_id}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger enable <id>`: enable a trigger.
async fn run_enable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.enable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' enabled");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger disable <id>`: disable a trigger.
async fn run_disable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.disable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' disabled");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger logs <id> [--last N]`: firing history.
async fn run_logs(client: &RuntimeClient, id: &str, last: usize, json: bool) -> i32 {
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
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger reload`: hot reload triggers from `apollia.toml`.
async fn run_reload(client: &RuntimeClient, json: bool) -> i32 {
    match client.reload_triggers().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let count = resp.get("reloaded").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("✔ Triggers reloaded - {count} active");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            if json {
                let output = serde_json::json!({ "error": body, "status": status });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: trigger reload failed ({status}): {body}");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

// ─── Formatters ───────────────────────────────────────────────────────────

/// Format trigger list as a human-readable table.
///
/// Columns: ID, AGENT, TYPE, ENABLED, FIRES, SKIPS, LAST FIRE
fn format_trigger_list(resp: &serde_json::Value) {
    let triggers = resp
        .get("triggers")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<24} {:<20} {:<12} {:<8} {:<6} {:<6} LAST FIRE",
        "ID", "AGENT", "TYPE", "ENABLED", "FIRES", "SKIPS"
    );

    if triggers.is_empty() {
        println!("  (no triggers configured)");
        return;
    }

    for t in &triggers {
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let agent = t.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = t.get("source_kind").and_then(|v| v.as_str()).unwrap_or("?");
        let enabled = if t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            "✔"
        } else {
            "✘"
        };
        let fires = t.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let skips = t.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let last = t
            .get("last_fired")
            .and_then(|v| v.as_str())
            .map(format_relative_time)
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  {:<24} {:<20} {:<12} {:<8} {:<6} {:<6} {}",
            id, agent, kind, enabled, fires, skips, last
        );
    }
}

/// Render an RFC3339 timestamp as a compact relative duration ("3m ago").
///
/// Falls back to the raw string when parsing fails. Used by the trigger
/// list / status outputs to surface "last fired" without dumping a full
/// RFC3339 string into the table.
fn format_relative_time(ts: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts).ok();
    let Some(dt) = parsed else {
        return ts.to_string();
    };
    let secs = chrono::Utc::now()
        .signed_duration_since(dt.with_timezone(&chrono::Utc))
        .num_seconds();
    if secs < 0 {
        return ts.to_string();
    }
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Format trigger detail as human-readable key-value pairs.
fn format_trigger_detail(resp: &serde_json::Value) {
    let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let agent = resp.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
    // The runtime returns `source_type` + structured `source_config` (a JSON
    // object). Older CLI builds looked for `source_kind` / `source_detail`
    // which never existed; fix here so `trigger status` shows the real
    // kind + the kind-specific config slot.
    let kind = resp
        .get("source_type")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let detail = trigger_detail_from_config(kind, resp.get("source_config"));
    let on_busy = resp.get("on_busy").and_then(|v| v.as_str()).unwrap_or("?");
    let enabled = resp
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let fires = resp.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let skips = resp.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let type_display = if detail.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} ({detail})")
    };

    println!("  Trigger   : {id}");
    println!("  Agent     : {agent}");
    println!("  Type      : {type_display}");
    println!("  On busy   : {on_busy}");
    println!("  Enabled   : {enabled}");
    println!("  Fires     : {fires} total, {skips} skipped");
}

/// Extract the human-readable detail string from a `source_config` JSON
/// object, picking the right field per `source_type`. Webhook intentionally
/// renders as `(secret hidden)` so we never print a shared secret in the
/// status output.
fn trigger_detail_from_config(kind: &str, config: Option<&serde_json::Value>) -> String {
    let Some(cfg) = config else {
        return String::new();
    };
    let pick = |k: &str| {
        cfg.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default()
    };
    match kind {
        "cron" => pick("schedule"),
        "interval" => pick("every"),
        "oneshot" => pick("fire_at"),
        "file_watch" => pick("path"),
        "webhook" => "secret hidden".to_string(),
        _ => String::new(),
    }
}

/// Format trigger logs as human-readable rows.
///
/// Columns: date, status, task_id (or a dash placeholder), reason (or a dash placeholder).
fn format_trigger_logs(resp: &serde_json::Value) {
    let entries = resp
        .get("entries")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    if entries.is_empty() {
        println!("  (no history)");
        return;
    }

    for entry in &entries {
        let fired_at = entry
            .get("fired_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        // Truncate RFC3339 to "YYYY-MM-DD HH:MM:SS"
        let date_display = if fired_at.len() >= 19 {
            fired_at[..19].replace('T', " ")
        } else {
            fired_at.to_string()
        };
        let status = entry.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");
        let reason = entry.get("reason").and_then(|v| v.as_str()).unwrap_or("-");
        println!("  {date_display}  {status:<8}  {task_id:<36}  {reason}");
    }
}

/// Fields supplied to `apollia-os trigger create`.
struct CreateArgs<'a> {
    id: &'a str,
    agent: &'a str,
    kind: &'a str,
    detail: Option<&'a str>,
    on_busy: &'a str,
    input: Option<&'a str>,
}

/// `apollia-os trigger create <id> --agent <agent> --kind <kind> [options]`
///
/// Creates a new trigger via `POST /api/v1/triggers`.
async fn run_create(client: &RuntimeClient, args: CreateArgs<'_>, json: bool) -> i32 {
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
            let payload = serde_json::json!({ "error": msg });
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).unwrap_or_default()
                );
            } else {
                eprintln!("Error: {msg}");
            }
            return exit_codes::GENERAL_ERROR;
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
                println!("✔ Trigger '{id}' created ({kind} → {agent})");
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
fn build_trigger_source(kind: &str, detail: Option<&str>) -> Result<serde_json::Value, String> {
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
struct UpdateArgs<'a> {
    id: &'a str,
    detail: Option<&'a str>,
    on_busy: Option<&'a str>,
    input: Option<&'a str>,
}

/// Maps a runtime `source_type` to the kind-specific detail field name.
fn detail_field_for(source_type: &str) -> Option<&'static str> {
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
fn report_trigger_not_found(id: &str, body: &str, json: bool) {
    if json {
        let out = serde_json::json!({ "error": body });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: trigger '{id}' not found");
    }
}

/// Patch the kind-specific `--detail` field onto `source_config` in place.
///
/// Returns `Err(exit_code)` when the source type is unknown (the caller must
/// return that code immediately); `Ok(())` otherwise (including no `--detail`).
fn apply_detail_patch(
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
        if json {
            println!("{}", serde_json::json!({ "error": msg }));
        } else {
            eprintln!("Error: {msg}");
        }
        return Err(exit_codes::GENERAL_ERROR);
    };
    if let Some(obj) = source_config.as_object_mut() {
        obj.insert(field.to_string(), serde_json::Value::String(d.to_string()));
    }
    Ok(())
}

async fn run_update(client: &RuntimeClient, args: UpdateArgs<'_>, json: bool) -> i32 {
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
                println!("✔ Trigger '{id}' updated");
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
async fn run_delete(client: &RuntimeClient, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Use --confirm to delete trigger '{id}' without prompt.");
        }
        return exit_codes::GENERAL_ERROR;
    }

    match client.delete_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: trigger '{id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

// ─── Error handling ───────────────────────────────────────────────────────

/// Uniform handling of client errors.
fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            if json {
                let output =
                    serde_json::json!({"error": "runtime not started (connection refused)"});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
            }
            exit_codes::RUNTIME_ERROR
        }
        other => {
            if json {
                let output = serde_json::json!({"error": other.to_string()});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: {other}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    /// Minimal CLI to test parsing of the trigger subcommands.
    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: super::TriggerCommand,
    }

    // ── Parsing tests ──────────────────────────────────────────────────────

    #[test]
    fn test_trigger_list_parses() {
        // GIVEN "list"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "list"]);
        // THEN TriggerCommand::List
        assert!(matches!(cli.command, super::TriggerCommand::List));
    }

    #[test]
    fn test_trigger_status_parses() {
        // GIVEN "status rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "status", "rapport-hebdo"]);
        // THEN TriggerCommand::Status { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Status { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_fire_parses() {
        // GIVEN "fire rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "fire", "rapport-hebdo"]);
        // THEN TriggerCommand::Fire { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Fire { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Fire, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_enable_parses() {
        // GIVEN "enable rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "enable", "rapport-hebdo"]);
        // THEN TriggerCommand::Enable { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Enable { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Enable, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_disable_parses() {
        // GIVEN "disable rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "disable", "rapport-hebdo"]);
        // THEN TriggerCommand::Disable { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Disable { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Disable, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_logs_default_last_20() {
        // GIVEN "logs rapport-hebdo" (no --last flag)
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs", "rapport-hebdo"]);
        // THEN default last = 20
        match &cli.command {
            super::TriggerCommand::Logs { id, last } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(*last, 20);
            }
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_logs_custom_last() {
        // GIVEN "logs rapport-hebdo --last 5"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs", "rapport-hebdo", "--last", "5"]);
        // THEN last = 5
        match &cli.command {
            super::TriggerCommand::Logs { last, .. } => assert_eq!(*last, 5),
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_reload_parses() {
        // GIVEN "reload"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "reload"]);
        // THEN TriggerCommand::Reload
        assert!(matches!(cli.command, super::TriggerCommand::Reload));
    }

    #[test]
    fn test_trigger_create_parses() {
        // GIVEN "create rapport-hebdo --agent mon-agent --kind cron --detail '0 9 * * 1'"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "create",
            "rapport-hebdo",
            "--agent",
            "mon-agent",
            "--kind",
            "cron",
            "--detail",
            "0 9 * * 1",
        ]);
        // THEN TriggerCommand::Create with the right fields
        match &cli.command {
            super::TriggerCommand::Create {
                id,
                agent,
                kind,
                detail,
                on_busy,
                input,
            } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(agent, "mon-agent");
                assert_eq!(kind, "cron");
                assert_eq!(detail.as_deref(), Some("0 9 * * 1"));
                // Default on_busy was changed from "skip" (CLI-only fiction)
                // to "queue" (runtime canonical value) in the v0.1.0 trigger
                // payload fix; see run_create.
                assert_eq!(on_busy, "queue");
                assert!(input.is_none());
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_create_with_on_busy_parses() {
        // GIVEN "create t1 --agent a1 --kind interval --on-busy queue"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "create",
            "t1",
            "--agent",
            "a1",
            "--kind",
            "interval",
            "--on-busy",
            "queue",
        ]);
        // THEN on_busy = "queue"
        match &cli.command {
            super::TriggerCommand::Create { on_busy, .. } => assert_eq!(on_busy, "queue"),
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_update_parses() {
        // GIVEN "update rapport-hebdo --detail '0 10 * * 1'"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "rapport-hebdo",
            "--detail",
            "0 10 * * 1",
        ]);
        // THEN Update { id: "rapport-hebdo", detail: Some("0 10 * * 1"), on_busy: None, input: None }
        match &cli.command {
            super::TriggerCommand::Update {
                id,
                detail,
                on_busy,
                input,
            } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(detail.as_deref(), Some("0 10 * * 1"));
                assert!(on_busy.is_none());
                assert!(input.is_none());
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_delete_parses() {
        // GIVEN "delete rapport-hebdo --confirm"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "rapport-hebdo", "--confirm"]);
        // THEN Delete { id: "rapport-hebdo", confirm: true }
        match &cli.command {
            super::TriggerCommand::Delete { id, confirm } => {
                assert_eq!(id, "rapport-hebdo");
                assert!(confirm);
            }
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_delete_without_confirm() {
        // GIVEN "delete rapport-hebdo" without --confirm
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "rapport-hebdo"]);
        // THEN confirm = false
        match &cli.command {
            super::TriggerCommand::Delete { confirm, .. } => assert!(!confirm),
            other => panic!("expected Delete, got {other:?}"),
        }
    }
}
