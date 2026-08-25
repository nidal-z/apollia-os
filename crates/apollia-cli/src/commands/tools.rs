//! `apollia-os tools`: local governance of the native tools.
//!
//! This subcommand operates directly on the `governance.db` database and the
//! machine's `apollia.toml` file, without going through the runtime. The
//! commands are safe even when the daemon is not started; the runtime rereads
//! the governance snapshot on every agent run (see
//! [`apollia_tools::load_governance_snapshot`]).
//!
//! `describe` remains exposed to query the descriptor catalogue via
//! `GET /api/v1/tools/<name>` when the runtime is running.

use std::path::{Path, PathBuf};
use std::time::Instant;

use apollia_core::{ToolsConfig, WebSearchBackend};
use apollia_tools::{
    governance_db::GOVERNANCE_DB_FILENAME, CredentialEntry, NativeToolRegistry,
    ToolCredentialStore, ToolGovernanceError, AGENT_CREDENTIALS_NAMESPACE, NATIVE_TOOL_NAMES,
};
use clap::Subcommand;
use toml_edit::{DocumentMut, Item, Value};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::config::parse_apollia_toml;
use crate::exit_codes;

/// Subcommands of `apollia-os tools`.
#[derive(Debug, Subcommand)]
pub enum ToolsCommand {
    /// Show the status of each native tool (active, backend, credentials).
    List,
    /// Enable the *name* tool (clears any disabled flag in `governance.db`).
    Enable {
        /// Canonical name of the native tool.
        name: String,
    },
    /// Disable the *name* tool (sets `enabled = FALSE` in `governance.db`).
    Disable {
        /// Canonical name of the native tool.
        name: String,
    },
    /// Read or update the `[tools.<name>]` configuration in `apollia.toml`.
    Config {
        /// Config subcommand.
        #[command(subcommand)]
        command: ToolsConfigCmd,
    },
    /// Reload the governance snapshot and print the effective state.
    Reload,
    /// Manage the encrypted credentials attached to a tool.
    Credentials {
        /// Credentials subcommand.
        #[command(subcommand)]
        command: ToolsCredentialsCmd,
    },
    /// Show the descriptor of a tool registered with the runtime.
    Show {
        /// Tool name.
        tool_name: String,
    },
    /// Inspect the HITL queue from the tool registry's side.
    Approvals {
        /// Approvals subcommand.
        #[command(subcommand)]
        command: ToolsApprovalsCmd,
    },
}

/// Subcommands of `apollia-os tools approvals`.
#[derive(Debug, Subcommand)]
pub enum ToolsApprovalsCmd {
    /// List approvals pending decision (tasks in `input_required`).
    Pending,
    /// List approvals resolved within the `--days` window.
    Resolved {
        /// Days of history to include (default: 7).
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Maximum number of entries to return (default: 50).
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

/// Subcommands of `apollia-os tools config`.
#[derive(Debug, Subcommand)]
pub enum ToolsConfigCmd {
    /// Show the effective configuration of *name*.
    Get {
        /// Native tool name (`web_search`, `web_read`, …).
        name: String,
    },
    /// Update a configuration key `<tool>.<path>` in `apollia.toml`.
    Set {
        /// Dotted key path, e.g. `web_search.backend` or `web_search.brave.timeout_secs`.
        key_path: String,
        /// New value (parsed according to the expected type).
        value: String,
    },
}

/// Subcommands of `apollia-os tools credentials`.
#[derive(Debug, Subcommand)]
pub enum ToolsCredentialsCmd {
    /// List stored credentials (values masked).
    List {
        /// Optional filter on a tool name.
        tool: Option<String>,
    },
    /// Store a credential `(tool, key)` after an interactive masked prompt.
    Set {
        /// Owning tool name, or `agent` for a secret declared by an agent manifest.
        tool: String,
        /// Logical key name (e.g. `brave.api_key`, or an agent's `hubspot_api_token`).
        key: String,
    },
    /// Delete the credential `(tool, key)`.
    Delete {
        /// Owning tool name.
        tool: String,
        /// Logical key name.
        key: String,
    },
    /// Validate a credential with a live call against the backend it targets.
    Test {
        /// Tool whose credentials should be checked.
        tool: String,
    },
}

/// Execute a `tools` subcommand.
pub async fn run(cmd: &ToolsCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        ToolsCommand::List => run_list(json),
        ToolsCommand::Enable { name } => run_set_enabled(name, true, json),
        ToolsCommand::Disable { name } => run_set_enabled(name, false, json),
        ToolsCommand::Config { command } => run_config(command, json),
        ToolsCommand::Reload => run_reload(json),
        ToolsCommand::Credentials { command } => run_credentials(command, json).await,
        ToolsCommand::Show { tool_name } => run_describe(socket, tool_name, json).await,
        ToolsCommand::Approvals { command } => run_approvals(socket, command, json).await,
    }
}

// ─── Approvals (HITL queue) ──────────────────────────────────────────────────

/// Dispatch `tools approvals <verb>` to the runtime client.
async fn run_approvals(socket: Option<PathBuf>, cmd: &ToolsApprovalsCmd, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);
    match cmd {
        ToolsApprovalsCmd::Pending => run_approvals_pending(&client, json).await,
        ToolsApprovalsCmd::Resolved { days, limit } => {
            run_approvals_resolved(&client, *days, *limit, json).await
        }
    }
}

