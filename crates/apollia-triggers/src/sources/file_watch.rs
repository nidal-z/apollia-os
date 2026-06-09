//! `FileWatchTrigger`: filesystem watch source.
//!
//! Spawns a Tokio task that watches a directory via the `notify` crate v6
//! (inotify on Linux, kqueue on macOS, ReadDirectoryChanges on Windows) and
//! forwards filtered events to the `TriggerEngine`'s async channel.
//!
//! ## Event filtering
//!
//! Before propagation, each event passes through two filters:
//! - **Path exclusion**: any segment or pattern matching `exclude_patterns` is
//!   silently ignored (e.g. `.git/`, `node_modules/`, `*.log`).
//! - **Symbolic links**: if `follow_symlinks = false` (default), paths
//!   identified as symlinks via `fs::symlink_metadata` are ignored.
//!
//! ## Deduplication
//!
//! Repeated events on the same path within a 1-second window are deduplicated:
//! a single event is propagated and duplicates are logged at `tracing::debug!`.
//!
//! ## Sync-to-async bridge
//!
//! `notify` uses a synchronous API (`std::sync::mpsc::Sender`). The bridge is
//! done via `recv_timeout(50ms)` in the Tokio task loop: if the async channel
//! is closed (`tx.is_closed()`), the loop terminates cleanly and the `Watcher`
//! is dropped, freeing the inotify/kqueue resources.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{
    FileEventKind, TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig,
};

/// Trigger source based on filesystem watching.
///
/// Uses the `notify` crate v6 (cross-platform: inotify/kqueue/FSEvents) with a
/// sync-to-async bridge via `recv_timeout` to guarantee a clean shutdown.
pub struct FileWatchTrigger;

impl FileWatchTrigger {
    /// Spawns a Tokio task that watches the configured directory and forwards
    /// filtered file events to the `TriggerEngine`'s async channel.
    ///
    /// The task terminates cleanly as soon as the channel is closed (`tx.is_closed()`).
    /// The `Watcher` is dropped automatically, freeing OS resources (inotify/kqueue).
    /// Returns a `JoinHandle<()>` for abort during hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Extract path and recursive from the source.
            let (raw_path, recursive) = match &def.source {
                TriggerSourceConfig::FileWatch {
                    path, recursive, ..
                } => (path.clone(), *recursive),
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "FileWatchTrigger::spawn called with non-FileWatch source"
                    );
                    return;
                }
            };

            let path = expand_tilde(&raw_path);

            // Recursive mode only matters for directories. For a file path,
            // `notify` silently ignores the mode, but we keep NonRecursive by
            // default to stay explicit.
            let path_is_dir = std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            let mode = if recursive && path_is_dir {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            // Sync notify channel to the blocking thread.
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let mut watcher = match notify::recommended_watcher(notify_tx) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(
                        trigger = %def.id,
                        error = %e,
                        "failed to create file watcher"
                    );
                    return;
                }
            };

            if let Err(e) = watcher.watch(&path, mode) {
                tracing::error!(
                    trigger = %def.id,
                    error = %e,
                    path = %path.display(),
                    "failed to watch path"
                );
                return;
            }

            // Sync-to-async bridge: spawn_blocking so we do not block the Tokio
            // workers. `blocking_send` is used from the blocking context, and
            // the watcher is kept alive inside the blocking closure.
            let source = def.source.clone();
            let trigger_id = def.id.clone();
            let agent = def.agent.clone();

            tokio::task::spawn_blocking(move || {
                let _watcher = watcher; // kept alive until the closure ends
                let ctx = WatchContext {
                    source: &source,
                    trigger_id: &trigger_id,
                    agent: &agent,
                };
                run_watch_loop(&notify_rx, &tx, &ctx);
            })
            .await
            .ok();
        })
    }
}

/// Immutable context of a FileWatch trigger, shared by the watch loop and event
/// forwarding (source configuration + trigger identity).
struct WatchContext<'a> {
    source: &'a TriggerSourceConfig,
    trigger_id: &'a str,
    agent: &'a str,
}

