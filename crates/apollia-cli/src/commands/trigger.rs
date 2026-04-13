//! `apollia-os trigger` subcommands — trigger management.
//!
//! Fournit les sous-commandes `list`, `status`, `fire`, `enable`, `disable`,
//! `logs` et `reload` pour gérer, déboguer et auditer les déclenchements
//! automatiques d'agents depuis le terminal sans modifier `apollia.toml`.
//!
//! Pattern noun-verb cohérent avec `agent` et `task`.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

// ─── Subcommands ──────────────────────────────────────────────────────────

/// Trigger subcommands : `apollia-os trigger <verb>`.
#[derive(Debug, Subcommand)]
pub enum TriggerCommand {
    /// Lister tous les triggers avec leur état.
    List,
    /// Afficher le statut détaillé d'un trigger.
    Status {
        /// Identifiant du trigger.
        id: String,
    },
    /// Déclencher immédiatement un trigger (debug/test).
    Fire {
        /// Identifiant du trigger.
        id: String,
    },
    /// Activer un trigger désactivé.
    Enable {
        /// Identifiant du trigger.
        id: String,
    },
    /// Désactiver un trigger sans modifier apollia.toml.
    Disable {
        /// Identifiant du trigger.
        id: String,
    },
    /// Afficher l'historique des déclenchements depuis SQLite.
    Logs {
        /// Identifiant du trigger.
        id: String,
        /// Nombre maximum d'entrées à afficher.
        #[arg(long, default_value = "20")]
        last: usize,
    },
    /// Recharger la config triggers depuis apollia.toml (hot reload).
    ///
    /// Rereads `[[triggers]]` from `apollia.toml`, validates the new definitions,
    /// and restarts modified sources. Invalid TOML or invalid trigger configuration
    /// returns an error without interrupting the currently-running triggers.
    Reload,
    /// Créer un nouveau trigger (CRUD — complète le hot-reload via apollia.toml).
    Create {
        /// Identifiant unique du trigger.
        id: String,
        /// Agent cible.
        #[arg(long)]
        agent: String,
        /// Type : cron, interval, filewatch, webhook.
        #[arg(long, value_name = "TYPE")]
        kind: String,
        /// Détail du trigger (expression cron, intervalle, chemin, etc.).
        #[arg(long)]
        detail: Option<String>,
        /// Politique si l'agent est occupé (skip, queue, preempt).
        #[arg(long, default_value = "skip")]
        on_busy: String,
        /// Payload d'entrée envoyé à l'agent lors du déclenchement.
        #[arg(long)]
        input: Option<String>,
    },
    /// Mettre à jour un trigger existant.
    Update {
        /// Identifiant du trigger.
        id: String,
        /// Nouveau détail (expression cron, intervalle, etc.).
        #[arg(long)]
        detail: Option<String>,
        /// Nouvelle politique on-busy.
        #[arg(long)]
        on_busy: Option<String>,
        /// Nouveau payload d'entrée.
        #[arg(long)]
        input: Option<String>,
    },
    /// Supprimer un trigger.
    Delete {
        /// Identifiant du trigger.
        id: String,
        /// Confirmer sans prompt interactif.
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
                id,
                agent,
                kind,
                detail.as_deref(),
                on_busy,
                input.as_deref(),
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
                id,
                detail.as_deref(),
                on_busy.as_deref(),
                input.as_deref(),
                json,
            )
            .await
        }
        TriggerCommand::Delete { id, confirm } => run_delete(&client, id, *confirm, json).await,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────

/// `apollia-os trigger list` — liste de tous les triggers.
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

/// `apollia-os trigger status <id>` — statut détaillé d'un trigger.
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

/// `apollia-os trigger fire <id>` — déclenchement immédiat.
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
                println!("✔ Trigger '{id}' déclenché → task {task_id}");
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

/// `apollia-os trigger enable <id>` — activer un trigger.
async fn run_enable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.enable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' activé");
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

/// `apollia-os trigger disable <id>` — désactiver un trigger.
async fn run_disable(client: &RuntimeClient, id: &str, json: bool) -> i32 {
    match client.disable_trigger(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' désactivé");
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

/// `apollia-os trigger logs <id> [--last N]` — historique des déclenchements.
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

/// `apollia-os trigger reload` — hot reload des triggers depuis `apollia.toml`.
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
                println!("✔ Triggers rechargés — {count} actif(s)");
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
                eprintln!("Erreur reload triggers ({status}): {body}");
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
        let last = t.get("last_fired").and_then(|v| v.as_str()).unwrap_or("—");
        println!(
            "  {:<24} {:<20} {:<12} {:<8} {:<6} {:<6} {}",
            id, agent, kind, enabled, fires, skips, last
        );
    }
}

/// Format trigger detail as human-readable key-value pairs.
fn format_trigger_detail(resp: &serde_json::Value) {
    let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let agent = resp.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = resp
        .get("source_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let detail = resp
        .get("source_detail")
        .and_then(|v| v.as_str())
        .unwrap_or("");
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

/// Format trigger logs as human-readable rows.
///
/// Format: `date  status  task_id|—  reason|—`
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
        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("—");
        let reason = entry.get("reason").and_then(|v| v.as_str()).unwrap_or("—");
        println!("  {date_display}  {status:<8}  {task_id:<36}  {reason}");
    }
}

/// `apollia-os trigger create <id> --agent <agent> --kind <kind> [options]`
///
/// Crée un nouveau trigger via `POST /api/v1/triggers`.
async fn run_create(
    client: &RuntimeClient,
    id: &str,
    agent: &str,
    kind: &str,
    detail: Option<&str>,
    on_busy: &str,
    input: Option<&str>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({
        "id": id,
        "agent": agent,
        "kind": kind,
        "on_busy": on_busy,
    });
    if let Some(d) = detail {
        body["detail"] = serde_json::Value::String(d.to_string());
    }
    if let Some(i) = input {
        body["input"] = serde_json::Value::String(i.to_string());
    }

    match client.create_trigger(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' créé ({kind} → {agent})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os trigger update <id> [options]`
///
/// Met à jour un trigger existant via `PUT /api/v1/triggers/{id}`.
async fn run_update(
    client: &RuntimeClient,
    id: &str,
    detail: Option<&str>,
    on_busy: Option<&str>,
    input: Option<&str>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({});
    if let Some(d) = detail {
        body["detail"] = serde_json::Value::String(d.to_string());
    }
    if let Some(ob) = on_busy {
        body["on_busy"] = serde_json::Value::String(ob.to_string());
    }
    if let Some(i) = input {
        body["input"] = serde_json::Value::String(i.to_string());
    }

    match client.update_trigger(id, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Trigger '{id}' mis à jour");
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

/// `apollia-os trigger delete <id> [--confirm]`
///
/// Supprime un trigger via `DELETE /api/v1/triggers/{id}`.
async fn run_delete(client: &RuntimeClient, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Utiliser --confirm pour supprimer le trigger '{id}' sans confirmation.");
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
                println!("✔ Trigger '{id}' supprimé");
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

/// Gestion uniforme des erreurs client.
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

    /// CLI minimal pour tester le parsing des sous-commandes trigger.
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
        // THEN TriggerCommand::Create avec les bons champs
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
                assert_eq!(on_busy, "skip");
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
        // GIVEN "delete rapport-hebdo" sans --confirm
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "rapport-hebdo"]);
        // THEN confirm = false
        match &cli.command {
            super::TriggerCommand::Delete { confirm, .. } => assert!(!confirm),
            other => panic!("expected Delete, got {other:?}"),
        }
    }
}
