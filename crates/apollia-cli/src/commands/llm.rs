//! `apollia-os llm` subcommands: diagnose and test LLM backends via the runtime API.
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
        /// Backend to use (optional, uses the configured default if omitted).
        #[arg(long)]
        backend: Option<String>,
    },
    /// Display aggregated usage and costs (tokens and estimated cost per backend).
    ///
    /// Without flags, prints the cost table. Use `--get-threshold` to print
    /// `[llm] cost_alert_threshold_usd` from `apollia.toml`, or
    /// `--threshold N` to set it.
    Costs {
        /// Read the cost alert threshold from `apollia.toml` instead of the
        /// cost table.
        #[arg(long, conflicts_with = "threshold")]
        get_threshold: bool,
        /// Set the cost alert threshold (USD). Writes
        /// `[llm] cost_alert_threshold_usd = N` to `apollia.toml`. Pass `0`
        /// or a negative value to clear the threshold.
        #[arg(long, value_name = "USD")]
        threshold: Option<f64>,
        /// Optional config file path override (default: `~/.apollia/apollia.toml`).
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// Manage configured LLM backends (list, create, update, delete, set-default).
    Backends {
        /// Backends subcommand.
        #[command(subcommand)]
        command: LlmBackendsCommand,
    },
    /// Reload the LLM router from `system.db` without restarting the runtime.
    ///
    /// `backends create/update/delete/set-default` write to the database but
    /// the in-memory router stays frozen until a reload. This command swaps
    /// the active router in place without interrupting running tasks.
    Reload,

    /// First-run helper: configure a local LLM in one step.
    ///
    /// `--local` is the only mode supported today. It expects a `.gguf` model
    /// path, copies it into `~/.apollia/models/` (no copy when already there),
    /// and creates a backend named `local` with `provider=llama-cpp` and
    /// `is_default=true` in `system.db`. The runtime picks the new default
    /// on the next `llm reload` (or daemon restart).
    Setup {
        /// Use the local llama-cpp backend (required for v0.1.0; cloud
        /// providers go through `auth login` + `llm backends create`).
        #[arg(long)]
        local: bool,
        /// Path to the `.gguf` model file.
        #[arg(long, value_name = "PATH")]
        model: PathBuf,
        /// Backend name (default: `local`). Overwrites the existing entry of
        /// the same name.
        #[arg(long, value_name = "NAME", default_value = "local")]
        name: String,
        /// Device hint for llama-cpp: `metal` (macOS default), `cuda`, `cpu`.
        /// When omitted, picks `metal` on macOS and `cpu` elsewhere.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
        /// Override the system database path (default: `~/.apollia/system.db`).
        #[arg(long, value_name = "PATH")]
        system_db: Option<PathBuf>,
        /// Override the models directory (default: `~/.apollia/models/`).
        #[arg(long, value_name = "DIR")]
        models_dir: Option<PathBuf>,
    },
}

