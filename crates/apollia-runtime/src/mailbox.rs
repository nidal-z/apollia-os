//! Agent-to-agent messaging via an in-memory mailbox actor.
//!
//! Each agent has a bounded queue of messages (`VecDeque<AgentMessage>`, max 100).
//! The actor follows the same Tokio `mpsc` + clonable handle pattern used by
//! `AgentRegistry`, `TaskRouter`, and `ExecutionCoordinator`.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::eventbus::EventBusSender;
use apollia_core::RuntimeEvent;

/// Maximum number of pending messages per agent.
const MAX_MAILBOX_SIZE: usize = 100;

/// Message stored in an agent's mailbox queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Name of the sending agent.
    pub from: String,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Timestamp of the send (RFC 3339).
    pub sent_at: String,
}

/// Errors returned by mailbox operations.
#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    /// The agent's message queue has reached its capacity.
    #[error("mailbox full for agent {0} (max {MAX_MAILBOX_SIZE})")]
    QueueFull(String),

    /// The internal actor channel is closed.
    #[error("mailbox channel closed")]
    ChannelClosed,
}

/// Internal messages handled by the mailbox actor loop.
enum MailboxMessage {
    Send {
        from: String,
        to: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<(), MailboxError>>,
    },
    Receive {
        agent_name: String,
        reply: oneshot::Sender<Option<AgentMessage>>,
    },
    PendingCount {
        agent_name: String,
        reply: oneshot::Sender<usize>,
    },
    ListMessages {
        agent_name: String,
        limit: usize,
        reply: oneshot::Sender<Vec<AgentMessage>>,
    },
    Shutdown,
}

/// Clonable handle to the `AgentMailbox` actor.
///
/// All interaction with the mailbox goes through this handle.
/// Cloning the handle is cheap (clones the inner `mpsc::Sender`).
#[derive(Clone)]
pub struct AgentMailboxHandle {
    tx: mpsc::Sender<MailboxMessage>,
}

impl AgentMailboxHandle {
    /// Spawns the mailbox actor and returns a handle.
    ///
    /// The actor runs until [`AgentMailboxHandle::shutdown`] is called or all
    /// handles are dropped.
    pub fn spawn(event_bus: EventBusSender) -> Self {
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(run_mailbox_actor(rx, event_bus));
        Self { tx }
    }

