//! `apollia rollback` — restore filesystem state after an agent session.
//!
//! Reads the reversible journal written by `FileWrite` and `FileEdit`
//! and replays the inverse of each mutation in reverse order.
//! Works standalone: no running runtime is required.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

use clap::Args;

use apollia_tools::journal::{list_sessions, rollback_session, JournalEntry, JournalError};

// ────────────────────────────────────────────────────────────────────────────
// CLI args
// ────────────────────────────────────────────────────────────────────────────

/// Revert filesystem mutations recorded by the agent during a chat session.
#[derive(Debug, Args)]
pub struct RollbackArgs {
    /// Session ID to roll back (from `apollia rollback --list`).
    ///
    /// Either `session_id` or `--last-n` must be provided.
    pub session_id: Option<String>,

    /// Roll back the N most recent sessions instead of a specific one.
    #[arg(long, value_name = "N", conflicts_with = "session_id")]
    pub last_n: Option<usize>,

    /// List recorded sessions without rolling anything back.
    #[arg(long, conflicts_with_all = ["session_id", "last_n"])]
    pub list: bool,

    /// Simulate the rollback: print what would be reverted without applying it.
    #[arg(long)]
    pub dry_run: bool,

    /// Output machine-readable JSON instead of human text.
    #[arg(long)]
    pub json: bool,

    /// Override the journal root directory (default: `~/.apollia/journal`).
    #[arg(long, value_name = "DIR")]
    pub journal_root: Option<PathBuf>,
}

// ────────────────────────────────────────────────────────────────────────────
// Error
// ────────────────────────────────────────────────────────────────────────────