/// Backends CRUD subcommands: `apollia-os llm backends <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmBackendsCommand {
    /// List all configured LLM backends.
    List,
    /// Show the full configuration of a backend (including config_json).
    Show {
        /// Backend name.
        name: String,
    },
    /// Create a new LLM backend.
    ///
    /// `--provider` drives the shape of the `config_json` sent to the
    /// runtime. For `llama-cpp` (local GGUF models), `--model` must be the
    /// absolute path to the .gguf file. For cloud providers, `--model` is
    /// the identifier (e.g. `claude-sonnet-4-6`, `gpt-4o`).
    Create {
        /// Unique backend name (snake_case or kebab-case).
        name: String,
        /// Provider: `llama-cpp` (local GGUF), `anthropic`, `openai`,
        /// `mistral`, `ollama`. `--kind` is accepted as an alias for
        /// backward compatibility.
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: String,
        /// Model identifier or path (absolute path for `llama-cpp`).
        #[arg(long)]
        model: String,
        /// API key (cloud providers only). Stored as-is in
        /// `config_json.api_key`. Prefer `--api-key-env VAR_NAME` to avoid
        /// persisting the key in system.db.
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// Environment variable name holding the API key.
        ///
        /// The runtime reads `std::env::var(NAME)` at boot. Recommended to
        /// keep the key out of system.db.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// Base URL (Ollama, self-hosted OpenAI-compatible gateway, ...).
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// Device for `llama-cpp` models: `metal` (Apple), `cuda`, `cpu`.
        #[arg(long, value_name = "DEVICE", default_value = "metal")]
        device: String,
        /// Inference timeout in seconds (default: 60).
        #[arg(long, value_name = "SECS", default_value = "60")]
        timeout_sec: u64,
        /// Create the backend disabled.
        #[arg(long)]
        disabled: bool,
        /// Mark this backend as the default (only one at a time).
        #[arg(long, alias = "is-default")]
        default: bool,
    },
    /// Update an existing LLM backend.
    ///
    /// Works in merge mode: flags that are not supplied keep their current
    /// value. The runtime exposes `PUT` as replace, so the CLI fetches the
    /// existing config first and only overwrites the fields the operator
    /// changed.
    Update {
        /// Backend name to update.
        name: String,
        /// New provider (rarely useful, changes the backend implementation).
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: Option<String>,
        /// New model (absolute path for `llama-cpp`).
        #[arg(long)]
        model: Option<String>,
        /// New API key (cloud providers).
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// New environment variable name for the API key.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// New base URL.
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// New device for `llama-cpp`.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
        /// New inference timeout in seconds.
        #[arg(long, value_name = "SECS")]
        timeout_sec: Option<u64>,
        /// Enable the backend.
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Disable the backend.
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        /// Mark as default.
        #[arg(long, alias = "is-default")]
        default: bool,
    },
    /// Delete an LLM backend.
    Delete {
        /// Backend name to delete.
        name: String,
        /// Confirm deletion without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Set a backend as the default backend.
    SetDefault {
        /// Backend name to mark as default.
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
        LlmCommand::Costs {
            get_threshold,
            threshold,
            config,
        } => {
            run_costs(
                &client,
                *get_threshold,
                *threshold,
                config.as_deref(),
                json,
            )
            .await
        }
        LlmCommand::Backends { command } => run_backends(&client, command, json).await,
        LlmCommand::Reload => run_reload(&client, json).await,
        LlmCommand::Setup {
            local,
            model,
            name,
            device,
            system_db,
            models_dir,
        } => {
            run_setup(SetupArgs {
                local: *local,
                model,
                backend_name: name,
                device_override: device.as_deref(),
                system_db_override: system_db.as_deref(),
                models_dir_override: models_dir.as_deref(),
                json,
            })
        }
    }
}

/// `apollia-os llm reload`: rebuild the in-memory router from `system.db`.
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

/// `apollia-os llm status`: display all LLM backends with their current state.
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

/// `apollia-os llm ping [backend]`: measure the latency of a backend.
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