/// Blocking loop: pumps the sync `notify` channel and forwards filtered,
/// deduplicated events to the `TriggerEngine`'s async channel.
///
/// Terminates when the async channel is closed (`tx.is_closed()`) or the sync
/// channel is disconnected.
fn run_watch_loop(
    notify_rx: &std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    tx: &mpsc::Sender<TriggerEvent>,
    ctx: &WatchContext,
) {
    // Deduplication: macOS FSEvents emits multiple Create events per `cp`
    // (fd allocation + data flush). Suppress repeated fires for the same
    // path within a 1-second window.
    let mut dedup: std::collections::HashMap<std::path::PathBuf, std::time::Instant> =
        std::collections::HashMap::new();
    loop {
        match notify_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(Ok(event)) => {
                if forward_event(event, tx, ctx, &mut dedup) {
                    break;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    trigger = %ctx.trigger_id,
                    error = %e,
                    "notify error"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Check whether the async channel is closed (engine stopped).
                if tx.is_closed() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Maps, deduplicates, and forwards a `notify` event.
///
/// Returns `true` if the async channel is closed (engine dropped) and the loop
/// must stop.
fn forward_event(
    event: notify::Event,
    tx: &mpsc::Sender<TriggerEvent>,
    ctx: &WatchContext,
    dedup: &mut std::collections::HashMap<std::path::PathBuf, std::time::Instant>,
) -> bool {
    let Some(payload) = map_notify_event(event, ctx.source) else {
        return false;
    };
    if is_duplicate_file_event(&payload, dedup) {
        return false;
    }
    let trigger_event = TriggerEvent {
        trigger_id: ctx.trigger_id.to_owned(),
        agent: ctx.agent.to_owned(),
        payload,
        fired_at: chrono::Utc::now(),
    };
    // Engine dropped: clean shutdown.
    tx.blocking_send(trigger_event).is_err()
}

/// Applies deduplication to `File` payloads within a 1-second window.
///
/// Returns `true` if the payload is a recent duplicate (to be ignored). Updates
/// the deduplication map and bounds it to 10s for the `File` payloads kept.
fn is_duplicate_file_event(
    payload: &TriggerPayload,
    dedup: &mut std::collections::HashMap<std::path::PathBuf, std::time::Instant>,
) -> bool {
    let TriggerPayload::File {
        path: ref file_path,
        ..
    } = *payload
    else {
        return false;
    };
    let now = std::time::Instant::now();
    if dedup
        .get(file_path)
        .is_some_and(|last| last.elapsed() < std::time::Duration::from_secs(1))
    {
        tracing::debug!(
            path = %file_path.display(),
            "deduplicated file event"
        );
        return true;
    }
    dedup.insert(file_path.clone(), now);
    // Keep the map bounded: prune entries older than 10s.
    dedup.retain(|_, t| t.elapsed() < std::time::Duration::from_secs(10));
    false
}

/// Maps a `notify` event to a [`TriggerPayload::File`] according to the declared filters.
///
/// Returns `None` when:
/// - The event does not match the configured filters (`events`).
/// - The event kind is unknown (`Access`, `Other`, etc.).
/// - The file path is missing from the event.
/// - The path matches an exclusion pattern (`exclude_patterns`).
/// - `follow_symlinks = false` and the path is a symbolic link.
///
/// This function is pure and testable without a real filesystem (except the
/// metadata and symlink checks, which need a real path).
pub fn map_notify_event(
    event: notify::Event,
    source: &TriggerSourceConfig,
) -> Option<TriggerPayload> {
    let (desired_kinds, follow_symlinks, exclude_patterns) = match source {
        TriggerSourceConfig::FileWatch {
            events,
            follow_symlinks,
            exclude_patterns,
            ..
        } => (
            events.as_slice(),
            *follow_symlinks,
            exclude_patterns.as_slice(),
        ),
        _ => return None,
    };

    let file_kind = match event.kind {
        EventKind::Create(_) => FileEventKind::Create,
        EventKind::Modify(_) => FileEventKind::Modify,
        EventKind::Remove(_) => FileEventKind::Delete,
        _ => return None,
    };

    let matches = desired_kinds.contains(&FileEventKind::Any) || desired_kinds.contains(&file_kind);
    if !matches {
        return None;
    }

    let path = event.paths.into_iter().next()?;

    // Exclusion check before any filesystem access, to minimize allocations.
    if is_excluded(&path, exclude_patterns) {
        return None;
    }

    // Symlink guard: symlink_metadata does NOT follow links, unlike metadata.
    if !follow_symlinks {
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() {
                return None;
            }
        }
    }

    let filename = path.file_name()?.to_string_lossy().into_owned();

    // For Create events: verify the file actually exists as a regular file.
    // This filters spurious Create events emitted by kqueue on macOS when a file is
    // deleted (the backend rescans the directory and may fire a Create for the parent
    // directory or a stale entry). Semantically, a FileWatch Create must point to a
    // real file: if the path is gone or is a directory, skip the event.
    // std::fs::metadata follows symlinks intentionally (size of the target, not the link).
    let size_bytes = if file_kind == FileEventKind::Create {
        match std::fs::metadata(&path) {
            Ok(m) if !m.is_dir() => m.len(),
            _ => return None,
        }
    } else {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    };

    Some(TriggerPayload::File {
        path,
        filename,
        size_bytes,
        event_kind: file_kind,
    })
}

/// Returns `true` if the path matches at least one exclusion pattern.
///
/// Supported patterns:
/// - `"name"` or `"name/"`: matches if a path segment equals `name`
/// - `"*.ext"`: matches if the file name ends with `.ext`
fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches_exclude_pattern(path, p))
}

/// Tests whether the path matches the given exclusion pattern.
fn matches_exclude_pattern(path: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    if let Some(ext_suffix) = pattern.strip_prefix("*.") {
        // Extension pattern: *.log, *.tmp
        return path
            .file_name()
            .map(|n| {
                let name = n.to_string_lossy();
                name.ends_with(&format!(".{ext_suffix}"))
            })
            .unwrap_or(false);
    }
    // Segment match: .git, node_modules, __pycache__, .apollia
    path.components()
        .any(|c| c.as_os_str().to_string_lossy() == pattern)
}

/// Resolves the `~` tilde to the user's HOME directory.
///
/// Only a leading `~` is resolved (not `~user`). If `$HOME` is unset or the path
/// does not start with `~`, it is returned unchanged.
fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, OnBusyPolicy};
    use tempfile::TempDir;

    fn make_file_watch_def(dir: &std::path::Path, events: Vec<FileEventKind>) -> TriggerDefinition {
        TriggerDefinition {
            id: "file-test".into(),
            agent: "file-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::FileWatch {
                path: dir.to_path_buf(),
                events,
                recursive: false,
                follow_symlinks: false,
                exclude_patterns: vec![],
            },
            input_template: InputTemplate("{{filename}}".into()),
        }
    }

    // --- detection of a created file -------------------------------------

    #[tokio::test]
    async fn test_detects_file_creation() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_file_watch_def(dir.path(), vec![FileEventKind::Create]);
        let _handle = FileWatchTrigger::spawn(def, tx);
        // Wait for the watcher to be ready.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // WHEN
        std::fs::write(dir.path().join("facture.pdf"), b"content").unwrap();

        // THEN
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for TriggerEvent")
            .expect("channel closed unexpectedly");

        assert!(
            matches!(
                &event.payload,
                TriggerPayload::File { filename, event_kind: FileEventKind::Create, .. }
                if filename == "facture.pdf"
            ),
            "unexpected payload: {:?}",
            event.payload
        );
    }

    // --- Delete event ignored when events = ["create"] -------------------

    #[tokio::test]
    async fn test_delete_ignored_when_filter_is_create() {
        // GIVEN watcher started on an empty directory
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_file_watch_def(dir.path(), vec![FileEventKind::Create]);
        let _handle = FileWatchTrigger::spawn(def, tx);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Create a file to ensure the watcher is active. Then drain every event
        // generated by the write (the kqueue backend may emit several events:
        // Create + Modify + watch registration). We wait up to 300ms of silence
        // to guarantee the channel is empty.
        let file = dir.path().join("existing.txt");
        std::fs::write(&file, b"data").unwrap();
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(300), rx.recv()).await {
                Ok(Some(_)) => {} // drain the write events
                _ => break,       // 300ms without an event means the channel is stable
            }
        }

        // WHEN deleting the file (must be ignored since filter = ["create"])
        std::fs::remove_file(&file).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // THEN no event must pass the create-only filter after a Delete
        assert!(
            rx.try_recv().is_err(),
            "should not receive any event after delete when filter is create-only"
        );
    }

    // --- map_notify_event: Create matches with events = ["create"] -------

    #[test]
    fn test_map_notify_event_create_matches() {
        // GIVEN a real temporary file (the metadata check requires the path to exist)
        use notify::{
            event::{CreateKind, EventAttributes},
            Event, EventKind,
        };
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let tmp_path = tmp.path().to_path_buf();
        let expected_name = tmp_path.file_name().unwrap().to_string_lossy().into_owned();

        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![tmp_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Create],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_some(), "expected Some payload");
        assert!(
            matches!(&payload.unwrap(), TriggerPayload::File { filename, .. }
                if filename == &expected_name),
            "unexpected filename"
        );
    }

    // --- map_notify_event: Delete filtered when events = ["create"] ------

    #[test]
    fn test_map_notify_event_delete_filtered_out() {
        // GIVEN
        use notify::{
            event::{EventAttributes, RemoveKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec!["/tmp/test.txt".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Create],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_none(), "delete should be filtered out");
    }

    // --- map_notify_event: Any matches all kinds -------------------------

    #[test]
    fn test_map_notify_event_any_matches_all() {
        // GIVEN
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/tmp/test.txt".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_some(), "Any filter should match Modify events");
    }

    // --- expand_tilde ----------------------------------------------------

    #[test]
    fn test_expand_tilde_resolves_home() {
        // GIVEN a path starting with ~
        let path: PathBuf = "~/Documents/factures".into();

        // WHEN
        let expanded = expand_tilde(&path);

        // THEN no longer starts with ~
        assert!(
            !expanded.starts_with("~"),
            "tilde should have been expanded: {:?}",
            expanded
        );
    }

    #[test]
    fn test_expand_tilde_passthrough_no_tilde() {
        // GIVEN an absolute path without a tilde
        let path: PathBuf = "/tmp/factures".into();

        // WHEN
        let expanded = expand_tilde(&path);

        // THEN path unchanged
        assert_eq!(expanded, path);
    }

    // --- exclusion patterns ----------------------------------------------

    #[test]
    fn test_default_exclude_patterns_applied() {
        // GIVEN / WHEN
        let patterns = crate::config::default_exclude_patterns();

        // THEN the 4 expected patterns are present
        assert!(patterns.contains(&".git".to_string()));
        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"__pycache__".to_string()));
        assert!(patterns.contains(&".apollia".to_string()));
        assert_eq!(patterns.len(), 4);
    }

    #[test]
    fn test_git_dir_excluded_by_default() {
        // GIVEN a path containing a .git/ segment
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/project/.git/HEAD".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/project".into(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: crate::config::default_exclude_patterns(),
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN .git/ is filtered by the default patterns
        assert!(
            payload.is_none(),
            "events from .git/ must be excluded by default"
        );
    }

    #[test]
    fn test_custom_exclude_pattern_respected() {
        // GIVEN patterns *.log and tmp/
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/app".into(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec!["*.log".into(), "tmp".into()],
        };

        // WHEN .log file
        let log_event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/app/app.log".into()],
            attrs: EventAttributes::default(),
        };
        let payload_log = map_notify_event(log_event, &source);

        // WHEN file inside tmp/
        let tmp_event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/app/tmp/data.txt".into()],
            attrs: EventAttributes::default(),
        };
        let payload_tmp = map_notify_event(tmp_event, &source);

        // THEN
        assert!(payload_log.is_none(), "*.log pattern must exclude app.log");
        assert!(
            payload_tmp.is_none(),
            "tmp pattern must exclude tmp/data.txt"
        );
    }

    #[test]
    fn test_empty_exclude_patterns_allows_all() {
        // GIVEN empty exclude_patterns, .git/ path
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/project/.git/HEAD".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/project".into(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN nothing is filtered when the list is empty
        assert!(
            payload.is_some(),
            "empty exclude_patterns must not filter any path"
        );
    }

    // --- symlinks --------------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn test_symlink_metadata_used_not_metadata() {
        // GIVEN a real symbolic link
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let dir = TempDir::new().unwrap();
        let real_file = dir.path().join("real.txt");
        std::fs::write(&real_file, b"content").unwrap();
        let symlink_path = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN symlink_metadata detects the link, so it is filtered when follow_symlinks = false
        assert!(
            payload.is_none(),
            "symlink must be filtered when follow_symlinks = false"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_outside_watch_dir_no_event() {
        // GIVEN a symlink in the watched directory pointing to an external file
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let watch_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let target_file = target_dir.path().join("external.txt");
        std::fs::write(&target_file, b"external content").unwrap();
        let symlink_in_watch = watch_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&target_file, &symlink_in_watch).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_in_watch],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: watch_dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN a symlink pointing outside the perimeter is filtered when follow_symlinks = false
        assert!(
            payload.is_none(),
            "symlink pointing outside watch dir must be excluded when follow_symlinks = false"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_follow_symlinks_true_generates_events() {
        // GIVEN a symbolic link with follow_symlinks = true
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let dir = TempDir::new().unwrap();
        let real_file = dir.path().join("real.txt");
        std::fs::write(&real_file, b"content").unwrap();
        let symlink_path = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            recursive: false,
            follow_symlinks: true,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN the event is propagated when follow_symlinks = true
        assert!(
            payload.is_some(),
            "symlink event must pass through when follow_symlinks = true"
        );
    }

    // --- deduplication logged at debug -----------------------------------

    #[tokio::test]
    async fn test_deduplicated_events_logged_debug() {
        // GIVEN an active watcher on a directory
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = TriggerDefinition {
            id: "dedup-test".into(),
            agent: "dedup-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::FileWatch {
                path: dir.path().to_path_buf(),
                events: vec![FileEventKind::Any],
                recursive: false,
                follow_symlinks: false,
                exclude_patterns: vec![],
            },
            input_template: InputTemplate("{{filename}}".into()),
        };
        let _handle = FileWatchTrigger::spawn(def, tx);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // WHEN creating then quickly rewriting the same file (< 1s apart)
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, b"first write").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        std::fs::write(&file, b"second write").unwrap();

        // Wait for the deduplication window (1s) plus margin.
        tokio::time::sleep(std::time::Duration::from_millis(1300)).await;

        // THEN at most 1 event propagated within the deduplication window
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(
            count <= 1,
            "deduplicated writes must result in at most 1 event within the debounce window, got {count}"
        );
    }
}
