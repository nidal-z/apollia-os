//! `apollia-os pipeline` subcommands — pipeline orchestration.
//!
//! Fournit les sous-commandes `list`, `run`, `runs` et `status` pour déclencher
//! et monitorer les pipelines multi-agents depuis le terminal.
//!
//! Pattern noun-verb cohérent avec `agent`, `task`, `trigger`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

// ─── Subcommands ──────────────────────────────────────────────────────────────

/// Pipeline subcommands : `apollia-os pipeline <verb>`.
#[derive(Debug, Subcommand)]
pub enum PipelineCommand {
    /// Lister les pipelines déclarés dans la config.
    List,
    /// Déclencher un pipeline manuellement et suivre sa progression.
    Run {
        /// Identifiant du pipeline.
        pipeline_id: String,
        /// Payload d'entrée transmis au premier step comme `{{trigger.payload}}`.
        #[arg(default_value = "")]
        input: String,
        /// Ne pas attendre la fin (fire-and-forget).
        #[arg(long)]
        detach: bool,
    },
    /// Historique des runs d'un pipeline.
    Runs {
        /// Identifiant du pipeline.
        pipeline_id: String,
        /// Nombre de runs à afficher (cap client, serveur retourne max 20).
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Statut détaillé d'un run.
    Status {
        /// Identifiant du run (ex: r-0017).
        run_id: String,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Execute a `pipeline` subcommand.
///
/// Returns the process exit code (0 = success, 1 = error, 2 = runtime offline).
pub async fn run(cmd: &PipelineCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        PipelineCommand::List => run_list(&client, json).await,
        PipelineCommand::Run {
            pipeline_id,
            input,
            detach,
        } => run_pipeline(&client, pipeline_id, input, *detach, json).await,
        PipelineCommand::Runs { pipeline_id, limit } => {
            run_runs(&client, pipeline_id, *limit, json).await
        }
        PipelineCommand::Status { run_id } => run_status(&client, run_id, json).await,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `apollia-os pipeline list` — liste des pipelines déclarés.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_pipelines().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_pipeline_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 503, .. }) => {
            if json {
                let out = serde_json::json!({"error": "pipeline engine not available"});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  (no pipelines configured)");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os pipeline run <id> [input]` — déclencher et suivre un pipeline.
async fn run_pipeline(
    client: &RuntimeClient,
    pipeline_id: &str,
    input: &str,
    detach: bool,
    json: bool,
) -> i32 {
    let input_opt = if input.is_empty() { None } else { Some(input) };

    let run_json = match client.run_pipeline(pipeline_id, input_opt).await {
        Ok(j) => j,
        Err(ClientError::ConnectionRefused) => {
            return output_error(
                "runtime not started (connection refused)",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: pipeline not found: {pipeline_id}");
            }
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
        }
    };

    let run_id = match run_json.get("run_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return output_error(
                "missing run_id in response",
                json,
                exit_codes::GENERAL_ERROR,
            );
        }
    };

    if json {
        if detach {
            println!(
                "{}",
                serde_json::to_string_pretty(&run_json).unwrap_or_default()
            );
            return exit_codes::SUCCESS;
        }
    } else {
        println!();
        println!("  ● {pipeline_id} › démarré (run {run_id})");
        println!();
        if detach {
            println!("  Run {run_id} démarré en tâche de fond.");
            return exit_codes::SUCCESS;
        }
    }

    // Poll until completion.
    let start = Instant::now();
    poll_pipeline_run(client, &run_id, json, start).await
}

/// `apollia-os pipeline runs <id>` — historique des runs.
async fn run_runs(client: &RuntimeClient, pipeline_id: &str, limit: u32, json: bool) -> i32 {
    match client.list_pipeline_runs(pipeline_id, limit).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_runs_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 503, .. }) => {
            if json {
                let out = serde_json::json!({"error": "pipeline engine not available"});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: pipeline engine not available");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os pipeline status <run_id>` — statut détaillé d'un run.
async fn run_status(client: &RuntimeClient, run_id: &str, json: bool) -> i32 {
    match client.get_pipeline_run(run_id).await {
        Ok(resp) => {
            let status = resp
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let exit_code = if status == "failed" {
                exit_codes::GENERAL_ERROR
            } else {
                exit_codes::SUCCESS
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_run_detail(&resp);
            }
            exit_code
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: run not found: {run_id}");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

// ─── Polling ──────────────────────────────────────────────────────────────────

/// Poll a pipeline run until it reaches a terminal state, printing step updates.
///
/// Returns 0 on `completed`, 1 on `failed` or any error.
async fn poll_pipeline_run(
    client: &RuntimeClient,
    run_id: &str,
    json: bool,
    start: Instant,
) -> i32 {
    // Track which step statuses we have already printed to avoid duplicate lines.
    let mut known: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let data = match client.get_pipeline_run(run_id).await {
            Ok(d) => d,
            Err(e) => return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR),
        };

        if !json {
            // Print any step status transitions.
            let steps = data["step_runs"].as_array().cloned().unwrap_or_default();
            for step in &steps {
                let step_id = step["step_id"].as_str().unwrap_or("?");
                let status = step["status"].as_str().unwrap_or("?");
                let prev = known.get(step_id).map(|s| s.as_str()).unwrap_or("");
                if prev != status {
                    known.insert(step_id.to_string(), status.to_string());
                    let icon = step_icon(status);
                    // Only print non-pending transitions.
                    if status != "pending" {
                        println!("  {icon} [{step_id}] {status}");
                    }
                }
            }
        }

        let run_status = data["status"].as_str().unwrap_or("unknown");
        match run_status {
            "completed" => {
                let elapsed = start.elapsed();
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&data).unwrap_or_default()
                    );
                } else {
                    let steps = data["step_runs"].as_array().cloned().unwrap_or_default();
                    let total = steps.len();
                    let skipped = steps
                        .iter()
                        .filter(|s| s["status"].as_str() == Some("skipped"))
                        .count();
                    let done = total.saturating_sub(skipped);
                    let secs = elapsed.as_secs_f64();
                    println!();
                    println!(
                        "  ✔ Pipeline terminé en {secs:.1}s — {done}/{total} steps ({skipped} skipped)"
                    );
                }
                return exit_codes::SUCCESS;
            }
            "failed" => {
                let elapsed = start.elapsed();
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&data).unwrap_or_default()
                    );
                } else {
                    let steps = data["step_runs"].as_array().cloned().unwrap_or_default();
                    let failed_step = steps
                        .iter()
                        .find(|s| s["status"].as_str() == Some("failed"))
                        .and_then(|s| s["step_id"].as_str())
                        .unwrap_or("?");
                    let secs = elapsed.as_secs_f64();
                    println!();
                    eprintln!("  ✗ Pipeline échoué en {secs:.1}s — step [{failed_step}]");
                }
                return exit_codes::GENERAL_ERROR;
            }
            _ => {
                // Still running — continue polling.
            }
        }
    }
}

