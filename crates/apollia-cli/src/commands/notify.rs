//! `apollia-os notify` subcommands — notification channel management.
//!
//! Fournit les sous-commandes `test`, `list` et `logs` pour vérifier les canaux
//! de notification configurés et consulter l'historique des alertes.
//!
//! Pattern noun-verb cohérent avec `trigger`, `agent`, `task` (ADR-008).

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
        /// Channel type: desktop, webhook.
        #[arg(long, value_name = "TYPE")]
        kind: String,
        /// Target URL (for webhook).
        #[arg(long)]
        url: Option<String>,
        /// Enable immediately.
        #[arg(long, default_value_t = true)]
        enabled: bool,
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
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        NotifyCommand::Test => run_test(&client, json).await,
        NotifyCommand::List => run_list(&client, json).await,
        NotifyCommand::Logs { last } => run_logs(&client, *last, json).await,
        NotifyCommand::Create { kind, url, enabled } => {
            run_create(&client, kind, url.as_deref(), *enabled, json).await
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

/// `apollia-os notify test` — envoi d'une notification de test sur tous les canaux actifs.
///
/// Exit code 0 si tous les canaux actifs réussissent.
/// Exit code 1 si au moins un canal actif échoue.
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
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let results = parsed
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    if json {
        // JSON structuré
        let output = serde_json::json!(results);
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("Error: JSON serialization failed: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        format_test_results(&results);
    }

    // exit code 1 si au moins un canal actif est en erreur
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

/// `apollia-os notify list` — liste des canaux configurés.
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
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
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
                eprintln!("Error: JSON serialization failed: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        format_channel_list(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os notify logs [--last N]` — historique des notifications récentes.
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
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
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
                eprintln!("Error: JSON serialization failed: {e}");
                return exit_codes::GENERAL_ERROR;
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
/// Disabled channels are shown with `✗` and the label `désactivé`.
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
                .map(|e| format!("error — {e}"))
                .unwrap_or_else(|| "unknown error".to_string()),
            "disabled" => "disabled".to_string(),
            other => other.to_string(),
        };

        let mark = if status == "ok" { "✔" } else { "✗" };
        println!("  {mark} {id:<12} — {detail}");
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

    println!("  {:<16} {:<10} {:<40} STATUT", "ID", "TYPE", "EVENTS");

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
        let events_display = if events_str.len() > 38 {
            format!("{}…", &events_str[..37])
        } else {
            events_str
        };

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
        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("—");

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

/// `apollia-os notify create --kind <type>` — créer un canal de notification.
async fn run_create(
    client: &RuntimeClient,
    kind: &str,
    url: Option<&str>,
    enabled: bool,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({
        "kind": kind,
        "enabled": enabled,
    });
    if let Some(u) = url {
        body["url"] = serde_json::Value::String(u.to_string());
    }

    match client.create_notification_channel(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let id = resp
                    .get("channel_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                println!("✔ Notification channel '{id}' created (type: {kind})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify update <id>` — mettre à jour un canal de notification.
async fn run_update_channel(
    client: &RuntimeClient,
    id: &str,
    url: Option<&str>,
    enabled: Option<bool>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({});
    if let Some(u) = url {
        body["url"] = serde_json::Value::String(u.to_string());
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
                println!("✔ Channel '{id}' updated");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: canal '{id}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify delete <id> [--confirm]` — supprimer un canal de notification.
async fn run_delete_channel(client: &RuntimeClient, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Utiliser --confirm pour supprimer le canal '{id}' sans confirmation.");
        }
        return exit_codes::GENERAL_ERROR;
    }

    match client.delete_notification_channel(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Channel '{id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: canal '{id}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os notify events get|set` — gérer les types d'événements.
async fn run_events(client: &RuntimeClient, command: &NotifyEventsCommand, json: bool) -> i32 {
    match command {
        NotifyEventsCommand::Get => run_events_get(client, json).await,
        NotifyEventsCommand::Set { events } => run_events_set(client, events, json).await,
    }
}

/// `apollia-os notify events get` — afficher les types d'événements configurés.
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
                    println!("  Active events:");
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

/// `apollia-os notify events set <event,...>` — modifier les types d'événements.
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
                println!("✔ Event types updated ({} active)", events.len());
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

/// Handle HTTP server errors.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    if json {
        let output = serde_json::json!({"error": error_msg});
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        eprintln!("Error: {error_msg}");
    }
    exit_codes::GENERAL_ERROR
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// CLI minimal pour tester le parsing des sous-commandes notify.
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

    // GIVEN "logs" sans --last
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

    // GIVEN deux canaux actifs en succès
    // WHEN has_error est calculé
    // THEN false → exit code 0
    #[test]
    fn test_ac1_all_ok_no_error_flag() {
        // GIVEN
        let results = vec![
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

    // ── un canal en erreur → exit 1 logic ──────────────────────────────

    // GIVEN un canal en erreur
    // WHEN has_error est calculé
    // THEN true → exit code 1
    #[test]
    fn test_ac2_one_error_sets_flag() {
        // GIVEN
        let results = vec![
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

    // ── JSON ChannelTestResult désérialisable ──────────────────────────

    // GIVEN un JSON conforme à la spec
    // WHEN désérialisé en ChannelTestResult
    // THEN tous les champs sont corrects
    #[test]
    fn test_ac5_notify_test_json_structure() {
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
    // THEN NotifyCommand::Create avec les bons champs
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
            NotifyCommand::Create { kind, url, enabled } => {
                assert_eq!(kind, "webhook");
                assert_eq!(url.as_deref(), Some("https://hooks.example.com"));
                assert!(*enabled);
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
    // THEN NotifyCommand::Update avec les bons champs
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
