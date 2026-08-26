//! `apollia-os notify` subcommands: notification channel management.
//!
//! Provides the `test`, `list`, and `logs` sub-commands to check the
//! configured notification channels and review the alert history.
//!
//! Noun-verb pattern, consistent with `trigger`, `agent`, and `task`.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

// ─── Subcommands ──────────────────────────────────────────────────────────────

/// Notify subcommands : `apollia-os notify <verb>`.
#[derive(Debug, Subcommand)]
pub enum NotifyCommand {
    /// Send a test notification to every active channel.
    ///
    /// Asks the runtime to dispatch a test payload to each channel enabled
    /// in `apollia.toml`. Exits 0 if every active channel succeeds, 1 if
    /// any channel returns an error.
    Test,
    /// List configured notification channels with their status.
    ///
    /// Shows the identifier, type, accepted events and state
    /// (enabled / disabled) for each channel declared in `apollia.toml`.
    List,
    /// Show the recent notification history from SQLite.
    ///
    /// Reads the `notification_logs` table in `~/.apollia/hitl.db`.
    /// Returns an empty list if the table does not exist yet.
    Logs {
        /// Number of lines to display (default: 20).
        #[arg(long, default_value = "20")]
        last: usize,
    },
    /// Create a new notification channel.
    Create {
        /// Channel type: `desktop` or `webhook`.
        #[arg(long, value_name = "TYPE", value_parser = ["desktop", "webhook"])]
        kind: String,
        /// Target URL (required for `webhook`).
        #[arg(long)]
        url: Option<String>,
        /// Channel identifier. Auto-generated as `<kind>-<timestamp>` when
        /// omitted (operator-friendly default for ad-hoc creation).
        #[arg(long, value_name = "ID")]
        id: Option<String>,
        /// Human-readable label shown in the UI. Defaults to the id.
        #[arg(long, value_name = "TEXT")]
        label: Option<String>,
        /// Create the channel disabled (default: enabled).
        #[arg(long)]
        disabled: bool,
    },
    /// Update an existing notification channel.
    Update {
        /// Channel identifier.
        id: String,
        /// New URL (for webhook).
        #[arg(long)]
        url: Option<String>,
        /// Enable or disable.
        #[arg(long)]
        enabled: Option<bool>,
    },
    /// Delete a notification channel.
    Delete {
        /// Channel identifier.
        id: String,
        /// Confirm without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Show or modify the event types that trigger notifications.
    Events {
        /// Events subcommand.
        #[command(subcommand)]
        command: NotifyEventsCommand,
    },
}

/// Subcommands for notification event management.
#[derive(Debug, Subcommand)]
pub enum NotifyEventsCommand {
    /// Show the configured event types.
    Get,
    /// Set the active event types (comma-separated list).
    Set {
        /// Enabled event types (e.g. task_completed,task_failed,agent_error).
        #[arg(value_delimiter = ',', value_name = "EVENT")]
        events: Vec<String>,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Execute a `notify` subcommand.
///
/// Returns the POSIX exit code (0 = success, 1 = error, 2 = runtime offline).
pub async fn run(cmd: &NotifyCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        NotifyCommand::Test => run_test(&client, json).await,
        NotifyCommand::List => run_list(&client, json).await,
        NotifyCommand::Logs { last } => run_logs(&client, *last, json).await,
        NotifyCommand::Create {
            kind,
            url,
            id,
            label,
            disabled,
        } => {
            run_create(
                &client,
                CreateArgs {
                    kind,
                    url: url.as_deref(),
                    id: id.as_deref(),
                    label: label.as_deref(),
                    enabled: !*disabled,
                },
                json,
            )
            .await
        }
        NotifyCommand::Update { id, url, enabled } => {
            run_update_channel(&client, id, url.as_deref(), *enabled, json).await
        }
        NotifyCommand::Delete { id, confirm } => {
            run_delete_channel(&client, id, *confirm, json).await
        }
        NotifyCommand::Events { command } => run_events(&client, command, json).await,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `apollia-os notify test`: send a test notification to every active channel.
///
/// Exit code 0 when every active channel succeeds.
/// Exit code 1 when at least one active channel fails.
async fn run_test(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.post("/api/v1/notifications/test", None).await {
        Ok(r) => r,
        Err(e) => return handle_client_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    let results = parsed
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    if json {
        let output = serde_json::json!(results);
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("JSON serialization failed: {e}"),
                );
            }
        }
    } else {
        format_test_results(&results);
    }

    // exit code 1 when at least one active channel is in error
    let has_error = results.iter().any(|r| {
        r.get("status")
            .and_then(|s| s.as_str())
            .map(|s| s == "error")
            .unwrap_or(false)
    });

    if has_error {
        exit_codes::GENERAL_ERROR
    } else {
        exit_codes::SUCCESS
    }
}

/// `apollia-os notify list`: list the configured channels.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/notifications/channels").await {
        Ok(r) => r,
        Err(e) => return handle_client_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        let channels = parsed
            .get("channels")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        match serde_json::to_string_pretty(&channels) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("JSON serialization failed: {e}"),
                );
            }
        }
    } else {
        format_channel_list(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os notify logs [--last N]`: recent notification history.
async fn run_logs(client: &RuntimeClient, last: usize, json: bool) -> i32 {
    let uri = format!("/api/v1/notifications/logs?last={last}");
    let resp = match client.get(&uri).await {
        Ok(r) => r,
        Err(e) => return handle_client_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        let entries = parsed
            .get("entries")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        match serde_json::to_string_pretty(&entries) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("JSON serialization failed: {e}"),
                );
            }
        }
    } else {
        format_log_entries(&parsed);
    }
    exit_codes::SUCCESS
}

// ─── Formatters ───────────────────────────────────────────────────────────────

/// Format `notify test` results as a human-readable list.
///
/// Each line shows a check/cross mark, the channel ID, and the status detail.
/// Disabled channels are shown with `✗` and a `disabled` label.
fn format_test_results(results: &[serde_json::Value]) {
    if results.is_empty() {
        println!("  (no channels configured)");
        return;
    }

    for r in results {
        let id = r.get("channel_id").and_then(|v| v.as_str()).unwrap_or("?");
        let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let latency = r.get("latency_ms").and_then(|v| v.as_u64());
        let error = r.get("error").and_then(|v| v.as_str());

        let detail = match status {
            "ok" => {
                if let Some(ms) = latency {
                    format!("notification sent ({ms}ms)")
                } else {
                    "notification sent".to_string()
                }
            }
            "error" => error
                .map(|e| format!("error - {e}"))
                .unwrap_or_else(|| "unknown error".to_string()),
            "disabled" => "disabled".to_string(),
            other => other.to_string(),
        };

        let mark = if status == "ok" { "✔" } else { "✗" };
        println!("  {mark} {id:<12} - {detail}");
    }
}

/// Format `notify list` as a human-readable table.
///
/// Columns: `ID | TYPE | EVENTS | STATUT`
fn format_channel_list(resp: &serde_json::Value) {
    let channels = resp
        .get("channels")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<16} {:<10} {:<40} STATUS", "ID", "TYPE", "EVENTS");

    if channels.is_empty() {
        println!("  (no channels configured)");
        return;
    }

    for ch in &channels {
        // The runtime API exposes channels as ChannelResponse with `id` +
        // `channel_type`. Older builds (or fallbacks) emitted `channel_id`
        // + `type`, so accept both for forward / backward compat.
        let id = ch
            .get("id")
            .or_else(|| ch.get("channel_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let kind = ch
            .get("channel_type")
            .or_else(|| ch.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let enabled = ch.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let events_list: Vec<&str> = ch
            .get("events")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|e| e.as_str()).collect())
            .unwrap_or_default();

        let events_str = events_list.join(", ");
        let events_display = crate::commands::truncate_for_display(&events_str, 38, 37, "…");

        let statut = if enabled {
            "✔ active"
        } else {
            "✗ disabled"
        };

        println!(
            "  {:<16} {:<10} {:<40} {}",
            id, kind, events_display, statut
        );
    }
}

/// Format `notify logs` as a human-readable table.
///
/// Format: `date  event  task_id  channel_statuses`
fn format_log_entries(resp: &serde_json::Value) {
    let entries = resp
        .get("entries")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    if entries.is_empty() {
        println!("  (no notifications logged)");
        return;
    }

    for entry in &entries {
        let sent_at = entry.get("sent_at").and_then(|v| v.as_str()).unwrap_or("?");
        let date_display = if sent_at.len() >= 19 {
            sent_at[..19].replace('T', " ")
        } else {
            sent_at.to_string()
        };

        let event = entry
            .get("event_name")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");

        // Format per-channel statuses from the JSON map
        let channels_str = entry
            .get("channels")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| {
                        let ok = v.as_str().map(|s| s == "ok").unwrap_or(false);
                        let mark = if ok { "✔" } else { "✗" };
                        format!("{k} {mark}")
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
            })
            .unwrap_or_default();

        println!("  {date_display}  {event:<24}  {task_id:<36}  {channels_str}");
    }
}

/// `apollia-os notify create --kind <type>`: create a notification channel.
///
/// Builds the `CreateChannelRequest` body the runtime expects: `{ id, label?,
/// channel_type, enabled, config }`. The kind-specific config slot is populated
/// from `--url` for webhook channels; desktop channels carry an empty config.
/// Fields supplied to `apollia-os notify create`.
struct CreateArgs<'a> {
    kind: &'a str,
    url: Option<&'a str>,
    id: Option<&'a str>,
    label: Option<&'a str>,
    enabled: bool,
}

async fn run_create(client: &RuntimeClient, args: CreateArgs<'_>, json: bool) -> i32 {
    let CreateArgs {
        kind,
        url,
        id,
        label,
        enabled,
    } = args;
    // Webhook requires `url`; fail fast with a clear message rather than
    // letting the runtime return a generic 422.
    if kind == "webhook" && url.is_none() {
        let msg = "--url is required for webhook channels";
        return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, msg);
    }

    // Auto-generate a sensible id when the caller didn't pass one. Format
    // `<kind>-<unix-seconds>` is unique-enough for scripting use cases and
    // mirrors how the Desktop UI labels new channels.
    let channel_id: String = match id {
        Some(s) => s.to_string(),
        None => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{kind}-{ts}")
        }
    };

    let config = if kind == "webhook" {
        serde_json::json!({ "url": url.unwrap_or("") })
    } else {
        serde_json::json!({})
    };

    let mut body = serde_json::json!({
        "id": channel_id,
        "channel_type": kind,
        "enabled": enabled,
        "config": config,
    });
    if let Some(l) = label {
        body["label"] = serde_json::Value::String(l.to_string());
    }

    match client.create_notification_channel(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let echoed = resp
                    .get("id")
                    .or_else(|| resp.get("channel_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(channel_id.as_str());
                note!("✔ Notification channel '{echoed}' created (type: {kind})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify update <id>`: update a notification channel.
///
/// Maps `--url` (kind-specific config) to `config.url`, and leaves all
/// untouched fields absent so the runtime preserves them
/// (`UpdateChannelRequest` merge semantics).
async fn run_update_channel(
    client: &RuntimeClient,
    id: &str,
    url: Option<&str>,
    enabled: Option<bool>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({});
    if let Some(u) = url {
        body["config"] = serde_json::json!({ "url": u });
    }
    if let Some(e) = enabled {
        body["enabled"] = serde_json::Value::Bool(e);
    }

    match client.update_notification_channel(id, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Channel '{id}' updated");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("channel '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify delete <id> [--confirm]`: delete a notification channel.
async fn run_delete_channel(client: &RuntimeClient, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to delete channel '{id}' without prompt"),
        );
    }

    match client.delete_notification_channel(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Channel '{id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("channel '{id}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify events get|set`: manage event types.
async fn run_events(client: &RuntimeClient, command: &NotifyEventsCommand, json: bool) -> i32 {
    match command {
        NotifyEventsCommand::Get => run_events_get(client, json).await,
        NotifyEventsCommand::Set { events } => run_events_set(client, events, json).await,
    }
}

/// `apollia-os notify events get`: show the configured event types.
async fn run_events_get(client: &RuntimeClient, json: bool) -> i32 {
    match client.get_notification_events().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let events: Vec<&str> = resp
                    .get("events")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|e| e.as_str()).collect())
                    .unwrap_or_default();
                if events.is_empty() {
                    println!("  (no event type configured)");
                } else {
                    note!("  Active events:");
                    for e in &events {
                        println!("    - {e}");
                    }
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify events set <event,...>`: change the event types.
async fn run_events_set(client: &RuntimeClient, events: &[String], json: bool) -> i32 {
    let body = serde_json::json!({ "events": events });
    match client.set_notification_events(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Event types updated ({} active)", events.len());
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

// ─── Error handling ───────────────────────────────────────────────────────────

/// Handle client errors uniformly.
fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

/// Handle HTTP server errors.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &error_msg.to_string())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// Minimal CLI to test parsing of the notify sub-commands.
    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: NotifyCommand,
    }

    // ── Parsing ────────────────────────────────────────────────────────────────

    // GIVEN "test"
    // WHEN parse
    // THEN NotifyCommand::Test
    #[test]
    fn test_notify_test_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "test"]);
        // THEN
        assert!(matches!(cli.command, NotifyCommand::Test));
    }

    // GIVEN "list"
    // WHEN parse
    // THEN NotifyCommand::List
    #[test]
    fn test_notify_list_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "list"]);
        // THEN
        assert!(matches!(cli.command, NotifyCommand::List));
    }

    // GIVEN "logs" without --last
    // WHEN parse
    // THEN NotifyCommand::Logs { last: 20 }
    #[test]
    fn test_notify_logs_default_last() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs"]);
        // THEN
        match &cli.command {
            NotifyCommand::Logs { last } => assert_eq!(*last, 20),
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    // GIVEN "logs --last 5"
    // WHEN parse
    // THEN NotifyCommand::Logs { last: 5 }
    #[test]
    fn test_notify_logs_custom_last() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs", "--last", "5"]);
        // THEN
        match &cli.command {
            NotifyCommand::Logs { last } => assert_eq!(*last, 5),
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    // ── format_test_results all OK → exit 0 logic ─────────────────────

    // GIVEN two active channels that both succeed
    // WHEN has_error is computed
    // THEN false → exit code 0
    #[test]
    fn test_all_ok_no_error_flag() {
        // GIVEN
        let results = [
            serde_json::json!({"channel_id": "desktop", "type": "desktop", "status": "ok", "error": null, "latency_ms": 12}),
            serde_json::json!({"channel_id": "slack",   "type": "webhook", "status": "ok", "error": null, "latency_ms": 88}),
        ];
        // WHEN
        let has_error = results.iter().any(|r| {
            r.get("status")
                .and_then(|s| s.as_str())
                .map(|s| s == "error")
                .unwrap_or(false)
        });
        // THEN
        assert!(!has_error, "all OK → no error flag");
    }

    // ── one channel in error → exit 1 logic ──────────────────────────────

    // GIVEN one channel in error
    // WHEN has_error is computed
    // THEN true → exit code 1
    #[test]
    fn test_one_error_sets_flag() {
        // GIVEN
        let results = [
            serde_json::json!({"channel_id": "desktop", "type": "desktop", "status": "ok",    "error": null,                  "latency_ms": 12}),
            serde_json::json!({"channel_id": "slack",   "type": "webhook", "status": "error", "error": "connexion refusée",  "latency_ms": 5001}),
        ];
        // WHEN
        let has_error = results.iter().any(|r| {
            r.get("status")
                .and_then(|s| s.as_str())
                .map(|s| s == "error")
                .unwrap_or(false)
        });
        // THEN
        assert!(has_error, "one error → error flag set");
    }

    // ── JSON ChannelTestResult deserializable ──────────────────────────

    // GIVEN a spec-conformant JSON
    // WHEN deserialized into ChannelTestResult
    // THEN all fields are correct
    #[test]
    fn test_notify_test_json_structure() {
        // GIVEN
        let raw = serde_json::json!({
            "channel_id": "desktop",
            "type": "desktop",
            "status": "ok",
            "error": null,
            "latency_ms": 42
        });

        // WHEN
        let result: crate::client::ChannelTestResult =
            serde_json::from_value(raw).expect("désérialisation échoue");

        // THEN
        assert_eq!(result.channel_id, "desktop");
        assert_eq!(result.kind, "desktop");
        assert_eq!(result.status, "ok");
        assert!(result.error.is_none());
        assert_eq!(result.latency_ms, Some(42));
    }

    // GIVEN "create --kind webhook --url https://hooks.example.com"
    // WHEN parse
    // THEN NotifyCommand::Create with the expected fields
    #[test]
    fn test_notify_create_webhook_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "create",
            "--kind",
            "webhook",
            "--url",
            "https://hooks.example.com",
        ]);
        // THEN
        match &cli.command {
            NotifyCommand::Create {
                kind,
                url,
                disabled,
                ..
            } => {
                assert_eq!(kind, "webhook");
                assert_eq!(url.as_deref(), Some("https://hooks.example.com"));
                assert!(!*disabled);
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    // GIVEN "create --kind desktop"
    // WHEN parse
    // THEN NotifyCommand::Create { url: None }
    #[test]
    fn test_notify_create_desktop_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "create", "--kind", "desktop"]);
        // THEN
        match &cli.command {
            NotifyCommand::Create { kind, url, .. } => {
                assert_eq!(kind, "desktop");
                assert!(url.is_none());
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    // GIVEN "update ch-01 --url https://new.example.com"
    // WHEN parse
    // THEN NotifyCommand::Update with the expected fields
    #[test]
    fn test_notify_update_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "ch-01",
            "--url",
            "https://new.example.com",
        ]);
        // THEN
        match &cli.command {
            NotifyCommand::Update { id, url, enabled } => {
                assert_eq!(id, "ch-01");
                assert_eq!(url.as_deref(), Some("https://new.example.com"));
                assert!(enabled.is_none());
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    // GIVEN "delete ch-01 --confirm"
    // WHEN parse
    // THEN NotifyCommand::Delete { id: "ch-01", confirm: true }
    #[test]
    fn test_notify_delete_confirm_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "ch-01", "--confirm"]);
        // THEN
        match &cli.command {
            NotifyCommand::Delete { id, confirm } => {
                assert_eq!(id, "ch-01");
                assert!(confirm);
            }
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    // GIVEN "events get"
    // WHEN parse
    // THEN NotifyCommand::Events { command: NotifyEventsCommand::Get }
    #[test]
    fn test_notify_events_get_parses() {
        // GIVEN / WHEN
        let cli = TestCli::parse_from(["apollia-os", "events", "get"]);
        // THEN
        match &cli.command {
            NotifyCommand::Events { command } => {
                assert!(matches!(command, NotifyEventsCommand::Get))
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    // GIVEN "events set task_completed,task_failed"
    // WHEN parse
    // THEN NotifyEventsCommand::Set { events: ["task_completed", "task_failed"] }
    #[test]
    fn test_notify_events_set_parses() {
        // GIVEN / WHEN
        let cli =
            TestCli::parse_from(["apollia-os", "events", "set", "task_completed,task_failed"]);
        // THEN
        match &cli.command {
            NotifyCommand::Events { command } => match command {
                NotifyEventsCommand::Set { events } => {
                    assert_eq!(events, &["task_completed", "task_failed"]);
                }
                other => panic!("expected Set, got {other:?}"),
            },
            other => panic!("expected Events, got {other:?}"),
        }
    }
}
