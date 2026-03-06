//! SSE streaming route for task events — `GET /api/v1/tasks/{id}/stream`.
//!
//! Subscribes to the runtime [`EventBus`] and filters [`RuntimeEvent`]s by
//! `task_id`, pushing each matching event as an SSE frame. The stream closes
//! automatically when a terminal event (completed/failed/canceled) is received.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use apollia_core::RuntimeEvent;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// SSE event sent to the client.
///
/// Serialized as a flat JSON object with an `event` field discriminating
/// the event type and additional data fields depending on the variant.
#[derive(Debug, Serialize)]
pub struct SseTaskEvent {
    /// Event type: "step", "tool_call", "completed", "failed", "canceled", "started".
    pub event: String,
    /// Additional event data (flattened into the top-level JSON object).
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// Standard error response body (same as routes_tasks).
#[derive(Debug, Serialize)]
struct ErrorResponse {
    /// Human-readable error description.
    error: String,
}

/// Convert a [`RuntimeEvent`] into an [`SseTaskEvent`] if it matches the given `task_id`.
///
/// Returns `None` for events that do not belong to the task or are not relevant
/// for per-task streaming (e.g. system-level events).
/// Returns `Some((event, is_terminal))` — `is_terminal` is true for events that
/// signal the end of the task execution.
fn runtime_event_to_sse(event: &RuntimeEvent, task_id: &str) -> Option<(SseTaskEvent, bool)> {
    match event {
        RuntimeEvent::StepExecuted {
            task_id: tid,
            step,
            tool,
        } if tid == task_id => {
            let data = match tool {
                Some(t) => serde_json::json!({ "step": step, "tool": t }),
                None => serde_json::json!({ "step": step }),
            };
            Some((
                SseTaskEvent {
                    event: "step".into(),
                    data,
                },
                false,
            ))
        }
        RuntimeEvent::TaskStarted {
            task_id: tid,
            agent_id,
        } if tid == task_id => Some((
            SseTaskEvent {
                event: "started".into(),
                data: serde_json::json!({ "agent_id": agent_id }),
            },
            false,
        )),
        RuntimeEvent::TaskCompleted {
            task_id: tid,
            success,
            ..
        } if tid == task_id => {
            let event_name = if *success { "completed" } else { "failed" };
            Some((
                SseTaskEvent {
                    event: event_name.into(),
                    data: serde_json::json!({ "success": success }),
                },
                true,
            ))
        }
        RuntimeEvent::TaskCanceled { task_id: tid } if tid == task_id => Some((
            SseTaskEvent {
                event: "canceled".into(),
                data: serde_json::json!({}),
            },
            true,
        )),
        _ => None,
    }
}

/// Handler for `GET /api/v1/tasks/{id}/stream`.
///
/// Subscribes to the runtime EventBus and streams task-specific events via SSE.
/// Returns 404 if the task does not exist in the TaskRouter.
/// The stream closes after a terminal event (completed/failed/canceled).
/// Client disconnection drops the subscriber automatically (no leak).
pub async fn stream_task<B: ExecutionBackend>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, impl IntoResponse> {
    // Verify the task exists before subscribing
    let status = state
        .router_handle
        .get_status(&task_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "router actor is dead".into(),
                }),
            )
        })?;

    if status.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("task not found: {task_id}"),
            }),
        ));
    }

    // Subscribe to the EventBus broadcast channel
    let rx = state.event_sender.subscribe();
    let stream = BroadcastStream::new(rx);

    // Filter events for this task_id, convert to SSE frames, stop on terminal
    let task_id_owned = task_id.clone();
    let sse_stream = stream
        .filter_map(move |result| {
            match result {
                Ok(event) => runtime_event_to_sse(&event, &task_id_owned),
                // Lagged receiver: skip lost events silently
                Err(_) => None,
            }
        })
        .map(|(sse_event, is_terminal)| (sse_event, is_terminal))
        // Take events until (and including) the first terminal event
        .take_while_inclusive(|(_event, is_terminal)| !is_terminal)
        .map(|(sse_event, _)| {
            let json = serde_json::to_string(&sse_event).unwrap_or_else(|_| "{}".into());
            Ok(Event::default().data(json))
        });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

/// Extension trait to add `take_while_inclusive` to streams.
///
/// Unlike `take_while`, this yields the first item that fails the predicate
/// before closing the stream — needed to send the terminal SSE event.
struct TakeWhileInclusive<S, F> {
    stream: S,
    predicate: F,
    done: bool,
}