/// Emit a non-success HTTP response (status >= 400) and return its exit code.
fn emit_approvals_http_error(resp: &crate::client::RawResponse, json: bool) -> i32 {
    crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("HTTP {}: {}", resp.status, resp.body),
    )
}

/// Emit a client transport error and return its exit code.
fn emit_approvals_client_error(err: &ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        e => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

async fn run_approvals_pending(client: &RuntimeClient, json: bool) -> i32 {
    match client.get("/api/v1/approvals/pending").await {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                match serde_json::from_str::<serde_json::Value>(&resp.body) {
                    Ok(v) => print_pending_human(&v),
                    Err(_) => println!("{}", resp.body),
                }
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => emit_approvals_http_error(&resp, json),
        Err(e) => emit_approvals_client_error(&e, json),
    }
}

async fn run_approvals_resolved(client: &RuntimeClient, days: u32, limit: u32, json: bool) -> i32 {
    let uri = format!("/api/v1/approvals/resolved?days={days}&limit={limit}");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                match serde_json::from_str::<serde_json::Value>(&resp.body) {
                    Ok(v) => print_resolved_human(&v),
                    Err(_) => println!("{}", resp.body),
                }
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => emit_approvals_http_error(&resp, json),
        Err(e) => emit_approvals_client_error(&e, json),
    }
}