    /// Sends a message from one agent to another.
    ///
    /// Returns `MailboxError::QueueFull` if the recipient's queue already
    /// contains [`MAX_MAILBOX_SIZE`] messages.
    pub async fn send(
        &self,
        from: &str,
        to: &str,
        payload: serde_json::Value,
    ) -> Result<(), MailboxError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MailboxMessage::Send {
                from: from.to_owned(),
                to: to.to_owned(),
                payload,
                reply: reply_tx,
            })
            .await
            .map_err(|_| MailboxError::ChannelClosed)?;
        reply_rx.await.map_err(|_| MailboxError::ChannelClosed)?
    }

    /// Receives the next pending message for `agent_name`, with a timeout.
    ///
    /// Returns `None` if no message arrives within the given duration.
    pub async fn receive(&self, agent_name: &str, timeout: Duration) -> Option<AgentMessage> {
        tokio::time::timeout(timeout, self.try_receive(agent_name))
            .await
            .unwrap_or_default()
    }

    /// Returns the number of pending messages for `agent_name`.
    ///
    /// Returns `0` if the agent has no mailbox yet.
    pub async fn pending_count(&self, agent_name: &str) -> usize {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(MailboxMessage::PendingCount {
                agent_name: agent_name.to_owned(),
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return 0;
        }
        reply_rx.await.unwrap_or(0)
    }

    /// Returns up to `limit` messages for `agent_name`, most recent first.
    ///
    /// The messages are cloned from the queue without consuming them.
    /// Returns an empty `Vec` if the agent has no pending messages.
    pub async fn list_messages(&self, agent_name: &str, limit: usize) -> Vec<AgentMessage> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(MailboxMessage::ListMessages {
                agent_name: agent_name.to_owned(),
                limit,
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Shuts down the mailbox actor gracefully.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(MailboxMessage::Shutdown).await;
    }

    /// Internal helper: sends a Receive message to the actor and awaits the reply.
    async fn try_receive(&self, agent_name: &str) -> Option<AgentMessage> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(MailboxMessage::Receive {
                agent_name: agent_name.to_owned(),
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return None;
        }
        reply_rx.await.ok().flatten()
    }
}

/// Actor loop: processes mailbox messages sequentially.
async fn run_mailbox_actor(mut rx: mpsc::Receiver<MailboxMessage>, event_bus: EventBusSender) {
    let mut queues: HashMap<String, VecDeque<AgentMessage>> = HashMap::new();

    while let Some(msg) = rx.recv().await {
        match msg {
            MailboxMessage::Send {
                from,
                to,
                payload,
                reply,
            } => {
                let queue = queues.entry(to.clone()).or_default();
                if queue.len() >= MAX_MAILBOX_SIZE {
                    let _ = reply.send(Err(MailboxError::QueueFull(to)));
                } else {
                    queue.push_back(AgentMessage {
                        from: from.clone(),
                        payload,
                        sent_at: now_rfc3339(),
                    });
                    // Fire-and-forget event emission
                    if event_bus
                        .send(RuntimeEvent::AgentMessageSent {
                            from: from.clone(),
                            to: to.clone(),
                        })
                        .is_err()
                    {
                        warn!(
                            from = %from,
                            to = %to,
                            "failed to emit AgentMessageSent event (no subscribers)"
                        );
                    }
                    let _ = reply.send(Ok(()));
                }
            }
            MailboxMessage::Receive { agent_name, reply } => {
                let message = queues.get_mut(&agent_name).and_then(|q| q.pop_front());
                let _ = reply.send(message);
            }
            MailboxMessage::PendingCount { agent_name, reply } => {
                let count = queues.get(&agent_name).map_or(0, |q| q.len());
                let _ = reply.send(count);
            }
            MailboxMessage::ListMessages {
                agent_name,
                limit,
                reply,
            } => {
                let messages = queues
                    .get(&agent_name)
                    .map(|q| q.iter().rev().take(limit).cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let _ = reply.send(messages);
            }
            MailboxMessage::Shutdown => {
                break;
            }
        }
    }
}

/// Returns the current time as an RFC 3339 string without external dependencies.
fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch → year/month/day (Gregorian civil from days)
    let (year, month, day) = civil_from_days(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Converts days since Unix epoch to (year, month, day).
///
/// Algorithm from Howard Hinnant's `chrono`-compatible date library.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;

    /// Agent A sends to Agent B, Agent B receives the message.
    #[tokio::test]
    async fn test_send_and_receive_message() {
        // GIVEN
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx);

        // WHEN
        let payload = serde_json::json!({"data": "hello"});
        handle
            .send("agent-a", "agent-b", payload.clone())
            .await
            .expect("send should succeed");

        // THEN
        let received = handle
            .receive("agent-b", Duration::from_secs(1))
            .await
            .expect("should receive a message");
        assert_eq!(received.from, "agent-a");
        assert_eq!(received.payload, payload);
        assert!(!received.sent_at.is_empty());

        handle.shutdown().await;
    }

    /// Receive with timeout returns None when no message is pending.
    #[tokio::test]
    async fn test_receive_timeout_returns_none() {
        // GIVEN
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx);

        // WHEN
        let result = handle.receive("agent-c", Duration::from_millis(100)).await;

        // THEN
        assert!(result.is_none());

        handle.shutdown().await;
    }

    /// Queue overflow returns MailboxError::QueueFull.
    #[tokio::test]
    async fn test_queue_full_returns_error() {
        // GIVEN
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx);

        // Fill the queue to MAX_MAILBOX_SIZE
        for i in 0..MAX_MAILBOX_SIZE {
            handle
                .send("sender", "agent-b", serde_json::json!({"i": i}))
                .await
                .expect("send should succeed");
        }

        // WHEN — 101st message
        let result = handle
            .send("sender", "agent-b", serde_json::json!({"overflow": true}))
            .await;

        // THEN
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, MailboxError::QueueFull(ref name) if name == "agent-b"),
            "expected QueueFull for agent-b, got: {err}"
        );

        // Verify count is still MAX_MAILBOX_SIZE (overflow message not added)
        let count = handle.pending_count("agent-b").await;
        assert_eq!(count, MAX_MAILBOX_SIZE);

        handle.shutdown().await;
    }

    /// AgentMessageSent event is emitted on successful send.
    #[tokio::test]
    async fn test_send_emits_event() {
        // GIVEN
        let (event_tx, mut event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx);

        // WHEN
        handle
            .send("agent-a", "agent-b", serde_json::json!({"ping": true}))
            .await
            .expect("send should succeed");

        // THEN
        let event = event_rx.recv().await.expect("should receive an event");
        assert!(
            matches!(
                event,
                RuntimeEvent::AgentMessageSent { ref from, ref to }
                if from == "agent-a" && to == "agent-b"
            ),
            "expected AgentMessageSent, got: {event:?}"
        );

        handle.shutdown().await;
    }
}