/// Errors produced by the rollback command.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    /// No session ID and no `--last-n` was provided.
    #[error("provide a session_id or --last-n (see --list for available sessions)")]
    NoTarget,

    /// The requested session was not found in the journal.
    #[error("session '{0}' not found (use --list to see available sessions)")]
    SessionNotFound(String),

    /// An underlying journal error.
    #[error("journal error: {0}")]
    Journal(#[from] JournalError),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ────────────────────────────────────────────────────────────────────────────
// Entry point
// ────────────────────────────────────────────────────────────────────────────

/// Execute the `rollback` command.
pub async fn run(args: &RollbackArgs, json: bool) -> i32 {
    let effective_json = json || args.json;

    let journal_root = resolve_journal_root(args.journal_root.as_deref());

    if args.list {
        return cmd_list(&journal_root, effective_json).await;
    }

    match cmd_rollback(args, &journal_root, effective_json).await {
        Ok(()) => crate::exit_codes::SUCCESS,
        Err(e) => {
            if effective_json {
                let msg = serde_json::json!({ "error": e.to_string() });
                eprintln!("{}", serde_json::to_string_pretty(&msg).unwrap_or_default());
            } else {
                eprintln!("Error: {e}");
            }
            crate::exit_codes::GENERAL_ERROR
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// list sub-command
// ────────────────────────────────────────────────────────────────────────────

async fn cmd_list(journal_root: &Path, json: bool) -> i32 {
    match list_sessions(journal_root).await {
        Ok(sessions) => {
            if json {
                match serde_json::to_string_pretty(&sessions) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("JSON error: {e}");
                        return crate::exit_codes::GENERAL_ERROR;
                    }
                }
            } else if sessions.is_empty() {
                println!("No recorded sessions in {}.", journal_root.display());
            } else {
                println!("Recorded sessions (newest first):");
                for s in &sessions {
                    println!("  {s}");
                }
            }
            crate::exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error listing sessions: {e}");
            crate::exit_codes::GENERAL_ERROR
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// rollback sub-command
// ────────────────────────────────────────────────────────────────────────────

async fn cmd_rollback(
    args: &RollbackArgs,
    journal_root: &Path,
    json: bool,
) -> Result<(), RollbackError> {
    let target_sessions = resolve_target_sessions(args, journal_root).await?;

    for session_id in &target_sessions {
        let ops = rollback_session(journal_root, session_id, args.dry_run)
            .await
            .map_err(|e| match e {
                JournalError::SessionNotFound(_) => {
                    RollbackError::SessionNotFound(session_id.clone())
                }
                other => RollbackError::Journal(other),
            })?;

        if json {
            print_json_report(session_id, &ops, args.dry_run)?;
        } else {
            print_human_report(session_id, &ops, args.dry_run);
        }
    }

    Ok(())
}

/// Determine which sessions to roll back based on the provided args.
async fn resolve_target_sessions(
    args: &RollbackArgs,
    journal_root: &Path,
) -> Result<Vec<String>, RollbackError> {
    if let Some(ref sid) = args.session_id {
        return Ok(vec![sid.clone()]);
    }

    if let Some(n) = args.last_n {
        let all = list_sessions(journal_root).await?;
        // list_sessions returns newest-first; take first N
        return Ok(all.into_iter().take(n).collect());
    }

    Err(RollbackError::NoTarget)
}

// ────────────────────────────────────────────────────────────────────────────
// Output formatting
// ────────────────────────────────────────────────────────────────────────────

fn print_human_report(session_id: &str, ops: &[JournalEntry], dry_run: bool) {
    let label = if dry_run { " (dry-run)" } else { "" };
    let is_tty = std::io::stdout().is_terminal();
    let sep = "─".repeat(50);

    println!("Rollback session {session_id}{label}");
    println!("{sep}");

    if ops.is_empty() {
        println!("  (no mutations recorded)");
    } else {
        for op in ops.iter().rev() {
            println!("  {}", format_op_line(op));
        }
    }

    let verb = if dry_run { "would revert" } else { "reverted" };
    let plural = if ops.len() == 1 { "" } else { "s" };
    let status = if is_tty { "No error." } else { "" };
    println!("{} operation{} {}. {}", ops.len(), plural, verb, status);
}

fn format_op_line(entry: &JournalEntry) -> String {
    match entry {
        JournalEntry::Write {
            path,
            previous_content,
            ..
        } => {
            let action = match previous_content {
                Some(bytes) => format!("restore {} B", bytes.len()),
                None => "remove (was new file)".to_string(),
            };
            format!(" \u{21b6} write  {} \u{2192} {}", path.display(), action)
        }
        JournalEntry::Delete {
            path,
            previous_content,
            previous_mode,
        } => {
            format!(
                " \u{21b6} delete {} \u{2192} recreate {} B (mode {:o})",
                path.display(),
                previous_content.len(),
                previous_mode
            )
        }
        JournalEntry::Mkdir { path } => {
            format!(" \u{21b6} mkdir  {} \u{2192} rmdir", path.display())
        }
        JournalEntry::Rmdir { path } => {
            format!(" \u{21b6} rmdir  {} \u{2192} mkdir", path.display())
        }
        JournalEntry::Chmod {
            path,
            previous_mode,
        } => {
            format!(
                " \u{21b6} chmod  {} \u{2192} restore mode {:o}",
                path.display(),
                previous_mode
            )
        }
    }
}

fn print_json_report(
    session_id: &str,
    ops: &[JournalEntry],
    dry_run: bool,
) -> Result<(), RollbackError> {
    let items: Vec<serde_json::Value> = ops.iter().rev().map(op_to_json).collect();

    let report = serde_json::json!({
        "session_id": session_id,
        "dry_run": dry_run,
        "operations_count": ops.len(),
        "operations": items,
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn op_to_json(entry: &JournalEntry) -> serde_json::Value {
    match entry {
        JournalEntry::Write {
            path,
            previous_content,
            ..
        } => {
            let (action, bytes) = match previous_content {
                Some(b) => ("restore", b.len()),
                None => ("remove", 0),
            };
            serde_json::json!({
                "op": "write",
                "path": path.display().to_string(),
                "action": action,
                "bytes": bytes,
            })
        }
        JournalEntry::Delete {
            path,
            previous_content,
            previous_mode,
        } => serde_json::json!({
            "op": "delete",
            "path": path.display().to_string(),
            "action": "recreate",
            "bytes": previous_content.len(),
            "mode": previous_mode,
        }),
        JournalEntry::Mkdir { path } => serde_json::json!({
            "op": "mkdir",
            "path": path.display().to_string(),
            "action": "rmdir",
        }),
        JournalEntry::Rmdir { path } => serde_json::json!({
            "op": "rmdir",
            "path": path.display().to_string(),
            "action": "mkdir",
        }),
        JournalEntry::Chmod {
            path,
            previous_mode,
        } => serde_json::json!({
            "op": "chmod",
            "path": path.display().to_string(),
            "action": "restore_mode",
            "previous_mode": previous_mode,
        }),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Resolve the effective journal root: CLI override → config default.
fn resolve_journal_root(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia").join("journal")
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::journal::{JournalEntry, JournalWriter};
    use tempfile::TempDir;

    async fn seed_session(
        journal_root: &std::path::Path,
        session_id: &str,
        entries: Vec<JournalEntry>,
    ) {
        let handle = JournalWriter::spawn(session_id.to_string(), journal_root.to_path_buf(), 50);
        for e in entries {
            handle.record(e).await.expect("record");
        }
        handle.shutdown().await;
    }

    // ── list returns seeded sessions ────────────────────────────────────────

    #[tokio::test]
    async fn rollback_cmd_list_returns_sessions() {
        // GIVEN two seeded sessions
        let dir = TempDir::new().expect("tmpdir");
        seed_session(
            dir.path(),
            "sess-a",
            vec![JournalEntry::Mkdir {
                path: PathBuf::from("/tmp/a"),
            }],
        )
        .await;
        seed_session(
            dir.path(),
            "sess-b",
            vec![JournalEntry::Mkdir {
                path: PathBuf::from("/tmp/b"),
            }],
        )
        .await;

        // WHEN listing
        let sessions = list_sessions(dir.path()).await.expect("list ok");

        // THEN both are returned
        assert!(sessions.contains(&"sess-a".to_string()));
        assert!(sessions.contains(&"sess-b".to_string()));
    }

    // ── dry-run returns ops without mutation ─────────────────────────────────

    #[tokio::test]
    async fn rollback_cmd_dry_run_returns_ops_without_mutation() {
        // GIVEN a file and a journal entry recording its previous content
        let dir = TempDir::new().expect("tmpdir");
        let file_path = dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"mutated")
            .await
            .expect("write");

        let journal_dir = dir.path().join("journal");

        seed_session(
            &journal_dir,
            "sess-dry",
            vec![JournalEntry::Write {
                path: file_path.clone(),
                previous_content: Some(b"original".to_vec()),
                previous_mode: None,
                previous_mtime: None,
            }],
        )
        .await;

        // WHEN dry-run rollback
        let ops = rollback_session(&journal_dir, "sess-dry", true)
            .await
            .expect("dry-run ok");

        // THEN ops are returned
        assert_eq!(ops.len(), 1);

        // AND the file is NOT restored
        let content = tokio::fs::read(&file_path).await.expect("read");
        assert_eq!(content, b"mutated");
    }

    // ── session not found returns error ──────────────────────────────────────

    #[tokio::test]
    async fn rollback_cmd_missing_session_returns_error() {
        // GIVEN an empty journal dir
        let dir = TempDir::new().expect("tmpdir");
        let journal_dir = dir.path().join("journal");
        tokio::fs::create_dir_all(&journal_dir)
            .await
            .expect("mkdir");

        // WHEN rolling back a non-existent session
        let result = rollback_session(&journal_dir, "no-such-session", false).await;

        // THEN SessionNotFound
        assert!(
            matches!(result, Err(JournalError::SessionNotFound(_))),
            "expected SessionNotFound"
        );
    }

    // ── resolve_journal_root uses HOME ───────────────────────────────────────

    #[test]
    fn resolve_journal_root_no_override_uses_home() {
        // GIVEN HOME is set
        // WHEN no override
        let root = resolve_journal_root(None);
        // THEN path ends with .apollia/journal
        assert!(root.ends_with(".apollia/journal"));
    }

    #[test]
    fn resolve_journal_root_override_is_used() {
        // GIVEN an override path
        let override_path = std::path::Path::new("/custom/journal");
        // WHEN resolve
        let root = resolve_journal_root(Some(override_path));
        // THEN exact path is returned
        assert_eq!(root, PathBuf::from("/custom/journal"));
    }
}
