//! Writing a conversation to disk and revealing it in the system file browser.
//! The frontend owns the formatting; these commands own the path.

use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

/// Writes exported conversation content to disk.
///
/// Thin IPC wrapper around [`std::fs::write`] - the frontend owns the
/// formatting (Markdown / JSON / Markdown-with-tools) via
/// `lib/chat/exportConversation.ts`, then calls this to persist at the
/// path chosen via the native save dialog.
///
/// `_mime` is accepted for telemetry parity with the frontend caller but
/// is not currently recorded; the extension in `dest_path` is authoritative.
#[tauri::command]
pub async fn export_conversation(
    dest_path: String,
    content: String,
    #[allow(unused_variables)] mime: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || std::fs::write(&dest_path, content.as_bytes()))
        .await
        .map_err(|e| format!("export_conversation join: {e}"))?
        .map_err(|e| format!("export_conversation write: {e}"))
}

/// Reveal a file referenced by a chat tool body in the OS file manager.
///
/// `path` may be absolute (a POSIX root `/…`, a home-relative `~/…`, or a
/// Windows drive `C:\…`) or relative to the session's working directory.
/// Absolute paths (with `~` expanded to the home directory) are revealed as-is;
/// a relative path is joined onto the session workspace resolved by the chat
/// manager, mirroring the sandbox root the agent used for its file tools, then
/// canonicalized best-effort.
///
/// Revealing is read-only. When the workspace cannot be resolved the raw path
/// is revealed best-effort; a reveal failure returns an error the frontend
/// swallows, so a click is never a hard failure but also never silently
/// misleading.
#[tauri::command]
pub async fn reveal_session_path(
    app: tauri::AppHandle,
    state: State<'_, RuntimeHandle>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    if path.trim().is_empty() {
        return Err("path must not be empty".to_string());
    }

    // An absolute path (or `~`) does not depend on the session workspace; a
    // relative path is joined onto the resolved workspace, or revealed raw as a
    // last resort when no directory can be resolved.
    let resolved = match expand_absolute_path(&path) {
        Some(abs) => abs,
        None => {
            let workspace = match state.chat_manager.as_ref() {
                Some(manager) => manager.resolve_session_workspace(session_id.clone()).await,
                None => None,
            };
            match workspace {
                Some(ws) => ws.join(&path),
                None => std::path::PathBuf::from(&path),
            }
        }
    };

    // Canonicalize so symlinks and `..` segments resolve to the real target;
    // fall back to the joined path when the target does not exist yet.
    let target = std::fs::canonicalize(&resolved).unwrap_or(resolved);

    match app.opener().reveal_item_in_dir(&target) {
        Ok(()) => {
            tracing::event!(
                tracing::Level::DEBUG,
                session_id = %session_id,
                target = %target.display(),
                "chat.reveal_session_path"
            );
            Ok(())
        }
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                session_id = %session_id,
                target = %target.display(),
                error = %e,
                "chat.reveal_session_path.failed"
            );
            Err(format!("failed to reveal path: {e}"))
        }
    }
}

/// Expand a path that is absolute without a working directory: a POSIX root
/// (`/…`), a home-relative path (`~` / `~/…`), or a Windows drive (`C:\…` /
/// `C:/…`). Returns `None` for a relative path so the caller joins it onto the
/// session workspace. Mirrors the frontend `isRevealablePath` classifier.
fn expand_absolute_path(path: &str) -> Option<std::path::PathBuf> {
    if path == "~" {
        return apollia_core::paths::home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return apollia_core::paths::home_dir().map(|h| h.join(rest));
    }
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    if path.starts_with('/') || windows_drive {
        return Some(std::path::PathBuf::from(path));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_absolute_path_classifies_paths() {
        // GIVEN a relative path
        // WHEN classified
        // THEN it is not treated as absolute (caller joins onto the workspace)
        assert!(expand_absolute_path("src/main.rs").is_none());
        assert!(expand_absolute_path("./notes.txt").is_none());
        assert!(expand_absolute_path("../sibling/file").is_none());

        // GIVEN a POSIX root or Windows drive path
        // WHEN classified
        // THEN it is returned unchanged
        assert_eq!(
            expand_absolute_path("/etc/hosts"),
            Some(std::path::PathBuf::from("/etc/hosts"))
        );
        assert_eq!(
            expand_absolute_path(r"C:\Users\me\file.txt"),
            Some(std::path::PathBuf::from(r"C:\Users\me\file.txt"))
        );
        assert_eq!(
            expand_absolute_path("D:/data/log"),
            Some(std::path::PathBuf::from("D:/data/log"))
        );
    }

    #[test]
    fn test_expand_absolute_path_expands_home() {
        // GIVEN a home directory is set
        // The variable is a process global: hold the shared guard so a test
        // reading the resolved home elsewhere cannot observe this value, and
        // put the previous one back before releasing it.
        let _guard = crate::commands::home_env_lock();
        let previous = std::env::var_os("HOME");
        // SAFETY: test-only mutation of a process env var, serialised by the
        // guard above and undone below.
        unsafe {
            std::env::set_var("HOME", "/home/tester");
        }

        // WHEN a `~`-prefixed path is expanded
        let bare = expand_absolute_path("~");
        let nested = expand_absolute_path("~/projects/apollia");

        // Restore before asserting: a failing assertion must not leave the fake
        // home behind for the next test that reads it.
        // SAFETY: same guard, restoring the value observed on entry.
        unsafe {
            match previous {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }

        // THEN the tilde is replaced by the home directory
        assert_eq!(bare, Some(std::path::PathBuf::from("/home/tester")));
        assert_eq!(
            nested,
            Some(std::path::PathBuf::from("/home/tester/projects/apollia"))
        );
    }
}