fn print_pending_human(v: &serde_json::Value) {
    let arr = v
        .get("pending")
        .or_else(|| v.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_else(|| v.as_array().cloned().unwrap_or_default());
    if arr.is_empty() {
        println!("  (no pending approvals)");
        return;
    }
    println!("  {:<24} {:<24} {:<24} REQUESTED", "TASK", "TOOL", "AGENT");
    for item in &arr {
        let task_id = item.get("task_id").and_then(|x| x.as_str()).unwrap_or("?");
        let tool = item
            .get("tool")
            .or_else(|| item.get("tool_name"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let agent = item
            .get("agent")
            .or_else(|| item.get("agent_id"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let when = item
            .get("requested_at")
            .or_else(|| item.get("created_at"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        println!("  {task_id:<24} {tool:<24} {agent:<24} {when}");
    }
}

fn print_resolved_human(v: &serde_json::Value) {
    let arr = v
        .get("resolved")
        .or_else(|| v.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_else(|| v.as_array().cloned().unwrap_or_default());
    if arr.is_empty() {
        println!("  (no resolved approvals in window)");
        return;
    }
    println!(
        "  {:<24} {:<10} {:<24} RESOLVED",
        "TASK", "DECISION", "TOOL"
    );
    for item in &arr {
        let task_id = item.get("task_id").and_then(|x| x.as_str()).unwrap_or("?");
        let decision = item
            .get("decision")
            .or_else(|| item.get("status"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let tool = item.get("tool").and_then(|x| x.as_str()).unwrap_or("?");
        let when = item
            .get("resolved_at")
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        println!("  {task_id:<24} {decision:<10} {tool:<24} {when}");
    }
}

// ─── List ─────────────────────────────────────────────────────────────

fn run_list(json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let registry = match open_registry(&data_dir, json) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let statuses = match registry.list() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("registry list failed: {e}"), json),
    };

    let credentials = open_credential_store(&data_dir).and_then(|s| s.list(None).ok());

    let tools_cfg = load_tools_config(json);

    if json {
        let arr: Vec<serde_json::Value> = statuses
            .iter()
            .map(|s| {
                let backend = backend_label(&s.name, &tools_cfg);
                let creds = credentials_summary(&s.name, credentials.as_deref());
                serde_json::json!({
                    "name": s.name,
                    "enabled": s.enabled,
                    "backend": backend,
                    "credentials": creds,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"tools": arr}))
                .unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "  {:<16} {:<7} {:<22} CREDENTIALS",
            "NOM", "ACTIF", "BACKEND"
        );
        for s in &statuses {
            let active = if s.enabled { "✓" } else { "✗" };
            let backend = backend_label(&s.name, &tools_cfg);
            let creds = credentials_text(&s.name, credentials.as_deref());
            println!("  {:<16} {:<7} {:<22} {}", s.name, active, backend, creds);
        }
    }
    exit_codes::SUCCESS
}

fn backend_label(tool: &str, cfg: &ToolsConfig) -> String {
    if tool == "web_search" {
        match cfg.web_search.backend {
            WebSearchBackend::Auto => "DuckDuckGo (auto)".to_string(),
            WebSearchBackend::DuckDuckGo => "DuckDuckGo".to_string(),
            WebSearchBackend::Brave => "Brave".to_string(),
        }
    } else {
        "-".to_string()
    }
}

fn credentials_summary(tool: &str, credentials: Option<&[CredentialEntry]>) -> serde_json::Value {
    let needs = required_credentials(tool);
    if needs.is_empty() {
        return serde_json::Value::Null;
    }
    let entries = credentials.unwrap_or(&[]);
    let arr: Vec<serde_json::Value> = needs
        .iter()
        .map(|key| {
            let present = entries
                .iter()
                .any(|e| e.tool_name == tool && e.key_name == *key);
            serde_json::json!({"key": key, "present": present})
        })
        .collect();
    serde_json::Value::Array(arr)
}

fn credentials_text(tool: &str, credentials: Option<&[CredentialEntry]>) -> String {
    let needs = required_credentials(tool);
    if needs.is_empty() {
        return "-".to_string();
    }
    let entries = credentials.unwrap_or(&[]);
    needs
        .iter()
        .map(|key| {
            let present = entries
                .iter()
                .any(|e| e.tool_name == tool && e.key_name == *key);
            if present {
                format!("{key}: present")
            } else {
                format!("{key}: absent ⚠")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn required_credentials(tool: &str) -> &'static [&'static str] {
    match tool {
        "web_search" => &["brave.api_key"],
        _ => &[],
    }
}

// ─── Enable / Disable ─────────────────────────────────────────────────

fn run_set_enabled(name: &str, enabled: bool, json: bool) -> i32 {
    if !is_known_tool(name) {
        return emit_unknown_tool(name, json);
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let mut registry = match open_registry(&data_dir, json) {
        Ok(r) => r,
        Err(code) => return code,
    };
    if let Err(e) = registry.set_enabled(name, enabled) {
        return emit_error(format!("registry update failed: {e}"), json);
    }
    let action = if enabled { "enabled" } else { "disabled" };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": name,
                "enabled": enabled,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("✔ {name} {action}");
    }
    exit_codes::SUCCESS
}

// ─── Config ───────────────────────────────────────────────────────────

fn run_config(cmd: &ToolsConfigCmd, json: bool) -> i32 {
    match cmd {
        ToolsConfigCmd::Get { name } => run_config_get(name, json),
        ToolsConfigCmd::Set { key_path, value } => run_config_set(key_path, value, json),
    }
}

fn run_config_get(name: &str, json: bool) -> i32 {
    if !is_known_tool(name) {
        return emit_unknown_tool(name, json);
    }
    let cfg = load_tools_config(json);
    let value = effective_config_for(name, &cfg);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        let toml_value = json_to_toml_value(&value);
        let mut doc = DocumentMut::new();
        let header = format!("tools.{name}");
        doc[&header] = toml_edit::Item::Value(toml_value);
        let s = doc.to_string();
        let trimmed = s.trim_start_matches('\n');
        print!("{trimmed}");
    }
    exit_codes::SUCCESS
}

fn effective_config_for(name: &str, cfg: &ToolsConfig) -> serde_json::Value {
    match name {
        "web_search" => serde_json::json!({
            "backend": backend_to_str(cfg.web_search.backend),
            "require_configured": cfg.web_search.require_configured,
            "brave": {
                "api_key_env_var": cfg.web_search.brave.api_key_env_var,
                "timeout_secs": cfg.web_search.brave.timeout_secs,
                "max_results": cfg.web_search.brave.max_results,
            },
            "duckduckgo": {
                "timeout_secs": cfg.web_search.duckduckgo.timeout_secs,
                "max_response_kb": cfg.web_search.duckduckgo.max_response_kb,
            },
        }),
        "web_read" => serde_json::json!({
            "timeout_secs": cfg.web_read.timeout_secs,
            "max_response_kb": cfg.web_read.max_response_kb,
            "ssrf_guard": cfg.web_read.ssrf_guard,
        }),
        _ => serde_json::Value::Object(serde_json::Map::new()),
    }
}

fn backend_to_str(b: WebSearchBackend) -> &'static str {
    match b {
        WebSearchBackend::Auto => "auto",
        WebSearchBackend::DuckDuckGo => "duckduckgo",
        WebSearchBackend::Brave => "brave",
    }
}

fn json_to_toml_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::from(""),
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(a) => {
            let arr: toml_edit::Array = a.iter().map(json_to_toml_value).collect();
            Value::Array(arr)
        }
        serde_json::Value::Object(o) => {
            let mut tbl = toml_edit::InlineTable::new();
            for (k, val) in o {
                tbl.insert(k, json_to_toml_value(val));
            }
            Value::InlineTable(tbl)
        }
    }
}

fn run_config_set(key_path: &str, value: &str, json: bool) -> i32 {
    let parts: Vec<&str> = key_path.split('.').collect();
    if parts.len() < 2 {
        return emit_error(
            format!("invalid key '{key_path}' - expected format: <tool>.<key>"),
            json,
        );
    }
    let tool = parts[0];
    let key_segments = &parts[1..];

    let parsed = match parse_value_for(tool, key_segments, value) {
        Ok(v) => v,
        Err(msg) => return emit_error(msg, json),
    };

    let path = match resolve_writable_config_path() {
        Ok(p) => p,
        Err(code) => return code,
    };

    let original = match read_or_empty(&path) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("read {} failed: {e}", path.display()), json),
    };
    let mut doc: DocumentMut = match original.parse() {
        Ok(d) => d,
        Err(e) => return emit_error(format!("invalid TOML in {}: {e}", path.display()), json),
    };

    set_nested_value(&mut doc, &["tools", tool], key_segments, parsed);

    let written = doc.to_string();

    if let Err(e) = write_config_file(&path, &written) {
        return emit_error(format!("write {} failed: {e}", path.display()), json);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "key": key_path,
                "value": value,
                "path": path.display().to_string(),
            }))
            .unwrap_or_default()
        );
    } else {
        println!("✔ {} → {} ({})", key_path, value, path.display());
    }
    exit_codes::SUCCESS
}

fn parse_value_for(tool: &str, key_segments: &[&str], raw: &str) -> Result<Value, String> {
    let key = key_segments.join(".");
    match (tool, key.as_str()) {
        ("web_search", "backend") => match raw {
            "auto" | "duckduckgo" | "brave" => Ok(Value::from(raw)),
            _ => Err(format!(
                "valeur invalide '{raw}' pour web_search.backend - attendu : auto | duckduckgo | brave"
            )),
        },
        ("web_search", "require_configured") => parse_bool(raw),
        ("web_search", "brave.api_key_env_var") => Ok(Value::from(raw)),
        ("web_search", "brave.timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_search", "brave.max_results") => parse_int_in(raw, 1, 20),
        ("web_search", "duckduckgo.timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_search", "duckduckgo.max_response_kb") => parse_int_in(raw, 16, 16_384),
        ("web_read", "timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_read", "max_response_kb") => parse_int_in(raw, 64, 32_768),
        ("web_read", "ssrf_guard") => parse_bool(raw),
        _ => Err(format!(
            "unknown key '{tool}.{key}'\n{}",
            valid_keys_help(tool)
        )),
    }
}

fn valid_keys_help(tool: &str) -> String {
    match tool {
        "web_search" => "valid keys: backend, require_configured, brave.api_key_env_var, \
                         brave.timeout_secs, brave.max_results, duckduckgo.timeout_secs, \
                         duckduckgo.max_response_kb"
            .to_string(),
        "web_read" => "valid keys: timeout_secs, max_response_kb, ssrf_guard".to_string(),
        _ => "outil sans configuration TOML - outils configurables : web_search, web_read"
            .to_string(),
    }
}

fn parse_bool(raw: &str) -> Result<Value, String> {
    match raw {
        "true" => Ok(Value::from(true)),
        "false" => Ok(Value::from(false)),
        _ => Err(format!("boolean expected (true|false), got '{raw}'")),
    }
}

fn parse_int_in(raw: &str, min: i64, max: i64) -> Result<Value, String> {
    let n: i64 = raw
        .parse()
        .map_err(|_| format!("integer expected, got '{raw}'"))?;
    if n < min || n > max {
        return Err(format!("valeur {n} hors bornes [{min}, {max}]"));
    }
    Ok(Value::from(n))
}

fn set_nested_value(
    doc: &mut DocumentMut,
    table_path: &[&str],
    leaf_segments: &[&str],
    value: Value,
) {
    let table = ensure_table(doc.as_table_mut(), table_path);
    let (last, parents) = match leaf_segments.split_last() {
        Some(s) => s,
        None => return,
    };
    let target = ensure_subtable(table, parents);
    target[last] = Item::Value(value);
}

fn ensure_table<'a>(root: &'a mut toml_edit::Table, path: &[&str]) -> &'a mut toml_edit::Table {
    let mut current = root;
    for segment in path {
        if !current.contains_key(segment) {
            let mut t = toml_edit::Table::new();
            t.set_implicit(false);
            current[segment] = Item::Table(t);
        }
        let item = &mut current[segment];
        if !matches!(item, Item::Table(_)) {
            *item = Item::Table(toml_edit::Table::new());
        }
        current = item
            .as_table_mut()
            .expect("freshly created or matched table item");
    }
    current
}

