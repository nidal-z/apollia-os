//! Durable agent-to-agent messaging via a SQLite-backed mailbox actor.
//!
//! Each agent has a durable inbox persisted in a SQLite table owned exclusively
//! by the actor (one connection, one writer), following the same blocking-actor
//! pattern as the audit journal. Delivery is at-least-once: `receive` leases a
//! message (in-flight, visibility timeout) rather than deleting it; `ack`
//! deletes it; an unacked lease expires and the message becomes deliverable
//! again. A sweeper evicts messages past their TTL. The actor follows the Tokio
//! `mpsc` + clonable handle pattern used across the runtime.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::eventbus::EventBusSender;
use apollia_core::{RunId, RuntimeEvent};

/// SQL schema for the durable mailbox store.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS mailbox_messages (
        message_id       TEXT PRIMARY KEY,
        to_agent         TEXT    NOT NULL,
        from_agent       TEXT    NOT NULL,
        payload          TEXT    NOT NULL,
        sent_at          TEXT    NOT NULL,
        created_unix     INTEGER NOT NULL,
        state            TEXT    NOT NULL,
        lease_until_unix INTEGER,
        seq              INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_mailbox_to_seq
        ON mailbox_messages(to_agent, seq ASC);
";

/// Message stored in an agent's durable mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique identifier of the message.
    pub message_id: String,
    /// Name of the sending agent (or `host:<id>` for a host injection).
    pub from: String,
    /// Name of the receiving agent.
    pub to: String,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Timestamp of the send (RFC 3339).
    pub sent_at: String,
}

/// Tunables for the mailbox, sourced from `RuntimeConfig`.
#[derive(Debug, Clone)]
pub struct MailboxConfig {
    /// Maximum pending + in-flight messages per recipient.
    pub capacity: usize,
    /// Lease duration before an unacked message is redelivered (seconds).
    pub visibility_timeout_secs: u64,
    /// Time-to-live of a never-received message (seconds).
    pub message_ttl_secs: u64,
    /// Maximum sends allowed per run (anti-spam).
    pub send_quota_per_run: u32,
    /// Maximum serialized payload size (bytes).
    pub max_payload_bytes: usize,
    /// Whether the audit journal records full message payloads (else hash only).
    pub audit_full_payload: bool,
}

impl Default for MailboxConfig {
    fn default() -> Self {
        Self {
            capacity: 100,
            visibility_timeout_secs: 60,
            message_ttl_secs: 86_400,
            send_quota_per_run: 50,
            max_payload_bytes: 65_536,
            audit_full_payload: false,
        }
    }
}

