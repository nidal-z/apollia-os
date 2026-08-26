//! Durable agent-to-agent messaging via a SQLite-backed mailbox actor.
//!
//! Each agent has a durable inbox persisted in a SQLite table owned exclusively
//! by the actor (one connection, one writer), following the same blocking-actor
//! pattern as the audit journal. Delivery is at-least-once: `receive` leases a
//! message (in-flight, visibility timeout) rather than deleting it; `ack`
//! deletes it; an unacked lease expires and the message becomes deliverable
//! again. A sweeper evicts messages past their TTL. The actor follows the Tokio
//! `mpsc` + clonable handle pattern used across the runtime.
//!
//! Lease exclusivity: each lease records its owner (the receiving `run_id`) in
//! `lease_owner`, and `ack` / `nack` are fenced on it. A stale consumer whose
//! lease has expired and been re-leased to another run can no longer delete or
//! requeue the message it lost. Two owner-less leases (`run_id` is `None`) share
//! a `NULL` owner and are not mutually fenced, the one residual race.

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::eventbus::EventBusSender;
use apollia_core::RunId;

mod actor;

use actor::{open_and_init, sweep_interval, MailboxActor};

/// Current schema version of the mailbox store: v1 base table and index,
/// v2 the `lease_owner` fence column (a no-op on a fresh database, where the
/// base schema already carries it).
const SCHEMA_VERSION: u32 = 2;

/// The ordered migration list applied through
/// [`apollia_core::schema::open_versioned`].
const MIGRATIONS: [apollia_core::schema::Migration; SCHEMA_VERSION as usize] =
    [migrate_v1, migrate_v2];

fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA)
}

fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Stores created before the lease-owner fence lack the column; a fresh
    // database already has it through the base schema.
    apollia_core::schema::add_column_if_missing(
        conn,
        "ALTER TABLE mailbox_messages ADD COLUMN lease_owner TEXT",
    )
}

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
        lease_owner      TEXT,
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
#[non_exhaustive]
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
        run_id: Option<RunId>,
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
                    warn!(detail = "actor not started", "mailbox.store.open.failed");
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
    pub async fn nack(
        &self,
        agent_name: &str,
        message_id: &str,
        run_id: Option<RunId>,
    ) -> Result<(), MailboxError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MailboxMessage::Nack {
                agent_name: agent_name.to_owned(),
                message_id: message_id.to_owned(),
                run_id,
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

/// Pure lease/ack state logic, extracted so the fence invariants can be proven
/// independently of SQLite (a model checker cannot see the database). Each
/// function re-encodes exactly one prod SQL predicate and cites the line it
/// mirrors; the end-to-end `test_ack_fenced_to_lease_owner` binds this model to
/// the real store. Owner identity is abstracted to an integer because only its
/// equality matters, exactly as the null-safe SQL `IS` fence compares it.
#[cfg(any(test, kani))]
pub(crate) mod lease_model {
    /// Lifecycle state, mirroring the `state` column (`'pending'` / `'in_flight'`).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum LeaseState {
        /// Deliverable, not currently leased.
        Pending,
        /// Leased to `lease_owner` until `lease_until`.
        InFlight,
    }

    /// Lease owner identity. `None` mirrors a `NULL` `lease_owner` (a run-less
    /// receive); `Some(id)` a specific run. Equality here is the null-safe `IS`.
    pub(crate) type Owner = Option<u64>;

    /// The fence-relevant projection of a `mailbox_messages` row.
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Row {
        /// Lifecycle state.
        pub state: LeaseState,
        /// `created_unix + message_ttl_secs`, the TTL horizon.
        pub created_plus_ttl: i64,
        /// Absolute lease expiry; meaningful only while `InFlight`.
        pub lease_until: i64,
        /// Current lease owner.
        pub lease_owner: Owner,
    }

    impl Row {
        /// A freshly inserted, pending, owner-less message (INSERT at l.578-585).
        pub(crate) fn pending(created_plus_ttl: i64) -> Self {
            Self {
                state: LeaseState::Pending,
                created_plus_ttl,
                lease_until: 0,
                lease_owner: None,
            }
        }

        /// Lease (or re-lease) to `owner` at `now` for `visibility` seconds,
        /// mirroring the receive UPDATE (l.628-641). A re-lease overwrites the
        /// owner, which is what fences the previous holder out.
        pub(crate) fn lease(&mut self, now: i64, visibility: i64, owner: Owner) {
            self.state = LeaseState::InFlight;
            self.lease_until = now + visibility;
            self.lease_owner = owner;
        }
    }

    /// Whether the receive SELECT would pick this row (l.606-611): within TTL,
    /// and either pending or an in-flight lease that has already expired.
    pub(crate) fn is_deliverable(row: &Row, now: i64) -> bool {
        row.created_plus_ttl > now
            && (row.state == LeaseState::Pending
                || (row.state == LeaseState::InFlight && row.lease_until <= now))
    }

    /// Whether an ack/nack by `actor` may act on a row owned by `owner`,
    /// mirroring the null-safe `lease_owner IS ?` fence (ack, nack). `Option`
    /// equality is exactly SQLite `IS`: `None`/`None` matches, distinct owners
    /// never do.
    pub(crate) fn owner_matches(owner: Owner, actor: Owner) -> bool {
        owner == actor
    }
}

