//! `apollia-os chat config`: the settings of the free chat.
//!
//! The free chat is the Apollia agent, so `get`, `set` and `reset` read and
//! write its mission and access overrides and its model through the runtime's
//! routes, the same ones `agent set` and the desktop panel use; every write is
//! journaled by the runtime. Without a running runtime they exit 2. The
//! `permissions` subcommands still read the rules of `governance.db` directly.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine};
use apollia_runtime::agents::apollia::{self, ChatConfigView, ConfigWrite};
use apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID;
use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Subcommands of `apollia-os chat config`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatConfigCommand {
    /// Print the settings of the free chat (the Apollia agent).
    Get {
        /// Accepted for compatibility and ignored: the settings are read from
        /// the runtime.
        #[arg(long, value_name = "PATH", hide = true)]
        db: Option<PathBuf>,
    },
    /// Update one setting of the free chat (the Apollia agent).
    Set {
        /// Field name: `system-prompt`, `allowed-tools`, or `llm-backend`.
        key: String,
        /// New value. For `allowed-tools`, expects a comma-separated list.
        /// For `llm-backend`, the literal `none` clears the backend.
        value: String,
        /// Accepted for compatibility and ignored: the settings are written
        /// through the runtime.
        #[arg(long, value_name = "PATH", hide = true)]
        db: Option<PathBuf>,
    },
    /// Reset the settings to the defaults (no prompt, no tools, default model).
    Reset {
        #[arg(long)]
        confirm: bool,
        /// Accepted for compatibility and ignored: the settings are written
        /// through the runtime.
        #[arg(long, value_name = "PATH", hide = true)]
        db: Option<PathBuf>,
    },
    /// Manage persisted permission rules scoped to the Apollia Chat agent.
    ///
    /// Mirrors the Desktop Settings → Chat permissions panel: lists or deletes
    /// rules stored in `governance.db` with `agent_id = apollia:chat`. The
    /// authorizations a running chat holds for its session only are not
    /// rules and are not visible here.
    Permissions {
        /// Permissions subcommand.
        #[command(subcommand)]
        command: ChatConfigPermissionsCommand,
    },

    /// Inspect or revoke in-memory session authorizations.
    ///
    /// These authorizations live only in the running daemon's
    /// `ChatSessionManager` (never persisted to `governance.db`). The CLI
    /// cannot reach in-memory state from outside the daemon, so this
    /// subcommand prints an explanatory error and exits 1 unless / until a
    /// runtime route is added. Use the Desktop Settings → Permissions panel
    /// or restart the daemon to clear stale authorizations.
    Authorizations {
        /// Authorizations subcommand.
        #[command(subcommand)]
        command: ChatConfigAuthorizationsCommand,
    },
}
/// Subcommands of `apollia-os chat config permissions`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatConfigPermissionsCommand {
    /// List every persisted rule scoped to the Apollia Chat agent.
    List {
        /// Override the `governance.db` path (default: `~/.apollia/governance.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Delete a chat-scoped permission rule by id.
    Delete {
        /// Rule id (as returned by `chat-config permissions list`).
        id: i64,
        /// Skip the confirmation prompt (required for scripts).
        #[arg(long)]
        confirm: bool,
        /// Override the `governance.db` path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
}

/// Subcommands of `apollia-os chat config authorizations`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatConfigAuthorizationsCommand {
    /// List active in-memory session authorizations (requires runtime route).
    List,
    /// Revoke a single in-memory session authorization (requires runtime route).
    Revoke {
        /// Session id.
        session_id: String,
        /// Tool name.
        tool: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Execute a `chat config` subcommand.
pub async fn run(cmd: &ChatConfigCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = || RuntimeClient::new(socket.clone().unwrap_or_else(default_socket_path));
    match cmd {
        ChatConfigCommand::Get { .. } => run_get(&client(), json).await,
        ChatConfigCommand::Set { key, value, .. } => run_set(&client(), key, value, json).await,
        ChatConfigCommand::Reset { confirm, .. } => run_reset(&client(), *confirm, json).await,
        ChatConfigCommand::Permissions { command } => run_permissions(command, json),
        ChatConfigCommand::Authorizations { command } => run_authorizations(command, json),
    }
}

fn resolve_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home).join(GOVERNANCE_DB_FILENAME)
}