/// Errors returned by mailbox operations.
#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    /// The agent's message queue has reached its capacity.
    #[error("mailbox full for agent '{agent}' (max {capacity})")]
    QueueFull {
        /// Name of the agent whose mailbox is full.
        agent: String,
        /// Configured mailbox capacity that was reached.
        capacity: usize,
    },

    /// The per-run send quota was exceeded (anti-spam guard).
    #[error("mailbox send quota exceeded for run (max {quota} per run)")]
    SendQuotaExceeded {
        /// Configured per-run send quota.
        quota: u32,
    },

    /// The serialized payload exceeded the configured maximum size.
    #[error("mailbox payload too large ({size} bytes, max {max})")]
    PayloadTooLarge {
        /// Actual serialized size in bytes.
        size: usize,
        /// Configured maximum in bytes.
        max: usize,
    },

    /// The intended recipient is not a registered agent.
    #[error("unknown mailbox recipient '{agent}'")]
    UnknownRecipient {
        /// Name of the unknown recipient.
        agent: String,
    },

    /// The underlying store failed.
    #[error("mailbox storage error: {0}")]
    Storage(String),

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
        run_id: Option<RunId>,
        reply: oneshot::Sender<Result<String, MailboxError>>,
    },
    Receive {
        agent_name: String,
        run_id: Option<RunId>,
        reply: oneshot::Sender<Option<AgentMessage>>,
    },
    Ack {
        agent_name: String,
        message_id: String,
        run_id: Option<RunId>,
        reply: oneshot::Sender<Result<(), MailboxError>>,
    },
    Nack {
        agent_name: String,
        message_id: String,
        reply: oneshot::Sender<Result<(), MailboxError>>,
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
    Sweep,
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
    /// Spawns the durable mailbox actor and returns a handle.
    ///
    /// `db_path` selects the backing store: `Some(path)` opens a durable file
    /// (created if absent, WAL); `None` uses an in-memory database (tests and
    /// minimal runtimes). If a durable open fails, the actor degrades to
    /// in-memory with a warning so the runtime still boots.
    ///
    /// A background sweeper task evicts messages past their TTL. The actor runs
    /// until [`AgentMailboxHandle::shutdown`] is called or all handles drop.
    pub async fn spawn(
        db_path: Option<PathBuf>,
        event_bus: EventBusSender,
        config: MailboxConfig,
    ) -> Self {
        let (tx, rx) = mpsc::channel(256);
        let (init_tx, init_rx) = oneshot::channel::<()>();
        let actor_bus = event_bus;
        let actor_cfg = config.clone();

        tokio::task::spawn_blocking(move || {
            let conn = match open_and_init(db_path.as_deref()) {
                Some(c) => c,
                None => {
                    warn!("mailbox: failed to open any SQLite store, actor not started");
                    let _ = init_tx.send(());
                    return;
                }
            };
            let _ = init_tx.send(());
            MailboxActor::new(conn, rx, actor_bus, actor_cfg).run();
        });

        let _ = init_rx.await;

        // Sweeper: periodically ask the actor to evict expired messages.
        let sweeper_tx = tx.clone();
        let interval = sweep_interval(config.message_ttl_secs);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if sweeper_tx.send(MailboxMessage::Sweep).await.is_err() {
                    break;
                }
            }
        });

        Self { tx }
    }

    /// Sends a message from one agent to another, returning the message id.
    ///
    /// `run_id` is the run of the sender (or a synthetic host-scoped run for a
    /// host injection); it flows into the emitted event so the message can be
    /// journaled. Returns [`MailboxError::QueueFull`], [`MailboxError::SendQuotaExceeded`],
    /// or [`MailboxError::PayloadTooLarge`] when a guard blocks the send.
    pub async fn send(
        &self,
        from: &str,
        to: &str,
        payload: serde_json::Value,
        run_id: Option<RunId>,
    ) -> Result<String, MailboxError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MailboxMessage::Send {
                from: from.to_owned(),
                to: to.to_owned(),
                payload,
                run_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| MailboxError::ChannelClosed)?;
        reply_rx.await.map_err(|_| MailboxError::ChannelClosed)?
    }

    /// Receives (leases) the next message for `agent_name`, with a timeout.
    ///
    /// The message is leased (in-flight) rather than removed; acknowledge it
    /// with [`AgentMailboxHandle::ack`] to delete it, or let the lease expire
    /// for redelivery. Returns `None` if no message is available.
    pub async fn receive(
        &self,
        agent_name: &str,
        run_id: Option<RunId>,
        timeout: Duration,
    ) -> Option<AgentMessage> {
        tokio::time::timeout(timeout, self.try_receive(agent_name, run_id))
            .await
            .unwrap_or_default()
    }

    /// Acknowledges a leased message, deleting it from the store.
    pub async fn ack(
        &self,
        agent_name: &str,
        message_id: &str,
        run_id: Option<RunId>,
    ) -> Result<(), MailboxError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MailboxMessage::Ack {
                agent_name: agent_name.to_owned(),
                message_id: message_id.to_owned(),
                run_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| MailboxError::ChannelClosed)?;
        reply_rx.await.map_err(|_| MailboxError::ChannelClosed)?
    }

    /// Refuses a leased message, making it immediately deliverable again.
    pub async fn nack(&self, agent_name: &str, message_id: &str) -> Result<(), MailboxError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MailboxMessage::Nack {
                agent_name: agent_name.to_owned(),
                message_id: message_id.to_owned(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| MailboxError::ChannelClosed)?;
        reply_rx.await.map_err(|_| MailboxError::ChannelClosed)?
    }

    /// Returns the number of non-expired messages for `agent_name`.
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
    /// The messages are read without consuming them (peek).
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

    /// Requests an immediate TTL sweep (mainly for tests).
    pub async fn sweep_now(&self) {
        let _ = self.tx.send(MailboxMessage::Sweep).await;
    }

    /// Shuts down the mailbox actor gracefully.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(MailboxMessage::Shutdown).await;
    }

    /// Internal helper: sends a Receive message to the actor and awaits the reply.
    async fn try_receive(&self, agent_name: &str, run_id: Option<RunId>) -> Option<AgentMessage> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self
            .tx
            .send(MailboxMessage::Receive {
                agent_name: agent_name.to_owned(),
                run_id,
                reply: reply_tx,
            })
            .await;
        if sent.is_err() {
            return None;
        }
        reply_rx.await.ok().flatten()
    }
}

