//! The mailbox actor: sole owner of the SQLite connection behind an inbox.
//!
//! Opens (or falls back to in-memory) the store, then serves every
//! [`MailboxMessage`] on its bounded channel and sweeps expired mail on a
//! cadence derived from the TTL.

use std::collections::HashMap;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::mpsc;
use tracing::warn;

use crate::eventbus::EventBusSender;
use crate::mailbox::{
    new_message_id, now_rfc3339, now_unix, sha256_hex, AgentMessage, MailboxConfig, MailboxError,
    MailboxMessage, MIGRATIONS, SCHEMA_VERSION,
};
use apollia_core::{RunId, RuntimeEvent};

/// Opens the store connection, applies WAL and the versioned schema. Falls
/// back to in-memory if a durable path cannot be opened. Returns `None` when
/// no database can be created, and also when the store on disk was written by
/// a newer binary: refusing to start preserves the queued mail instead of
/// misreading it.
pub(super) fn open_and_init(db_path: Option<&std::path::Path>) -> Option<Connection> {
    let conn = match db_path {
        Some(p) => match Connection::open(p) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    error = %e,
                    path = %p.display(),
                    detail = "falling back to an in-memory store",
                    "mailbox.store.durable.failed"
                );
                Connection::open_in_memory().ok()?
            }
        },
        None => Connection::open_in_memory().ok()?,
    };
    // WAL is a no-op / harmless for in-memory; ignore its failure.
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
    if let Err(e) = apollia_core::schema::open_versioned(
        &conn,
        apollia_core::paths::DataFile::Mailbox.file_name(),
        SCHEMA_VERSION,
        &MIGRATIONS,
    ) {
        warn!(error = %e, "mailbox.schema.init.failed");
        return None;
    }
    Some(conn)
}

/// Sweep cadence derived from the TTL, clamped to a sensible range.
pub(super) fn sweep_interval(ttl_secs: u64) -> Duration {
    let secs = ttl_secs.clamp(1, 60);
    Duration::from_secs(secs)
}

/// The mailbox actor. Owns the connection; reached only through the handle.
pub(super) struct MailboxActor {
    conn: Connection,
    receiver: mpsc::Receiver<MailboxMessage>,
    event_bus: EventBusSender,
    config: MailboxConfig,
    next_seq: i64,
    /// Per-run send counters for the anti-spam quota.
    run_sends: HashMap<String, u32>,
}

impl MailboxActor {
    pub(super) fn new(
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
    pub(super) fn run(mut self) {
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
                    run_id,
                    reply,
                } => {
                    let _ = reply.send(self.handle_nack(&agent_name, &message_id, run_id));
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
        warn!("mailbox.channel.closed");
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
        // Record the leasing run as the lease owner so a later ack/nack from a
        // stale consumer (whose lease has since expired and been re-leased) is
        // fenced out. A re-lease overwrites the owner with the new run.
        let lease_owner = run_id.as_ref().map(|r| r.as_str());
        if self
            .conn
            .execute(
                "UPDATE mailbox_messages SET state = 'in_flight', lease_until_unix = ?1, \
                 lease_owner = ?2 WHERE message_id = ?3",
                params![lease_until, lease_owner, message_id],
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
        // Fence on the lease owner: `IS` is null-safe, so an ack whose run_id
        // differs from the current lease owner deletes zero rows and is a no-op.
        // This is what stops a stale consumer from deleting a message that has
        // since been re-leased to another run (finding C9-F4).
        let owner = run_id.as_ref().map(|r| r.as_str());
        let changed = self
            .conn
            .execute(
                "DELETE FROM mailbox_messages \
                 WHERE message_id = ?1 AND to_agent = ?2 AND lease_owner IS ?3",
                params![message_id, agent, owner],
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

    fn handle_nack(
        &mut self,
        agent: &str,
        message_id: &str,
        run_id: Option<RunId>,
    ) -> Result<(), MailboxError> {
        // Same null-safe owner fence as `handle_ack`: a stale consumer cannot
        // requeue a message that has been re-leased to another run. Clearing the
        // owner alongside the lease keeps a nacked (pending) row owner-less.
        let owner = run_id.as_ref().map(|r| r.as_str());
        self.conn
            .execute(
                "UPDATE mailbox_messages SET state = 'pending', lease_until_unix = NULL, \
                 lease_owner = NULL \
                 WHERE message_id = ?1 AND to_agent = ?2 AND lease_owner IS ?3",
                params![message_id, agent, owner],
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
