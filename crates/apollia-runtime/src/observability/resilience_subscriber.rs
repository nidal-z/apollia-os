//! Hydrates the shared [`ResilienceLayer`] from the runtime event stream.
//!
//! The CLI / HTTP `/api/v1/resilience/*` surfaces operate on a single shared
//! [`Arc<Mutex<ResilienceLayer>>`] stored in [`crate::api::server::AppState`].
//! Per-task ORIA engines do not (yet) update that layer directly, they keep
//! their own short-lived breakers for local logic. To keep the operator
//! snapshot meaningful we subscribe to the `EventBus` and mirror every
//! `ToolCallCompleted` / `ToolCallDenied` into the shared layer:
//!
//! - `success = true`  → `record_success(tool)`
//! - `success = false` → `record_failure(tool, ErrorClass::Transient)`
//! - `ToolCallDenied { reason = "circuit_open" }` → counted as a transient
//!   failure too, so the operator sees the breaker keep tripping.
//!
//! Tools the layer hasn't seen before are auto-registered with the default
//! threshold (3) / cooldown (30 s) via [`ResilienceLayer::ensure_tool`], so
//! the snapshot reflects production traffic without operator intervention.

use std::sync::{Arc, Mutex};

use apollia_core::events::RuntimeEvent;
use apollia_oria::{ErrorClass, ResilienceLayer};

use crate::eventbus::EventBusSender;

/// Spawn the subscriber task. Returns the `JoinHandle` for shutdown.
pub fn spawn_resilience_subscriber(
    layer: Arc<Mutex<ResilienceLayer>>,
    event_bus: &EventBusSender,
) -> tokio::task::JoinHandle<()> {
    let mut rx = event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => apply(&layer, event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        skipped = n,
                        "resilience subscriber lagged - circuit snapshot may briefly diverge"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("resilience subscriber: event bus closed, exiting");
                    return;
                }
            }
        }
    })
}

fn apply(layer: &Mutex<ResilienceLayer>, event: RuntimeEvent) {
    let (tool_name, outcome) = match event {
        RuntimeEvent::ToolCallCompleted {
            tool_name, success, ..
        } => (tool_name, if success { Outcome::Ok } else { Outcome::Fail }),
        RuntimeEvent::ToolCallDenied {
            tool_name, reason, ..
        } if reason == "circuit_open" => (tool_name, Outcome::Fail),
        _ => return,
    };

    let guard = match layer.lock() {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(
                error = %e,
                "resilience subscriber: mutex poisoned, snapshot updates stop"
            );
            return;
        }
    };
    guard.ensure_tool(&tool_name);
    match outcome {
        Outcome::Ok => {
            if let Err(e) = guard.record_success(&tool_name) {
                tracing::debug!(tool = %tool_name, error = %e, "record_success failed");
            }
        }
        Outcome::Fail => {
            if let Err(e) = guard.record_failure(&tool_name, &ErrorClass::Transient) {
                tracing::debug!(tool = %tool_name, error = %e, "record_failure failed");
            }
        }
    }
}

enum Outcome {
    Ok,
    Fail,
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentId, TaskId};

    fn mk_completed(tool: &str, success: bool) -> RuntimeEvent {
        RuntimeEvent::ToolCallCompleted {
            parent_event_id: "evt-1".to_string(),
            task_id: TaskId::from("t-1".to_string()),
            agent_id: AgentId::from("a-1"),
            tool_name: tool.to_string(),
            output_json: None,
            exit_code: None,
            duration_ms: 1,
            success,
        }
    }

    #[test]
    fn records_success_resets_failure_count() {
        let layer = Arc::new(Mutex::new(ResilienceLayer::default()));
        apply(&layer, mk_completed("file_read", false));
        apply(&layer, mk_completed("file_read", true));
        let snap = layer.lock().unwrap().snapshot();
        let entry = snap.iter().find(|e| e.tool_name == "file_read").unwrap();
        assert_eq!(entry.failure_count, 0);
        assert_eq!(entry.state, "closed");
    }

    #[test]
    fn enough_failures_open_the_breaker() {
        let layer = Arc::new(Mutex::new(ResilienceLayer::new(
            2,
            std::time::Duration::from_secs(60),
        )));
        apply(&layer, mk_completed("flaky", false));
        apply(&layer, mk_completed("flaky", false));
        let snap = layer.lock().unwrap().snapshot();
        let entry = snap.iter().find(|e| e.tool_name == "flaky").unwrap();
        assert_eq!(entry.state, "open");
        assert_eq!(entry.failure_count, 2);
    }

    #[test]
    fn unknown_event_kind_is_ignored() {
        let layer = Arc::new(Mutex::new(ResilienceLayer::default()));
        apply(&layer, RuntimeEvent::AllReady);
        assert!(layer.lock().unwrap().snapshot().is_empty());
    }
}