/// `apollia-os llm chat "prompt"`: send a prompt to an LLM backend.
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

    // Cost ceiling line: shown only when the runtime reports a session cost.
    // `ceiling_usd` is absent without hybrid routing, in which case the line
    // states that no ceiling is configured rather than printing a phantom value.
    if let Some(cost) = resp.get("cost_usd").and_then(serde_json::Value::as_f64) {
        match resp.get("ceiling_usd").and_then(serde_json::Value::as_f64) {
            Some(ceiling) => {
                println!("  session cost: {cost:.2} USD / {ceiling:.2} USD ceiling");
                let reached = resp
                    .get("ceiling_reached")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if reached {
                    println!(
                        "  cost ceiling reached: the run stops cleanly when ceiling_action = hard_stop"
                    );
                }
            }
            None => {
                println!("  session cost: {cost:.2} USD (no hybrid cost ceiling configured)");
            }
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

/// `apollia-os llm costs`: display aggregated usage and costs per backend.
async fn run_costs(
    client: &RuntimeClient,
    get_threshold: bool,
    set_threshold: Option<f64>,
    config_path_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    if get_threshold {
        return run_get_cost_threshold(config_path_override, json);
    }
    if let Some(v) = set_threshold {
        return run_set_cost_threshold(v, config_path_override, json);
    }
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

fn resolve_apollia_toml(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("apollia.toml")
}

fn emit_llm_error(msg: String, json: bool) -> i32 {
    if json {
        let out = serde_json::json!({ "error": msg });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}

fn run_get_cost_threshold(override_path: Option<&std::path::Path>, json: bool) -> i32 {
    let path = resolve_apollia_toml(override_path);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Treat an absent file as "no threshold set" so scripts can detect
            // the unconfigured state with `--json | jq .threshold == null`.
            if json {
                let body = serde_json::json!({
                    "path": path.display().to_string(),
                    "threshold": serde_json::Value::Null,
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else {
                println!("  (no cost alert threshold set in {})", path.display());
            }
            return exit_codes::SUCCESS;
        }
        Err(e) => return emit_llm_error(format!("read {} failed: {e}", path.display()), json),
    };
    let doc: toml_edit::DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(e) => return emit_llm_error(format!("parse {} failed: {e}", path.display()), json),
    };
    let threshold = doc
        .get("llm")
        .and_then(|t| t.get("cost_alert_threshold_usd"))
        .and_then(|v| v.as_float());
    if json {
        let body = serde_json::json!({
            "path": path.display().to_string(),
            "threshold": threshold,
        });
        println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    } else {
        match threshold {
            Some(v) => println!("  cost_alert_threshold_usd = {v} (in {})", path.display()),
            None => println!("  (no cost alert threshold set in {})", path.display()),
        }
    }
    exit_codes::SUCCESS
}

fn run_set_cost_threshold(
    value: f64,
    override_path: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let path = resolve_apollia_toml(override_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return emit_llm_error(
                format!("create {} failed: {e}", parent.display()),
                json,
            );
        }
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(e) => return emit_llm_error(format!("parse {} failed: {e}", path.display()), json),
    };
    if value <= 0.0 {
        // Clear semantics: drop the key but keep the table.
        if let Some(table) = doc.get_mut("llm").and_then(|i| i.as_table_mut()) {
            table.remove("cost_alert_threshold_usd");
        }
    } else {
        let llm_table = doc
            .entry("llm")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut();
        if let Some(table) = llm_table {
            table.insert("cost_alert_threshold_usd", toml_edit::value(value));
        }
    }
    if let Err(e) = std::fs::write(&path, doc.to_string()) {
        return emit_llm_error(format!("write {} failed: {e}", path.display()), json);
    }
    if json {
        let body = serde_json::json!({
            "path": path.display().to_string(),
            "threshold": if value <= 0.0 { serde_json::Value::Null } else { serde_json::json!(value) },
            "cleared": value <= 0.0,
        });
        println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    } else if value <= 0.0 {
        println!("  * cost_alert_threshold cleared in {}", path.display());
    } else {
        println!("  * cost_alert_threshold_usd = {value} written to {}", path.display());
    }
    exit_codes::SUCCESS
}

/// Arguments for [`run_setup`], grouped to keep the signature small.
struct SetupArgs<'a> {
    local: bool,
    model: &'a std::path::Path,
    backend_name: &'a str,
    device_override: Option<&'a str>,
    system_db_override: Option<&'a std::path::Path>,
    models_dir_override: Option<&'a std::path::Path>,
    json: bool,
}

fn run_setup(args: SetupArgs<'_>) -> i32 {
    let SetupArgs {
        local,
        model,
        backend_name,
        device_override,
        system_db_override,
        models_dir_override,
        json,
    } = args;

    if let Err(code) = validate_setup_model(local, model, json) {
        return code;
    }

    let device = device_override.map(str::to_string).unwrap_or_else(|| {
        if cfg!(target_os = "macos") {
            "metal".to_string()
        } else {
            "cpu".to_string()
        }
    });

    let apollia_home = dirs::home_dir()
        .map(|h| h.join(".apollia"))
        .unwrap_or_else(|| PathBuf::from(".apollia"));
    let models_dir = models_dir_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| apollia_home.join("models"));
    if let Err(e) = std::fs::create_dir_all(&models_dir) {
        return emit_llm_error(
            format!("create {} failed: {e}", models_dir.display()),
            json,
        );
    }

    let file_name = match model.file_name().and_then(|s| s.to_str()) {
        Some(n) => n.to_string(),
        None => return emit_llm_error("invalid model filename".into(), json),
    };
    let dest = models_dir.join(&file_name);
    if dest != model {
        if let Err(e) = std::fs::copy(model, &dest) {
            return emit_llm_error(
                format!("copy {} → {}: {e}", model.display(), dest.display()),
                json,
            );
        }
    }
    let model_path_storage = format!("~/.apollia/models/{file_name}");

    let system_db_path = system_db_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| apollia_home.join("system.db"));
    if let Some(parent) = system_db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let quantization = infer_quantization(&file_name);

    let backend_config = apollia_core::LlmBackendConfig {
        name: backend_name.to_string(),
        provider: apollia_core::LlmProvider::LlamaCpp,
        model: model_path_storage.clone(),
        config_json: serde_json::json!({
            "model_path": model_path_storage,
            "device": device,
            "quantization": quantization,
        }),
        enabled: true,
        is_default: true,
    };

    let repo = match apollia_core::LlmBackendRepository::open(&system_db_path) {
        Ok(r) => r,
        Err(e) => {
            return emit_llm_error(
                format!("open {} failed: {e}", system_db_path.display()),
                json,
            );
        }
    };
    if let Err(e) = repo.save(&backend_config) {
        return emit_llm_error(format!("save backend failed: {e}"), json);
    }

    if json {
        let body = serde_json::json!({
            "backend_name": backend_name,
            "model_path": dest.display().to_string(),
            "model_path_storage": model_path_storage,
            "device": device,
            "quantization": quantization,
            "system_db": system_db_path.display().to_string(),
            "is_default": true,
        });
        println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    } else {
        println!("  * local LLM backend '{backend_name}' configured");
        println!("    model    : {}", dest.display());
        println!("    device   : {device}");
        println!("    quant.   : {quantization}");
        println!("    system   : {}", system_db_path.display());
        println!(
            "    -> run `apollia-os llm reload` (or restart the daemon) to make it live."
        );
    }
    exit_codes::SUCCESS
}