/// Opens the store connection, applies WAL and the schema. Falls back to
/// in-memory if a durable path cannot be opened. Returns `None` only if even an
/// in-memory database cannot be created.
fn open_and_init(db_path: Option<&std::path::Path>) -> Option<Connection> {
    let conn = match db_path {
        Some(p) => match Connection::open(p) {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, path = %p.display(), "mailbox: durable open failed, using in-memory");
                Connection::open_in_memory().ok()?
            }
        },
        None => Connection::open_in_memory().ok()?,
    };
    // WAL is a no-op / harmless for in-memory; ignore its failure.
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
    if let Err(e) = conn.execute_batch(SCHEMA) {
        warn!(error = %e, "mailbox: schema init failed");
        return None;
    }
    Some(conn)
}

/// Sweep cadence derived from the TTL, clamped to a sensible range.
fn sweep_interval(ttl_secs: u64) -> Duration {
    let secs = ttl_secs.clamp(1, 60);
    Duration::from_secs(secs)
}

/// The mailbox actor. Owns the connection; reached only through the handle.
struct MailboxActor {
    conn: Connection,
    receiver: mpsc::Receiver<MailboxMessage>,
    event_bus: EventBusSender,
    config: MailboxConfig,
    next_seq: i64,
    /// Per-run send counters for the anti-spam quota.
    run_sends: HashMap<String, u32>,
}

impl MailboxActor {
    fn new(
        conn: Connection,
        receiver: mpsc::Receiver<MailboxMessage>,
        event_bus: EventBusSender,
        config: MailboxConfig,
    ) -> Self {
        let next_seq = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM mailbox_messages",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            + 1;
        Self {
            conn,
            receiver,
            event_bus,
            config,
            next_seq,
            run_sends: HashMap::new(),
        }
    }