fn ensure_subtable<'a>(root: &'a mut toml_edit::Table, path: &[&str]) -> &'a mut toml_edit::Table {
    ensure_table(root, path)
}

// ─── Reload ───────────────────────────────────────────────────────────

fn run_reload(json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let snapshot = match apollia_tools::load_governance_snapshot(&data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("snapshot reload failed: {e}"), json),
    };
    if json {
        let payload = serde_json::json!({
            "disabled_tools": snapshot.disabled_tools,
            "brave_api_key_present": snapshot.brave_api_key.is_some(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    } else {
        println!("✔ Governance snapshot reloaded");
        if snapshot.disabled_tools.is_empty() {
            println!("  Disabled tools    : (none)");
        } else {
            println!(
                "  Disabled tools    : {}",
                snapshot.disabled_tools.join(", ")
            );
        }
        let brave = if snapshot.brave_api_key.is_some() {
            "present"
        } else {
            "absent"
        };
        println!("  Brave API key     : {brave}");
        println!(
            "  Note: the runtime rereads this snapshot on every agent run - \
             no restart required."
        );
    }
    exit_codes::SUCCESS
}

// ─── Credentials ──────────────────────────────────────────────────────

async fn run_credentials(cmd: &ToolsCredentialsCmd, json: bool) -> i32 {
    match cmd {
        ToolsCredentialsCmd::List { tool } => run_credentials_list(tool.as_deref(), json),
        ToolsCredentialsCmd::Set { tool, key } => run_credentials_set(tool, key, json),
        ToolsCredentialsCmd::Delete { tool, key } => run_credentials_delete(tool, key, json),
        ToolsCredentialsCmd::Test { tool } => run_credentials_test(tool, json).await,
    }
}

fn run_credentials_list(filter: Option<&str>, json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let store = match open_credential_store(&data_dir) {
        Some(s) => s,
        None => {
            return emit_error(
                "unable to open the credential store - check ~/.apollia".to_string(),
                json,
            );
        }
    };
    let entries = match store.list(filter) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("list credentials failed: {e}"), json),
    };
    if json {
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "tool": e.tool_name,
                    "key": e.key_name,
                    "created_at": e.created_at,
                    "last_used_at": e.last_used_at,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"credentials": arr}))
                .unwrap_or_default()
        );
    } else if entries.is_empty() {
        println!("  (no credential stored)");
    } else {
        println!(
            "  {:<14} {:<18} {:<12} DERNIÈRE UTILISATION",
            "OUTIL", "CLÉ", "AJOUTÉ LE"
        );
        for e in &entries {
            let created = format_unix_date(e.created_at);
            let last = e
                .last_used_at
                .map(format_unix_date)
                .unwrap_or_else(|| "jamais".to_string());
            println!(
                "  {:<14} {:<18} {:<12} {}",
                e.tool_name, e.key_name, created, last
            );
        }
    }
    exit_codes::SUCCESS
}