/// Validate the `--local` flag and the supplied model file.
///
/// Returns `Err(exit_code)` when the caller should bail out early.
fn validate_setup_model(local: bool, model: &std::path::Path, json: bool) -> Result<(), i32> {
    if !local {
        return Err(emit_llm_error(
            "only --local is supported in v0.1.0 (cloud providers go through `auth login` + `llm backends create`)".into(),
            json,
        ));
    }
    if !model.exists() {
        return Err(emit_llm_error(
            format!("model file not found: {}", model.display()),
            json,
        ));
    }
    let extension_ok = model
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false);
    if !extension_ok {
        return Err(emit_llm_error(
            format!("expected a .gguf file, got {}", model.display()),
            json,
        ));
    }
    Ok(())
}

/// Infer the GGUF quantization tag from a filename (best-effort).
///
/// Mirrors the helper in `apollia-desktop/src/commands/config.rs` so the CLI
/// and Desktop classify the same file the same way.
fn infer_quantization(file_name: &str) -> String {
    let upper = file_name.to_uppercase();
    let patterns = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M",
        "Q3_K_S", "Q2_K", "IQ4_XS", "IQ3_M", "IQ2_S", "F16", "F32",
    ];
    for p in &patterns {
        if upper.contains(p) {
            return p.to_lowercase();
        }
    }
    "q4_k_m".to_string()
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
/// Inputs for [`build_config_json`], grouped to keep the signature small.
struct BuildConfigArgs<'a> {
    provider: &'a str,
    model: &'a str,
    api_key: Option<&'a str>,
    api_key_env: Option<&'a str>,
    base_url: Option<&'a str>,
    device: &'a str,
    timeout_sec: u64,
}