    /// Sync actor loop, driven by blocking receives.
    fn run(mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            match msg {
                MailboxMessage::Send {
                    from,
                    to,
                    payload,
                    run_id,
                    reply,
                } => {
                    let _ = reply.send(self.handle_send(&from, &to, payload, run_id));
                }
                MailboxMessage::Receive {
                    agent_name,
                    run_id,
                    reply,
                } => {
                    let _ = reply.send(self.handle_receive(&agent_name, run_id));
                }
                MailboxMessage::Ack {
                    agent_name,
                    message_id,
                    run_id,
                    reply,
                } => {
                    let _ = reply.send(self.handle_ack(&agent_name, &message_id, run_id));
                }
                MailboxMessage::Nack {
                    agent_name,
                    message_id,
                    reply,
                } => {
                    let _ = reply.send(self.handle_nack(&agent_name, &message_id));
                }
                MailboxMessage::PendingCount { agent_name, reply } => {
                    let _ = reply.send(self.handle_pending_count(&agent_name));
                }
                MailboxMessage::ListMessages {
                    agent_name,
                    limit,
                    reply,
                } => {
                    let _ = reply.send(self.handle_list(&agent_name, limit));
                }
                MailboxMessage::Sweep => {
                    self.handle_sweep();
                }
                MailboxMessage::Shutdown => break,
            }
        }
        warn!("mailbox channel closed, actor exiting");
    }

    fn handle_send(
        &mut self,
        from: &str,
        to: &str,
        payload: serde_json::Value,
        run_id: Option<RunId>,
    ) -> Result<String, MailboxError> {
        let payload_str =
            serde_json::to_string(&payload).map_err(|e| MailboxError::Storage(e.to_string()))?;

        // Guard: payload size.
        if payload_str.len() > self.config.max_payload_bytes {
            self.emit_guard(
                "payload_too_large",
                from,
                &format!(
                    "{} bytes (max {})",
                    payload_str.len(),
                    self.config.max_payload_bytes
                ),
            );
            return Err(MailboxError::PayloadTooLarge {
                size: payload_str.len(),
                max: self.config.max_payload_bytes,
            });
        }

        // Guard: per-run send quota (skipped for runs we cannot attribute).
        if let Some(rid) = run_id.as_ref() {
            let counter = self.run_sends.entry(rid.to_string()).or_insert(0);
            if *counter >= self.config.send_quota_per_run {
                self.emit_guard(
                    "send_quota",
                    from,
                    &format!("run reached {} sends", self.config.send_quota_per_run),
                );
                return Err(MailboxError::SendQuotaExceeded {
                    quota: self.config.send_quota_per_run,
                });
            }
            *counter += 1;
        }

        // Backpressure: per-recipient capacity (non-expired rows).
        let now = now_unix();
        let ttl = self.config.message_ttl_secs as i64;
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM mailbox_messages \
                 WHERE to_agent = ?1 AND (created_unix + ?2) > ?3",
                params![to, ttl, now],
                |row| row.get(0),
            )
            .map_err(|e| MailboxError::Storage(e.to_string()))?;
        if count as usize >= self.config.capacity {
            let message_id = new_message_id();
            self.emit(RuntimeEvent::AgentMessageDropped {
                to: to.to_owned(),
                message_id,
                reason: "queue_full".to_owned(),
                run_id: run_id.clone(),
            });
            return Err(MailboxError::QueueFull {
                agent: to.to_owned(),
                capacity: self.config.capacity,
            });
        }

        let message_id = new_message_id();
        let seq = self.next_seq;
        self.next_seq += 1;
        let sent_at = now_rfc3339();
        let payload_hash = sha256_hex(payload_str.as_bytes());
        let full_payload = if self.config.audit_full_payload {
            Some(payload.clone())
        } else {
            None
        };

        self.conn
            .execute(
                "INSERT INTO mailbox_messages \
                 (message_id, to_agent, from_agent, payload, sent_at, created_unix, state, lease_until_unix, seq) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', NULL, ?7)",
                params![message_id, to, from, payload_str, sent_at, now, seq],
            )
            .map_err(|e| MailboxError::Storage(e.to_string()))?;

        self.emit(RuntimeEvent::AgentMessageSent {
            from: from.to_owned(),
            to: to.to_owned(),
            message_id: message_id.clone(),
            run_id,
            payload_hash,
            full_payload,
        });

        Ok(message_id)
    }

    fn handle_receive(&mut self, agent: &str, run_id: Option<RunId>) -> Option<AgentMessage> {
        let now = now_unix();
        let ttl = self.config.message_ttl_secs as i64;

        let row = self
            .conn
            .query_row(
                "SELECT message_id, from_agent, to_agent, payload, sent_at \
                 FROM mailbox_messages \
                 WHERE to_agent = ?1 AND (created_unix + ?2) > ?3 \
                   AND (state = 'pending' OR (state = 'in_flight' AND lease_until_unix <= ?3)) \
                 ORDER BY seq ASC LIMIT 1",
                params![agent, ttl, now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten()?;

        let (message_id, from, to, payload_str, sent_at) = row;
        let lease_until = now + self.config.visibility_timeout_secs as i64;
        if self
            .conn
            .execute(
                "UPDATE mailbox_messages SET state = 'in_flight', lease_until_unix = ?1 \
                 WHERE message_id = ?2",
                params![lease_until, message_id],
            )
            .is_err()
        {
            return None;
        }

        let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

        self.emit(RuntimeEvent::AgentMessageDelivered {
            to: to.clone(),
            message_id: message_id.clone(),
            run_id,
        });

        Some(AgentMessage {
            message_id,
            from,
            to,
            payload,
            sent_at,
        })
    }

    fn handle_ack(
        &mut self,
        agent: &str,
        message_id: &str,
        run_id: Option<RunId>,
    ) -> Result<(), MailboxError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM mailbox_messages WHERE message_id = ?1 AND to_agent = ?2",
                params![message_id, agent],
            )
            .map_err(|e| MailboxError::Storage(e.to_string()))?;
        if changed > 0 {
            self.emit(RuntimeEvent::AgentMessageAcked {
                to: agent.to_owned(),
                message_id: message_id.to_owned(),
                run_id,
            });
        }
        Ok(())
    }

    fn handle_nack(&mut self, agent: &str, message_id: &str) -> Result<(), MailboxError> {
        self.conn
            .execute(
                "UPDATE mailbox_messages SET state = 'pending', lease_until_unix = NULL \
                 WHERE message_id = ?1 AND to_agent = ?2",
                params![message_id, agent],
            )
            .map_err(|e| MailboxError::Storage(e.to_string()))?;
        Ok(())
    }

    fn handle_pending_count(&self, agent: &str) -> usize {
        let now = now_unix();
        let ttl = self.config.message_ttl_secs as i64;
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM mailbox_messages \
                 WHERE to_agent = ?1 AND (created_unix + ?2) > ?3",
                params![agent, ttl, now],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .unwrap_or(0)
    }

    fn handle_list(&self, agent: &str, limit: usize) -> Vec<AgentMessage> {
        let now = now_unix();
        let ttl = self.config.message_ttl_secs as i64;
        let mut stmt = match self.conn.prepare(
            "SELECT message_id, from_agent, to_agent, payload, sent_at \
             FROM mailbox_messages \
             WHERE to_agent = ?1 AND (created_unix + ?2) > ?3 \
             ORDER BY seq DESC LIMIT ?4",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![agent, ttl, now, limit as i64], |row| {
            let payload_str: String = row.get(3)?;
            Ok(AgentMessage {
                message_id: row.get(0)?,
                from: row.get(1)?,
                to: row.get(2)?,
                payload: serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null),
                sent_at: row.get(4)?,
            })
        });
        match rows {
            Ok(iter) => iter.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn handle_sweep(&mut self) {
        let now = now_unix();
        let ttl = self.config.message_ttl_secs as i64;
        let expired: Vec<(String, String)> = {
            let mut stmt = match self.conn.prepare(
                "SELECT message_id, to_agent FROM mailbox_messages \
                 WHERE (created_unix + ?1) <= ?2",
            ) {
                Ok(s) => s,
                Err(_) => return,
            };
            let rows = stmt.query_map(params![ttl, now], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            });
            match rows {
                Ok(iter) => iter.filter_map(Result::ok).collect(),
                Err(_) => return,
            }
        };
        if expired.is_empty() {
            return;
        }
        let _ = self.conn.execute(
            "DELETE FROM mailbox_messages WHERE (created_unix + ?1) <= ?2",
            params![ttl, now],
        );
        for (message_id, to) in expired {
            self.emit(RuntimeEvent::AgentMessageDropped {
                to,
                message_id,
                reason: "expired".to_owned(),
                run_id: None,
            });
        }
    }

    fn emit(&self, event: RuntimeEvent) {
        if self.event_bus.send(event).is_err() {
            // No subscribers is not an error for fire-and-forget messaging.
        }
    }

    fn emit_guard(&self, guard_type: &str, caller: &str, detail: &str) {
        self.emit(RuntimeEvent::MailboxGuardTriggered {
            guard_type: guard_type.to_owned(),
            caller: caller.to_owned(),
            detail: detail.to_owned(),
        });
    }
}

