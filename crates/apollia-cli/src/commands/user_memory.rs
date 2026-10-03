//! `apollia-os profile` subcommands: the named user profiles.
//!
//! Operates directly on `~/.apollia/user_memory.db` (a [`UserMemoryRepository`])
//! without requiring the runtime to be running. `list`, `show`, `create`,
//! `rename`, `set`, `set-default` and `delete` address a profile by its
//! identifier (`default` names the default profile, see [`profiles`]);
//! `forget`, `reset`, `schema`, `export` and `import` act on the default
//! profile and the free-form entries.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_memory::user_memory::{ProfileEntry, UserMemoryRepository, WrittenBy};

use crate::exit_codes;

mod profiles;

/// Subcommands of `apollia-os profile`.
#[derive(Debug, Subcommand)]
pub enum UserMemoryCommand {
    /// List the profiles: identifier, name, and which one is the default.
    List {
        /// Optional override for the user_memory.db path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Display a profile: its fields, then the free-form entries every
    /// profile shares. Without an identifier, the default profile.
    Show {
        /// Profile identifier, or `default` (the default when omitted).
        id: Option<String>,
        /// Optional override for the user_memory.db path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Create a profile with empty fields; it is not the default.
    Create {
        /// Display name of the profile (e.g. `Formateur`).
        name: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Rename a profile.
    Rename {
        /// Profile identifier, or `default`.
        id: String,
        /// New display name.
        name: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Write one field of a profile: `name`, `role`, `function`, `tools`,
    /// `preferences` or `free_text`. Any other key is a free-form entry,
    /// shared by every profile.
    Set {
        /// Profile identifier, or `default`.
        id: String,
        /// Field to write (e.g. `role`).
        field: String,
        /// Value to store; an empty value empties a field.
        value: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Make a profile the default; the previous default stops being one.
    SetDefault {
        /// Profile identifier.
        id: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Delete a profile. The default profile cannot be deleted: make another
    /// one the default first.
    Delete {
        /// Profile identifier.
        id: String,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Remove the entry stored at `KEY`.
    Forget {
        /// Profile key to remove.
        key: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,

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
        UserMemoryCommand::List { db } => profiles::run_list(db.as_deref(), json),
        UserMemoryCommand::Show { id, db } => {
            profiles::run_show(db.as_deref(), id.as_deref(), json)
        }
        UserMemoryCommand::Create { name, db } => profiles::run_create(db.as_deref(), name, json),
        UserMemoryCommand::Rename { id, name, db } => {
            profiles::run_rename(db.as_deref(), id, name, json)
        }
        UserMemoryCommand::Set {
            id,
            field,
            value,
            db,
        } => profiles::run_set(db.as_deref(), id, field, value, json),
        UserMemoryCommand::SetDefault { id, db } => {
            profiles::run_set_default(db.as_deref(), id, json)
        }
        UserMemoryCommand::Delete { id, confirm, db } => {
            profiles::run_delete(db.as_deref(), id, *confirm, json)
        }
        UserMemoryCommand::Forget { key, confirm, db } => {
            run_forget(db.as_deref(), key, *confirm, json)
        }
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
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home)
        .join(apollia_core::paths::DataFile::UserMemory.file_name())
}

pub(super) fn open_repo(db_path: Option<&Path>, json: bool) -> Option<UserMemoryRepository> {
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
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.into());
}

pub(super) fn format_written_by(by: &WrittenBy) -> String {
    match by {
        WrittenBy::Onboarding => "onboarding".to_string(),
        WrittenBy::User => "user".to_string(),
        WrittenBy::Agent(name) => format!("agent:{name}"),
    }
}

// ─── forget ───────────────────────────────────────────────────────────────────

fn run_forget(db_path: Option<&Path>, key: &str, confirm: bool, json: bool) -> i32 {
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("forget the profile entry '{key}'"),
    ) {
        return code;
    }
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
        // GIVEN "user-memory show"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "show"]);
        // THEN the show subcommand is selected
        assert!(matches!(cli.cmd, UserMemoryCommand::Show { .. }));
    }

    #[test]
    fn parses_show_with_an_identifier() {
        // GIVEN "profile show default"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "show", "default"]);
        // THEN the identifier is captured
        match cli.cmd {
            UserMemoryCommand::Show { id, .. } => assert_eq!(id.as_deref(), Some("default")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_set() {
        // GIVEN "profile set <id> name Alice"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "set", "p1", "name", "Alice"]);
        // THEN identifier, field and value land in the right order
        match cli.cmd {
            UserMemoryCommand::Set {
                id, field, value, ..
            } => {
                assert_eq!(id, "p1");
                assert_eq!(field, "name");
                assert_eq!(value, "Alice");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_list_create_rename_set_default_and_delete() {
        // GIVEN the five other profile verbs
        // WHEN clap parses each argument line
        let list = TestCli::parse_from(["x", "list"]).cmd;
        let create = TestCli::parse_from(["x", "create", "Formateur"]).cmd;
        let rename = TestCli::parse_from(["x", "rename", "p1", "Formatrice"]).cmd;
        let set_default = TestCli::parse_from(["x", "set-default", "p1"]).cmd;
        let delete = TestCli::parse_from(["x", "delete", "p1", "--confirm"]).cmd;
        // THEN each lands on its variant with its arguments
        assert!(matches!(list, UserMemoryCommand::List { .. }));
        assert!(
            matches!(create, UserMemoryCommand::Create { ref name, .. } if name == "Formateur")
        );
        assert!(
            matches!(rename, UserMemoryCommand::Rename { ref id, ref name, .. } if id == "p1" && name == "Formatrice")
        );
        assert!(matches!(set_default, UserMemoryCommand::SetDefault { ref id, .. } if id == "p1"));
        assert!(
            matches!(delete, UserMemoryCommand::Delete { ref id, confirm: true, .. } if id == "p1")
        );
    }

    #[test]
    fn parses_forget() {
        // GIVEN "user-memory forget name"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "forget", "name"]);
        // THEN the key is captured
        match cli.cmd {
            UserMemoryCommand::Forget { key, .. } => assert_eq!(key, "name"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reset_without_confirm_errors() {
        // GIVEN a user-memory database
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        // WHEN a reset runs without confirmation
        let code = run_reset(Some(&db), false, true);
        // THEN it stops on an error rather than wiping the profile
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn set_then_show_round_trip() {
        // GIVEN an empty user-memory database
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        // WHEN a key is set, then the profile is shown
        let code = profiles::run_set(Some(&db), "default", "name", "Bob", true);
        // THEN both report success
        assert_eq!(code, exit_codes::SUCCESS);
        let code = profiles::run_show(Some(&db), None, true);
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn export_then_import_overwrite_round_trip() {
        // GIVEN a source database holding two entries and an empty destination
        let tmp = tempfile::tempdir().unwrap();
        let db_src = tmp.path().join("src.db");
        let db_dst = tmp.path().join("dst.db");
        let out = tmp.path().join("dump.json");

        profiles::run_set(Some(&db_src), "default", "alpha", "1", true);
        profiles::run_set(Some(&db_src), "default", "beta", "2", true);
        // WHEN the source is exported, imported into the destination, then imported again without overwrite
        // THEN all three report success, so a second import is a no-op rather than a failure
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
        // GIVEN a database holding one entry
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("um.db");
        profiles::run_set(Some(&db), "default", "x", "y", true);
        // WHEN it is forgotten with confirmation
        // THEN the command reports success
        assert_eq!(run_forget(Some(&db), "x", true, true), exit_codes::SUCCESS);
    }
}