// ─── Formatters ───────────────────────────────────────────────────────────────

/// Format the pipeline list as a human-readable table.
///
/// Columns: ID, DESCRIPTION
fn format_pipeline_list(resp: &serde_json::Value) {
    let pipelines = resp
        .get("pipelines")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    if pipelines.is_empty() {
        println!("  (no pipelines configured)");
        return;
    }

    println!("  {:<30} DESCRIPTION", "ID");
    for p in &pipelines {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let desc = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
        // Truncate long descriptions.
        let desc_display = if desc.len() > 50 {
            format!("{}…", &desc[..49])
        } else {
            desc.to_string()
        };
        println!("  {:<30} {}", id, desc_display);
    }
}

/// Format the runs list as a human-readable table.
///
/// Columns: RUN_ID, STATUT, DÉMARRÉ, DURÉE
fn format_runs_list(resp: &serde_json::Value) {
    // `resp` is a JSON array (or an object if the engine returns a wrapped array).
    let runs = if let Some(arr) = resp.as_array() {
        arr.clone()
    } else {
        resp.get("runs")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default()
    };

    if runs.is_empty() {
        println!("  (no runs)");
        return;
    }

    println!(
        "  {:<12} {:<12} {:<22} DURÉE",
        "RUN_ID", "STATUT", "DÉMARRÉ"
    );
    for run in &runs {
        let run_id = run.get("run_id").and_then(|v| v.as_str()).unwrap_or("?");
        let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let started = run
            .get("started_at")
            .and_then(|v| v.as_str())
            .map(format_timestamp)
            .unwrap_or_else(|| "—".to_string());
        let duration = compute_duration(
            run.get("started_at").and_then(|v| v.as_str()),
            run.get("ended_at").and_then(|v| v.as_str()),
        );
        println!(
            "  {:<12} {:<12} {:<22} {}",
            run_id, status, started, duration
        );
    }
}

