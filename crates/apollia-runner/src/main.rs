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
    // 1. Init structured logging on stderr (stdout is reserved for the READY handshake).
    logs::init();

    // 2. Bind TCP loopback on a free port chosen by the OS.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    tracing::info!(port, "runner.bound");

    // 3. Set up the shutdown channel.
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state = AppState::new(shutdown_tx);

    // 4. Build the axum router.
    let app = build_router(state);

    // 5. Tell the parent (daemon) we are ready.
    //    Must happen BEFORE axum::serve so we do not miss the daemon's
    //    handshake timeout (`RUNNER_HANDSHAKE_TIMEOUT` = 10s).
    ready::announce(port)?;

    // 6. Combine the external signal (SIGTERM/Ctrl-C) with the internal signal (POST /shutdown).
    let combined_shutdown = async move {
        tokio::select! {
            _ = signals::shutdown_signal() => {
                tracing::info!(reason = "external signal", "runner.shutdown.requested");
            }
            _ = shutdown_rx => {
                tracing::info!(reason = "the /shutdown endpoint", "runner.shutdown.requested");
            }
        }
    };

    // 7. Serve until shutdown.
    axum::serve(listener, app)
        .with_graceful_shutdown(combined_shutdown)
        .await?;

    tracing::info!("runner.exited");
    Ok(())
}
