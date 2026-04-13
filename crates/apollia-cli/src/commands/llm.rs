//! `apollia-os llm` subcommands — diagnose and test LLM backends via the runtime API.
//!
//! Provides `status`, `ping`, `chat`, `costs` and `backends` (CRUD) operations
//! for LLM backend management.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// LLM subcommands: `apollia-os llm <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmCommand {
    /// Display the status of all configured LLM backends.
    Status,
    /// Measure the latency of a specific LLM backend.
    Ping {
        /// Backend name (default: the router's configured default backend).
        backend: Option<String>,
    },
    /// Send a direct prompt to an LLM backend and print the response.
    Chat {
        /// The prompt text to send to the LLM.
        prompt: String,
        /// Backend to use (optional — uses the configured default if omitted).
        #[arg(long)]
        backend: Option<String>,
    },
    /// Afficher l'utilisation et les coûts agrégés (tokens et coût estimé par backend).
    Costs,
    /// Gérer les backends LLM configurés (list, create, update, delete, set-default).
    Backends {
        /// Sous-commande backends.
        #[command(subcommand)]
        command: LlmBackendsCommand,
    },
}

/// Backends CRUD subcommands: `apollia-os llm backends <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmBackendsCommand {
    /// Lister tous les backends LLM configurés.
    List,
    /// Créer un nouveau backend LLM.
    Create {
        /// Nom unique du backend.
        name: String,
        /// Type de backend (anthropic, openai, ollama, local, vertex, bedrock).
        #[arg(long)]
        kind: String,
        /// Identifiant ou chemin du modèle.
        #[arg(long)]
        model: String,
        /// Clé API (pour les backends cloud).
        #[arg(long)]
        api_key: Option<String>,
        /// URL de base optionnelle.
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Mettre à jour un backend LLM existant.
    Update {
        /// Nom du backend à modifier.
        name: String,
        /// Nouveau modèle (optionnel).
        #[arg(long)]
        model: Option<String>,
        /// Nouvelle clé API (optionnel).
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Supprimer un backend LLM.
    Delete {
        /// Nom du backend à supprimer.
        name: String,
        /// Confirmer la suppression sans prompt interactif.
        #[arg(long)]
        confirm: bool,
    },
    /// Définir un backend comme backend par défaut.
    SetDefault {
        /// Nom du backend à définir comme défaut.
        name: String,
    },
}

/// Execute a `llm` subcommand.
///
/// Returns the process exit code: `0` = success, non-zero = error.
pub async fn run(cmd: &LlmCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        LlmCommand::Status => run_status(&client, json).await,
        LlmCommand::Ping { backend } => run_ping(&client, backend.as_deref(), json).await,
        LlmCommand::Chat { prompt, backend } => {
            run_chat(&client, prompt, backend.as_deref(), json).await
        }
        LlmCommand::Costs => run_costs(&client, json).await,
        LlmCommand::Backends { command } => run_backends(&client, command, json).await,
    }
}

