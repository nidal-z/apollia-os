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
    /// Recharger le router LLM depuis `system.db` sans redémarrer le runtime.
    ///
    /// Les mutations `backends create/update/delete/set-default` écrivent en
    /// base mais le router en mémoire reste figé jusqu'à un reload. Cette
    /// commande swap le router actif sans interrompre les tâches en cours.
    Reload,
}

/// Backends CRUD subcommands: `apollia-os llm backends <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmBackendsCommand {
    /// Lister tous les backends LLM configurés.
    List,
    /// Afficher la configuration complète d'un backend (config_json inclus).
    Show {
        /// Nom du backend.
        name: String,
    },
    /// Créer un nouveau backend LLM.
    ///
    /// Le `--provider` détermine la structure du `config_json` envoyée au
    /// runtime. Pour `llama-cpp` (modèles locaux GGUF), `--model` doit être
    /// un chemin absolu vers le fichier .gguf. Pour les providers cloud,
    /// `--model` est l'identifiant (ex: `claude-sonnet-4-6`, `gpt-4o`).
    Create {
        /// Nom unique du backend (snake_case ou kebab-case).
        name: String,
        /// Provider: `llama-cpp` (local GGUF), `anthropic`, `openai`,
        /// `mistral`, `ollama`. Alias `--kind` accepté pour rétrocompat.
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: String,
        /// Identifiant ou chemin du modèle (chemin absolu pour `llama-cpp`).
        #[arg(long)]
        model: String,
        /// Clé API (providers cloud uniquement). Stockée telle quelle dans
        /// `config_json.api_key`. Préférer `--api-key-env VAR_NAME` pour
        /// éviter de coucher la clé dans system.db.
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// Nom de la variable d'environnement contenant la clé API.
        ///
        /// Le runtime lit `std::env::var(NAME)` au boot. Recommandé pour
        /// éviter de persister la clé dans system.db.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// URL de base (Ollama, OpenAI-compatible self-hosted, etc.).
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// Device pour les modèles `llama-cpp`: `metal` (Apple), `cuda`, `cpu`.
        #[arg(long, value_name = "DEVICE", default_value = "metal")]
        device: String,
        /// Timeout d'inférence en secondes (défaut: 60).
        #[arg(long, value_name = "SECS", default_value = "60")]
        timeout_sec: u64,
        /// Désactiver le backend après création.
        #[arg(long)]
        disabled: bool,
        /// Marquer ce backend comme défaut (un seul à la fois).
        #[arg(long, alias = "is-default")]
        default: bool,
    },
    /// Mettre à jour un backend LLM existant.
    ///
    /// Fonctionne en mode merge: les flags non spécifiés conservent leur
    /// valeur actuelle. Le runtime expose `PUT` en mode replace, donc le
    /// CLI lit d'abord la config existante et n'écrase que les champs
    /// demandés.
    Update {
        /// Nom du backend à modifier.
        name: String,
        /// Nouveau provider (rarement utile, change l'implémentation backend).
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: Option<String>,
        /// Nouveau modèle (chemin absolu pour `llama-cpp`).
        #[arg(long)]
        model: Option<String>,
        /// Nouvelle clé API (cloud providers).
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// Nouvelle variable d'environnement pour la clé API.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// Nouvelle URL de base.
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// Nouveau device pour `llama-cpp`.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
        /// Nouveau timeout d'inférence en secondes.
        #[arg(long, value_name = "SECS")]
        timeout_sec: Option<u64>,
        /// Activer le backend.
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Désactiver le backend.
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        /// Marquer comme défaut.
        #[arg(long, alias = "is-default")]
        default: bool,
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
        LlmCommand::Reload => run_reload(&client, json).await,
    }
}

