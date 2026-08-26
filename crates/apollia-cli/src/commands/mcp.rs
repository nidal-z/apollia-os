//! `apollia-os mcp` subcommands: MCP server management, discovery, and HITL approvals.
//!
//! The `add`, `remove`, `get`, `test`, and `restart` sub-commands talk to the
//! runtime over the Unix socket. The others operate on the local config.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod approvals;
mod listing;
mod secrets;
mod servers;

use approvals::{run_list_pending, run_revoke_approval, run_set_approval};
use listing::run_list;
use secrets::run_secret;
use servers::{
    run_add, run_get_raw_config, run_get_server, run_remove, run_restart_server,
    run_test_connection, run_update_server, ServerPatch, ServerSpec,
};

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

    /// Register a new MCP server with the runtime (persisted in the config).
    Add {
        /// Unique server name.
        name: String,
        /// Command to launch (stdio transport) or URL (HTTP/SSE transport).
        #[arg(long)]
        command: Option<String>,
        /// HTTP/SSE connection URL.
        #[arg(long)]
        url: Option<String>,
        /// Require HITL approval for every tool call.
        #[arg(long)]
        require_approval: bool,
    },

    /// Remove an MCP server from the runtime.
    Remove {
        /// Server name.
        name: String,
        /// Confirm without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },

    /// Show the details of an MCP server.
    Show {
        /// Server name.
        name: String,
    },

    /// Test the connection to an MCP server.
    Test {
        /// URL or command to test.
        target: String,
    },

    /// Restart an MCP server.
    Restart {
        /// Server name.
        name: String,
    },

    /// Update the raw configuration of an existing MCP server.
    ///
    /// At least one of `--command`, `--url`, or `--require-approval` must
    /// be supplied. Fields that are omitted keep their previous value.
    Update {
        /// Server name.
        name: String,
        /// New stdio command (stdio transport).
        #[arg(long)]
        command: Option<String>,
        /// New HTTP/SSE URL.
        #[arg(long)]
        url: Option<String>,
        /// Enable / disable the HITL approval lock.
        #[arg(long, value_name = "BOOL")]
        require_approval: Option<bool>,
    },

    /// Show the raw persisted configuration of an MCP server.
    ///
    /// Reads `mcp.db` directly via the runtime and returns the original
    /// definition (useful for bisecting a configuration regression).
    RawConfig {
        /// Server name.
        name: String,
    },

    /// Interactive OAuth (PKCE) management for HTTP/streamable-http MCP servers.
    ///
    /// Same keychain entries as the Desktop wizard
    /// (`apollia-mcp-oauth/<server>`), so once a server is connected from one
    /// surface the other inherits the token.
    Oauth {
        /// OAuth subcommand.
        #[command(subcommand)]
        command: crate::commands::mcp_oauth::McpOauthCommand,
    },

    /// Manage MCP server secrets (env-var values) in the OS keychain.
    ///
    /// Mirrors the Desktop secret store: entries are keyed by
    /// `{server}:{env_var}` under the keychain service `apollia-mcp`, so a
    /// secret stored by the CLI is read transparently by the Desktop runtime.
    Secret {
        /// Secret subcommand.
        #[command(subcommand)]
        command: McpSecretCommand,
    },

    /// Launch Apollia as an MCP stdio server for external clients.
    ///
    /// Exposes native tools to MCP clients (Claude Desktop, VS Code, Cursor).
    /// Use `--with-runtime` to additionally expose `submit_task`.
    Server(super::mcp_server::McpServerArgs),
}

/// Subcommands of `apollia-os mcp secret`.
#[derive(Debug, Subcommand)]
pub enum McpSecretCommand {
    /// Persist `<value>` as the secret for `(<server>, <env_var>)`.
    ///
    /// The value is written to the OS keychain under service `apollia-mcp` and
    /// composite key `{server}:{env_var}`. Use `delete` to remove. The CLI does
    /// not echo the value back, but it is stored as-is (no trimming beyond
    /// stripping leading / trailing whitespace).
    Set {
        /// MCP server name (matches the name in `mcp.db` / `mcp.toml`).
        server: String,
        /// Environment variable name (e.g. `NOTION_API_KEY`).
        env_var: String,
        /// Secret value to store.
        value: String,
    },

