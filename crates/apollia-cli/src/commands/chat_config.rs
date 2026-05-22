//! `apollia-os chat-config` — manage chat libre defaults.
//!
//! Wraps [`ChatLibreConfigRepository`] directly against `governance.db` so
//! the runtime does not need to be running. The Desktop equivalents are
//! `chat_libre.rs::{get_chat_libre_config, update_chat_libre_config}`.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_tools::chat_libre_config::{ChatLibreConfig, ChatLibreConfigRepository};
use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;

use crate::exit_codes;

/// Subcommands of `apollia-os chat-config`.
#[derive(Debug, Subcommand)]
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
}

/// Execute a `chat-config` subcommand.
pub fn run(cmd: &ChatConfigCommand, json: bool) -> i32 {
    match cmd {
        ChatConfigCommand::Get { db } => run_get(db.as_deref(), json),
        ChatConfigCommand::Set { key, value, db } => run_set(db.as_deref(), key, value, json),
        ChatConfigCommand::Reset { confirm, db } => run_reset(db.as_deref(), *confirm, json),
    }
}

fn resolve_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
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
}