fn build_config_json(args: BuildConfigArgs<'_>) -> serde_json::Value {
    let BuildConfigArgs {
        provider,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
    } = args;

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

/// `apollia-os llm backends list`: list all configured backends.
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

/// `apollia-os llm backends show <name>`: display the full configuration
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
                eprintln!("Error: backend '{name}' not found");
                eprintln!("Hint: run `apollia-os llm backends list` to see existing backends.");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends create`: create a new LLM backend.
///
/// Builds the full payload expected by `POST /api/v1/llm/backends`
/// (canonical provider + provider-specific config_json) instead of sending
/// only the `kind/model` fields, which the runtime rejects with a 400.
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
    let config_json = build_config_json(BuildConfigArgs {
        provider: canonical,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
    });

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
                println!("OK backend '{name}' created (provider: {canonical}, model: {model})");
                if is_default {
                    println!("    marked as default backend");
                }
                if !enabled {
                    println!("    disabled (pass --enable to activate)");
                }
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            emit_backend_create_server_error(status, &body, json)
        }
        Err(e) => handle_error(e, json),
    }
}

/// Render a server-side error from `POST /api/v1/llm/backends`.
fn emit_backend_create_server_error(status: u16, body: &str, json: bool) -> i32 {
    if json {
        let out = serde_json::json!({"error": body, "status": status});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error ({status}): {body}");
        if status == 422 {
            eprintln!();
            eprintln!("Hint: accepted providers: llama-cpp, anthropic, openai, mistral, ollama");
            eprintln!("      (the --kind alias is still accepted for backward compatibility)");
        }
    }
    exit_codes::GENERAL_ERROR
}

/// `apollia-os llm backends update`: update an existing backend.
///
/// The runtime exposes `PUT` in replace mode (all fields required). The CLI
/// first reads the current state via `GET /api/v1/llm/backends/:name`, applies
/// the provided flags in merge mode, then sends the full payload. This allows
/// `--model X` without having to re-specify provider/config_json/enabled.
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
                eprintln!("Error: backend '{name}' not found");
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

    // Step 4: merge config_json, start from the existing object, overlay
    // any field that the user explicitly changed.
    let merge = ConfigMerge {
        new_provider,
        new_model,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
    };
    let cfg_map = merge_backend_config(cur_cfg, &merge);

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
                println!("OK backend '{name}' updated");
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

/// Resolved-then-overlaid values used by [`merge_backend_config`].
struct ConfigMerge<'a> {
    new_provider: &'a str,
    new_model: &'a str,
    model: Option<&'a str>,
    api_key: Option<&'a str>,
    api_key_env: Option<&'a str>,
    base_url: Option<&'a str>,
    device: Option<&'a str>,
    timeout_sec: Option<u64>,
}

/// Merge the existing `config_json` object with the user-supplied overrides,
/// applying only the fields that were explicitly changed.
fn merge_backend_config(
    cur_cfg: serde_json::Value,
    m: &ConfigMerge<'_>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut cfg_map = match cur_cfg {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    if let Some(secs) = m.timeout_sec {
        cfg_map.insert("timeout_sec".into(), serde_json::Value::from(secs));
    }
    if m.new_provider == "llama-cpp" {
        // For local backends, the model PATH lives in config_json.model_path
        // and must stay in sync with the top-level `model` column.
        if m.model.is_some() {
            cfg_map.insert(
                "model_path".into(),
                serde_json::Value::String(m.new_model.to_string()),
            );
        }
        if let Some(d) = m.device {
            cfg_map.insert("device".into(), serde_json::Value::String(d.to_string()));
        }
    } else {
        if let Some(k) = m.api_key {
            cfg_map.insert("api_key".into(), serde_json::Value::String(k.to_string()));
        }
        if let Some(v) = m.api_key_env {
            cfg_map.insert(
                "api_key_env".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(u) = m.base_url {
            cfg_map.insert("base_url".into(), serde_json::Value::String(u.to_string()));
        }
    }
    cfg_map
}

/// `apollia-os llm backends delete`: delete a backend.
async fn run_backends_delete(client: &RuntimeClient, name: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Pass --confirm to delete backend '{name}' without an interactive prompt.");
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
                println!("OK backend '{name}' deleted");
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends set-default`: set the default backend.
async fn run_backends_set_default(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.set_default_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' set as default backend");
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: backend '{name}' not found");
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
        // GIVEN "ping" without an argument
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
        // THEN LlmCommand::Costs (no threshold flags)
        assert!(matches!(
            cli.command,
            LlmCommand::Costs {
                get_threshold: false,
                threshold: None,
                ..
            }
        ));
    }

    #[test]
    fn test_llm_costs_get_threshold_parses() {
        let cli = TestCli::parse_from(["apollia-os", "costs", "--get-threshold"]);
        match &cli.command {
            LlmCommand::Costs {
                get_threshold,
                threshold,
                ..
            } => {
                assert!(*get_threshold);
                assert!(threshold.is_none());
            }
            other => panic!("expected Costs, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_set_threshold_parses() {
        let cli = TestCli::parse_from(["apollia-os", "costs", "--threshold", "0.5"]);
        match &cli.command {
            LlmCommand::Costs {
                get_threshold,
                threshold,
                ..
            } => {
                assert!(!*get_threshold);
                assert_eq!(*threshold, Some(0.5));
            }
            other => panic!("expected Costs, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_threshold_conflicts_with_get_threshold() {
        let result = TestCli::try_parse_from([
            "apollia-os",
            "costs",
            "--threshold",
            "0.5",
            "--get-threshold",
        ]);
        assert!(result.is_err(), "--threshold + --get-threshold must conflict");
    }

    #[test]
    fn test_llm_setup_local_parses() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "setup",
            "--local",
            "--model",
            "/tmp/model.gguf",
        ]);
        match &cli.command {
            LlmCommand::Setup {
                local,
                model,
                name,
                ..
            } => {
                assert!(*local);
                assert_eq!(model, &PathBuf::from("/tmp/model.gguf"));
                assert_eq!(name, "local");
            }
            other => panic!("expected Setup, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_setup_with_custom_name_and_device() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "setup",
            "--local",
            "--model",
            "/tmp/m.gguf",
            "--name",
            "tiny",
            "--device",
            "cpu",
        ]);
        match &cli.command {
            LlmCommand::Setup {
                local,
                name,
                device,
                ..
            } => {
                assert!(*local);
                assert_eq!(name, "tiny");
                assert_eq!(device.as_deref(), Some("cpu"));
            }
            other => panic!("expected Setup, got {other:?}"),
        }
    }

    #[test]
    fn test_setup_without_local_flag_errors() {
        // Pretend file exists by referencing the binary itself (any extant file
        // with a non-.gguf extension would still pass the existence check, but
        // we exit before that on --local missing).
        let code = run_setup(SetupArgs {
            local: false,
            model: std::path::Path::new("/tmp/never.gguf"),
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_setup_rejects_missing_model_file() {
        let code = run_setup(SetupArgs {
            local: true,
            model: std::path::Path::new("/definitely/missing/model.gguf"),
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_setup_rejects_non_gguf_extension() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Write to a path with a non-gguf extension.
        let path = tmp.path().with_extension("bin");
        std::fs::copy(tmp.path(), &path).unwrap();
        let code = run_setup(SetupArgs {
            local: true,
            model: &path,
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn infer_quantization_picks_known_pattern() {
        assert_eq!(infer_quantization("llama-Q4_K_M.gguf"), "q4_k_m");
        assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0.gguf"), "q8_0");
        assert_eq!(infer_quantization("unknown.gguf"), "q4_k_m");
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
        // THEN Create with the right fields
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
