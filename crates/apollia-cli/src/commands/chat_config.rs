//! `apollia-os chat-config`: manage chat libre defaults.
//!
//! Wraps [`ChatLibreConfigRepository`] directly against `governance.db` so
//! the runtime does not need to be running. The Desktop equivalents are
//! `chat_libre.rs::{get_chat_libre_config, update_chat_libre_config}`.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine};
use apollia_runtime::chat::APOLLIA_CHAT_AGENT_ID;
use apollia_tools::chat_libre_config::{ChatLibreConfig, ChatLibreConfigRepository};
use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;

use crate::exit_codes;

/// Subcommands of `apollia-os chat config`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatConfigCommand {
    /// Print the current chat libre configuration.
    Get {
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Update one field of the chat libre configuration.
    Set {
        /// Field name: `system-prompt`, `allowed-tools`, or `llm-backend`.
        key: String,
        /// New value. For `allowed-tools`, expects a comma-separated list.
        /// For `llm-backend`, the literal `none` clears the backend.
        value: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Reset the configuration to the defaults (empty prompt, no tools).
    Reset {
        #[arg(long)]
        confirm: bool,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Manage persisted permission rules scoped to the Apollia Chat agent.
    ///
    /// Mirrors the Desktop Settings → Chat permissions panel: lists or deletes
    /// rules stored in `governance.db` with `agent_id = apollia:chat`. Rules
    /// of scope `session` live in the runtime memory and are not visible here
    /// (same caveat as `permissions list`).
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
    },
}

/// Execute a `chat-config` subcommand.
pub fn run(cmd: &ChatConfigCommand, json: bool) -> i32 {
    match cmd {
        ChatConfigCommand::Get { db } => run_get(db.as_deref(), json),
        ChatConfigCommand::Set { key, value, db } => run_set(db.as_deref(), key, value, json),
        ChatConfigCommand::Reset { confirm, db } => run_reset(db.as_deref(), *confirm, json),
        ChatConfigCommand::Permissions { command } => run_permissions(command, json),
        ChatConfigCommand::Authorizations { command } => run_authorizations(command, json),
    }
}

fn resolve_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    let home = apollia_core::paths::home_dir_or_temp();
    home.join(".apollia").join(GOVERNANCE_DB_FILENAME)
}

fn open_repo(db: Option<&Path>, json: bool) -> Option<ChatLibreConfigRepository> {
    let path = resolve_db(db);
    let parent = match path.parent() {
        Some(p) => p,
        None => {
            emit_error(format!("invalid db path: {}", path.display()), json);
            return None;
        }
    };
    let _ = std::fs::create_dir_all(parent);
    // GovernanceDb::open runs the schema migration that creates chat_libre_config.
    // We drop the handle immediately and re-open through the config repository so
    // the two access paths share the same fully-migrated DB file.
    if let Err(e) = apollia_tools::GovernanceDb::open(parent) {
        emit_error(format!("governance migration failed: {e}"), json);
        return None;
    }
    match ChatLibreConfigRepository::open(&path) {
        Ok(r) => Some(r),
        Err(e) => {
            emit_error(format!("open {} failed: {e}", path.display()), json);
            None
        }
    }
}

fn emit_error(msg: impl Into<String>, json: bool) {
    let s = msg.into();
    if json {
        let out = serde_json::json!({"error": s});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {s}");
    }
}

