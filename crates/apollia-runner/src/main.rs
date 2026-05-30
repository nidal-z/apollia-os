//! Apollia OS sidecar runner binary.
//!
//! Spawned by the `apollia-os` daemon at boot. Binds on `127.0.0.1:0`,
//! announces the chosen port over stdout, serves the IPC HTTP API.

use std::error::Error;

use apollia_runner::{
    lifecycle::{ready, signals},
    observability::logs,
    server::{build_router, AppState},
};
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Init logging structuré sur stderr (stdout réservé au handshake READY).
    logs::init();

    // 2. Bind TCP loopback sur un port libre choisi par l'OS.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    tracing::info!(port, "apollia-runner bound to loopback");

    // 3. Préparer le canal de shutdown.
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state = AppState::new(shutdown_tx);

    // 4. Build router axum.
    let app = build_router(state);

    // 5. Annoncer au parent (daemon) qu'on est prêt.
    //    Doit être fait AVANT axum::serve pour ne pas rater le timeout
    //    du daemon (`RUNNER_HANDSHAKE_TIMEOUT` = 10s).
    ready::announce(port)?;

    // 6. Combine signal externe (SIGTERM/Ctrl-C) et signal interne (POST /shutdown).
    let combined_shutdown = async move {
        tokio::select! {
            _ = signals::shutdown_signal() => {
                tracing::info!("external shutdown signal received");
            }
            _ = shutdown_rx => {
                tracing::info!("internal shutdown signal received (/shutdown endpoint)");
            }
        }
    };

    // 7. Sert jusqu'au shutdown.
    axum::serve(listener, app)
        .with_graceful_shutdown(combined_shutdown)
        .await?;

    tracing::info!("apollia-runner exited cleanly");
    Ok(())
}