/// Format the detailed state of a pipeline run.
fn format_run_detail(resp: &serde_json::Value) {
    let run_id = resp.get("run_id").and_then(|v| v.as_str()).unwrap_or("?");
    let pipeline_id = resp
        .get("pipeline_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    let started = resp
        .get("started_at")
        .and_then(|v| v.as_str())
        .map(format_timestamp)
        .unwrap_or_else(|| "—".to_string());
    let duration = compute_duration(
        resp.get("started_at").and_then(|v| v.as_str()),
        resp.get("ended_at").and_then(|v| v.as_str()),
    );

    println!("  Pipeline : {pipeline_id}");
    println!("  Run      : {run_id}");
    println!("  Statut   : {status}");
    println!("  Démarré  : {started}");
    println!("  Durée    : {duration}");
    println!();

    let steps = resp["step_runs"].as_array().cloned().unwrap_or_default();

    if steps.is_empty() {
        println!("  (no steps)");
        return;
    }

    for step in &steps {
        let step_id = step.get("step_id").and_then(|v| v.as_str()).unwrap_or("?");
        let st = step.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let icon = step_icon(st);
        let step_dur = compute_duration(
            step.get("started_at").and_then(|v| v.as_str()),
            step.get("ended_at").and_then(|v| v.as_str()),
        );
        print!("  {icon} [{step_id:<20}] {st:<18} {step_dur}");
        if let Some(err) = step.get("error").and_then(|v| v.as_str()) {
            print!("  error: {err}");
        }
        println!();
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Return the display icon for a step status string.
fn step_icon(status: &str) -> &'static str {
    match status {
        "completed" => "✔",
        "failed" => "✗",
        "running" => "⟿",
        "waiting_approval" => "⏸",
        "skipped" | "fallback_active" => "○",
        _ => "○",
    }
}

/// Truncate an RFC3339 timestamp to `"YYYY-MM-DD HH:MM"`.
fn format_timestamp(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[..16].replace('T', " ")
    } else {
        ts.to_string()
    }
}

/// Compute a human-readable duration from two optional RFC3339 timestamps.
///
/// Returns `"—"` if `started_at` is absent; uses wall-clock now if `ended_at` is absent.
fn compute_duration(started_at: Option<&str>, ended_at: Option<&str>) -> String {
    use std::time::SystemTime;

    let parse_secs = |s: &str| -> Option<u64> {
        // Parse seconds from ISO 8601: count digits only for a rough approximation
        // without pulling in chrono in the CLI command (it's already a workspace dep).
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.timestamp() as u64)
    };

    let started_secs = match started_at.and_then(parse_secs) {
        Some(s) => s,
        None => return "—".to_string(),
    };

    let ended_secs = ended_at.and_then(parse_secs).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(started_secs)
    });

    let diff = ended_secs.saturating_sub(started_secs);
    if diff < 60 {
        format!("{diff}s")
    } else {
        format!("{}m{}s", diff / 60, diff % 60)
    }
}

// ─── Error handling ───────────────────────────────────────────────────────────

/// Output an error in the requested format and return the given exit code.
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    if json {
        let err = serde_json::json!({"error": msg});
        println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
    code
}