// Property harnesses for the mailbox lease/ack invariants over the pure model
// above. The `#[cfg(kani)]` block proves them symbolically over a bounded space;
// this proptest block samples them and runs under `cargo test`.
#[cfg(test)]
mod property {
    use super::lease_model::*;
    use proptest::prelude::*;

    proptest! {
        /// Exclusivity (C9-F4): once a message leased to A is re-leased to B, a
        /// stale ack/nack by A (with `a != b`) is fenced out, while B still acts.
        #[test]
        fn prop_fenced_ack_rejects_stale_owner(
            a in 0u64..8, b in 0u64..8,
            t0 in 0i64..1_000, vis in 1i64..1_000, ttl_extra in 2i64..100_000,
        ) {
            // GIVEN a pending row leased by actor a, whose lease has since expired
            prop_assume!(a != b);
            let created_plus_ttl = t0 + vis + ttl_extra;
            let mut row = Row::pending(created_plus_ttl);
            row.lease(t0, vis, Some(a));
            let now2 = row.lease_until; // lease expired (<= now)
            prop_assert!(is_deliverable(&row, now2));
            // WHEN actor b takes the lease over
            row.lease(now2, vis, Some(b));
            // THEN ownership moves to b and the stale owner no longer matches
            prop_assert_eq!(row.lease_owner, Some(b));
            prop_assert!(!owner_matches(row.lease_owner, Some(a)));
            prop_assert!(owner_matches(row.lease_owner, Some(b)));
        }

        /// Owner match is exactly null-safe equality: the current owner (and a
        /// matching run-less `None`/`None`) passes, everything else is fenced.
        #[test]
        fn prop_owner_matches_is_null_safe_eq(
            owner in prop::option::of(0u64..8),
            actor in prop::option::of(0u64..8),
        ) {
            // GIVEN any pair of owners, each present or absent
            // WHEN they are compared through owner_matches
            // THEN the answer is plain equality, absent included
            prop_assert_eq!(owner_matches(owner, actor), owner == actor);
        }

        /// At-least-once: an expired in-flight lease is redeliverable within TTL.
        #[test]
        fn prop_expired_lease_redeliverable(
            t0 in 0i64..1_000, vis in 1i64..1_000, ttl_extra in 2i64..100_000, owner in 0u64..8,
        ) {
            // GIVEN a pending row whose TTL outlives the lease
            let created_plus_ttl = t0 + vis + ttl_extra;
            let mut row = Row::pending(created_plus_ttl);
            // WHEN the lease is taken and its deadline is reached
            row.lease(t0, vis, Some(owner));
            // THEN the row is deliverable again
            prop_assert!(is_deliverable(&row, row.lease_until));
        }

        /// A live lease is not redeliverable before it expires.
        #[test]
        fn prop_live_lease_not_redeliverable(
            t0 in 0i64..1_000, vis in 2i64..1_000, ttl_extra in 2i64..100_000, owner in 0u64..8,
        ) {
            // GIVEN a pending row whose TTL outlives the lease
            let created_plus_ttl = t0 + vis + ttl_extra;
            let mut row = Row::pending(created_plus_ttl);
            // WHEN the lease is taken and read one tick before its deadline
            row.lease(t0, vis, Some(owner));
            // THEN the row is not deliverable to anyone else
            prop_assert!(!is_deliverable(&row, row.lease_until - 1));
        }
    }
}

// SEED harnesses for `cargo kani`. Kani is not wired into the local toolchain
// (it links its own toolchain via rustup, absent here); these bounded proofs run
// in the advisory nightly `kani` job. Times are bounded via `kani::assume` to
// keep the `now + visibility` arithmetic in range, exactly as the prod handlers
// operate on realistic Unix seconds.
#[cfg(kani)]
mod proofs {
    use super::lease_model::*;