/// `apollia-os llm status` — display all LLM backends with their current state.
async fn run_status(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/llm/status").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
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
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_llm_status(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os llm ping [backend]` — measure the latency of a backend.
///
/// Returns exit code `0` if the backend is available, `1` otherwise.
async fn run_ping(client: &RuntimeClient, backend: Option<&str>, json: bool) -> i32 {
    let body = serde_json::json!({ "backend": backend });
    let resp = match client.post("/api/v1/llm/ping", Some(&body)).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_ping_result(&parsed);
    }

    let available = parsed
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// `apollia-os llm chat "prompt"` — send a prompt to an LLM backend.
async fn run_chat(client: &RuntimeClient, prompt: &str, backend: Option<&str>, json: bool) -> i32 {
    let body = serde_json::json!({ "prompt": prompt, "backend": backend });
    let resp = match client.post("/api/v1/llm/chat", Some(&body)).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
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
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let content = parsed.get("content").and_then(|v| v.as_str()).unwrap_or("");
        println!("{content}");
    }
    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────

/// Render `GET /api/v1/llm/status` response as a human-readable table.
fn format_llm_status(resp: &serde_json::Value) {
    let backends = resp
        .get("backends")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<24} {:<32} STATUS", "BACKEND", "MODEL");
    if backends.is_empty() {
        println!("  (no LLM backends configured)");
    } else {
        for b in &backends {
            let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let model = b.get("model_id").and_then(|v| v.as_str()).unwrap_or("?");
            let available = b
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let status = if available { "ready" } else { "unavailable" };
            println!("  {name:<24} {model:<32} {status}");
        }
    }
}

/// Render `POST /api/v1/llm/ping` response as a human-readable line.
fn format_ping_result(resp: &serde_json::Value) {
    let backend = resp.get("backend").and_then(|v| v.as_str()).unwrap_or("?");
    let available = resp
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        let latency = resp.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("{backend}: OK ({latency}ms)");
    } else {
        let error = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        println!("{backend}: UNAVAILABLE ({error})");
    }
}

// ─────────────────────────────────────────────
// Error helpers
// ─────────────────────────────────────────────

/// Handle client-level errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
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

/// Handle HTTP server errors uniformly.
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

// ─────────────────────────────────────────────
// Costs handler
// ─────────────────────────────────────────────

/// `apollia-os llm costs` — afficher l'utilisation et les coûts agrégés par backend.
async fn run_costs(client: &RuntimeClient, json: bool) -> i32 {
    match client.get_llm_costs().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_llm_costs(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Render `GET /api/v1/llm/costs` as a human-readable table.
fn format_llm_costs(resp: &serde_json::Value) {
    let backends = resp
        .get("backends")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<24} {:>12} {:>12} {:>12}",
        "BACKEND", "IN TOKENS", "OUT TOKENS", "COST ($)"
    );

    if backends.is_empty() {
        println!("  (no usage recorded)");
    } else {
        for b in &backends {
            let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let input_tokens = b.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output_tokens = b.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cost = b
                .get("estimated_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            println!(
                "  {:<24} {:>12} {:>12} {:>12.4}",
                name, input_tokens, output_tokens, cost
            );
        }
    }

    if let Some(total) = resp.get("total") {
        let total_cost = total
            .get("estimated_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        println!(
            "  {:<24} {:>12} {:>12} {:>12.4}",
            "TOTAL", "", "", total_cost
        );
    }
}

// ─────────────────────────────────────────────
// Backends CRUD handlers
// ─────────────────────────────────────────────

/// Route `apollia-os llm backends <verb>` to the appropriate handler.
async fn run_backends(client: &RuntimeClient, command: &LlmBackendsCommand, json: bool) -> i32 {
    match command {
        LlmBackendsCommand::List => run_backends_list(client, json).await,
        LlmBackendsCommand::Create {
            name,
            kind,
            model,
            api_key,
            base_url,
        } => {
            run_backends_create(
                client,
                name,
                kind,
                model,
                api_key.as_deref(),
                base_url.as_deref(),
                json,
            )
            .await
        }
        LlmBackendsCommand::Update {
            name,
            model,
            api_key,
        } => run_backends_update(client, name, model.as_deref(), api_key.as_deref(), json).await,
        LlmBackendsCommand::Delete { name, confirm } => {
            run_backends_delete(client, name, *confirm, json).await
        }
        LlmBackendsCommand::SetDefault { name } => {
            run_backends_set_default(client, name, json).await
        }
    }
}

/// `apollia-os llm backends list` — lister tous les backends configurés.
async fn run_backends_list(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_llm_backends().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_llm_status(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends create` — créer un nouveau backend.
async fn run_backends_create(
    client: &RuntimeClient,
    name: &str,
    kind: &str,
    model: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({
        "name": name,
        "kind": kind,
        "model": model,
    });
    if let Some(k) = api_key {
        body["api_key"] = serde_json::Value::String(k.to_string());
    }
    if let Some(u) = base_url {
        body["base_url"] = serde_json::Value::String(u.to_string());
    }

    match client.create_llm_backend(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Backend '{name}' créé (type: {kind}, modèle: {model})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends update` — mettre à jour un backend existant.
async fn run_backends_update(
    client: &RuntimeClient,
    name: &str,
    model: Option<&str>,
    api_key: Option<&str>,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({});
    if let Some(m) = model {
        body["model"] = serde_json::Value::String(m.to_string());
    }
    if let Some(k) = api_key {
        body["api_key"] = serde_json::Value::String(k.to_string());
    }

    match client.update_llm_backend(name, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Backend '{name}' mis à jour");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends delete` — supprimer un backend.
async fn run_backends_delete(client: &RuntimeClient, name: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Utiliser --confirm pour supprimer le backend '{name}' sans confirmation.");
        }
        return exit_codes::GENERAL_ERROR;
    }

    match client.delete_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Backend '{name}' supprimé");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends set-default` — définir le backend par défaut.
async fn run_backends_set_default(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.set_default_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Backend '{name}' défini comme backend par défaut");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: LlmCommand,
    }

    #[test]
    fn test_llm_status_parses() {
        // GIVEN "status"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "status"]);
        // THEN LlmCommand::Status
        assert!(matches!(cli.command, LlmCommand::Status));
    }

    #[test]
    fn test_llm_ping_no_backend_parses() {
        // GIVEN "ping" sans argument
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "ping"]);
        // THEN LlmCommand::Ping { backend: None }
        match &cli.command {
            LlmCommand::Ping { backend } => assert!(backend.is_none()),
            other => panic!("expected Ping, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_ping_with_backend_parses() {
        // GIVEN "ping anthropic"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "ping", "anthropic"]);
        // THEN LlmCommand::Ping { backend: Some("anthropic") }
        match &cli.command {
            LlmCommand::Ping { backend } => assert_eq!(backend.as_deref(), Some("anthropic")),
            other => panic!("expected Ping, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_parses() {
        // GIVEN "costs"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "costs"]);
        // THEN LlmCommand::Costs
        assert!(matches!(cli.command, LlmCommand::Costs));
    }

    #[test]
    fn test_llm_backends_list_parses() {
        // GIVEN "backends list"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "backends", "list"]);
        // THEN LlmCommand::Backends { command: LlmBackendsCommand::List }
        match &cli.command {
            LlmCommand::Backends { command } => {
                assert!(matches!(command, LlmBackendsCommand::List))
            }
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_create_parses() {
        // GIVEN "backends create anthropic --kind anthropic --model claude-3-5-sonnet-20241022"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "create",
            "anthropic",
            "--kind",
            "anthropic",
            "--model",
            "claude-3-5-sonnet-20241022",
        ]);
        // THEN Create avec les bons champs
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Create {
                    name,
                    kind,
                    model,
                    api_key,
                    base_url,
                } => {
                    assert_eq!(name, "anthropic");
                    assert_eq!(kind, "anthropic");
                    assert_eq!(model, "claude-3-5-sonnet-20241022");
                    assert!(api_key.is_none());
                    assert!(base_url.is_none());
                }
                other => panic!("expected Create, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_create_with_api_key_parses() {
        // GIVEN "backends create openai --kind openai --model gpt-4o --api-key sk-test"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "create",
            "openai",
            "--kind",
            "openai",
            "--model",
            "gpt-4o",
            "--api-key",
            "sk-test",
        ]);
        // THEN api_key = Some("sk-test")
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Create { api_key, .. } => {
                    assert_eq!(api_key.as_deref(), Some("sk-test"))
                }
                other => panic!("expected Create, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_update_parses() {
        // GIVEN "backends update anthropic --model claude-3-opus-20240229"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "update",
            "anthropic",
            "--model",
            "claude-3-opus-20240229",
        ]);
        // THEN Update { name: "anthropic", model: Some("claude-3-opus-20240229"), api_key: None }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Update {
                    name,
                    model,
                    api_key,
                } => {
                    assert_eq!(name, "anthropic");
                    assert_eq!(model.as_deref(), Some("claude-3-opus-20240229"));
                    assert!(api_key.is_none());
                }
                other => panic!("expected Update, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_delete_confirm_parses() {
        // GIVEN "backends delete anthropic --confirm"
        // WHEN
        let cli =
            TestCli::parse_from(["apollia-os", "backends", "delete", "anthropic", "--confirm"]);
        // THEN Delete { name: "anthropic", confirm: true }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Delete { name, confirm } => {
                    assert_eq!(name, "anthropic");
                    assert!(confirm);
                }
                other => panic!("expected Delete, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_set_default_parses() {
        // GIVEN "backends set-default anthropic"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "backends", "set-default", "anthropic"]);
        // THEN SetDefault { name: "anthropic" }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::SetDefault { name } => assert_eq!(name, "anthropic"),
                other => panic!("expected SetDefault, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }
}
