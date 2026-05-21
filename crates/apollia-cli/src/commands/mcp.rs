//! `apollia-os mcp` subcommands — MCP server management, discovery, and HITL approvals.
//!
//! Les sous-commandes `add`, `remove`, `get`, `test`, `restart` communiquent avec
//! le runtime via socket Unix. Les autres opèrent sur la config locale.

use std::io::IsTerminal as _;
use std::path::PathBuf;

use clap::Subcommand;

use apollia_mcp::approvals::McpApprovalStore;
use apollia_mcp::config::McpConfig;
use apollia_mcp::discovery;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// MCP server subcommands.
#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// List configured and, optionally, discovered MCP servers.
    List {
        /// Scan the local network via mDNS and append discovered servers.
        ///
        /// Performs a 3-second broadcast scan for `_apollia-mcp._tcp.local.`
        /// in addition to listing servers from the configuration file.
        #[arg(long)]
        discover: bool,

        /// Path to the MCP configuration file (default: `~/.apollia/mcp.toml`).
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Approve all calls to a tool on a server, persisted with the configured TTL.
    ///
    /// After approval, calls to `<tool>` on `<server>` bypass the HITL suspension
    /// gate until the approval expires (default TTL: 24 h, configurable in apollia.toml).
    SetApproval {
        /// MCP server name (as declared in mcp.toml).
        server: String,

        /// Tool name to approve.
        tool: String,

        /// Path to the approvals database (default: `~/.apollia/mcp_approvals.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,

        /// Override the TTL for this approval, in hours (0 = never expires).
        #[arg(long, value_name = "HOURS", default_value_t = 24)]
        ttl_hours: u64,

        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// List all pending HITL approval requests awaiting human decision.
    ListPending {
        /// Path to the approvals database (default: `~/.apollia/mcp_approvals.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,

        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Revoke a previously granted tool approval.
    ///
    /// After revocation, calls to `<tool>` on `<server>` will be suspended again
    /// until a new approval is granted with `set-approval`.
    RevokeApproval {
        /// MCP server name (as declared in mcp.toml).
        server: String,

        /// Tool name to revoke.
        tool: String,

        /// Path to the approvals database (default: `~/.apollia/mcp_approvals.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,

        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Ajouter un serveur MCP au runtime (persiste dans la config).
    Add {
        /// Nom unique du serveur.
        name: String,
        /// Commande à lancer (transport stdio) ou URL (transport HTTP/SSE).
        #[arg(long)]
        command: Option<String>,
        /// URL de connexion HTTP/SSE.
        #[arg(long)]
        url: Option<String>,
        /// Exiger une approbation HITL pour chaque outil.
        #[arg(long)]
        require_approval: bool,
    },

    /// Retirer un serveur MCP du runtime.
    Remove {
        /// Nom du serveur.
        name: String,
        /// Confirmer sans prompt interactif.
        #[arg(long)]
        confirm: bool,
    },

    /// Afficher les détails d'un serveur MCP.
    Get {
        /// Nom du serveur.
        name: String,
    },

    /// Tester la connexion à un serveur MCP.
    Test {
        /// URL ou commande à tester.
        target: String,
    },

    /// Redémarrer un serveur MCP.
    Restart {
        /// Nom du serveur.
        name: String,
    },

    /// Mettre à jour la configuration brute d'un serveur MCP existant.
    ///
    /// Au moins un des champs `--command`, `--url`, ou `--require-approval`
    /// doit être fourni. Les champs omis conservent leur valeur précédente.
    Update {
        /// Nom du serveur.
        name: String,
        /// Nouvelle commande stdio (transport stdio).
        #[arg(long)]
        command: Option<String>,
        /// Nouvelle URL HTTP/SSE.
        #[arg(long)]
        url: Option<String>,
        /// Activer / désactiver le verrou d'approbation HITL.
        #[arg(long, value_name = "BOOL")]
        require_approval: Option<bool>,
    },

    /// Afficher la configuration brute persistée d'un serveur MCP.
    ///
    /// Lit directement `mcp.db` via le runtime et renvoie la définition
    /// originale (utile pour bisecter une régression de configuration).
    RawConfig {
        /// Nom du serveur.
        name: String,
    },
}

/// Errors returned by MCP subcommands.
#[derive(Debug, thiserror::Error)]
pub enum McpCommandError {
    /// The MCP configuration file could not be loaded or parsed.
    #[error("failed to load MCP config: {0}")]
    ConfigLoad(String),

    /// The mDNS discovery scan failed.
    #[error("mDNS discovery error: {0}")]
    Discovery(#[from] apollia_mcp::DiscoveryError),

    /// The approval database could not be opened or written.
    #[error("approval store error: {0}")]
    Approval(#[from] apollia_mcp::McpApprovalError),
}

/// Entry point for `apollia-os mcp <subcommand>`.
pub async fn run(command: &McpCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match command {
        McpCommand::List {
            discover,
            config,
            json: cmd_json,
        } => {
            let use_json = json || *cmd_json;
            let client = make_runtime_client(socket.clone());
            match run_list(&client, *discover, config.as_deref(), use_json).await {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            }
        }

        McpCommand::SetApproval {
            server,
            tool,
            db,
            ttl_hours,
            json: cmd_json,
        } => {
            let use_json = json || *cmd_json;
            match run_set_approval(server, tool, db.as_deref(), *ttl_hours, use_json) {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            }
        }

        McpCommand::ListPending { db, json: cmd_json } => {
            let use_json = json || *cmd_json;
            match run_list_pending(db.as_deref(), use_json) {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            }
        }

        McpCommand::RevokeApproval {
            server,
            tool,
            db,
            json: cmd_json,
        } => {
            let use_json = json || *cmd_json;
            match run_revoke_approval(server, tool, db.as_deref(), use_json) {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            }
        }

        McpCommand::Add {
            name,
            command,
            url,
            require_approval,
        } => {
            let client = make_runtime_client(socket);
            run_add(
                &client,
                name,
                command.as_deref(),
                url.as_deref(),
                *require_approval,
                json,
            )
            .await
        }

        McpCommand::Remove { name, confirm } => {
            let client = make_runtime_client(socket);
            run_remove(&client, name, *confirm, json).await
        }

        McpCommand::Get { name } => {
            let client = make_runtime_client(socket);
            run_get_server(&client, name, json).await
        }

        McpCommand::Test { target } => {
            let client = make_runtime_client(socket);
            run_test_connection(&client, target, json).await
        }

        McpCommand::Restart { name } => {
            let client = make_runtime_client(socket);
            run_restart_server(&client, name, json).await
        }

        McpCommand::Update {
            name,
            command,
            url,
            require_approval,
        } => {
            let client = make_runtime_client(socket);
            run_update_server(
                &client,
                name,
                command.as_deref(),
                url.as_deref(),
                *require_approval,
                json,
            )
            .await
        }

        McpCommand::RawConfig { name } => {
            let client = make_runtime_client(socket);
            run_get_raw_config(&client, name, json).await
        }
    }
}

/// Create a RuntimeClient from an optional socket path.
fn make_runtime_client(socket: Option<PathBuf>) -> RuntimeClient {
    let path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    RuntimeClient::new(path)
}

/// Gestion uniforme des erreurs client MCP (socket Unix).
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

// ─── Runtime-based CRUD handlers ──────────────────────────────────────────────

/// `apollia-os mcp add <name>` — ajouter un serveur MCP au runtime.
async fn run_add(
    client: &RuntimeClient,
    name: &str,
    command: Option<&str>,
    url: Option<&str>,
    require_approval: bool,
    json: bool,
) -> i32 {
    let mut body = serde_json::json!({
        "name": name,
        "require_approval": require_approval,
    });
    if let Some(cmd) = command {
        body["command"] = serde_json::Value::String(cmd.to_string());
    }
    if let Some(u) = url {
        body["url"] = serde_json::Value::String(u.to_string());
    }

    match client.add_mcp_server(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Serveur MCP '{name}' ajouté au runtime");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp remove <name>` — retirer un serveur MCP du runtime.
async fn run_remove(client: &RuntimeClient, name: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let output = serde_json::json!({"error": "use --confirm to remove without prompt"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Utiliser --confirm pour retirer le serveur '{name}' sans confirmation.");
        }
        return exit_codes::GENERAL_ERROR;
    }

    match client.remove_mcp_server(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Serveur MCP '{name}' retiré du runtime");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: serveur MCP '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp get <name>` — afficher les détails d'un serveur MCP.
async fn run_get_server(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.get_mcp_server_detail(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                let transport = resp
                    .get("transport")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                println!("  Serveur   : {name}");
                println!("  Transport : {transport}");
                println!("  Statut    : {status}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: serveur MCP '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp test <target>` — tester la connexion à un serveur MCP.
async fn run_test_connection(client: &RuntimeClient, target: &str, json: bool) -> i32 {
    let body = serde_json::json!({ "target": target });
    match client.test_mcp_connection(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let latency = resp.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                if ok {
                    println!("✔ Connexion réussie ({latency}ms)");
                } else {
                    let err = resp
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    println!("✗ Connexion échouée: {err}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp restart <name>` — redémarrer un serveur MCP.
async fn run_restart_server(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.restart_mcp_server(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Serveur MCP '{name}' redémarré");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: serveur MCP '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp update <name>` — patch a server configuration.
///
/// Fails when no patch field is supplied; otherwise forwards a partial body to
/// `PUT /api/v1/mcp/servers/{name}/config`. The runtime merges with the
/// existing stored definition.
async fn run_update_server(
    client: &RuntimeClient,
    name: &str,
    command: Option<&str>,
    url: Option<&str>,
    require_approval: Option<bool>,
    json: bool,
) -> i32 {
    if command.is_none() && url.is_none() && require_approval.is_none() {
        if json {
            let out = serde_json::json!({
                "error": "no patch field provided",
                "hint": "supply at least one of --command, --url, --require-approval",
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            eprintln!("Error: provide at least one of --command, --url, --require-approval");
        }
        return exit_codes::GENERAL_ERROR;
    }

    let mut body = serde_json::Map::new();
    if let Some(c) = command {
        body.insert("command".to_string(), serde_json::Value::String(c.to_string()));
    }
    if let Some(u) = url {
        body.insert("url".to_string(), serde_json::Value::String(u.to_string()));
    }
    if let Some(req) = require_approval {
        body.insert(
            "require_approval".to_string(),
            serde_json::Value::Bool(req),
        );
    }

    match client
        .update_mcp_server_config(name, &serde_json::Value::Object(body))
        .await
    {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("* Serveur MCP '{name}' mis à jour");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: serveur MCP '{name}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp raw-config <name>` — read the persisted launch definition.
async fn run_get_raw_config(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    let uri = format!("/api/v1/mcp/servers/{name}/raw_config");
    match client.get(&uri).await {
        Ok(resp) => {
            if resp.status >= 400 {
                if json {
                    let out = serde_json::json!({"error": resp.body});
                    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
                } else {
                    eprintln!("Error: {}", resp.body);
                }
                if resp.status == 404 {
                    return exit_codes::GENERAL_ERROR;
                }
                return exit_codes::GENERAL_ERROR;
            }
            // Body is JSON already — pretty-print when --json, raw otherwise.
            match serde_json::from_str::<serde_json::Value>(&resp.body) {
                Ok(v) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    } else {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or(resp.body));
                    }
                }
                Err(_) => {
                    println!("{}", resp.body);
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

// ─── list ────────────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp list [--discover]`.
///
/// Hits the runtime API first so the listing reflects the same servers the
/// Desktop app sees (persisted in `mcp.db` via `apollia_mcp::McpServerRepository`).
/// Falls back to reading the legacy `mcp.toml` on disk when:
/// - `--config` is explicitly supplied (operator opted into the legacy path), or
/// - the runtime is not running (so `mcp list` stays useful as a quick local probe).
async fn run_list(
    client: &RuntimeClient,
    discover: bool,
    config_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    if config_path.is_none() {
        match client.get("/api/v1/mcp/servers").await {
            Ok(resp) if resp.status < 400 => {
                let servers: serde_json::Value = serde_json::from_str(&resp.body)
                    .map_err(|e| McpCommandError::ConfigLoad(e.to_string()))?;
                return if json {
                    format_runtime_list_json(&servers, discover).await
                } else {
                    format_runtime_list_human(&servers, discover).await
                };
            }
            Ok(resp) => {
                return Err(McpCommandError::ConfigLoad(format!(
                    "runtime returned HTTP {}: {}",
                    resp.status, resp.body
                )));
            }
            Err(ClientError::ConnectionRefused) => {
                // Runtime offline — fall through to the local mcp.toml fallback below.
            }
            Err(e) => {
                return Err(McpCommandError::ConfigLoad(e.to_string()));
            }
        }
    }

    let path = resolve_config_path(config_path);
    let config = McpConfig::load(&path).map_err(|e| McpCommandError::ConfigLoad(e.to_string()))?;

    if json {
        format_list_json(&config, discover).await
    } else {
        format_list_human(&config, discover).await
    }
}

/// Render `GET /api/v1/mcp/servers` as a JSON envelope including optional mDNS discovery.
async fn format_runtime_list_json(
    servers: &serde_json::Value,
    discover: bool,
) -> Result<String, McpCommandError> {
    let discovered = if discover {
        discovery::discover_mcp_servers().await?
    } else {
        vec![]
    };
    let envelope = serde_json::json!({
        "servers": servers,
        "discovered": discovered,
        "source": "runtime",
    });
    Ok(serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string()))
}

/// Render `GET /api/v1/mcp/servers` as a human-readable table.
///
/// The runtime endpoint only returns *connected* servers. We supplement with
/// `~/.apollia/mcp.db` so disconnected ones (e.g. failed OAuth, missing
/// command) still appear with a clear status — matching the Desktop view.
async fn format_runtime_list_human(
    servers: &serde_json::Value,
    discover: bool,
) -> Result<String, McpCommandError> {
    let live = parse_runtime_servers(servers);
    let configured = read_configured_servers();
    let merged = merge_runtime_and_configured(&live, &configured);

    let mut out = String::new();
    if merged.is_empty() {
        out.push_str("No MCP servers configured.\n");
    } else {
        out.push_str("MCP servers:\n");
        out.push_str(&format!(
            "  {:<24} {:<10} {:<14} TOOLS\n",
            "NAME", "TRANSPORT", "STATUS"
        ));
        for row in &merged {
            out.push_str(&format!(
                "  {:<24} {:<10} {:<14} {}\n",
                row.name, row.transport, row.status, row.tools
            ));
        }
    }

    if discover {
        out.push('\n');
        out.push_str("Scanning local network for MCP servers (3s)...\n");
        let discovered = discovery::discover_mcp_servers().await?;
        if discovered.is_empty() {
            out.push_str("No discovered servers on the local network.\n");
        } else {
            out.push_str("\nDiscovered (mDNS):\n");
            for s in &discovered {
                let addrs = s.addresses.join(", ");
                out.push_str(&format!("  {}    {}:{}\n", s.name, addrs, s.port));
            }
        }
    }
    Ok(out)
}

/// Single row produced by [`merge_runtime_and_configured`] for table rendering.
struct McpListRow {
    name: String,
    transport: String,
    status: String,
    tools: String,
}

/// Extract a `(name -> McpServerStatus)`-ish map from the raw runtime JSON.
fn parse_runtime_servers(servers: &serde_json::Value) -> Vec<serde_json::Value> {
    servers
        .get("servers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_else(|| servers.as_array().cloned().unwrap_or_default())
}

/// Open `~/.apollia/mcp.db` and list every persisted server config (enabled or not).
/// Returns an empty vec on any error — the live runtime list still wins.
fn read_configured_servers() -> Vec<apollia_mcp::config::McpServerConfig> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let db = home.join(".apollia").join("mcp.db");
    if !db.exists() {
        return Vec::new();
    }
    match apollia_mcp::McpServerRepository::open(&db) {
        Ok(repo) => repo.list().unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Build the final table rows from the live status list and the persisted
/// configuration. Connected servers report `connected` + tools count; missing
/// servers report `not connected` (or `disabled` when explicitly disabled in
/// the config).
fn merge_runtime_and_configured(
    live: &[serde_json::Value],
    configured: &[apollia_mcp::config::McpServerConfig],
) -> Vec<McpListRow> {
    use std::collections::BTreeMap;

    let mut by_name: BTreeMap<String, McpListRow> = BTreeMap::new();

    for cfg in configured {
        let transport = if cfg.transport.is_empty() {
            "stdio".to_string()
        } else {
            cfg.transport.clone()
        };
        by_name.insert(
            cfg.name.clone(),
            McpListRow {
                name: cfg.name.clone(),
                transport,
                status: "not connected".to_string(),
                tools: "-".to_string(),
            },
        );
    }

    for s in live {
        let name = s
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let transport = s
            .get("transport")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio")
            .to_string();
        let connected = s
            .get("connected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tools = s
            .get("tools_count")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .or_else(|| {
                s.get("tools")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len().to_string())
            })
            .unwrap_or_else(|| "-".to_string());
        let status = if connected { "connected" } else { "disconnected" };
        by_name.insert(
            name.clone(),
            McpListRow {
                name,
                transport,
                status: status.to_string(),
                tools,
            },
        );
    }

    by_name.into_values().collect()
}

/// Formats the list output as JSON.
async fn format_list_json(config: &McpConfig, discover: bool) -> Result<String, McpCommandError> {
    #[derive(serde::Serialize)]
    struct Output<'a> {
        configured: &'a [apollia_mcp::config::McpServerConfig],
        discovered: Vec<apollia_mcp::DiscoveredServer>,
    }

    let discovered = if discover {
        discovery::discover_mcp_servers().await?
    } else {
        vec![]
    };

    let output = Output {
        configured: &config.servers,
        discovered,
    };

    Ok(serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string()))
}

/// Formats the list output for human-readable terminal display.
async fn format_list_human(config: &McpConfig, discover: bool) -> Result<String, McpCommandError> {
    let mut out = String::new();
    let tty = std::io::stdout().is_terminal();

    // ── Configured servers ────────────────────────────────────────────────
    if config.servers.is_empty() {
        out.push_str("No MCP servers configured in mcp.toml.\n");
    } else {
        out.push_str("Configured MCP servers:\n");
        for s in &config.servers {
            let approval = if s.requires_approval {
                " [approval]"
            } else {
                ""
            };
            let cmd = if s.command.is_empty() {
                s.url.as_deref().unwrap_or("").to_string()
            } else {
                format!("{} {}", s.command, s.args.join(" "))
            };
            if tty {
                out.push_str(&format!(
                    "  \x1b[1m{}\x1b[0m    {}    {}{}\n",
                    s.name, s.transport, cmd, approval
                ));
            } else {
                out.push_str(&format!(
                    "  {}    {}    {}{}\n",
                    s.name, s.transport, cmd, approval
                ));
            }
        }
    }

    if !discover {
        return Ok(out);
    }

    // ── mDNS discovery ────────────────────────────────────────────────────
    out.push('\n');
    out.push_str("Scanning local network for MCP servers (3s)...\n");

    let discovered = discovery::discover_mcp_servers().await?;

    if discovered.is_empty() {
        out.push_str("Aucun serveur MCP découvert sur le réseau local.\n");
    } else {
        out.push_str("\nDiscovered MCP servers:\n");
        for s in &discovered {
            let addrs = s.addresses.join(", ");
            let tools = if s.tools.is_empty() {
                String::new()
            } else {
                format!("    tools: {}", s.tools.join(", "))
            };
            if tty {
                out.push_str(&format!(
                    "  \x1b[1m{}\x1b[0m    {}:{}{}  \n",
                    s.name, addrs, s.port, tools
                ));
            } else {
                out.push_str(&format!("  {}    {}:{}{}\n", s.name, addrs, s.port, tools));
            }
        }
    }

    Ok(out)
}

// ─── set-approval ────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp set-approval <server> <tool>`.
fn run_set_approval(
    server: &str,
    tool: &str,
    db_path: Option<&std::path::Path>,
    ttl_hours: u64,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, ttl_hours)?;
    store.approve(server, tool)?;

    if json {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "server": server,
            "tool": tool,
            "ttl_hours": ttl_hours,
            "approved": true,
        }))
        .unwrap_or_else(|_| "{}".to_string()))
    } else {
        let expiry = if ttl_hours == 0 {
            "never".to_string()
        } else {
            format!("in {ttl_hours}h")
        };
        Ok(format!("Approved: {server}/{tool}  (expires {expiry})"))
    }
}

// ─── list-pending ─────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp list-pending`.
fn run_list_pending(
    db_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, 24)?;
    let entries = store.list_pending()?;

    if json {
        Ok(serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string()))
    } else {
        format_pending_human(&entries)
    }
}

/// Human-readable formatter for pending approval entries.
fn format_pending_human(
    entries: &[apollia_mcp::PendingApprovalEntry],
) -> Result<String, McpCommandError> {
    if entries.is_empty() {
        return Ok("No pending approval requests.\n".to_string());
    }

    let tty = std::io::stdout().is_terminal();
    let mut out = format!("{} pending approval request(s):\n", entries.len());

    for e in entries {
        if tty {
            out.push_str(&format!(
                "  \x1b[33m[{}]\x1b[0m  {}/{}  requested_at={}\n",
                e.id, e.server_name, e.tool_name, e.requested_at,
            ));
        } else {
            out.push_str(&format!(
                "  [{}]  {}/{}  requested_at={}\n",
                e.id, e.server_name, e.tool_name, e.requested_at,
            ));
        }
    }

    out.push_str("\nRun `apollia mcp set-approval <server> <tool>` to approve.\n");
    Ok(out)
}

// ─── revoke-approval ─────────────────────────────────────────────────────────

/// Implements `apollia-os mcp revoke-approval <server> <tool>`.
fn run_revoke_approval(
    server: &str,
    tool: &str,
    db_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, 24)?;
    store.revoke(server, tool)?;

    if json {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "server": server,
            "tool": tool,
            "revoked": true,
        }))
        .unwrap_or_else(|_| "{}".to_string()))
    } else {
        Ok(format!("Revoked: {server}/{tool}"))
    }
}

// ─── path resolution ─────────────────────────────────────────────────────────

/// Returns the effective path to `mcp.toml`.
///
/// Uses the caller-supplied path when present; otherwise falls back to
/// `~/.apollia/mcp.toml` (or `.apollia/mcp.toml` relative to the current
/// directory when the home directory cannot be determined).
fn resolve_config_path(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("mcp.toml")
}

/// Returns the effective path to the approvals SQLite database.
///
/// Uses the caller-supplied path when present; otherwise falls back to
/// `~/.apollia/mcp_approvals.db`.
fn resolve_approvals_db_path(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("mcp_approvals.db")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: McpCommand,
    }

    #[test]
    fn test_mcp_add_parses() {
        // GIVEN "add code-tools --command 'npx @modelcontextprotocol/server-filesystem'"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "add",
            "code-tools",
            "--command",
            "npx @modelcontextprotocol/server-filesystem",
        ]);
        // THEN McpCommand::Add avec les bons champs
        match &cli.command {
            McpCommand::Add {
                name,
                command,
                url,
                require_approval,
            } => {
                assert_eq!(name, "code-tools");
                assert_eq!(
                    command.as_deref(),
                    Some("npx @modelcontextprotocol/server-filesystem")
                );
                assert!(url.is_none());
                assert!(!require_approval);
            }
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_add_with_url_and_approval_parses() {
        // GIVEN "add my-server --url http://localhost:8080 --require-approval"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "add",
            "my-server",
            "--url",
            "http://localhost:8080",
            "--require-approval",
        ]);
        // THEN require_approval = true, url set
        match &cli.command {
            McpCommand::Add {
                url,
                require_approval,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("http://localhost:8080"));
                assert!(require_approval);
            }
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_remove_confirm_parses() {
        // GIVEN "remove code-tools --confirm"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "remove", "code-tools", "--confirm"]);
        // THEN Remove { name: "code-tools", confirm: true }
        match &cli.command {
            McpCommand::Remove { name, confirm } => {
                assert_eq!(name, "code-tools");
                assert!(confirm);
            }
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_get_parses() {
        // GIVEN "get code-tools"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "get", "code-tools"]);
        // THEN McpCommand::Get { name: "code-tools" }
        match &cli.command {
            McpCommand::Get { name } => assert_eq!(name, "code-tools"),
            other => panic!("expected Get, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_test_parses() {
        // GIVEN "test http://localhost:8080"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "test", "http://localhost:8080"]);
        // THEN McpCommand::Test { target: "http://localhost:8080" }
        match &cli.command {
            McpCommand::Test { target } => assert_eq!(target, "http://localhost:8080"),
            other => panic!("expected Test, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_restart_parses() {
        // GIVEN "restart code-tools"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "restart", "code-tools"]);
        // THEN McpCommand::Restart { name: "code-tools" }
        match &cli.command {
            McpCommand::Restart { name } => assert_eq!(name, "code-tools"),
            other => panic!("expected Restart, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_update_with_url() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "code-tools",
            "--url",
            "http://localhost:9090",
        ]);
        match &cli.command {
            McpCommand::Update {
                name,
                command,
                url,
                require_approval,
            } => {
                assert_eq!(name, "code-tools");
                assert!(command.is_none());
                assert_eq!(url.as_deref(), Some("http://localhost:9090"));
                assert!(require_approval.is_none());
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_update_require_approval_flag() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "srv",
            "--require-approval",
            "true",
        ]);
        match &cli.command {
            McpCommand::Update {
                require_approval, ..
            } => {
                assert_eq!(*require_approval, Some(true));
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_raw_config_parses() {
        let cli = TestCli::parse_from(["apollia-os", "raw-config", "code-tools"]);
        match &cli.command {
            McpCommand::RawConfig { name } => assert_eq!(name, "code-tools"),
            other => panic!("expected RawConfig, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_config_path_with_override() {
        // GIVEN an explicit path override
        let p = std::path::Path::new("/tmp/custom-mcp.toml");
        // WHEN resolving
        let result = resolve_config_path(Some(p));
        // THEN the override is returned verbatim
        assert_eq!(result, PathBuf::from("/tmp/custom-mcp.toml"));
    }

    #[test]
    fn test_resolve_config_path_default_ends_with_mcp_toml() {
        // GIVEN no override
        // WHEN resolving
        let result = resolve_config_path(None);
        // THEN the path ends with .apollia/mcp.toml
        assert!(
            result.to_string_lossy().ends_with(".apollia/mcp.toml"),
            "unexpected path: {result:?}"
        );
    }

    #[test]
    fn test_resolve_approvals_db_path_with_override() {
        // GIVEN an explicit path override
        let p = std::path::Path::new("/tmp/my_approvals.db");
        // WHEN
        let result = resolve_approvals_db_path(Some(p));
        // THEN the override is returned verbatim
        assert_eq!(result, PathBuf::from("/tmp/my_approvals.db"));
    }

    #[test]
    fn test_resolve_approvals_db_path_default_ends_with_db() {
        // GIVEN no override
        // WHEN
        let result = resolve_approvals_db_path(None);
        // THEN path ends with .apollia/mcp_approvals.db
        assert!(
            result
                .to_string_lossy()
                .ends_with(".apollia/mcp_approvals.db"),
            "unexpected path: {result:?}"
        );
    }

    // ── set-approval / revoke round-trip ─────────────────────────────────────

    #[test]
    fn test_set_approval_then_revoke_round_trip() {
        // GIVEN a temp db
        let tmp = tempfile::NamedTempFile::new().expect("tmp file");
        // WHEN approved
        let out = run_set_approval("code-tools", "bash_exec", Some(tmp.path()), 24, false)
            .expect("set-approval must succeed");
        assert!(out.contains("Approved"));
        // AND revoked
        let out = run_revoke_approval("code-tools", "bash_exec", Some(tmp.path()), false)
            .expect("revoke must succeed");
        assert!(out.contains("Revoked"));
        // THEN is_approved returns false
        let store = McpApprovalStore::open(tmp.path(), 24).expect("store");
        assert!(!store.is_approved("code-tools", "bash_exec"));
    }

    #[test]
    fn test_set_approval_json_output() {
        // GIVEN a temp db
        let tmp = tempfile::NamedTempFile::new().expect("tmp file");
        // WHEN
        let out =
            run_set_approval("srv", "tool", Some(tmp.path()), 24, true).expect("set-approval json");
        // THEN JSON is valid and approved=true
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["approved"], true);
        assert_eq!(v["server"], "srv");
        assert_eq!(v["tool"], "tool");
    }

    #[test]
    fn test_list_pending_empty() {
        // GIVEN a fresh db
        let tmp = tempfile::NamedTempFile::new().expect("tmp file");
        // WHEN
        let out = run_list_pending(Some(tmp.path()), false).expect("list-pending");
        // THEN shows empty message
        assert!(out.contains("No pending"));
    }

    #[test]
    fn test_list_pending_with_entry() {
        // GIVEN a db with one registered pending request
        let tmp = tempfile::NamedTempFile::new().expect("tmp file");
        let store = McpApprovalStore::open(tmp.path(), 24).expect("store");
        store
            .register("code-tools", "bash_exec", &serde_json::json!({}))
            .expect("register");
        // WHEN
        let out = run_list_pending(Some(tmp.path()), false).expect("list-pending");
        // THEN the entry appears
        assert!(out.contains("code-tools"));
        assert!(out.contains("bash_exec"));
    }
}