/// `apollia-os llm reload` — rebuild the in-memory router from `system.db`.
///
/// The mutating sub-commands (`create`, `update`, `delete`, `set-default`)
/// invoke this automatically. The standalone command is useful when the
/// operator edited `system.db` directly, restored a backup, or wants to
/// retry after a transient model load failure.
async fn run_reload(client: &RuntimeClient, json: bool) -> i32 {
    match client.reload_llm_router().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let default = resp.get("default").and_then(|v| v.as_str()).unwrap_or("");
            let count = resp
                .get("backends")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("OK LLM router reloaded ({count} backend(s) active, default: {default})");
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"error": "runtime not started"})
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
                eprintln!("Hint: run `apollia-os start` first.");
            }
            exit_codes::RUNTIME_ERROR
        }
        Err(ClientError::ServerError { status, body }) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"error": body, "status": status})
                );
            } else {
                eprintln!("Error ({status}): {body}");
                if status == 503 {
                    eprintln!("Hint: configure at least one backend with");
                    eprintln!("      `apollia-os llm backends create ... --default`");
                }
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
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
        LlmBackendsCommand::Show { name } => run_backends_show(client, name, json).await,
        LlmBackendsCommand::Create {
            name,
            provider,
            model,
            api_key,
            api_key_env,
            base_url,
            device,
            timeout_sec,
            disabled,
            default,
        } => {
            run_backends_create(
                client,
                name,
                provider,
                model,
                api_key.as_deref(),
                api_key_env.as_deref(),
                base_url.as_deref(),
                device,
                *timeout_sec,
                !*disabled,
                *default,
                json,
            )
            .await
        }
        LlmBackendsCommand::Update {
            name,
            provider,
            model,
            api_key,
            api_key_env,
            base_url,
            device,
            timeout_sec,
            enable,
            disable,
            default,
        } => {
            let enabled = if *enable {
                Some(true)
            } else if *disable {
                Some(false)
            } else {
                None
            };
            let is_default = if *default { Some(true) } else { None };
            run_backends_update(
                client,
                name,
                provider.as_deref(),
                model.as_deref(),
                api_key.as_deref(),
                api_key_env.as_deref(),
                base_url.as_deref(),
                device.as_deref(),
                *timeout_sec,
                enabled,
                is_default,
                json,
            )
            .await
        }
        LlmBackendsCommand::Delete { name, confirm } => {
            run_backends_delete(client, name, *confirm, json).await
        }
        LlmBackendsCommand::SetDefault { name } => {
            run_backends_set_default(client, name, json).await
        }
    }
}

/// Map a CLI-friendly provider name to the runtime's canonical identifier.
///
/// The runtime accepts: `llama-cpp`, `openai`, `mistral`, `anthropic`, `ollama`.
/// `local` is a common shortcut for `llama-cpp`. Unknown providers are
/// passed through unchanged so the runtime can return its own validation
/// error with the full list of valid values.
fn canonicalize_provider(p: &str) -> &str {
    match p {
        "local" | "llama" | "llama_cpp" | "llamacpp" => "llama-cpp",
        other => other,
    }
}

/// Build the `config_json` object that the runtime stores alongside the
/// backend row. Shape varies per provider:
///
/// - `llama-cpp`: `{model_path, device, quantization?, timeout_sec}`
/// - `anthropic` / `openai` / `mistral`: `{api_key?, api_key_env?, base_url?, timeout_sec}`
/// - `ollama`: `{base_url, timeout_sec}`
fn build_config_json(
    provider: &str,
    model: &str,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
    device: &str,
    timeout_sec: u64,
) -> serde_json::Value {
    let mut cfg = serde_json::Map::new();
    cfg.insert("timeout_sec".into(), serde_json::Value::from(timeout_sec));

    match provider {
        "llama-cpp" => {
            cfg.insert(
                "model_path".into(),
                serde_json::Value::String(model.to_string()),
            );
            cfg.insert(
                "device".into(),
                serde_json::Value::String(device.to_string()),
            );
        }
        "ollama" => {
            cfg.insert(
                "base_url".into(),
                serde_json::Value::String(base_url.unwrap_or("http://localhost:11434").to_string()),
            );
        }
        _ => {
            // Cloud providers: API key + optional base_url for self-hosted gateways.
            if let Some(k) = api_key {
                cfg.insert("api_key".into(), serde_json::Value::String(k.to_string()));
            }
            if let Some(v) = api_key_env {
                cfg.insert(
                    "api_key_env".into(),
                    serde_json::Value::String(v.to_string()),
                );
            }
            if let Some(u) = base_url {
                cfg.insert("base_url".into(), serde_json::Value::String(u.to_string()));
            }
        }
    }
    serde_json::Value::Object(cfg)
}

