//! Shows a running runtime the `apollia.toml` the command line has just
//! written, so the signed journal records the change as made by the command
//! line.
//!
//! The commands write the file themselves, daemon or not. The runtime is the
//! one writer of its journal: when it runs, it is shown the text through the
//! save route, under the origin `cli`; the fingerprint presented is the one of
//! that same text, so the route writes nothing again. When no runtime answers,
//! nothing is shown, and the next start of the runtime finds a file whose
//! fingerprint is not the last one it journaled and records the change then.

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::config::writer::read_snapshot;

use crate::client::{default_socket_path, RuntimeClient};

/// The save route of the runtime.
const CONFIG_ROUTE: &str = "/api/v1/config";

/// How long a runtime is given to answer. The command has already succeeded:
/// a runtime that does not answer must not hold the terminal.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

/// What the save route is sent: the text as written, its own fingerprint, and
/// the origin the journal records.
fn declaration(text: &str, sha256: &str) -> serde_json::Value {
    serde_json::json!({ "text": text, "sha256": sha256, "origin": "cli" })
}

/// Shows the file at `path` to the runtime behind `socket`, when one answers.
/// Returns whether the runtime took it. Never an error for the caller: the
/// write is done, and the journal catches up at the next start otherwise.
pub(crate) async fn show_to_runtime(path: &Path, socket: Option<PathBuf>) -> bool {
    let Ok(snapshot) = read_snapshot(path) else {
        return false;
    };
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    let body = declaration(&snapshot.text, &snapshot.sha256);
    let answer = tokio::time::timeout(ANSWER_TIMEOUT, client.put(CONFIG_ROUTE, Some(&body))).await;
    let taken = matches!(&answer, Ok(Ok(response)) if response.status == 200);
    tracing::debug!(taken, "config.write.shown_to_runtime");
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_declaration_presents_the_text_written_under_its_own_fingerprint_as_cli() {
        // GIVEN a file the command line has just written
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[api]\nport = 7790\n").unwrap();
        let snapshot = read_snapshot(&path).unwrap();

        // WHEN the declaration for the save route is built
        let body = declaration(&snapshot.text, &snapshot.sha256);

        // THEN it carries that text, the fingerprint of that same text, so the
        // route has nothing to write, and the origin the journal records
        assert_eq!(
            body,
            serde_json::json!({
                "text": "[api]\nport = 7790\n",
                "sha256": apollia_core::config::writer::fingerprint("[api]\nport = 7790\n"),
                "origin": "cli",
            })
        );
    }

    #[tokio::test]
    async fn test_without_a_runtime_nothing_is_shown_and_the_file_is_left_as_written() {
        // GIVEN a written file and a socket no runtime listens on
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "[api]\nport = 7790\n").unwrap();
        let socket = dir.path().join("absent.sock");

        // WHEN the command line shows its write
        let taken = show_to_runtime(&path, Some(socket)).await;

        // THEN no runtime took it, and the file is as the command wrote it:
        // the next start of the runtime records the change
        assert!(!taken);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[api]\nport = 7790\n"
        );
    }
}
