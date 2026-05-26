//! Gestion cross-platform des signaux pour shutdown gracieux.
//!
//! Le daemon envoie typiquement `SIGTERM` (Unix) ou `CTRL_C_EVENT` (Windows)
//! quand il veut arrêter le runner. Le runner intercepte et déclenche le
//! shutdown propre de l'axum server.

/// Future qui complète quand un signal de shutdown est reçu.
///
/// Sur Unix : `SIGINT` (Ctrl+C) ou `SIGTERM`.
/// Sur Windows : `Ctrl+C` ou `Ctrl+Break`.
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
        let mut ctrl_c =
            tokio::signal::windows::ctrl_c().expect("install Ctrl-C handler");
        let mut ctrl_break =
            tokio::signal::windows::ctrl_break().expect("install Ctrl-Break handler");

        tokio::select! {
            _ = ctrl_c.recv() => tracing::info!("Ctrl-C received, shutting down"),
            _ = ctrl_break.recv() => tracing::info!("Ctrl-Break received, shutting down"),
        }
    }
}
