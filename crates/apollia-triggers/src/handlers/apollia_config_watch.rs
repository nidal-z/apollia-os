//! `ApolliaConfigWatcher`: says when `apollia.toml` changed on disk.
//!
//! The same `notify` backend as [`super::config_watch::McpConfigWatcher`], with
//! two differences the file asks for. The folder is watched rather than the
//! file: the one writer of `apollia.toml` replaces it by a rename, as most
//! editors do, and a watch held on the file itself would follow the replaced
//! inode and fall silent. And the quiet period is trailing: an editor that
//! truncates then writes produces several events, and the tick is sent once
//! the file has been left alone for the debounce, so a half-written file is
//! never read.
//!
//! The watcher reads nothing and decides nothing: each tick says "look again",
//! and the runtime reads the file, compares it with what it applied, and
//! applies it or keeps the last valid one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// Capacity of the tick channel. One pending tick is enough: the reader reads
/// the whole file, so two ticks waiting say no more than one.
const TICK_CAPACITY: usize = 1;

/// Watches the folder of `apollia.toml` and sends a tick once the file has
/// changed and stayed quiet for the debounce.
pub struct ApolliaConfigWatcher;

impl ApolliaConfigWatcher {
    /// Starts watching `path` and returns the receiver of its ticks, or `None`
    /// when the folder cannot be watched (logged at `warn`; the runtime then
    /// applies what its routes save and nothing else).
    ///
    /// The watch stops when the receiver is dropped.
    pub fn spawn(path: PathBuf, debounce: Duration) -> Option<mpsc::Receiver<()>> {
        let folder = path.parent().map(Path::to_path_buf)?;
        let file_name = path.file_name()?.to_os_string();
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(notify_tx) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "config.watch.create.failed");
                return None;
            }
        };
        if let Err(e) = watcher.watch(&folder, RecursiveMode::NonRecursive) {
            tracing::warn!(path = %folder.display(), error = %e, "config.watch.failed");
            return None;
        }
        let (tick_tx, tick_rx) = mpsc::channel(TICK_CAPACITY);
        tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            run_debounced(&notify_rx, &tick_tx, &file_name, debounce);
        });
        tracing::info!(path = %path.display(), "config.watch.started");
        Some(tick_rx)
    }
}

/// Whether `event` touches the file named `file_name`.
fn touches(event: &notify::Event, file_name: &std::ffi::OsStr) -> bool {
    matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| p.file_name() == Some(file_name))
}

/// The blocking loop: collects the events of the file and sends one tick
/// after `debounce` without a new one. Ends when the receiver is dropped or
/// the watcher disconnects.
fn run_debounced(
    notify_rx: &std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    tick_tx: &mpsc::Sender<()>,
    file_name: &std::ffi::OsStr,
    debounce: Duration,
) {
    let poll = Duration::from_millis(50);
    let mut dirty_since: Option<std::time::Instant> = None;
    loop {
        match notify_rx.recv_timeout(poll) {
            Ok(Ok(event)) if touches(&event, file_name) => {
                dirty_since = Some(std::time::Instant::now());
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "config.watch.error");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if tick_tx.is_closed() {
            break;
        }
        if dirty_since.is_some_and(|since| since.elapsed() >= debounce) {
            dirty_since = None;
            // A tick already waiting says the same thing: a full channel is
            // not an error.
            let _ = tick_tx.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn tick_within(rx: &mut mpsc::Receiver<()>, wait: Duration) -> bool {
        tokio::time::timeout(wait, rx.recv())
            .await
            .ok()
            .flatten()
            .is_some()
    }

    #[tokio::test]
    async fn a_file_replaced_by_a_rename_sends_one_tick_after_the_quiet_period() {
        // GIVEN a watched apollia.toml
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "# first\n").unwrap();
        let mut rx = ApolliaConfigWatcher::spawn(path.clone(), Duration::from_millis(200))
            .expect("the folder is watchable");
        // The watch is armed asynchronously on some platforms.
        tokio::time::sleep(Duration::from_millis(300)).await;

        // WHEN the file is written twice in a row, then replaced by a rename
        std::fs::write(&path, "# second\n").unwrap();
        std::fs::write(&path, "# third\n").unwrap();
        let staged = dir.path().join(".apollia.toml.writing");
        std::fs::write(&staged, "# fourth\n").unwrap();
        std::fs::rename(&staged, &path).unwrap();

        // THEN a tick arrives, and the burst does not leave a second one
        // behind once the file is quiet
        assert!(tick_within(&mut rx, Duration::from_secs(10)).await);
        assert!(!tick_within(&mut rx, Duration::from_millis(800)).await);
    }

    #[tokio::test]
    async fn another_file_of_the_folder_sends_nothing() {
        // GIVEN a watched apollia.toml
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "").unwrap();
        let mut rx = ApolliaConfigWatcher::spawn(path, Duration::from_millis(100))
            .expect("the folder is watchable");
        tokio::time::sleep(Duration::from_millis(300)).await;

        // WHEN a neighbour file changes
        std::fs::write(dir.path().join("mcp.toml"), "x = 1\n").unwrap();

        // THEN no tick is sent
        assert!(!tick_within(&mut rx, Duration::from_millis(800)).await);
    }
}