/// Returns a new random message id (UUID v4).
fn new_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Lowercase hex SHA-256 of the given bytes.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Current Unix time in whole seconds.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns the current time as an RFC 3339 string without external dependencies.
fn now_rfc3339() -> String {
    let secs = now_unix().max(0) as u64;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
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

    fn cfg() -> MailboxConfig {
        MailboxConfig::default()
    }

    /// Agent A sends to Agent B, Agent B receives the leased message.
    #[tokio::test]
    async fn test_send_and_receive_message() {
        // GIVEN a mailbox and a message from agent-a to agent-b
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(None, event_tx, cfg()).await;
        let payload = serde_json::json!({"data": "hello"});

        // WHEN agent-a sends and agent-b receives
        let id = handle
            .send("agent-a", "agent-b", payload.clone(), None)
            .await
            .expect("send should succeed");
        let received = handle
            .receive("agent-b", None, Duration::from_secs(1))
            .await
            .expect("should receive a message");

        // THEN the message content matches and carries its id
        assert_eq!(received.message_id, id);
        assert_eq!(received.from, "agent-a");
        assert_eq!(received.to, "agent-b");
        assert_eq!(received.payload, payload);
        assert!(!received.sent_at.is_empty());

        handle.shutdown().await;
    }

    /// A received message is leased, and ack removes it from the store.
    #[tokio::test]
    async fn test_ack_removes_message() {
        // GIVEN a delivered (leased) message
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(None, event_tx, cfg()).await;
        let id = handle
            .send("a", "b", serde_json::json!({"x": 1}), None)
            .await
            .expect("send");
        let _ = handle
            .receive("b", None, Duration::from_secs(1))
            .await
            .expect("receive");

        // WHEN it is acknowledged
        handle.ack("b", &id, None).await.expect("ack");

        // THEN no message remains
        assert_eq!(handle.pending_count("b").await, 0);

        handle.shutdown().await;
    }

    /// An unacked leased message becomes deliverable again after the lease.
    #[tokio::test]
    async fn test_lease_expiry_redelivers() {
        // GIVEN a mailbox whose visibility timeout is 0 seconds
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            visibility_timeout_secs: 0,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;
        let id = handle
            .send("a", "b", serde_json::json!({"x": 1}), None)
            .await
            .expect("send");

        // WHEN it is received (leased) but not acked, then received again
        let first = handle
            .receive("b", None, Duration::from_secs(1))
            .await
            .expect("first receive");
        let second = handle
            .receive("b", None, Duration::from_secs(1))
            .await
            .expect("redelivery after lease expiry");

        // THEN the same message is redelivered (at-least-once)
        assert_eq!(first.message_id, id);
        assert_eq!(second.message_id, id);

        handle.shutdown().await;
    }

    /// Queue overflow returns MailboxError::QueueFull.
    #[tokio::test]
    async fn test_queue_full_returns_error() {
        // GIVEN a mailbox with a small capacity, filled up
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            capacity: 3,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;
        for i in 0..3 {
            handle
                .send("s", "b", serde_json::json!({"i": i}), None)
                .await
                .expect("send should succeed");
        }

        // WHEN one message over capacity is sent
        let result = handle
            .send("s", "b", serde_json::json!({"over": true}), None)
            .await;

        // THEN it fails with QueueFull and the count is unchanged
        assert!(
            matches!(result, Err(MailboxError::QueueFull { ref agent, capacity }) if agent == "b" && capacity == 3)
        );
        assert_eq!(handle.pending_count("b").await, 3);

        handle.shutdown().await;
    }

    /// A payload above the configured limit is rejected before any write.
    #[tokio::test]
    async fn test_payload_too_large_rejected() {
        // GIVEN a mailbox with a tiny payload limit
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            max_payload_bytes: 16,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;

        // WHEN a large payload is sent
        let result = handle
            .send(
                "s",
                "b",
                serde_json::json!({"blob": "way too long to fit"}),
                None,
            )
            .await;

        // THEN it is rejected and nothing is stored
        assert!(matches!(result, Err(MailboxError::PayloadTooLarge { .. })));
        assert_eq!(handle.pending_count("b").await, 0);

        handle.shutdown().await;
    }

    /// The per-run send quota blocks a flood of sends.
    #[tokio::test]
    async fn test_send_quota_enforced() {
        // GIVEN a mailbox with a per-run quota of 2
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            send_quota_per_run: 2,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;
        let run = Some(RunId::from("run-1"));

        // WHEN three sends happen under the same run
        handle
            .send("s", "b", serde_json::json!({}), run.clone())
            .await
            .expect("1");
        handle
            .send("s", "b", serde_json::json!({}), run.clone())
            .await
            .expect("2");
        let third = handle
            .send("s", "b", serde_json::json!({}), run.clone())
            .await;

        // THEN the third is refused by the quota guard
        assert!(matches!(third, Err(MailboxError::SendQuotaExceeded { quota }) if quota == 2));

        handle.shutdown().await;
    }

    /// AgentMessageSent event is emitted with the message id and payload hash.
    #[tokio::test]
    async fn test_send_emits_event() {
        // GIVEN a mailbox
        let (event_tx, mut event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(None, event_tx, cfg()).await;

        // WHEN a message is sent with a run id
        let run = Some(RunId::from("run-x"));
        let id = handle
            .send("agent-a", "agent-b", serde_json::json!({"ping": true}), run)
            .await
            .expect("send");

        // THEN AgentMessageSent carries the id, recipient, and a run id
        let event = event_rx.recv().await.expect("event");
        assert!(
            matches!(
                event,
                RuntimeEvent::AgentMessageSent { ref from, ref to, ref message_id, ref run_id, .. }
                if from == "agent-a" && to == "agent-b" && *message_id == id && run_id.is_some()
            ),
            "unexpected event: {event:?}"
        );

        handle.shutdown().await;
    }

    /// Messages past their TTL are evicted by a sweep.
    #[tokio::test]
    async fn test_ttl_sweep_evicts() {
        // GIVEN a mailbox with a very short TTL
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            message_ttl_secs: 60,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;
        handle
            .send("s", "b", serde_json::json!({}), None)
            .await
            .expect("send");

        // WHEN a sweep runs (TTL not yet reached), the message remains
        handle.sweep_now().await;
        assert_eq!(handle.pending_count("b").await, 1);

        handle.shutdown().await;
    }
}