    /// Delete the stored secret for `(<server>, <env_var>)`.
    Delete {
        /// MCP server name.
        server: String,
        /// Environment variable name.
        env_var: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Errors returned by MCP subcommands.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
                    println!("{}", output.trim_end());
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string())
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
                    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string())
                }
            }
        }

        McpCommand::ListPending { db, json: cmd_json } => {
            let use_json = json || *cmd_json;
            match run_list_pending(db.as_deref(), use_json) {
                Ok(output) => {
                    println!("{}", output.trim_end());
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string())
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
                    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string())
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
                ServerSpec {
                    name,
                    command: command.as_deref(),
                    url: url.as_deref(),
                    require_approval: *require_approval,
                },
                json,
            )
            .await
        }

        McpCommand::Remove { name, confirm } => {
            let client = make_runtime_client(socket);
            run_remove(&client, name, *confirm, json).await
        }

        McpCommand::Show { name } => {
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
                ServerPatch {
                    name,
                    command: command.as_deref(),
                    url: url.as_deref(),
                    require_approval: *require_approval,
                },
                json,
            )
            .await
        }

        McpCommand::RawConfig { name } => {
            let client = make_runtime_client(socket);
            run_get_raw_config(&client, name, json).await
        }

        McpCommand::Server(args) => match super::mcp_server::run(args).await {
            Ok(()) => exit_codes::SUCCESS,
            Err(e) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
        },

        McpCommand::Oauth { command } => crate::commands::mcp_oauth::run(command, json).await,

        McpCommand::Secret { command } => run_secret(command, json),
    }
}

/// Create a RuntimeClient from an optional socket path.
fn make_runtime_client(socket: Option<PathBuf>) -> RuntimeClient {
    let path = socket.unwrap_or_else(default_socket_path);
    RuntimeClient::new(path)
}

/// Uniform handling of MCP client errors (Unix socket).
fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::approvals::{resolve_approvals_db_path, resolve_config_path};
    use super::secrets::{mcp_secret_key, run_secret_set};
    use super::*;
    use apollia_mcp::approvals::McpApprovalStore;

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
        // THEN McpCommand::Add with the expected fields
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
        let cli = TestCli::parse_from(["apollia-os", "show", "code-tools"]);
        // THEN McpCommand::Show { name: "code-tools" }
        match &cli.command {
            McpCommand::Show { name } => assert_eq!(name, "code-tools"),
            other => panic!("expected Show, got {other:?}"),
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
        // GIVEN an update carrying only --url
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "code-tools",
            "--url",
            "http://localhost:9090",
        ]);
        // WHEN clap parses the argument line
        // THEN the name and the URL are captured and the untouched fields stay unset
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
        // GIVEN an update carrying --require-approval true
        let cli =
            TestCli::parse_from(["apollia-os", "update", "srv", "--require-approval", "true"]);
        // WHEN clap parses the argument line
        // THEN the flag is captured as a boolean, not as a string
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
        // GIVEN "mcp raw-config code-tools"
        let cli = TestCli::parse_from(["apollia-os", "raw-config", "code-tools"]);
        // WHEN clap parses the argument line
        // THEN the server name is captured
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

    #[test]
    fn parses_secret_set() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: McpCommand,
        }
        // GIVEN a secret set naming a server, a variable and a value
        let cli = TestCli::parse_from([
            "x",
            "secret",
            "set",
            "notion",
            "NOTION_API_KEY",
            "secret_value_xyz",
        ]);
        // WHEN clap parses the argument line
        // THEN the three land in the right order
        match cli.cmd {
            McpCommand::Secret {
                command:
                    McpSecretCommand::Set {
                        server,
                        env_var,
                        value,
                    },
            } => {
                assert_eq!(server, "notion");
                assert_eq!(env_var, "NOTION_API_KEY");
                assert_eq!(value, "secret_value_xyz");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_secret_delete() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: McpCommand,
        }
        // GIVEN a secret delete naming a server and a variable, with no --confirm
        let cli = TestCli::parse_from(["x", "secret", "delete", "notion", "NOTION_API_KEY"]);
        // WHEN clap parses the argument line
        // THEN both are captured and the confirmation stays down by default
        match cli.cmd {
            McpCommand::Secret {
                command:
                    McpSecretCommand::Delete {
                        server,
                        env_var,
                        confirm,
                    },
            } => {
                assert_eq!(server, "notion");
                assert_eq!(env_var, "NOTION_API_KEY");
                assert!(!confirm, "the confirmation is opt-in, never the default");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn secret_set_rejects_empty_value() {
        // GIVEN a secret value made of spaces
        // WHEN it is stored
        let code = run_secret_set("notion", "NOTION_API_KEY", "   ", true);
        // THEN the command stops on an error rather than writing a blank secret to the keyring
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn secret_set_rejects_empty_server() {
        // GIVEN a server name made of spaces
        // WHEN a secret is stored for it
        let code = run_secret_set("  ", "K", "v", true);
        // THEN the command stops on an error
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn mcp_secret_key_composes_pair() {
        // GIVEN a server name and a variable name
        // WHEN the keyring key is composed
        // THEN it is the two joined by a colon, which is what the keyring is searched with
        assert_eq!(
            mcp_secret_key("notion", "NOTION_API_KEY"),
            "notion:NOTION_API_KEY"
        );
    }
}