fn emit_error(msg: impl Into<String>, json: bool) {
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.into());
}

/// The view with `key` set to `value`, as typed on the command line.
fn with_key(mut view: ChatConfigView, key: &str, value: &str) -> Result<ChatConfigView, String> {
    match key {
        "system-prompt" | "system_prompt" => view.system_prompt = value.to_string(),
        "allowed-tools" | "allowed_tools" => {
            view.allowed_tools = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        "llm-backend" | "llm_backend" => {
            view.llm_backend = if value.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(value.to_string())
            };
        }
        other => {
            return Err(format!(
                "unknown key '{other}' (use: system-prompt, allowed-tools, llm-backend)"
            ))
        }
    }
    Ok(view)
}

/// The settings of the chat as the runtime holds them.
async fn current(client: &RuntimeClient) -> Result<ChatConfigView, ClientError> {
    let resolved = client.get_agent_definition(APOLLIA_CHAT_AGENT_ID).await?;
    Ok(ChatConfigView::from_resolved(&resolved))
}

/// Writes `desired` over the current settings, only the fields that change.
async fn write(
    client: &RuntimeClient,
    current: &ChatConfigView,
    desired: &ChatConfigView,
) -> Result<(), ClientError> {
    for change in apollia::changes(current, desired) {
        match &change {
            ConfigWrite::SetMission(prompt) => {
                client
                    .set_agent_override(APOLLIA_CHAT_AGENT_ID, "mission", serde_json::json!(prompt))
                    .await?;
            }
            ConfigWrite::SetAccess(tools) => {
                client
                    .set_agent_override(APOLLIA_CHAT_AGENT_ID, "access", serde_json::json!(tools))
                    .await?;
            }
            ConfigWrite::ClearMission => {
                client
                    .remove_agent_override(APOLLIA_CHAT_AGENT_ID, "mission")
                    .await?;
            }
            ConfigWrite::ClearAccess => {
                client
                    .remove_agent_override(APOLLIA_CHAT_AGENT_ID, "access")
                    .await?;
            }
            ConfigWrite::SetModel(backend) => {
                let provider = client.get_llm_backend(backend).await.ok().and_then(|b| {
                    b.get("provider")
                        .and_then(|p| p.as_str())
                        .map(str::to_owned)
                });
                let body = apollia::model_body(backend, provider.as_deref());
                client.set_agent_model(APOLLIA_CHAT_AGENT_ID, &body).await?;
            }
            ConfigWrite::ClearModel => {
                client.clear_agent_model(APOLLIA_CHAT_AGENT_ID).await?;
            }
        }
    }
    Ok(())
}

fn print_view(view: &ChatConfigView, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(view).unwrap_or_default());
        return;
    }
    let prompt = if view.system_prompt.is_empty() {
        "(empty)".to_string()
    } else {
        view.system_prompt.clone()
    };
    let tools = if view.allowed_tools.is_empty() {
        "(none)".to_string()
    } else {
        view.allowed_tools.join(", ")
    };
    let backend = view.llm_backend.as_deref().unwrap_or("(runtime default)");
    println!("  system-prompt  {prompt}");
    println!("  allowed-tools  {tools}");
    println!("  llm-backend    {backend}");
}

