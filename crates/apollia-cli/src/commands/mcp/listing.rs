//! `mcp list`, with and without the runtime, and its rendering.

use std::io::IsTerminal as _;

use apollia_mcp::config::McpConfig;
use apollia_mcp::discovery;

use crate::client::{ClientError, RuntimeClient};

use super::approvals::resolve_config_path;
use super::McpCommandError;

// ─── list ────────────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp list [--discover]`.
///
/// Hits the runtime API first so the listing reflects the same servers the
/// Desktop app sees (persisted in `mcp.db` via `apollia_mcp::McpServerRepository`).
/// Falls back to reading the legacy `mcp.toml` on disk when:
/// - `--config` is explicitly supplied (operator opted into the legacy path), or
/// - the runtime is not running (so `mcp list` stays useful as a quick local probe).
pub(super) async fn run_list(
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
                // Runtime offline: fall through to the local mcp.toml fallback below.
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
pub(super) async fn format_runtime_list_json(
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
/// command) still appear with a clear status, matching the Desktop view.
pub(super) async fn format_runtime_list_human(
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
pub(super) struct McpListRow {
    name: String,
    transport: String,
    status: String,
    tools: String,
}

/// Extract a `(name -> McpServerStatus)`-ish map from the raw runtime JSON.
pub(super) fn parse_runtime_servers(servers: &serde_json::Value) -> Vec<serde_json::Value> {
    servers
        .get("servers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_else(|| servers.as_array().cloned().unwrap_or_default())
}

/// Open `~/.apollia/mcp.db` and list every persisted server config (enabled or not).
/// Returns an empty vec on any error; the live runtime list still wins.
pub(super) fn read_configured_servers() -> Vec<apollia_mcp::config::McpServerConfig> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let db = apollia_core::paths::data_dir_under(home)
        .join(apollia_core::paths::DataFile::Mcp.file_name());
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
pub(super) fn merge_runtime_and_configured(
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
        let status = if connected {
            "connected"
        } else {
            "disconnected"
        };
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
pub(super) async fn format_list_json(
    config: &McpConfig,
    discover: bool,
) -> Result<String, McpCommandError> {
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

/// Renders the configured-servers section of the human list output.
pub(super) fn push_configured_servers(out: &mut String, config: &McpConfig, tty: bool) {
    if config.servers.is_empty() {
        out.push_str("No MCP servers configured in mcp.toml.\n");
        return;
    }
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

/// Renders the discovered-servers section of the human list output.
pub(super) fn push_discovered_servers(
    out: &mut String,
    discovered: &[discovery::DiscoveredServer],
    tty: bool,
) {
    if discovered.is_empty() {
        out.push_str("No MCP server discovered on the local network.\n");
        return;
    }
    out.push_str("\nDiscovered MCP servers:\n");
    for s in discovered {
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

/// Formats the list output for human-readable terminal display.
pub(super) async fn format_list_human(
    config: &McpConfig,
    discover: bool,
) -> Result<String, McpCommandError> {
    let mut out = String::new();
    let tty = std::io::stdout().is_terminal();

    // ── Configured servers ────────────────────────────────────────────────
    push_configured_servers(&mut out, config, tty);

    if !discover {
        return Ok(out);
    }

    // ── mDNS discovery ────────────────────────────────────────────────────
    out.push('\n');
    out.push_str("Scanning local network for MCP servers (3s)...\n");

    let discovered = discovery::discover_mcp_servers().await?;
    push_discovered_servers(&mut out, &discovered, tty);

    Ok(out)
}
