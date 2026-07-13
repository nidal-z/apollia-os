mod ids;
mod preview;
mod rationale;
mod runtime_event;

#[cfg(test)]
mod tests;
// ── Pipeline event tests ─────────────────────────────────────────
#[cfg(test)]
mod pipeline_event_tests;
#[cfg(test)]
mod tool_call_rationale_tests;

pub use ids::{AgentId, RunId, TaskId};
pub use preview::FilesystemPreview;
pub use rationale::ToolCallRationale;
pub use runtime_event::RuntimeEvent;

/// Write handle on the EventBus: clonable, shareable between actors.
///
/// Public alias defined in `apollia-core` so that `apollia-llm` (and any other
/// crate without a dependency on `apollia-runtime`) can emit events on the bus
/// without creating a circular dependency.
///
/// Publishing is non-blocking; if the buffer is full, the send returns an error
/// that is silently ignored (fire-and-forget).
pub type EventBusSender = tokio::sync::broadcast::Sender<RuntimeEvent>;
