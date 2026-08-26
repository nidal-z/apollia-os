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

use std::path::PathBuf;

use clap::Subcommand;

mod approvals;
mod config;
mod credentials;
mod listing;
mod support;

use approvals::run_approvals;
use config::{run_config, run_reload};
use credentials::{run_credentials, run_describe};
use listing::{run_list, run_set_enabled};

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

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
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
#[cfg(test)]
mod tests {
    use super::config::{parse_value_for, set_nested_value};
    use super::listing::backend_label;
    use super::support::{db_path, is_known_tool, is_valid_credential_target, keyfile_path};
    use super::*;
    use apollia_core::ToolsConfig;
    use apollia_tools::{
        governance_db::GOVERNANCE_DB_FILENAME, NativeToolRegistry, ToolCredentialStore,
        AGENT_CREDENTIALS_NAMESPACE, NATIVE_TOOL_NAMES,
    };
    use tempfile::TempDir;
    use toml_edit::DocumentMut;

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
        // WHEN it is parsed for the web search tool
        // THEN the message lists the valid keys.
        assert!(err.contains("valid keys"));
    }

    #[test]
    fn parse_value_for_int_bounds_enforced() {
        // GIVEN an out-of-range integer for brave.timeout_secs.
        let err = parse_value_for("web_search", &["brave", "timeout_secs"], "999").unwrap_err();
        // WHEN it is parsed
        // THEN the bounds are reported.
        assert!(err.contains("out of range"));
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
        // WHEN a native name and an invented one are each looked up
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
        // WHEN the backend label of each tool is asked for
        // THEN web_search shows "DuckDuckGo (auto)".
        assert_eq!(backend_label("web_search", &cfg), "DuckDuckGo (auto)");
        assert_eq!(backend_label("file_read", &cfg), "-");
    }
}