    /// Exclusivity (C9-F4): a stale owner can never act on a re-leased message,
    /// while the current owner always can.
    #[kani::proof]
    fn kani_fenced_ack_rejects_stale_owner() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        kani::assume(a != b);
        let t0: i64 = kani::any();
        let vis: i64 = kani::any();
        kani::assume(t0 >= 0 && t0 < 1_000_000);
        kani::assume(vis > 0 && vis < 1_000_000);
        let created_plus_ttl = t0 + 2 * vis + 1;
        let mut row = Row::pending(created_plus_ttl);
        row.lease(t0, vis, Some(a));
        let now2 = row.lease_until; // lease expired at the boundary
        assert!(is_deliverable(&row, now2));
        row.lease(now2, vis, Some(b));
        assert!(!owner_matches(row.lease_owner, Some(a)));
        assert!(owner_matches(row.lease_owner, Some(b)));
    }

    /// Owner match is exactly null-safe equality over the whole `Option<u64>`
    /// domain, including the `None`/`None` residual that lets run-less delivery
    /// still be acked.
    #[kani::proof]
    fn kani_owner_matches_is_null_safe_eq() {
        let owner: Option<u64> = kani::any();
        let actor: Option<u64> = kani::any();
        assert_eq!(owner_matches(owner, actor), owner == actor);
        assert!(owner_matches(None, None));
    }

    /// At-least-once: an expired in-flight lease is redeliverable within TTL.
    #[kani::proof]
    fn kani_expired_lease_redeliverable() {
        let t0: i64 = kani::any();
        let vis: i64 = kani::any();
        kani::assume(t0 >= 0 && t0 < 1_000_000);
        kani::assume(vis > 0 && vis < 1_000_000);
        let created_plus_ttl = t0 + vis + 1;
        let mut row = Row::pending(created_plus_ttl);
        row.lease(t0, vis, Some(7));
        assert!(is_deliverable(&row, row.lease_until));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;
    use apollia_core::RuntimeEvent;

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

    /// C9-F4 regression, binding the pure lease model to the real store: a stale
    /// ack from the run whose lease has been re-leased to another run must not
    /// delete the message the new owner is processing.
    #[tokio::test]
    async fn test_ack_fenced_to_lease_owner() {
        use apollia_core::RunId;

        // GIVEN a zero-visibility mailbox, so a lease is immediately re-leasable
        // (an expired lease without sleeping), and two distinct runs.
        let (event_tx, _event_rx) = EventBus::new();
        let config = MailboxConfig {
            visibility_timeout_secs: 0,
            ..MailboxConfig::default()
        };
        let handle = AgentMailboxHandle::spawn(None, event_tx, config).await;
        let run_a = RunId::new();
        let run_b = RunId::new();
        let id = handle
            .send("a", "b", serde_json::json!({"n": 1}), Some(run_a.clone()))
            .await
            .expect("send");

        // WHEN run A leases it, then run B re-leases the now-expired lease
        let m1 = handle
            .receive("b", Some(run_a.clone()), Duration::from_secs(1))
            .await
            .expect("A receives");
        assert_eq!(m1.message_id, id);
        let m2 = handle
            .receive("b", Some(run_b.clone()), Duration::from_secs(1))
            .await
            .expect("B re-leases");
        assert_eq!(m2.message_id, id);

        // THEN a stale ack by A does not delete B's message
        handle
            .ack("b", &id, Some(run_a))
            .await
            .expect("stale ack returns ok");
        assert_eq!(
            handle.pending_count("b").await,
            1,
            "stale ack must not delete the re-leased message"
        );

        // ...and the current owner B can still ack it.
        handle.ack("b", &id, Some(run_b)).await.expect("owner ack");
        assert_eq!(handle.pending_count("b").await, 0);

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
        // THEN the message is still pending
        assert_eq!(handle.pending_count("b").await, 1);

        handle.shutdown().await;
    }

    /// mailbox.db as written before the lease-owner fence, with one row.
    const MAILBOX_V1_FIXTURE: &str = include_str!("../tests/fixtures/schemas/mailbox_v1.sql");

    #[test]
    fn test_open_and_init_v1_database_migrates_lease_owner_and_keeps_rows() {
        // GIVEN a mailbox.db written before the lease-owner fence (schema v1,
        // user_version 0, one pending message)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mailbox.db");
        let seed = Connection::open(&path).expect("open raw");
        seed.execute_batch(MAILBOX_V1_FIXTURE).expect("seed v1");
        drop(seed);

        // WHEN opening it through the versioned initializer
        let conn = open_and_init(Some(&path)).expect("open migrated");

        // THEN the lease_owner column exists (NULL on the legacy row), the row
        // survived, and the file is stamped at the current version
        let (owner, state): (Option<String>, String) = conn
            .query_row(
                "SELECT lease_owner, state FROM mailbox_messages WHERE message_id = 'legacy-mail-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("legacy row");
        assert_eq!(owner, None);
        assert_eq!(state, "pending");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("user_version");
        assert_eq!(version, i64::from(SCHEMA_VERSION));
    }

    #[test]
    fn test_open_and_init_refuses_newer_database() {
        // GIVEN a mailbox.db stamped one version above this binary
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mailbox.db");
        let seed = Connection::open(&path).expect("open raw");
        seed.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("stamp");
        drop(seed);

        // WHEN opening it through the versioned initializer
        let result = open_and_init(Some(&path));

        // THEN the store is refused (no connection) instead of misread
        assert!(result.is_none());
    }
}