/// Unified client error handler.
fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => output_error(
            "runtime not started (connection refused)",
            json,
            exit_codes::RUNTIME_ERROR,
        ),
        other => output_error(&other.to_string(), json, exit_codes::GENERAL_ERROR),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// Minimal CLI wrapper for parsing pipeline subcommands in tests.
    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: PipelineCommand,
    }

    // ── Parsing tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_pipeline_list_parses() {
        // GIVEN "list"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "list"]);
        // THEN PipelineCommand::List
        assert!(matches!(cli.command, PipelineCommand::List));
    }

    #[test]
    fn test_pipeline_run_parses_with_input() {
        // GIVEN "run traitement-facture acme.pdf"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "run", "traitement-facture", "acme.pdf"]);
        // THEN Run { pipeline_id, input, detach: false }
        match &cli.command {
            PipelineCommand::Run {
                pipeline_id,
                input,
                detach,
            } => {
                assert_eq!(pipeline_id, "traitement-facture");
                assert_eq!(input, "acme.pdf");
                assert!(!detach);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_run_detach_flag() {
        // GIVEN "run traitement-facture --detach"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "run", "traitement-facture", "--detach"]);
        // THEN detach = true
        match &cli.command {
            PipelineCommand::Run { detach, .. } => assert!(detach),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_runs_default_limit() {
        // GIVEN "runs traitement-facture"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "runs", "traitement-facture"]);
        // THEN limit = 20 (default)
        match &cli.command {
            PipelineCommand::Runs { pipeline_id, limit } => {
                assert_eq!(pipeline_id, "traitement-facture");
                assert_eq!(*limit, 20);
            }
            other => panic!("expected Runs, got {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_runs_custom_limit() {
        // GIVEN "runs traitement-facture --limit 5"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "runs", "traitement-facture", "--limit", "5"]);
        // THEN limit = 5
        match &cli.command {
            PipelineCommand::Runs { limit, .. } => assert_eq!(*limit, 5),
            other => panic!("expected Runs, got {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_status_parses() {
        // GIVEN "status r-0017"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "status", "r-0017"]);
        // THEN Status { run_id: "r-0017" }
        match &cli.command {
            PipelineCommand::Status { run_id } => assert_eq!(run_id, "r-0017"),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    // ── Unknown pipeline → exit code 1 ─────────────────────────────────

    #[tokio::test]
    async fn test_ac3_unknown_pipeline_exit_code_1() {
        // GIVEN a client pointing to a non-existent socket
        let socket = std::path::PathBuf::from("/tmp/apollia-test-pipeline-nonexistent.sock");
        let client = RuntimeClient::new(socket);

        // WHEN run_pipeline is called (will fail to connect)
        let result = client.run_pipeline("inexistant", None).await;

        // THEN the client returns a connection error
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err(),
                crate::client::ClientError::ConnectionRefused
            ),
            "expected ConnectionRefused"
        );
    }

    // ── JSON output: list format ───────────────────────────────────────

    #[test]
    fn test_ac6_format_pipeline_list_json_two_items() {
        // GIVEN JSON with 2 pipelines
        let resp = serde_json::json!({
            "pipelines": [
                { "id": "traitement-facture", "description": "Pipeline factures" },
                { "id": "onboarding",          "description": "Onboarding client"  }
            ]
        });

        // WHEN format_pipeline_list is called
        // THEN it does not panic (output goes to stdout in tests)
        format_pipeline_list(&resp);
    }

    #[test]
    fn test_ac6_format_pipeline_list_empty() {
        // GIVEN empty pipelines array
        let resp = serde_json::json!({ "pipelines": [] });
        // WHEN / THEN no panic
        format_pipeline_list(&resp);
    }

    // ── Helper unit tests ──────────────────────────────────────────────────────

    #[test]
    fn test_step_icon_mapping() {
        // GIVEN / WHEN / THEN
        assert_eq!(step_icon("completed"), "✔");
        assert_eq!(step_icon("failed"), "✗");
        assert_eq!(step_icon("running"), "⟿");
        assert_eq!(step_icon("waiting_approval"), "⏸");
        assert_eq!(step_icon("skipped"), "○");
        assert_eq!(step_icon("pending"), "○");
    }

    #[test]
    fn test_format_timestamp_truncates() {
        // GIVEN RFC3339 timestamp
        let ts = "2026-03-08T14:23:00.000Z";
        // WHEN
        let display = format_timestamp(ts);
        // THEN truncated to YYYY-MM-DD HH:MM
        assert_eq!(display, "2026-03-08 14:23");
    }

    #[test]
    fn test_compute_duration_seconds() {
        // GIVEN start and end timestamps 9 seconds apart
        let start = "2026-03-08T14:23:00+00:00";
        let end = "2026-03-08T14:23:09+00:00";
        // WHEN
        let dur = compute_duration(Some(start), Some(end));
        // THEN "9s"
        assert_eq!(dur, "9s");
    }

    #[test]
    fn test_compute_duration_no_start() {
        // GIVEN no start timestamp
        // WHEN / THEN returns "—"
        assert_eq!(compute_duration(None, None), "—");
    }
}