async fn run_get(client: &RuntimeClient, json: bool) -> i32 {
    match current(client).await {
        Ok(view) => {
            print_view(&view, json);
            exit_codes::SUCCESS
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

async fn run_set(client: &RuntimeClient, key: &str, value: &str, json: bool) -> i32 {
    // An unknown key is refused before the runtime is reached.
    if let Err(msg) = with_key(ChatConfigView::default(), key, value) {
        emit_error(msg, json);
        return exit_codes::GENERAL_ERROR;
    }
    let before = match current(client).await {
        Ok(view) => view,
        Err(e) => return crate::output::emit_client_error(json, &e),
    };
    let desired = match with_key(before.clone(), key, value) {
        Ok(view) => view,
        Err(msg) => {
            emit_error(msg, json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if let Err(e) = write(client, &before, &desired).await {
        return crate::output::emit_client_error(json, &e);
    }
    if json {
        match current(client).await {
            Ok(view) => print_view(&view, true),
            Err(e) => return crate::output::emit_client_error(json, &e),
        }
    } else {
        crate::note!("  * updated {key}");
    }
    exit_codes::SUCCESS
}

async fn run_reset(client: &RuntimeClient, confirm: bool, json: bool) -> i32 {
    let before = match current(client).await {
        Ok(view) => view,
        Err(e) => return crate::output::emit_client_error(json, &e),
    };
    if let Some(code) =
        crate::output::require_confirmation(confirm, json, "reset the chat settings")
    {
        return code;
    }
    if let Err(e) = write(client, &before, &ChatConfigView::default()).await {
        return crate::output::emit_client_error(json, &e);
    }
    if json {
        println!("{{\"reset\":true}}");
    } else {
        crate::note!("  * chat config reset");
    }
    exit_codes::SUCCESS
}

// ─── permissions ──────────────────────────────────────────────────────────────

fn run_permissions(cmd: &ChatConfigPermissionsCommand, json: bool) -> i32 {
    match cmd {
        ChatConfigPermissionsCommand::List { db } => run_permissions_list(db.as_deref(), json),
        ChatConfigPermissionsCommand::Delete { id, confirm, db } => {
            run_permissions_delete(*id, *confirm, db.as_deref(), json)
        }
    }
}

fn open_prefix_engine(db: Option<&Path>, json: bool) -> Option<PrefixRuleEngine> {
    let path = resolve_db(db);
    let parent = match path.parent() {
        Some(p) => p,
        None => {
            emit_error(format!("invalid db path: {}", path.display()), json);
            return None;
        }
    };
    let _ = std::fs::create_dir_all(parent);
    if let Err(e) = apollia_tools::GovernanceDb::open(parent) {
        emit_error(format!("governance migration failed: {e}"), json);
        return None;
    }
    match PrefixRuleEngine::new(&path) {
        Ok(e) => Some(e),
        Err(e) => {
            emit_error(format!("open prefix rule engine: {e}"), json);
            None
        }
    }
}

fn rule_to_json(rule: &PrefixRule) -> serde_json::Value {
    serde_json::json!({
        "id": rule.id,
        "tool": rule.tool_name,
        "scope": scope_str(rule.scope),
        "argument": rule.arg_prefix,
        "action": format!("{:?}", rule.action).to_lowercase(),
        "agent_id": rule.agent_id,
        "created_at": rule.created_at,
        "expires_at": rule.expires_at,
    })
}

fn scope_str(scope: PermissionScope) -> &'static str {
    scope.as_str()
}

fn run_permissions_list(db: Option<&Path>, json: bool) -> i32 {
    let Some(engine) = open_prefix_engine(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let rules = match engine.list_rules_for_agent(APOLLIA_CHAT_AGENT_ID) {
        Ok(r) => r,
        Err(e) => {
            emit_error(format!("list chat permission rules: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if json {
        let array: Vec<serde_json::Value> = rules.iter().map(rule_to_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if rules.is_empty() {
        println!("No chat permission rules persisted (agent_id = {APOLLIA_CHAT_AGENT_ID}).");
        println!("  -> session-scope authorizations live in the runtime memory; use the Desktop Settings → Chat panel to inspect.");
    } else {
        println!("  Chat permission rules (agent_id = {APOLLIA_CHAT_AGENT_ID}):");
        println!(
            "  {:<6} {:<24} {:<8} {:<20} EXPIRES",
            "ID", "TOOL", "ACTION", "ARGUMENT"
        );
        for r in &rules {
            let action = format!("{:?}", r.action).to_lowercase();
            let argument = r
                .arg_prefix
                .as_deref()
                .map(|s| crate::commands::truncate_for_display(s, 18, 17, "…"))
                .unwrap_or_else(|| "*".to_string());
            let expires = match r.expires_at {
                Some(ts) => format!("unix {ts}"),
                None => "never".to_string(),
            };
            println!(
                "  {:<6} {:<24} {:<8} {:<20} {}",
                r.id, r.tool_name, action, argument, expires
            );
        }
    }
    exit_codes::SUCCESS
}

fn run_permissions_delete(id: i64, confirm: bool, db: Option<&Path>, json: bool) -> i32 {
    if !confirm {
        emit_error(
            format!("use --confirm to delete chat permission rule #{id}"),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    let Some(mut engine) = open_prefix_engine(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match engine.remove_rule_checked(id) {
        Ok(true) => {
            if json {
                let out = serde_json::json!({ "id": id, "removed": true });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * chat permission rule #{id} removed");
            }
            exit_codes::SUCCESS
        }
        Ok(false) => {
            emit_error(format!("chat permission rule {id} not found"), json);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            emit_error(format!("remove rule {id}: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── authorizations ───────────────────────────────────────────────────────────

fn run_authorizations(cmd: &ChatConfigAuthorizationsCommand, json: bool) -> i32 {
    match cmd {
        ChatConfigAuthorizationsCommand::List | ChatConfigAuthorizationsCommand::Revoke { .. } => {
            emit_authz_unavailable(json)
        }
    }
}

fn emit_authz_unavailable(json: bool) -> i32 {
    let msg = "Session-scope chat authorizations live only in the running daemon's memory. \
        The CLI cannot reach in-memory runtime state without a dedicated HTTP route \
        (not yet wired for v0.1.0). Use the Desktop Settings → Permissions panel or \
        restart the daemon to clear stale authorizations.";
    emit_error(msg, json);
    exit_codes::GENERAL_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ChatConfigCommand,
    }

    #[test]
    fn parses_get() {
        // GIVEN "chat-config get"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "get"]);
        // THEN the get subcommand is selected
        assert!(matches!(cli.cmd, ChatConfigCommand::Get { .. }));
    }

    #[test]
    fn parses_set() {
        // GIVEN a set carrying a key and a sentence as value
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "set", "system-prompt", "You are helpful."]);
        // THEN key and value land in the right order, spaces included
        match cli.cmd {
            ChatConfigCommand::Set { key, value, .. } => {
                assert_eq!(key, "system-prompt");
                assert_eq!(value, "You are helpful.");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// A client aimed at a socket nothing listens on.
    fn absent_runtime() -> (tempfile::TempDir, RuntimeClient) {
        let tmp = tempfile::tempdir().unwrap();
        let client = RuntimeClient::new(tmp.path().join("absent.sock"));
        (tmp, client)
    }

    #[tokio::test]
    async fn without_a_runtime_get_set_and_reset_exit_2() {
        // GIVEN no runtime behind the socket
        let (_tmp, client) = absent_runtime();
        // WHEN the settings are read, set and reset
        // THEN each reports the runtime as unreachable, exit 2, and writes nothing
        assert_eq!(run_get(&client, true).await, exit_codes::RUNTIME_ERROR);
        assert_eq!(
            run_set(&client, "system-prompt", "Hi", true).await,
            exit_codes::RUNTIME_ERROR
        );
        assert_eq!(
            run_reset(&client, true, true).await,
            exit_codes::RUNTIME_ERROR
        );
    }

    #[test]
    fn allowed_tools_comma_split() {
        // GIVEN a tool list given as one comma separated string, with spaces
        // WHEN it is read as the value of allowed-tools
        let view = with_key(
            ChatConfigView::default(),
            "allowed-tools",
            "file_read, bash, http",
        )
        .unwrap();
        // THEN it holds three trimmed names rather than one string
        assert_eq!(view.allowed_tools, vec!["file_read", "bash", "http"]);
    }

    #[test]
    fn llm_backend_none_clears() {
        // GIVEN settings whose backend has been set
        let view = with_key(ChatConfigView::default(), "llm-backend", "anthropic").unwrap();
        // WHEN the backend is set to none
        let view = with_key(view, "llm-backend", "none").unwrap();
        // THEN the backend is cleared rather than being the literal word none
        assert!(view.llm_backend.is_none());
    }

    #[tokio::test]
    async fn unknown_key_errors_before_the_runtime() {
        // GIVEN a key the chat settings do not define, and no runtime
        let (_tmp, client) = absent_runtime();
        // WHEN it is set
        // THEN the command stops on a usage error, exit 1, without reaching
        // for the runtime
        assert_eq!(
            run_set(&client, "wrong", "value", true).await,
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn parses_permissions_list() {
        // GIVEN "chat-config permissions list", with no --db
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "permissions", "list"]);
        // THEN the nested list subcommand is selected and the database stays the default one
        match cli.cmd {
            ChatConfigCommand::Permissions {
                command: ChatConfigPermissionsCommand::List { db },
            } => assert!(db.is_none()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_permissions_delete_requires_confirm_flag() {
        // GIVEN a permissions delete naming a rule, with no --confirm
        let cli = TestCli::parse_from(["x", "permissions", "delete", "42"]);
        // WHEN clap parses the argument line
        // THEN the identifier is captured as a number and the confirmation stays down
        match cli.cmd {
            ChatConfigCommand::Permissions {
                command: ChatConfigPermissionsCommand::Delete { id, confirm, .. },
            } => {
                assert_eq!(id, 42);
                assert!(!confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_authorizations_list() {
        // GIVEN "chat-config authorizations list"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "authorizations", "list"]);
        // THEN the nested list subcommand is selected
        assert!(matches!(
            cli.cmd,
            ChatConfigCommand::Authorizations {
                command: ChatConfigAuthorizationsCommand::List,
            }
        ));
    }

    #[test]
    fn parses_authorizations_revoke() {
        // GIVEN a revoke naming a session and a tool, with no --confirm
        let cli = TestCli::parse_from(["x", "authorizations", "revoke", "sess-1", "file_read"]);
        // WHEN clap parses the argument line
        // THEN both are captured in the right order and the confirmation stays down
        match cli.cmd {
            ChatConfigCommand::Authorizations {
                command:
                    ChatConfigAuthorizationsCommand::Revoke {
                        session_id,
                        tool,
                        confirm,
                    },
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(tool, "file_read");
                assert!(!confirm, "the confirmation is opt-in, never the default");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn authorizations_list_returns_explanatory_error() {
        // GIVEN the authorizations listing, which no store backs yet
        // WHEN it runs
        let code = run_authorizations(&ChatConfigAuthorizationsCommand::List, true);
        // THEN it stops on an error rather than printing an empty list that would read as none granted
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn permissions_delete_without_confirm_errors() {
        // GIVEN a governance database
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        // WHEN a rule deletion runs without confirmation
        // THEN it stops on an error
        assert_eq!(
            run_permissions_delete(1, false, Some(&db), true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn permissions_list_empty_db_returns_success() {
        // GIVEN a governance database with no rule in it
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        // WHEN the rules are listed
        // THEN the command reports success rather than treating an empty store as a failure
        assert_eq!(run_permissions_list(Some(&db), true), exit_codes::SUCCESS);
    }
}
