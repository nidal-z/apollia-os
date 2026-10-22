//! The state a connection reads in: found, dated, kept, and announced.
//!
//! Four states, never six: working, needs repair (with its reason), paused,
//! blocked by the organisation; and, before any of them was found, not
//! checked yet, which is neither working nor broken. A state is found, never
//! declared:
//!
//! - An MCP server's state follows its health. A session whose handshake
//!   passed but whose operations nothing has yet proven is not working: it
//!   keeps the working state it was last found in, and otherwise reads as
//!   not checked yet.
//! - Every state found is kept in `connection_states` of `system.db` by the
//!   recorder ([`spawn_state_recorder`]) and announced as
//!   [`RuntimeEvent::ConnectionStateChanged`]. The list reads the kept state,
//!   so it shows what was last found, dated, even for a connection nothing
//!   has run since the start, and even when the event was sent before its
//!   reader subscribed.
//! - A connection with no state kept reads as not checked yet, never as
//!   needing repair: a 0.3.0 installation, which kept no state, shows nothing
//!   to repair until something has failed.

use std::collections::HashMap;
use std::path::PathBuf;

use apollia_core::{
    ConnectionReason, ConnectionReasonKey, ConnectionStateError, ConnectionStateRecord,
    ConnectionStateStore, ConnectionStatus, EventBusSender, McpHealth, RuntimeEvent,
};
use tracing::{event, Level};

use super::ConnectionState;

/// Detail of the state of an MCP session whose handshake passed and whose
/// operations nothing has proven yet.
const NOT_VERIFIED: &str = "reachable; no operation has succeeded yet";

/// The state an MCP server's `health` says, found at `now`, given the state
/// `stored` it was last found in.
///
/// `None` when the health proves nothing new: a session that is reachable
/// but unproven does not undo a working state found before.
#[must_use]
pub fn mcp_record(
    id: &str,
    health: &McpHealth,
    now: &str,
    stored: Option<&ConnectionStateRecord>,
) -> Option<ConnectionStateRecord> {
    let (state, reason) = match health {
        McpHealth::Healthy { verified: true } => (ConnectionStatus::Working, None),
        McpHealth::Healthy { verified: false } => {
            if stored.is_some_and(|s| s.state == ConnectionStatus::Working) {
                return None;
            }
            (
                ConnectionStatus::NotChecked,
                Some(ConnectionReason::new(
                    ConnectionReasonKey::NotChecked,
                    Some(NOT_VERIFIED.to_owned()),
                )),
            )
        }
        McpHealth::Degraded { last_error, .. } => (
            ConnectionStatus::NeedsRepair,
            Some(ConnectionReason::new(
                ConnectionReasonKey::Failing,
                Some(last_error.clone()),
            )),
        ),
        McpHealth::NeedsReauth { reason } => (
            ConnectionStatus::NeedsRepair,
            Some(ConnectionReason::new(
                ConnectionReasonKey::SignInAgain,
                Some(reason.clone()),
            )),
        ),
        McpHealth::Unavailable { reason } => (
            ConnectionStatus::NeedsRepair,
            Some(ConnectionReason::new(
                ConnectionReasonKey::Unreachable,
                Some(reason.clone()),
            )),
        ),
    };
    Some(ConnectionStateRecord {
        id: id.to_owned(),
        state,
        reason,
        checked_at: now.to_owned(),
    })
}

/// The reason of a connection no check has found in any state yet.
pub(crate) fn not_checked() -> ConnectionReason {
    ConnectionReason::new(ConnectionReasonKey::NotChecked, None)
}

/// What the list shows of one connection: its state, its reason, and when
/// it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShownState {
    pub status: ConnectionStatus,
    pub reason: Option<ConnectionReason>,
    pub checked_at: Option<String>,
}

/// What the list shows of connection `id`, from the state kept for it,
/// whether the policy forbade it, and the health of its live MCP session.
///
/// The kept state is shown with its date. A live health that says something
/// else than the kept state is shown instead, undated: it is newer than the
/// record, which the recorder has not written yet.
pub(crate) fn shown_state(
    id: &str,
    states: &HashMap<String, ConnectionStateRecord>,
    forbidden: bool,
    live: Option<&McpHealth>,
) -> ShownState {
    let stored = states.get(id).map(read_as_today);
    let stored = stored.as_ref();
    if forbidden {
        return ShownState {
            status: ConnectionStatus::Blocked,
            reason: Some(ConnectionReason::new(
                ConnectionReasonKey::ForbiddenByPolicy,
                None,
            )),
            checked_at: stored.map(|s| s.checked_at.clone()),
        };
    }
    let newer = live
        .and_then(|health| mcp_record(id, health, "", stored))
        .filter(|found| {
            stored.is_none_or(|kept| kept.state != found.state || kept.reason != found.reason)
        });
    match (newer, stored) {
        (Some(found), _) => ShownState {
            status: found.state,
            reason: found.reason,
            checked_at: None,
        },
        (None, Some(kept)) => ShownState {
            status: kept.state,
            reason: kept.reason.clone(),
            checked_at: Some(kept.checked_at.clone()),
        },
        (None, None) => ShownState {
            status: ConnectionStatus::NotChecked,
            reason: Some(not_checked()),
            checked_at: None,
        },
    }
}