fn run_credentials_set(tool: &str, key: &str, json: bool) -> i32 {
    if !is_valid_credential_target(tool) {
        return emit_unknown_tool(tool, json);
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let prompt = format!("Valeur pour {tool}/{key} : ");
    let value = match rpassword::prompt_password(&prompt) {
        Ok(v) => v,
        Err(e) => return emit_error(format!("failed to read prompt: {e}"), json),
    };
    if value.is_empty() {
        return emit_error("empty value - credential not stored".to_string(), json);
    }
    let mut store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    if let Err(e) = store.set(tool, key, &value) {
        return emit_error(format!("set credential failed: {e}"), json);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": tool,
                "key": key,
                "stored": true,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("✔ credential {tool}/{key} stored (encrypted)");
    }
    exit_codes::SUCCESS
}

fn run_credentials_delete(tool: &str, key: &str, json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let mut store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    let removed = match store.delete(tool, key) {
        Ok(b) => b,
        Err(e) => return emit_error(format!("delete failed: {e}"), json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": tool,
                "key": key,
                "removed": removed,
            }))
            .unwrap_or_default()
        );
    } else if removed {
        println!("✔ credential {tool}/{key} deleted");
    } else {
        println!("ℹ no credential {tool}/{key} stored");
    }
    exit_codes::SUCCESS
}

