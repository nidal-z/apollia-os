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
    /// Envoyer une notification de test sur tous les canaux actifs.
    ///
    /// Contacte le runtime pour déclencher un envoi de test sur chaque canal
    /// activé dans `apollia.toml`. Le code de sortie est 0 si tous les canaux
    /// actifs réussissent, 1 si au moins un retourne une erreur.
    Test,
    /// Lister les canaux de notification configurés avec leur statut.
    ///
    /// Affiche l'identifiant, le type, les événements acceptés et l'état
    /// (actif / désactivé) de chaque canal déclaré dans `apollia.toml`.
    List,
    /// Afficher l'historique des notifications récentes depuis SQLite.
    ///
    /// Lit la table `notification_logs` dans `~/.apollia/hitl.db`.
    /// Si la table n'existe pas encore, le résultat est vide.
    Logs {
        /// Nombre de lignes à afficher (défaut : 20).
        #[arg(long, default_value = "20")]
        last: usize,
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
                    format!("notification envoyée ({ms}ms)")
                } else {
                    "notification envoyée".to_string()
                }
            }
            "error" => error
                .map(|e| format!("erreur — {e}"))
                .unwrap_or_else(|| "erreur inconnue".to_string()),
            "disabled" => "désactivé".to_string(),
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
        let id = ch.get("channel_id").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = ch.get("type").and_then(|v| v.as_str()).unwrap_or("?");
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
            "✔ actif"
        } else {
            "✗ désactivé"
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
}
