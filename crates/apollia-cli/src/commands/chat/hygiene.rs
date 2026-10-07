//! `apollia-os chat` hygiene verbs: list, delete, rename, export.

use std::path::{Path, PathBuf};

use apollia_runtime::chat::{ChatSessionRepository, MessageRow, SessionRow};

use crate::exit_codes;

use super::ChatHygieneCommand;

// ─── hygiene subcommands ──────────────────────────────────────────────────────

/// Resolve the chat database path: `--db <PATH>` override, then
/// `~/.apollia/chat.db`. Tests inject explicit paths via the `--db` flag.
pub(super) fn resolve_chat_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    apollia_core::paths::home_dir()
        .map(apollia_core::paths::data_dir_under)
        .unwrap_or_else(|| PathBuf::from(apollia_core::paths::DATA_DIR_NAME))
        .join(apollia_core::paths::DataFile::Chat.file_name())
}

pub(super) fn open_chat_repo(db: Option<&Path>, json: bool) -> Option<ChatSessionRepository> {
    let path = resolve_chat_db(db);
    if !path.exists() {
        emit_chat_error(
            format!(
                "chat database not found at {} (no chat session has ever been persisted)",
                path.display()
            ),
            json,
        );
        return None;
    }
    match ChatSessionRepository::open(&path) {
        Ok(r) => Some(r),
        Err(e) => {
            emit_chat_error(format!("open {} failed: {e}", path.display()), json);
            None
        }
    }
}

pub(super) fn emit_chat_error(msg: String, json: bool) {
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
}

/// Entry point for `chat <subcommand>`. `chat config`, `chat capabilities`,
/// `chat template`, `chat settings` and `chat notes` reach the runtime,
/// through `socket`.
pub async fn run_hygiene(
    cmd: &ChatHygieneCommand,
    socket: Option<std::path::PathBuf>,
    json: bool,
) -> i32 {
    match cmd {
        ChatHygieneCommand::List { db } => run_chat_list(db.as_deref(), json),
        ChatHygieneCommand::Delete {
            session_id,
            confirm,
            db,
        } => run_chat_delete(session_id, *confirm, db.as_deref(), json),
        ChatHygieneCommand::Rename {
            session_id,
            title,
            db,
        } => run_chat_rename(session_id, title, db.as_deref(), json),
        ChatHygieneCommand::Export {
            session_id,
            output,
            format,
            db,
        } => run_chat_export(session_id, output.as_deref(), format, db.as_deref(), json),
        ChatHygieneCommand::Capabilities { session_id } => {
            super::capabilities::run(session_id, socket, json).await
        }
        ChatHygieneCommand::Template { session_id } => {
            super::template::run(session_id, socket, json).await
        }
        ChatHygieneCommand::Settings { command } => {
            super::settings::run_settings(command, socket, json).await
        }
        ChatHygieneCommand::Notes { command } => {
            super::settings::run_notes(command, socket, json).await
        }
        ChatHygieneCommand::Config { command } => {
            crate::commands::chat_config::run(command, socket, json).await
        }
    }
}

/// One line of `chat list`: a session and the folder it is in.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct ListedSession {
    id: String,
    title: Option<String>,
    folder: Option<String>,
    mode: String,
    status: String,
    created_at: String,
}

/// The sessions of `repo` as `chat list` shows them, the most recent first.
pub(super) fn listed_sessions(
    repo: &ChatSessionRepository,
) -> Result<Vec<ListedSession>, apollia_runtime::chat::ChatError> {
    let mut folders = repo.session_folder_names()?;
    Ok(repo
        .list_sessions(None)?
        .into_iter()
        .map(|row| ListedSession {
            folder: folders.remove(&row.id),
            id: row.id,
            title: row.title.filter(|title| !title.trim().is_empty()),
            mode: row.mode,
            status: row.status,
            created_at: row.created_at,
        })
        .collect())
}