/// A kept state as it reads today: a record written before the state
/// `not_checked` existed said "needs repair, not checked", which reads as
/// not checked yet.
fn read_as_today(kept: &ConnectionStateRecord) -> ConnectionStateRecord {
    let mut record = kept.clone();
    let unproven = record
        .reason
        .as_ref()
        .is_some_and(|r| r.key == ConnectionReasonKey::NotChecked);
    if record.state == ConnectionStatus::NeedsRepair && unproven {
        record.state = ConnectionStatus::NotChecked;
    }
    record
}

/// The detailed state of an MCP server with no live session, read from the
/// state kept for it: a start that failed keeps what its failure said
/// instead of reading as stopped.
pub(crate) fn kept_mcp_state(stored: Option<&ConnectionStateRecord>) -> ConnectionState {
    let key = stored
        .filter(|s| s.state == ConnectionStatus::NeedsRepair)
        .and_then(|s| s.reason.as_ref())
        .map(|r| r.key);
    match key {
        Some(ConnectionReasonKey::SignInAgain) => ConnectionState::NeedsReauth,
        Some(ConnectionReasonKey::Unreachable) => ConnectionState::Unavailable,
        Some(ConnectionReasonKey::Failing) => ConnectionState::Degraded,
        _ => ConnectionState::Stopped,
    }
}

/// The detailed state of a native account, a mailbox or a folder, read from
/// the state kept for it: one no check has found failing reads as connected.
pub(crate) fn kept_detail(stored: Option<&ConnectionStateRecord>) -> ConnectionState {
    let key = stored
        .filter(|s| s.state == ConnectionStatus::NeedsRepair)
        .and_then(|s| s.reason.as_ref())
        .map(|r| r.key);
    match key {
        Some(ConnectionReasonKey::SignInAgain | ConnectionReasonKey::PasswordRefused) => {
            ConnectionState::NeedsReauth
        }
        Some(ConnectionReasonKey::Unreachable | ConnectionReasonKey::FolderNotFound) => {
            ConnectionState::Unavailable
        }
        Some(ConnectionReasonKey::Failing) => ConnectionState::Degraded,
        _ => ConnectionState::Connected,
    }
}

/// The state kept for every connection, by identifier.
///
/// Never an error: a store that cannot be read is logged and answers no
/// state, so every connection then reads as not checked yet and the list
/// still loads.
pub(crate) fn kept_states(data_dir: &std::path::Path) -> HashMap<String, ConnectionStateRecord> {
    let path = apollia_core::paths::DataFile::System.path(data_dir);
    match ConnectionStateStore::open(&path).and_then(|store| store.list()) {
        Ok(records) => records.into_iter().map(|r| (r.id.clone(), r)).collect(),
        Err(e) => {
            event!(Level::WARN, error = %e, "connections.states.read.failed");
            HashMap::new()
        }
    }
}

/// Keep the state the health of MCP server `name` says, and announce it.
///
/// Answers the record kept, or `None` when the health proved nothing new.
fn record_mcp_health(
    data_dir: &std::path::Path,
    name: &str,
    health: &McpHealth,
) -> Result<Option<ConnectionStateRecord>, ConnectionStateError> {
    let store = ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(data_dir))?;
    let id = format!("mcp:{name}");
    let stored = store.get(&id).ok().flatten();
    let now = chrono::Utc::now().to_rfc3339();
    let Some(record) = mcp_record(&id, health, &now, stored.as_ref()) else {
        return Ok(None);
    };
    store.put(&record)?;
    Ok(Some(record))
}

/// Spawn the recorder: it keeps the state each MCP health change says in
/// `system.db` and announces it as [`RuntimeEvent::ConnectionStateChanged`].
///
/// Subscribed when it is spawned, so it is spawned before the MCP manager
/// starts, whose first announcements are the boot ones. It stops when the
/// bus closes or on [`RuntimeEvent::ShutdownRequested`]. Changes are handled
/// one at a time, in the order they were sent.
pub fn spawn_state_recorder(bus: EventBusSender, data_dir: PathBuf) -> tokio::task::JoinHandle<()> {
    let mut events = apollia_core::subscribe_resilient(&bus, "connections.state_recorder");
    tokio::spawn(async move {
        while let Some(received) = events.recv().await {
            let (name, health) = match received {
                RuntimeEvent::McpServerHealthChanged { name, health } => (name, health),
                RuntimeEvent::ShutdownRequested => break,
                _ => continue,
            };
            let dir = data_dir.clone();
            let server = name.clone();
            let recorded =
                tokio::task::spawn_blocking(move || record_mcp_health(&dir, &server, &health))
                    .await;
            match recorded {
                Ok(Ok(Some(record))) => {
                    let _ = bus.send(RuntimeEvent::ConnectionStateChanged {
                        id: record.id,
                        state: record.state,
                        reason: record.reason,
                        checked_at: record.checked_at,
                    });
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    event!(Level::WARN, server = %name, error = %e, "connections.state.record.failed");
                }
                Err(e) => {
                    event!(Level::WARN, server = %name, error = %e, "connections.state.record.aborted");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests;
