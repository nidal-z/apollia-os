//! `McpConfigWatcher`: watches `mcp.toml` for changes and hot-reloads modified servers.
//!
//! Uses the same `notify` backend as [`crate::sources::file_watch::FileWatchTrigger`]
//! (inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows).
//!
//! ## Behaviour
//!
//! 1. On startup, the current `mcp.toml` is parsed and cached as the *baseline*.
//! 2. When a `Modify` event is received for the file, the new config is parsed.
//! 3. Each server whose JSON representation differs from the baseline is scheduled
//!    for a hot reload via [`apollia_mcp::manager::McpClientManagerHandle::reload_server`].
//! 4. The baseline is updated to the new config after all reloads are dispatched.
//!
//! New servers in the updated config are not started automatically (out of scope).
//! Removed servers are not stopped automatically (out of scope).

use std::path::PathBuf;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::task::JoinHandle;

use apollia_mcp::config::McpConfig;
use apollia_mcp::manager::McpClientManagerHandle;

/// Watches a `mcp.toml` file and triggers a hot reload for each server whose
/// configuration changes.
///
/// Obtain a running instance via [`McpConfigWatcher::spawn`].
pub struct McpConfigWatcher;

impl McpConfigWatcher {
    /// Spawn a background task that monitors `mcp_toml_path` for modifications.
    ///
    /// When the file changes, each server whose configuration differs from the
    /// cached baseline is reloaded via `handle.reload_server()`. Errors from
    /// individual server reloads are logged at `warn` level and do not affect
    /// other servers.
    ///
    /// The task terminates when the returned [`JoinHandle`] is aborted or when
    /// the `notify` watcher encounters an unrecoverable error.
    pub fn spawn(mcp_toml_path: PathBuf, handle: McpClientManagerHandle) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Load the initial config as the baseline for diffing.
            let mut baseline = load_baseline(&mcp_toml_path);

            // Set up the notify watcher and the sync→async bridge. On a fatal
            // setup error the task ends without watching anything.
            let Some(mut bridge_rx) = start_notify_bridge(&mcp_toml_path) else {
                return;
            };

            tracing::info!(
                path = %mcp_toml_path.display(),
                "McpConfigWatcher: watching mcp.toml for changes"
            );

            // Deduplication: a single logical save can generate multiple consecutive
            // Modify events within 1 second on some platforms. Only the first fires.
            let mut last_reload = std::time::Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now);

            while bridge_rx.recv().await.is_some() {
                let now = std::time::Instant::now();
                if now.duration_since(last_reload) < Duration::from_secs(1) {
                    // Debounce: skip rapid successive events for the same file.
                    continue;
                }
                last_reload = now;
                handle_reload_tick(&mcp_toml_path, &handle, &mut baseline).await;
            }
        })
    }
}

/// Load `mcp.toml` as the diffing baseline, falling back to an empty config
/// (logged at `warn`) when the initial parse fails.
fn load_baseline(mcp_toml_path: &std::path::Path) -> McpConfig {
    McpConfig::load(mcp_toml_path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %mcp_toml_path.display(),
            error = %e,
            "McpConfigWatcher: failed to load initial mcp.toml - starting with empty baseline"
        );
        McpConfig {
            servers: Vec::new(),
            mdns_discovery: false,
        }
    })
}

/// Create the notify watcher, start watching `mcp_toml_path`, and spawn the
/// blocking poll loop that bridges sync `notify` events to an async channel.
///
/// Returns the async receiver of debounce-eligible ticks, or `None` if the
/// watcher could not be created or could not watch the file (both fatal,
/// logged at `error`).
fn start_notify_bridge(mcp_toml_path: &std::path::Path) -> Option<tokio::sync::mpsc::Receiver<()>> {
    // Sync channel: notify (sync) → async bridge.
    let (notify_tx, notify_rx) = std::sync::mpsc::channel();

    let mut watcher = match notify::recommended_watcher(notify_tx) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(
                path = %mcp_toml_path.display(),
                error = %e,
                "McpConfigWatcher: failed to create file watcher"
            );
            return None;
        }
    };

    if let Err(e) = watcher.watch(mcp_toml_path, RecursiveMode::NonRecursive) {
        tracing::error!(
            path = %mcp_toml_path.display(),
            error = %e,
            "McpConfigWatcher: failed to watch mcp.toml"
        );
        return None;
    }

    // Blocking bridge: poll the sync channel from a spawn_blocking context
    // so we never block Tokio worker threads.
    let path_clone = mcp_toml_path.to_path_buf();
    let (bridge_tx, bridge_rx) = tokio::sync::mpsc::channel::<()>(4);

    tokio::task::spawn_blocking(move || {
        let _watcher = watcher;
        run_bridge_poll_loop(&notify_rx, &bridge_tx, &path_clone);
    });

    Some(bridge_rx)
}

