//! `apollia-os start` — start the runtime in foreground (ADR-018).
//!
//! Uses inline sequential bootstrap (EventBus -> AgentRegistry -> TaskRouter ->
//! APIServer) until the Supervisor (STORY-039) is implemented.

use std::path::PathBuf;
use std::pin::Pin;
use std::time::Instant;

use apollia_core::{AIPResult, AIPTask, RuntimeEvent, TaskStatus};
use apollia_runtime::api::{APIServer, APIServerConfig, APIServerHandle, AppState};
use apollia_runtime::coordinator::ExecutionBackend;
use apollia_runtime::eventbus::EventBus;
use apollia_runtime::registry::AgentRegistry;
use apollia_runtime::router::TaskRouterHandle;

use crate::client::{DEFAULT_SOCKET_PATH, DEFAULT_TCP_PORT};

/// Errors that can occur during runtime startup.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// APIServer failed to start.
    #[error("failed to start APIServer: {0}")]
    ApiServer(#[from] apollia_runtime::api::APIServerError),
}

/// Placeholder execution backend for MVP (no real agent execution yet).
///
/// Will be replaced by the real ORIA backend when the full pipeline is wired.
#[derive(Clone)]
struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Completed,
                output: Vec::new(),
                error: Some(apollia_core::AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
            })
        })
    }
}

/// Bootstrap and run the runtime in foreground (ADR-018).
///
/// Starts actors in sequence: EventBus -> AgentRegistry -> TaskRouter -> APIServer.
/// Blocks until Ctrl+C or `POST /api/v1/shutdown` is received.
pub async fn run(socket: Option<PathBuf>, port: Option<u16>) -> Result<(), StartError> {
    let start = Instant::now();
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let tcp_port = port.unwrap_or(DEFAULT_TCP_PORT);

    // 1. EventBus
    let (event_tx, _event_rx) = EventBus::new();
    println!("  * EventBus        ready");

    // 2. AgentRegistry
    let registry_handle = AgentRegistry::spawn(event_tx.clone());
    println!("  * AgentRegistry   ready");

    // 3. TaskRouter
    let router_handle: TaskRouterHandle<NoopBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 256);
    println!("  * TaskRouter      ready");

    // 4. APIServer
    let state = AppState {
        router_handle,
        registry_handle,
        event_sender: event_tx.clone(),
    };
    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port,
    };
    let server = APIServer::new(config, state);
    let handle: APIServerHandle = server.start().await?;
    println!(
        "  * APIServer       listening on {} + localhost:{}",
        socket_path.display(),
        tcp_port
    );

    let elapsed = start.elapsed();
    println!("  -------------------------------------------------");
    println!("  * Runtime ready in {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("  Press Ctrl+C or run `apollia-os stop` to shut down.");

    // Wait for Ctrl+C or ShutdownRequested event
    let mut shutdown_rx = event_tx.subscribe();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!();
            println!("  Ctrl+C received, shutting down...");
        }
        _ = wait_for_shutdown(&mut shutdown_rx) => {
            println!("  Shutdown requested via API, shutting down...");
        }
    }

    handle.shutdown();

    // Clean up socket file
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    println!("  * Runtime stopped.");
    Ok(())
}

/// Wait until a `RuntimeEvent::ShutdownRequested` event is received.
async fn wait_for_shutdown(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) {
    loop {
        match rx.recv().await {
            Ok(RuntimeEvent::ShutdownRequested) => return,
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(lagged = n, "EventBus receiver lagged");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_backend_is_clone() {
        let backend = NoopBackend;
        let _cloned = backend.clone();
    }
}