async fn run_credentials_test(tool: &str, json: bool) -> i32 {
    if tool != "web_search" {
        return emit_error(
            format!("credential test not implemented for '{tool}' (only web_search is supported)"),
            json,
        );
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    let api_key = match store.get("web_search", "brave.api_key") {
        Ok(Some(k)) => k,
        Ok(None) => {
            return emit_error(
                "no brave.api_key stored - use `apollia-os tools credentials set web_search brave.api_key`"
                    .to_string(),
                json,
            );
        }
        Err(e) => return emit_error(format!("failed to read credential: {e}"), json),
    };

    let cfg = load_tools_config(json);
    let timeout = std::time::Duration::from_secs(cfg.web_search.brave.timeout_secs);
    let client = match apollia_core::net::safe_client_builder()
        .timeout(timeout)
        .build()
    {
        Ok(c) => c,
        Err(e) => return emit_error(format!("HTTP client init failed: {e}"), json),
    };
    let url = "https://api.search.brave.com/res/v1/web/search?q=apollia&count=1";
    let started = Instant::now();
    let response = client
        .get(url)
        .header("X-Subscription-Token", &api_key)
        .header("Accept", "application/json")
        .send()
        .await;
    let elapsed_ms = started.elapsed().as_millis();

    match response {
        Ok(resp) => {
            let status = resp.status();
            let mut store_mut = store;
            let _ = store_mut.touch_last_used("web_search", "brave.api_key");
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "tool": "web_search",
                        "key": "brave.api_key",
                        "http_status": status.as_u16(),
                        "latency_ms": elapsed_ms as u64,
                        "ok": status.is_success(),
                    }))
                    .unwrap_or_default()
                );
            } else if status.is_success() {
                println!("✔ brave.api_key valide ({elapsed_ms}ms, HTTP {status})");
            } else {
                println!("✗ brave.api_key rejected (HTTP {status}, {elapsed_ms}ms)");
            }
            if status.is_success() {
                exit_codes::SUCCESS
            } else {
                exit_codes::GENERAL_ERROR
            }
        }
        Err(e) => emit_error(format!("Brave call failed: {e}"), json),
    }
}

// ─── Describe (legacy) ────────────────────────────────────────────────

async fn run_describe(socket: Option<PathBuf>, tool_name: &str, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);
    let resp = match client.get(&format!("/api/v1/tools/{tool_name}")).await {
        Ok(r) => r,
        Err(e) => return handle_client_error(e, json),
    };
    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }
    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => return emit_error(format!("invalid JSON response: {e}"), json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = parsed
            .get("kind")
            .and_then(|v| v.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("?");
        let desc = parsed
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  Name      : {name}");
        println!("  Kind      : {kind}");
        if !desc.is_empty() {
            println!("  Desc      : {desc}");
        }
    }
    exit_codes::SUCCESS
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn resolve_data_dir() -> Result<PathBuf, i32> {
    match apollia_core::paths::home_string() {
        Some(h) => Ok(apollia_core::paths::data_dir_under(h)),
        None => Err(emit_error(
            "variable d'environnement HOME absente".to_string(),
            false,
        )),
    }
}

fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(GOVERNANCE_DB_FILENAME)
}

fn keyfile_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".keyfile")
}

fn open_registry(data_dir: &Path, json: bool) -> Result<NativeToolRegistry, i32> {
    if !data_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            return Err(emit_error(
                format!("create {} failed: {e}", data_dir.display()),
                json,
            ));
        }
    }
    if !db_path(data_dir).exists() {
        if let Err(e) = apollia_tools::GovernanceDb::open(data_dir) {
            return Err(emit_error(format!("init governance.db failed: {e}"), json));
        }
    }
    NativeToolRegistry::new(&db_path(data_dir))
        .map_err(|e: ToolGovernanceError| emit_error(format!("open registry failed: {e}"), json))
}

