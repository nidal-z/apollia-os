//! `apollia-os user-memory` subcommands: manage the global user profile.
//!
//! Operates directly on `~/.apollia/user_memory.db` (a [`UserMemoryRepository`])
//! without requiring the runtime to be running. This is the CLI counterpart
//! to the Desktop `user_memory` Tauri commands.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_memory::user_memory::{ProfileEntry, UserMemoryRepository, WrittenBy};

use crate::exit_codes;

/// Subcommands of `apollia-os user-memory`.
#[derive(Debug, Subcommand)]
pub enum UserMemoryCommand {
    /// Display every key currently stored in the global user profile.
    Show {
        /// Optional override for the user_memory.db path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Insert or replace the value of `KEY`.
    Set {
        /// Profile key (e.g. `name`, `email`, `preferences.tone`).
        key: String,
        /// Value to store.
        value: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Remove the entry stored at `KEY`.
    Forget {
        /// Profile key to remove.
        key: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Reset the entire profile (deletes every entry).
    Reset {
        /// Skip the confirmation prompt.
        #[arg(long)]
        confirm: bool,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Print the canonical schema (the known structured fields).
    Schema {
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Dump every entry as a JSON array on stdout (or to `--output`).
    Export {
        /// Optional destination file (default: stdout).
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Import entries from a JSON file produced by `export`.
    Import {
        /// Source file (JSON array of `ProfileEntry`).
        #[arg(long, value_name = "PATH")]
        input: PathBuf,
        /// Overwrite existing entries with the same key.
        #[arg(long)]
        overwrite: bool,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
}

/// Entry point for `apollia-os user-memory <verb>`.
pub fn run(cmd: &UserMemoryCommand, json: bool) -> i32 {
    match cmd {
        UserMemoryCommand::Show { db } => run_show(db.as_deref(), json),
        UserMemoryCommand::Set { key, value, db } => run_set(db.as_deref(), key, value, json),
        UserMemoryCommand::Forget { key, db } => run_forget(db.as_deref(), key, json),
        UserMemoryCommand::Reset { confirm, db } => run_reset(db.as_deref(), *confirm, json),
        UserMemoryCommand::Schema { db } => run_schema(db.as_deref(), json),
        UserMemoryCommand::Export { output, db } => {
            run_export(db.as_deref(), output.as_deref(), json)
        }
        UserMemoryCommand::Import {
            input,
            overwrite,
            db,
        } => run_import(db.as_deref(), input, *overwrite, json),
    }
}

/// Resolve the path to `user_memory.db` (`~/.apollia/user_memory.db` by default).
fn resolve_db(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".apollia").join("user_memory.db")
}

fn open_repo(db_path: Option<&Path>, json: bool) -> Option<UserMemoryRepository> {
    let path = resolve_db(db_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match UserMemoryRepository::new(&path) {
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

// ─── show ─────────────────────────────────────────────────────────────────────

fn run_show(db_path: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let entries = match repo.list_all() {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("read failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
    } else if entries.is_empty() {
        println!("  (profile is empty - run `apollia-os onboard` to fill it)");
    } else {
        let kw = entries.iter().map(|e| e.key.len()).max().unwrap_or(8);
        println!("  {:<width$}  WRITTEN BY  VALUE", "KEY", width = kw);
        for e in &entries {
            let by = format_written_by(&e.written_by);
            println!("  {:<width$}  {:<10}  {}", e.key, by, e.value, width = kw);
        }
    }
    exit_codes::SUCCESS
}

fn format_written_by(by: &WrittenBy) -> String {
    match by {
        WrittenBy::Onboarding => "onboarding".to_string(),
        WrittenBy::User => "user".to_string(),
        WrittenBy::Agent(name) => format!("agent:{name}"),
    }
}

// ─── set ──────────────────────────────────────────────────────────────────────

fn run_set(db_path: Option<&Path>, key: &str, value: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.set(key, value, WrittenBy::User) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({"key": key, "value": value, "written_by": "user"});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * {key} = {value}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("set failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── forget ───────────────────────────────────────────────────────────────────

fn run_forget(db_path: Option<&Path>, key: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.forget(key) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({"forgotten": key});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * forgot {key}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("forget failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── reset ────────────────────────────────────────────────────────────────────

fn run_reset(db_path: Option<&Path>, confirm: bool, json: bool) -> i32 {
    if !confirm {
        emit_error("use --confirm to reset the entire profile", json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.reset() {
        Ok(n) => {
            if json {
                let out = serde_json::json!({"removed": n});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * {n} entries removed");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("reset failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── schema ───────────────────────────────────────────────────────────────────

fn run_schema(db_path: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let entries = match repo.list_schema() {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("schema query failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
    } else if entries.is_empty() {
        println!("  (no schema-tracked entries)");
    } else {
        for e in &entries {
            println!("  * {} = {}", e.key, e.value);
        }
    }
    exit_codes::SUCCESS
}

// ─── export ───────────────────────────────────────────────────────────────────

fn run_export(db_path: Option<&Path>, output: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let entries = match repo.list_all() {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("read failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let body = match serde_json::to_string_pretty(&entries) {
        Ok(s) => s,
        Err(e) => {
            emit_error(format!("serialize failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &body) {
                emit_error(format!("write {} failed: {e}", path.display()), json);
                return exit_codes::GENERAL_ERROR;
            }
            if !json {
                println!("  * wrote {} entries to {}", entries.len(), path.display());
            } else {
                let out = serde_json::json!({
                    "exported": entries.len(),
                    "path": path.display().to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            }
        }
        None => {
            println!("{body}");
        }
    }
    exit_codes::SUCCESS
}

// ─── import ───────────────────────────────────────────────────────────────────

fn run_import(db_path: Option<&Path>, input: &Path, overwrite: bool, json: bool) -> i32 {
    let body = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            emit_error(format!("read {} failed: {e}", input.display()), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let entries: Vec<ProfileEntry> = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("invalid JSON: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    for entry in &entries {
        if !overwrite {
            match repo.get(&entry.key) {
                Ok(Some(_)) => {
                    skipped += 1;
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    emit_error(format!("read {} failed: {e}", entry.key), json);
                    return exit_codes::GENERAL_ERROR;
                }
            }
        }
        if let Err(e) = repo.set(&entry.key, &entry.value, entry.written_by.clone()) {
            emit_error(format!("write {} failed: {e}", entry.key), json);
            return exit_codes::GENERAL_ERROR;
        }
        inserted += 1;
    }
    if json {
        let out = serde_json::json!({
            "inserted": inserted,
            "skipped": skipped,
            "total_in_file": entries.len(),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "  * imported {inserted}, skipped {skipped} (of {} entries)",
            entries.len()
        );
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
        cmd: UserMemoryCommand,
    }

    #[test]
    fn parses_show() {
        let cli = TestCli::parse_from(["x", "show"]);
        assert!(matches!(cli.cmd, UserMemoryCommand::Show { .. }));
    }

    #[test]
    fn parses_set() {
        let cli = TestCli::parse_from(["x", "set", "name", "Alice"]);
        match cli.cmd {
            UserMemoryCommand::Set { key, value, .. } => {
                assert_eq!(key, "name");
                assert_eq!(value, "Alice");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_forget() {
        let cli = TestCli::parse_from(["x", "forget", "name"]);
        match cli.cmd {
            UserMemoryCommand::Forget { key, .. } => assert_eq!(key, "name"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reset_without_confirm_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        let code = run_reset(Some(&db), false, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn set_then_show_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        let code = run_set(Some(&db), "name", "Bob", true);
        assert_eq!(code, exit_codes::SUCCESS);
        let code = run_show(Some(&db), true);
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn export_then_import_overwrite_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db_src = tmp.path().join("src.db");
        let db_dst = tmp.path().join("dst.db");
        let out = tmp.path().join("dump.json");

        run_set(Some(&db_src), "alpha", "1", true);
        run_set(Some(&db_src), "beta", "2", true);
        assert_eq!(
            run_export(Some(&db_src), Some(&out), true),
            exit_codes::SUCCESS
        );
        assert!(out.exists());
        assert_eq!(
            run_import(Some(&db_dst), &out, true, true),
            exit_codes::SUCCESS
        );
        // re-import without overwrite should skip everything.
        assert_eq!(
            run_import(Some(&db_dst), &out, false, true),
            exit_codes::SUCCESS
        );
    }

    #[test]
    fn forget_clears_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        run_set(Some(&db), "x", "y", true);
        assert_eq!(run_forget(Some(&db), "x", true), exit_codes::SUCCESS);
    }
}