fn run_get(db: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let cfg = match repo.load() {
        Ok(c) => c,
        Err(e) => {
            emit_error(format!("read failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&cfg).unwrap_or_default());
    } else {
        let prompt = if cfg.system_prompt.is_empty() {
            "(empty)".to_string()
        } else {
            cfg.system_prompt.clone()
        };
        let tools = if cfg.allowed_tools.is_empty() {
            "(none)".to_string()
        } else {
            cfg.allowed_tools.join(", ")
        };
        let backend = cfg.llm_backend.as_deref().unwrap_or("(runtime default)");
        println!("  system-prompt  {prompt}");
        println!("  allowed-tools  {tools}");
        println!("  llm-backend    {backend}");
    }
    exit_codes::SUCCESS
}

fn run_set(db: Option<&Path>, key: &str, value: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let mut cfg = match repo.load() {
        Ok(c) => c,
        Err(e) => {
            emit_error(format!("read failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    match key {
        "system-prompt" | "system_prompt" => cfg.system_prompt = value.to_string(),
        "allowed-tools" | "allowed_tools" => {
            cfg.allowed_tools = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        "llm-backend" | "llm_backend" => {
            cfg.llm_backend = if value.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(value.to_string())
            };
        }
        other => {
            emit_error(
                format!("unknown key '{other}' (use: system-prompt, allowed-tools, llm-backend)"),
                json,
            );
            return exit_codes::GENERAL_ERROR;
        }
    }
    if let Err(e) = repo.save(&cfg) {
        emit_error(format!("write failed: {e}"), json);
        return exit_codes::GENERAL_ERROR;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&cfg).unwrap_or_default());
    } else {
        println!("  * updated {key}");
    }
    exit_codes::SUCCESS
}

fn run_reset(db: Option<&Path>, confirm: bool, json: bool) -> i32 {
    if !confirm {
        emit_error("use --confirm to reset", json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    if let Err(e) = repo.save(&ChatLibreConfig::default()) {
        emit_error(format!("reset failed: {e}"), json);
        return exit_codes::GENERAL_ERROR;
    }
    if json {
        println!("{{\"reset\":true}}");
    } else {
        println!("  * chat config reset");
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
        "project_path": rule.project_path,
        "created_at": rule.created_at,
        "expires_at": rule.expires_at,
    })
}

fn scope_str(scope: PermissionScope) -> &'static str {
    match scope {
        PermissionScope::Session => "session",
        PermissionScope::Project => "project",
        PermissionScope::Global => "global",
        PermissionScope::Agent => "agent",
    }
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
        let cli = TestCli::parse_from(["x", "get"]);
        assert!(matches!(cli.cmd, ChatConfigCommand::Get { .. }));
    }

    #[test]
    fn parses_set() {
        let cli = TestCli::parse_from(["x", "set", "system-prompt", "You are helpful."]);
        match cli.cmd {
            ChatConfigCommand::Set { key, value, .. } => {
                assert_eq!(key, "system-prompt");
                assert_eq!(value, "You are helpful.");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reset_without_confirm_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(run_reset(Some(&db), false, true), exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn set_then_get_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(
            run_set(Some(&db), "system-prompt", "Hi", true),
            exit_codes::SUCCESS
        );
        assert_eq!(run_get(Some(&db), true), exit_codes::SUCCESS);
    }

    #[test]
    fn allowed_tools_comma_split() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(
            run_set(Some(&db), "allowed-tools", "file_read, bash, http", true),
            exit_codes::SUCCESS
        );
        let repo = ChatLibreConfigRepository::open(&db).unwrap();
        let cfg = repo.load().unwrap();
        assert_eq!(cfg.allowed_tools, vec!["file_read", "bash", "http"]);
    }

    #[test]
    fn llm_backend_none_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        run_set(Some(&db), "llm-backend", "anthropic", true);
        run_set(Some(&db), "llm-backend", "none", true);
        let repo = ChatLibreConfigRepository::open(&db).unwrap();
        let cfg = repo.load().unwrap();
        assert!(cfg.llm_backend.is_none());
    }

    #[test]
    fn unknown_key_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(
            run_set(Some(&db), "wrong", "value", true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn parses_permissions_list() {
        let cli = TestCli::parse_from(["x", "permissions", "list"]);
        match cli.cmd {
            ChatConfigCommand::Permissions {
                command: ChatConfigPermissionsCommand::List { db },
            } => assert!(db.is_none()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_permissions_delete_requires_confirm_flag() {
        let cli = TestCli::parse_from(["x", "permissions", "delete", "42"]);
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
        let cli = TestCli::parse_from(["x", "authorizations", "list"]);
        assert!(matches!(
            cli.cmd,
            ChatConfigCommand::Authorizations {
                command: ChatConfigAuthorizationsCommand::List,
            }
        ));
    }

    #[test]
    fn parses_authorizations_revoke() {
        let cli = TestCli::parse_from(["x", "authorizations", "revoke", "sess-1", "file_read"]);
        match cli.cmd {
            ChatConfigCommand::Authorizations {
                command: ChatConfigAuthorizationsCommand::Revoke { session_id, tool },
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(tool, "file_read");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn authorizations_list_returns_explanatory_error() {
        let code = run_authorizations(&ChatConfigAuthorizationsCommand::List, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn permissions_delete_without_confirm_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(
            run_permissions_delete(1, false, Some(&db), true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn permissions_list_empty_db_returns_success() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("governance.db");
        assert_eq!(run_permissions_list(Some(&db), true), exit_codes::SUCCESS);
    }
}
