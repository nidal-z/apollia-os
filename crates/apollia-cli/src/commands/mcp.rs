//! `apollia-os mcp` subcommands — MCP server management and discovery.

use std::io::IsTerminal as _;
use std::path::PathBuf;

use clap::Subcommand;

use apollia_mcp::config::McpConfig;
use apollia_mcp::discovery;

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
}

/// Entry point for `apollia-os mcp <subcommand>`.
pub async fn run(command: &McpCommand, json: bool) -> i32 {
    match command {
        McpCommand::List {
            discover,
            config,
            json: cmd_json,
        } => {
            let use_json = json || *cmd_json;
            match run_list(*discover, config.as_deref(), use_json).await {
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
    }
}

/// Implements `apollia-os mcp list [--discover]`.
async fn run_list(
    discover: bool,
    config_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_config_path(config_path);
    let config = McpConfig::load(&path).map_err(|e| McpCommandError::ConfigLoad(e.to_string()))?;

    if json {
        format_list_json(&config, discover).await
    } else {
        format_list_human(&config, discover).await
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
