//! Logger structuré JSON Lines sur stderr.
//!
//! **Critique** : stdout est réservé à la communication handshake avec le
//! parent (daemon). Tout log doit aller sur stderr pour ne pas polluer.
//! Cf. [IPC-PROTOCOL §1.2](../../../../docs/internal/architecture/IPC-PROTOCOL.md).

use std::io;

/// Initialise le logger en mode JSON Lines vers stderr.
///
/// Le niveau de log par défaut est `info`. Override possible via la variable
/// d'environnement `RUST_LOG` (format `tracing_subscriber::EnvFilter` standard).
pub fn init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "apollia-runner logger initialized"
    );
}