impl<S, F, T> tokio_stream::Stream for TakeWhileInclusive<S, F>
where
    S: tokio_stream::Stream<Item = T> + Unpin,
    F: FnMut(&T) -> bool + Unpin,
{
    type Item = T;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return std::task::Poll::Ready(None);
        }
        match std::pin::Pin::new(&mut this.stream).poll_next(cx) {
            std::task::Poll::Ready(Some(item)) => {
                if (this.predicate)(&item) {
                    std::task::Poll::Ready(Some(item))
                } else {
                    this.done = true;
                    std::task::Poll::Ready(Some(item))
                }
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Extension trait providing `take_while_inclusive` on any stream.
trait TakeWhileInclusiveExt: tokio_stream::Stream + Sized {
    /// Yields items while `predicate` returns true, then yields the first
    /// item for which `predicate` returns false and closes.
    fn take_while_inclusive<F>(self, predicate: F) -> TakeWhileInclusive<Self, F>
    where
        F: FnMut(&Self::Item) -> bool;
}

impl<S: tokio_stream::Stream> TakeWhileInclusiveExt for S {
    fn take_while_inclusive<F>(self, predicate: F) -> TakeWhileInclusive<Self, F>
    where
        F: FnMut(&Self::Item) -> bool,
    {
        TakeWhileInclusive {
            stream: self,
            predicate,
            done: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    /// Minimal backend for testing (never actually executes in these tests).
    #[derive(Clone)]
    struct MockBackend;

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async move {
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                })
            })
        }
    }

    /// Build a minimal test router (no agents) for 404 tests.
    fn test_router_no_agents() -> Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
        };
        Router::new()
            .route(
                "/api/v1/tasks/:id/stream",
                get(stream_task::<MockBackend>),
            )
            .with_state(state)
    }

    #[tokio::test]
    async fn test_sse_unknown_task_returns_404() {
        // GIVEN aucune tache "t-999"
        let router = test_router_no_agents();

        // WHEN GET /api/v1/tasks/t-999/stream
        let req = Request::builder()
            .uri("/api/v1/tasks/t-999/stream")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404 avec erreur
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("read body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse json");
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("task not found"));
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_step_event() {
        // GIVEN un StepExecuted event pour task "t-001"
        let event = RuntimeEvent::StepExecuted {
            task_id: "t-001".into(),
            step: 3,
            tool: Some("file_io".into()),
        };

        // WHEN on le convertit
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN on obtient un SseTaskEvent "step" non-terminal
        let (sse, is_terminal) = result.expect("should match");
        assert_eq!(sse.event, "step");
        assert_eq!(sse.data["step"], 3);
        assert_eq!(sse.data["tool"], "file_io");
        assert!(!is_terminal);
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_filters_by_task_id() {
        // GIVEN un StepExecuted event pour task "t-002"
        let event = RuntimeEvent::StepExecuted {
            task_id: "t-002".into(),
            step: 1,
            tool: None,
        };

        // WHEN on le convertit avec task_id "t-001"
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN None (pas le bon task_id)
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_completed_is_terminal() {
        // GIVEN un TaskCompleted event pour task "t-001"
        let event = RuntimeEvent::TaskCompleted {
            agent_id: "agent-1".into(),
            task_id: "t-001".into(),
            success: true,
        };

        // WHEN on le convertit
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN on obtient un SseTaskEvent "completed" terminal
        let (sse, is_terminal) = result.expect("should match");
        assert_eq!(sse.event, "completed");
        assert!(is_terminal);
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_canceled_is_terminal() {
        // GIVEN un TaskCanceled event pour task "t-001"
        let event = RuntimeEvent::TaskCanceled {
            task_id: "t-001".into(),
        };

        // WHEN on le convertit
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN on obtient un SseTaskEvent "canceled" terminal
        let (sse, is_terminal) = result.expect("should match");
        assert_eq!(sse.event, "canceled");
        assert!(is_terminal);
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_failed_is_terminal() {
        // GIVEN un TaskCompleted event avec success=false
        let event = RuntimeEvent::TaskCompleted {
            agent_id: "agent-1".into(),
            task_id: "t-001".into(),
            success: false,
        };

        // WHEN on le convertit
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN on obtient un SseTaskEvent "failed" terminal
        let (sse, is_terminal) = result.expect("should match");
        assert_eq!(sse.event, "failed");
        assert!(is_terminal);
    }

    #[tokio::test]
    async fn test_runtime_event_to_sse_ignores_system_events() {
        // GIVEN un event systeme (AllReady)
        let event = RuntimeEvent::AllReady;

        // WHEN on le convertit
        let result = runtime_event_to_sse(&event, "t-001");

        // THEN None (pas un event de tache)
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_take_while_inclusive_includes_terminal() {
        // GIVEN un stream de nombres
        let items = vec![1, 2, 3, 4, 5];
        let stream = tokio_stream::iter(items);

        // WHEN on applique take_while_inclusive avec predicate > 3
        let collected: Vec<i32> = stream
            .take_while_inclusive(|x| *x < 4)
            .collect()
            .await;

        // THEN on obtient [1, 2, 3, 4] (4 inclus, le premier qui echoue le predicate)
        assert_eq!(collected, vec![1, 2, 3, 4]);
    }
}