/// Trigger an in-place LlmRouter reload via `POST /api/v1/llm/reload` so the
/// runtime picks up the freshly mutated `system.db` without restarting.
///
/// Mutations (`create`, `update`, `delete`, `set-default`) call this after a
/// successful write. Reload failures are reported but do not change the exit
/// code: the database mutation succeeded; only the in-memory swap is missing.
/// Callers can re-run `apollia-os llm reload` to retry, or restart the daemon.
async fn auto_reload_after_mutation(client: &RuntimeClient, json: bool) {
    match client.reload_llm_router().await {
        Ok(resp) => {
            if !json {
                let default = resp.get("default").and_then(|v| v.as_str()).unwrap_or("");
                let count = resp
                    .get("backends")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!(
                    "    router reloaded in place ({count} backend(s) active, default: {default})"
                );
            }
        }
        Err(ClientError::ServerError { status, body }) => {
            if !json {
                eprintln!("    warning: router reload failed ({status}): {body}");
                eprintln!("    the mutation is persisted; run `apollia-os llm reload` to retry");
            }
        }
        Err(e) => {
            if !json {
                eprintln!("    warning: router reload failed: {e}");
                eprintln!("    the mutation is persisted; run `apollia-os llm reload` to retry");
            }
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

/// `apollia-os llm backends show <name>` — display the full configuration
/// of a backend, including the provider-specific `config_json` blob.
async fn run_backends_show(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.get_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let provider = resp.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
            let model = resp.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            let enabled = resp
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let is_default = resp
                .get("is_default")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            println!("Backend     : {name}");
            println!("Provider    : {provider}");
            println!("Model       : {model}");
            println!(
                "State       : {}{}",
                if enabled { "enabled" } else { "disabled" },
                if is_default { " (default)" } else { "" }
            );
            if let Some(cfg) = resp.get("config_json") {
                println!("Config      :");
                let rendered = serde_json::to_string_pretty(cfg).unwrap_or_default();
                for line in rendered.lines() {
                    println!("  {line}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' introuvable");
                eprintln!("Hint: run `apollia-os llm backends list` to see existing backends.");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends create` — créer un nouveau backend LLM.
///
/// Construit la payload complète attendue par `POST /api/v1/llm/backends`
/// (provider canonique + config_json provider-specific) au lieu d'envoyer
/// les seuls champs `kind/model` que le runtime rejette en 400.
#[allow(clippy::too_many_arguments)]
async fn run_backends_create(
    client: &RuntimeClient,
    name: &str,
    provider: &str,
    model: &str,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
    device: &str,
    timeout_sec: u64,
    enabled: bool,
    is_default: bool,
    json: bool,
) -> i32 {
    let canonical = canonicalize_provider(provider);
    let config_json = build_config_json(
        canonical,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
    );

    let body = serde_json::json!({
        "name": name,
        "provider": canonical,
        "model": model,
        "config_json": config_json,
        "enabled": enabled,
        "is_default": is_default,
    });

    match client.create_llm_backend(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' créé (provider: {canonical}, modèle: {model})");
                if is_default {
                    println!("    marqué comme backend par défaut");
                }
                if !enabled {
                    println!("    désactivé (passer --enable pour activer)");
                }
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            if json {
                let out = serde_json::json!({"error": body, "status": status});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error ({status}): {body}");
                if status == 422 {
                    eprintln!();
                    eprintln!(
                        "Hint: providers acceptés: llama-cpp, anthropic, openai, mistral, ollama"
                    );
                    eprintln!("      (alias --kind toujours accepté pour rétrocompat)");
                }
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends update` — mettre à jour un backend existant.
///
/// Le runtime expose `PUT` en mode replace (tous les champs requis). Le CLI
/// lit d'abord l'état courant via `GET /api/v1/llm/backends/:name`, applique
/// les flags fournis en mode merge, puis renvoie la payload complète. Permet
/// `--model X` sans avoir à respécifier provider/config_json/enabled.
#[allow(clippy::too_many_arguments)]
async fn run_backends_update(
    client: &RuntimeClient,
    name: &str,
    provider: Option<&str>,
    model: Option<&str>,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
    device: Option<&str>,
    timeout_sec: Option<u64>,
    enabled: Option<bool>,
    is_default: Option<bool>,
    json: bool,
) -> i32 {
    // Step 1: fetch the current configuration so we can merge.
    let current = match client.get_llm_backend(name).await {
        Ok(v) => v,
        Err(ClientError::ServerError { status: 404, .. }) => {
            if json {
                let out = serde_json::json!({"error": format!("backend '{name}' not found")});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' introuvable");
                eprintln!("Hint: run `apollia-os llm backends list` to see existing backends.");
            }
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => return handle_error(e, json),
    };

    // Step 2: extract current values as the merge baseline.
    let cur_provider = current
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("anthropic");
    let cur_model = current.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let cur_cfg = current
        .get("config_json")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let cur_enabled = current
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let cur_default = current
        .get("is_default")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Step 3: compute the merged values.
    let new_provider_raw = provider.unwrap_or(cur_provider);
    let new_provider = canonicalize_provider(new_provider_raw);
    let new_model = model.unwrap_or(cur_model);
    let new_enabled = enabled.unwrap_or(cur_enabled);
    let new_default = is_default.unwrap_or(cur_default);

    // Step 4: merge config_json — start from the existing object, overlay
    // any field that the user explicitly changed.
    let mut cfg_map = match cur_cfg {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Some(secs) = timeout_sec {
        cfg_map.insert("timeout_sec".into(), serde_json::Value::from(secs));
    }
    if new_provider == "llama-cpp" {
        // For local backends, the model PATH lives in config_json.model_path
        // and must stay in sync with the top-level `model` column.
        if model.is_some() {
            cfg_map.insert(
                "model_path".into(),
                serde_json::Value::String(new_model.to_string()),
            );
        }
        if let Some(d) = device {
            cfg_map.insert("device".into(), serde_json::Value::String(d.to_string()));
        }
    } else {
        if let Some(k) = api_key {
            cfg_map.insert("api_key".into(), serde_json::Value::String(k.to_string()));
        }
        if let Some(v) = api_key_env {
            cfg_map.insert(
                "api_key_env".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(u) = base_url {
            cfg_map.insert("base_url".into(), serde_json::Value::String(u.to_string()));
        }
    }

    let body = serde_json::json!({
        "provider": new_provider,
        "model": new_model,
        "config_json": serde_json::Value::Object(cfg_map),
        "enabled": new_enabled,
        "is_default": new_default,
    });

    match client.update_llm_backend(name, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' mis à jour");
                println!(
                    "    provider={new_provider}, model={new_model}, enabled={new_enabled}, default={new_default}"
                );
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            if json {
                let out = serde_json::json!({"error": body, "status": status});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error ({status}): {body}");
            }
            exit_codes::GENERAL_ERROR
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
                println!("OK backend '{name}' supprimé");
                auto_reload_after_mutation(client, json).await;
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
                println!("OK backend '{name}' défini comme backend par défaut");
                auto_reload_after_mutation(client, json).await;
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
                    provider,
                    model,
                    api_key,
                    base_url,
                    ..
                } => {
                    assert_eq!(name, "anthropic");
                    assert_eq!(provider, "anthropic");
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
                    ..
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
