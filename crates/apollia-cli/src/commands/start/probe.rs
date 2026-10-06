//! The configuration file the runtime reads, and the port and socket probes.

use std::path::PathBuf;

/// The configuration file the runtime loads when no `--config` is given:
/// `~/.apollia/apollia.toml` when it exists, `None` otherwise.
///
/// The working directory and `~/.config/apollia/` are not searched: the first
/// is where an agent package keeps a manifest of the same name, and both made
/// the CLI, the daemon and the desktop disagree on which file was in force.
pub(crate) fn find_config_file() -> Option<PathBuf> {
    apollia_core::paths::config_file().filter(|p| p.is_file())
}

/// Returns `true` if a TCP listener is already bound on `port`.
pub(super) async fn port_is_in_use(port: u16) -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
}

/// Returns `true` only when a live runtime is currently listening on `path`.
///
/// A previous crash can leave a stale socket file behind. To distinguish it
/// from a real running daemon we attempt a short-timeout connect: success
/// means a process is bound, `ConnectionRefused` means the file is stale and
/// safe to remove on the caller's side.
#[cfg(unix)]
pub(super) async fn socket_is_in_use(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    match tokio::time::timeout(
        std::time::Duration::from_millis(200),
        tokio::net::UnixStream::connect(path),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(_)) | Err(_) => false,
    }
}

/// On Windows there is no Unix socket, so we ask the named pipe instead.
///
/// Opening it answers the question directly: a pipe exists only while a
/// runtime holds it. `ERROR_PIPE_BUSY` counts as in use too, since a busy pipe
/// is a pipe someone is serving. This used to probe the TCP port, which stops
/// being the right question once the port is no longer how the CLI reaches the
/// runtime, and would have reported "free" on a running daemon that opened no
/// port.
#[cfg(windows)]
pub(super) async fn socket_is_in_use(_path: &std::path::Path) -> bool {
    /// `ERROR_PIPE_BUSY`: every instance is serving a client, so one exists.
    const ERROR_PIPE_BUSY: i32 = 231;
    let name = apollia_core::paths::named_pipe_name();
    match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
        Ok(_) => true,
        Err(e) => e.raw_os_error() == Some(ERROR_PIPE_BUSY),
    }
}

/// Remove a stale Unix socket file left over from a previous crashed runtime.
///
/// Called only after [`socket_is_in_use`] returned `false` for an existing
/// path, so we know no live daemon is listening. Any error is logged but not
/// propagated: the subsequent bind will surface the real cause.
pub(super) fn cleanup_stale_socket(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::info!(
                socket = %path.display(),
                "api.socket.stale.removed"
            );
        }
        Err(e) => {
            tracing::warn!(
                socket = %path.display(),
                error = %e,
                "api.socket.stale.remove.failed"
            );
        }
    }
}