fn open_credential_store(data_dir: &Path) -> Option<ToolCredentialStore> {
    if !db_path(data_dir).exists() {
        return None;
    }
    ToolCredentialStore::new(&db_path(data_dir), &keyfile_path(data_dir)).ok()
}

fn is_known_tool(name: &str) -> bool {
    NATIVE_TOOL_NAMES.contains(&name)
}

/// A credential can be attached to a native tool or to the shared `agent`
/// namespace (secrets an agent declares in its manifest).
fn is_valid_credential_target(name: &str) -> bool {
    is_known_tool(name) || name == AGENT_CREDENTIALS_NAMESPACE
}

fn emit_unknown_tool(name: &str, json: bool) -> i32 {
    let known = NATIVE_TOOL_NAMES.join(", ");
    emit_error(
        format!(
            "cible inconnue '{name}' - outils natifs disponibles : {known} ; \
             ou '{AGENT_CREDENTIALS_NAMESPACE}' pour un secret déclaré par un agent"
        ),
        json,
    )
}

fn emit_error(msg: String, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.to_string())
}

fn load_tools_config(json: bool) -> ToolsConfig {
    match find_config_path() {
        Some(p) => match parse_apollia_toml(&p) {
            Ok(c) => c.tools.unwrap_or_default(),
            Err(e) => {
                if !json {
                    eprintln!("Warning: apollia.toml unreadable ({e}) - using defaults");
                }
                ToolsConfig::default()
            }
        },
        None => ToolsConfig::default(),
    }
}

/// Looks for `apollia.toml` in the current directory, then `~/.config/apollia/`.
fn find_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let local = cwd.join("apollia.toml");
    if local.exists() {
        return Some(local);
    }
    let home = apollia_core::paths::home_string()?;
    let user = PathBuf::from(home).join(".config/apollia/apollia.toml");
    if user.exists() {
        Some(user)
    } else {
        None
    }
}

/// Returns a path to write the config to: prefers an existing file, otherwise
/// `~/.config/apollia/apollia.toml` (created on the fly if needed).
fn resolve_writable_config_path() -> Result<PathBuf, i32> {
    if let Some(p) = find_config_path() {
        return Ok(p);
    }
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| emit_error("variable d'environnement HOME absente".to_string(), false))?;
    let dir = PathBuf::from(home).join(".config/apollia");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| emit_error(format!("create {} failed: {e}", dir.display()), false))?;
    }
    Ok(dir.join("apollia.toml"))
}

fn read_or_empty(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}

fn format_unix_date(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            // Daemon-off is exit 2, distinct from generic failures (exit 1).
            // `emit_error` returns GENERAL_ERROR, so we emit the message
            // ourselves and override the return code here.
            crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            )
        }
        other => emit_error(other.to_string(), json),
    }
}

fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));
    emit_error(msg, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_toml(dir: &TempDir, content: &str) -> PathBuf {
        let p = dir.path().join("apollia.toml");
        std::fs::write(&p, content).expect("write");
        p
    }

    #[test]
    fn parse_value_for_web_search_backend_ok() {
        // GIVEN an accepted backend value.
        // WHEN parse_value_for is called.
        let v = parse_value_for("web_search", &["backend"], "duckduckgo").expect("parse");
        // THEN the TOML value is the expected string.
        assert_eq!(v.as_str(), Some("duckduckgo"));
    }

    #[test]
    fn parse_value_for_unknown_key_returns_help() {
        // GIVEN an unknown key.
        let err = parse_value_for("web_search", &["plouf"], "x").unwrap_err();
        // THEN the message lists the valid keys.
        assert!(err.contains("valid keys"));
    }

    #[test]
    fn parse_value_for_int_bounds_enforced() {
        // GIVEN an out-of-range integer for brave.timeout_secs.
        let err = parse_value_for("web_search", &["brave", "timeout_secs"], "999").unwrap_err();
        // THEN the bounds are reported.
        assert!(err.contains("hors bornes"));
    }

    #[test]
    fn config_set_writes_toml_section() {
        // GIVEN an empty TOML file.
        let dir = TempDir::new().expect("tempdir");
        let path = write_toml(&dir, "");

        // WHEN we inject tools.web_search.backend = "duckduckgo".
        let mut doc: DocumentMut = "".parse().expect("empty parse");
        let value = parse_value_for("web_search", &["backend"], "duckduckgo").expect("parse");
        set_nested_value(&mut doc, &["tools", "web_search"], &["backend"], value);
        std::fs::write(&path, doc.to_string()).expect("write");

        // THEN the file contains the expected section.
        let read = std::fs::read_to_string(&path).expect("read");
        assert!(read.contains("[tools.web_search]"));
        assert!(read.contains("backend = \"duckduckgo\""));
    }

    #[test]
    fn known_tool_check_recognizes_native_set() {
        // GIVEN the NATIVE_TOOL_NAMES set.
        // THEN bash_executor is known, but "fake_tool" is not.
        assert!(is_known_tool("bash_executor"));
        assert!(!is_known_tool("fake_tool"));
    }

    #[test]
    fn credential_target_accepts_agent_namespace() {
        // GIVEN a credential target.
        // WHEN it is a native tool, the agent namespace, or an unknown name.
        // THEN native tools and the agent namespace are valid targets, others not.
        assert!(is_valid_credential_target("web_search"));
        assert!(is_valid_credential_target(AGENT_CREDENTIALS_NAMESPACE));
        assert!(is_valid_credential_target("agent"));
        assert!(!is_valid_credential_target("fake_tool"));
        // The agent namespace is not itself a native tool.
        assert!(!is_known_tool(AGENT_CREDENTIALS_NAMESPACE));
    }

    #[test]
    fn agent_secret_roundtrips_through_the_credential_store() {
        // GIVEN a fresh credential store (same path the CLI resolves).
        let dir = TempDir::new().expect("tempdir");
        apollia_tools::GovernanceDb::open(dir.path()).expect("init governance");
        let mut store = ToolCredentialStore::new(&db_path(dir.path()), &keyfile_path(dir.path()))
            .expect("open store");
        // WHEN a secret is stored under the agent namespace and read back.
        store
            .set(AGENT_CREDENTIALS_NAMESPACE, "hubspot_api_token", "sk-demo")
            .expect("set agent secret");
        // THEN it is retrievable, proving the CLI path writes where agents read.
        let got = store
            .get(AGENT_CREDENTIALS_NAMESPACE, "hubspot_api_token")
            .expect("get agent secret");
        assert_eq!(got.as_deref(), Some("sk-demo"));
    }

    #[test]
    fn list_includes_all_native_tools_via_registry() {
        // GIVEN a fresh data_dir.
        let dir = TempDir::new().expect("tempdir");
        apollia_tools::GovernanceDb::open(dir.path()).expect("init governance");
        let reg = NativeToolRegistry::new(&dir.path().join(GOVERNANCE_DB_FILENAME))
            .expect("open registry");
        // WHEN we list.
        let entries = reg.list().expect("list");
        // THEN all native tools are present.
        for native in NATIVE_TOOL_NAMES {
            assert!(
                entries.iter().any(|e| e.name == *native),
                "outil natif manquant : {native}"
            );
        }
    }

    #[test]
    fn disable_then_enable_roundtrip() {
        // GIVEN a fresh registry.
        let dir = TempDir::new().expect("tempdir");
        apollia_tools::GovernanceDb::open(dir.path()).expect("init governance");
        let mut reg = NativeToolRegistry::new(&dir.path().join(GOVERNANCE_DB_FILENAME))
            .expect("open registry");
        // WHEN we disable then re-enable bash_executor.
        reg.set_enabled("bash_executor", false).expect("disable");
        assert!(!reg.is_enabled("bash_executor").expect("read"));
        reg.set_enabled("bash_executor", true).expect("enable");
        // THEN the tool is active again.
        assert!(reg.is_enabled("bash_executor").expect("read"));
    }

    #[test]
    fn credential_set_then_list_masks_value() {
        // GIVEN a fresh store with one credential.
        let dir = TempDir::new().expect("tempdir");
        apollia_tools::GovernanceDb::open(dir.path()).expect("init governance");
        let mut store = ToolCredentialStore::new(
            &dir.path().join(GOVERNANCE_DB_FILENAME),
            &dir.path().join(".keyfile"),
        )
        .expect("store");
        store
            .set("web_search", "brave.api_key", "BSA-test")
            .expect("set");
        // WHEN we list the entries.
        let entries = store.list(None).expect("list");
        // THEN one entry exists without exposing the cleartext value.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_name, "web_search");
        assert_eq!(entries[0].key_name, "brave.api_key");
    }

    #[test]
    fn backend_label_reflects_config() {
        // GIVEN a default config (auto).
        let cfg = ToolsConfig::default();
        // THEN web_search shows "DuckDuckGo (auto)".
        assert_eq!(backend_label("web_search", &cfg), "DuckDuckGo (auto)");
        assert_eq!(backend_label("file_read", &cfg), "-");
    }
}