/// The table `chat list` prints. The id is whole: `delete`, `rename` and
/// `export` take it as it is shown.
pub(super) fn format_session_list(sessions: &[ListedSession]) -> String {
    use std::fmt::Write as _;
    if sessions.is_empty() {
        return "No sessions found.\n".to_owned();
    }
    let id_width = sessions
        .iter()
        .map(|s| s.id.chars().count())
        .max()
        .unwrap_or(2)
        .max(2);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<id_width$}  {:<20}  {:<10}  {:<40}  DATE",
        "ID", "FOLDER", "STATUS", "TITLE"
    );
    let _ = writeln!(out, "{}", "-".repeat(id_width + 100));
    for s in sessions {
        let folder =
            crate::commands::truncate_for_display(s.folder.as_deref().unwrap_or("-"), 20, 20, "");
        let title =
            crate::commands::truncate_for_display(s.title.as_deref().unwrap_or("-"), 40, 40, "");
        let _ = writeln!(
            out,
            "{:<id_width$}  {:<20}  {:<10}  {:<40}  {}",
            s.id, folder, s.status, title, s.created_at
        );
    }
    out
}

pub(super) fn run_chat_list(db: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match listed_sessions(&repo) {
        Ok(sessions) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sessions).unwrap_or_default()
                );
            } else {
                print!("{}", format_session_list(&sessions));
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_chat_error(format!("list failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_chat_delete(
    session_id: &str,
    confirm: bool,
    db: Option<&Path>,
    json: bool,
) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    if !confirm {
        emit_chat_error(
            format!("use --confirm to delete chat session '{session_id}' (irreversible)"),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.delete_session(session_id) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({ "session_id": session_id, "deleted": true });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * chat session '{session_id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_chat_error(format!("delete failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_chat_rename(session_id: &str, title: &str, db: Option<&Path>, json: bool) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let trimmed = title.trim();
    if trimmed.is_empty() {
        emit_chat_error("title must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let cleaned: String = trimmed.chars().take(100).collect();
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.rename_session(session_id, &cleaned) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "session_id": session_id,
                    "title": cleaned,
                    "renamed": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * chat session '{session_id}' renamed to: {cleaned}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_chat_error(format!("rename failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_chat_export(
    session_id: &str,
    output: Option<&Path>,
    format: &str,
    db: Option<&Path>,
    json: bool,
) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let session = match repo.get_session(session_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            emit_chat_error(format!("chat session '{session_id}' not found"), json);
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            emit_chat_error(format!("read session failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let messages = match repo.get_messages(session_id, None) {
        Ok(m) => m,
        Err(e) => {
            emit_chat_error(format!("read messages failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let body = match format {
        "json" => format_export_json(&session, &messages),
        _ => format_export_markdown(&session, &messages),
    };

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, body.as_bytes()) {
                emit_chat_error(format!("write to {} failed: {e}", path.display()), json);
                return exit_codes::GENERAL_ERROR;
            }
            if json {
                let out = serde_json::json!({
                    "session_id": session_id,
                    "format": format,
                    "output": path.display().to_string(),
                    "bytes": body.len(),
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!(
                    "  * chat session '{session_id}' exported to {} ({} bytes)",
                    path.display(),
                    body.len()
                );
            }
        }
        None => {
            print!("{body}");
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn format_export_markdown(session: &SessionRow, messages: &[MessageRow]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let title = session
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled session");
    let _ = writeln!(out, "# {title}");
    let _ = writeln!(out, "\n- id: {}", session.id);
    let _ = writeln!(out, "- created_at: {}", session.created_at);
    let _ = writeln!(out, "- mode: {}", session.mode);
    let _ = writeln!(out, "- status: {}", session.status);
    let _ = writeln!(out, "- messages: {}", messages.len());
    let _ = writeln!(out);
    for m in messages {
        let _ = writeln!(out, "## {} ({})", m.role, m.created_at);
        let _ = writeln!(out, "\n{}\n", m.content);
    }
    out
}

pub(super) fn format_export_json(session: &SessionRow, messages: &[MessageRow]) -> String {
    let payload = serde_json::json!({
        "session": {
            "id": session.id,
            "title": session.title,
            "mode": session.mode,
            "status": session.status,
            "created_at": session.created_at,
            "closed_at": session.closed_at,
            "agent_name": session.agent_name,
        },
        "messages": messages.iter().map(|m| serde_json::json!({
            "id": m.id,
            "role": m.role,
            "content": m.content,
            "created_at": m.created_at,
            "seq": m.seq,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}