/// Blocking poll loop bridging the sync `notify` channel to the async
/// `bridge_tx`. Forwards a `()` tick for each `Modify` event and exits when the
/// async side is dropped or the sync channel disconnects.
fn run_bridge_poll_loop(
    notify_rx: &std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    bridge_tx: &tokio::sync::mpsc::Sender<()>,
    path: &std::path::Path,
) {
    loop {
        match notify_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(event)) => {
                if matches!(event.kind, EventKind::Modify(_))
                    && bridge_tx.blocking_send(()).is_err()
                {
                    break; // async side dropped
                }
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "McpConfigWatcher: notify error"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if bridge_tx.is_closed() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Handles a reload tick: reloads `mcp.toml`, detects the changed servers, and
/// triggers their hot reload. Updates `baseline` to the new configuration once
/// the reloads are dispatched.
///
/// Parse errors are logged and leave `baseline` unchanged.
async fn handle_reload_tick(
    mcp_toml_path: &std::path::Path,
    handle: &McpClientManagerHandle,
    baseline: &mut McpConfig,
) {
    let new_config = match McpConfig::load(mcp_toml_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                path = %mcp_toml_path.display(),
                error = %e,
                "McpConfigWatcher: failed to parse updated mcp.toml - skipping reload"
            );
            return;
        }
    };

    // Identify servers whose configuration changed.
    let changed_names = detect_changed_servers(baseline, &new_config);

    if changed_names.is_empty() {
        tracing::debug!(
            path = %mcp_toml_path.display(),
            "McpConfigWatcher: mcp.toml changed but no server config differences found"
        );
        *baseline = new_config;
        return;
    }

    tracing::info!(
        path = %mcp_toml_path.display(),
        servers = ?changed_names,
        "McpConfigWatcher: detected MCP server config changes - hot reloading"
    );

    for name in &changed_names {
        reload_one_server(handle, name).await;
    }

    *baseline = new_config;
}

/// Reloads a single MCP server, logging success and failure without propagating the error.
async fn reload_one_server(handle: &McpClientManagerHandle, name: &str) {
    match handle.reload_server(name).await {
        Ok(()) => {
            tracing::info!(server = %name, "McpConfigWatcher: server hot reloaded");
        }
        Err(e) => {
            tracing::warn!(
                server = %name,
                error = %e,
                "McpConfigWatcher: hot reload failed"
            );
        }
    }
}

/// Returns the names of servers whose JSON-serialised config changed between
/// `baseline` and `updated`.
///
/// Only servers that exist in both configs are compared; new and removed servers
/// are not reported (callers handle those separately).
fn detect_changed_servers(baseline: &McpConfig, updated: &McpConfig) -> Vec<String> {
    let baseline_map: std::collections::HashMap<&str, &apollia_mcp::config::McpServerConfig> =
        baseline
            .servers
            .iter()
            .map(|s| (s.name.as_str(), s))
            .collect();

    updated
        .servers
        .iter()
        .filter_map(|new_server| {
            let old = baseline_map.get(new_server.name.as_str())?;
            let old_json = serde_json::to_string(old).ok()?;
            let new_json = serde_json::to_string(new_server).ok()?;
            if old_json != new_json {
                Some(new_server.name.clone())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_mcp::config::McpServerConfig;
    use std::collections::HashMap;

    fn make_server(name: &str, command: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: command.to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        }
    }

    fn make_config(servers: Vec<McpServerConfig>) -> McpConfig {
        McpConfig {
            servers,
            mdns_discovery: false,
        }
    }

    #[test]
    fn test_detect_changed_servers_no_change() {
        // GIVEN identical baseline and updated configs
        let s = make_server("notion", "npx");
        let baseline = make_config(vec![s.clone()]);
        let updated = make_config(vec![s]);

        // WHEN
        let changed = detect_changed_servers(&baseline, &updated);

        // THEN no change detected
        assert!(changed.is_empty(), "expected no change, got: {:?}", changed);
    }

    #[test]
    fn test_detect_changed_servers_command_changed() {
        // GIVEN a server whose command changed
        let baseline = make_config(vec![make_server("notion", "npx")]);
        let updated = make_config(vec![make_server("notion", "uvx")]);

        // WHEN
        let changed = detect_changed_servers(&baseline, &updated);

        // THEN "notion" is detected as changed
        assert_eq!(changed, vec!["notion"]);
    }

    #[test]
    fn test_detect_changed_servers_new_server_not_reported() {
        // GIVEN an updated config with a brand-new server
        let baseline = make_config(vec![make_server("notion", "npx")]);
        let updated = make_config(vec![
            make_server("notion", "npx"),
            make_server("sqlite", "uvx"),
        ]);

        // WHEN
        let changed = detect_changed_servers(&baseline, &updated);

        // THEN "sqlite" is new and not reported; "notion" is unchanged
        assert!(
            changed.is_empty(),
            "new server must not be reported as changed"
        );
    }

    #[test]
    fn test_detect_changed_servers_removed_server_not_reported() {
        // GIVEN an updated config that dropped one server
        let baseline = make_config(vec![
            make_server("notion", "npx"),
            make_server("sqlite", "uvx"),
        ]);
        let updated = make_config(vec![make_server("notion", "npx")]);

        // WHEN
        let changed = detect_changed_servers(&baseline, &updated);

        // THEN removed server is not reported
        assert!(
            changed.is_empty(),
            "removed server must not be reported as changed"
        );
    }

    #[test]
    fn test_detect_changed_servers_multiple_changes() {
        // GIVEN two servers both with changed commands
        let baseline = make_config(vec![
            make_server("notion", "npx"),
            make_server("sqlite", "uvx"),
        ]);
        let updated = make_config(vec![
            make_server("notion", "bunx"),
            make_server("sqlite", "pipx"),
        ]);

        // WHEN
        let mut changed = detect_changed_servers(&baseline, &updated);
        changed.sort();

        // THEN both changed servers are reported
        assert_eq!(changed, vec!["notion", "sqlite"]);
    }
}
