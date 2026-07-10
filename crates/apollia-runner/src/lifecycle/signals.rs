//! Cross-platform signal handling for graceful shutdown.
//!
//! The daemon typically sends `SIGTERM` (Unix) or `CTRL_C_EVENT` (Windows)
//! when it wants to stop the runner. The runner intercepts it and triggers a
//! clean shutdown of the axum server.

/// Future that completes when a shutdown signal is received.
///
/// On Unix: `SIGINT` (Ctrl+C) or `SIGTERM`.
/// On Windows: `Ctrl+C` or `Ctrl+Break`.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received, shutting down"),
            _ = sigint.recv() => tracing::info!("SIGINT received, shutting down"),
        }
    }

    #[cfg(windows)]
    {
        let mut ctrl_c = tokio::signal::windows::ctrl_c().expect("install Ctrl-C handler");
        let mut ctrl_break =
            tokio::signal::windows::ctrl_break().expect("install Ctrl-Break handler");

        tokio::select! {
            _ = ctrl_c.recv() => tracing::info!("Ctrl-C received, shutting down"),
            _ = ctrl_break.recv() => tracing::info!("Ctrl-Break received, shutting down"),
        }
    }
}
