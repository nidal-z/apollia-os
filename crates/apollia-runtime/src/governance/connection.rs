//! Connection events and their journal form, and the handle the places that
//! add, change or remove a connection hold.
//!
//! Each connection gets its own journal run, `connection:<connection_id>`, so
//! its history reads in order with `query_run` and verifies with
//! `verify_chain`; the global chain of the journal still weaves it with every
//! other run. A secret is journaled by the name of its key, never by its value.

use std::sync::Arc;

use apollia_mcp::Provenance;
use serde::{Deserialize, Serialize};

use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind};
use crate::connections::ConnectionKind;
use crate::governance::admission::{
    admit, AdmissionDecision, AdmissionRequest, ConnectionAdmission, NoPolicyAdmission,
};
use crate::governance::contract::GovernanceError;

/// Prefix of the journal run that holds the history of one connection.
const CONNECTION_RUN_PREFIX: &str = "connection:";

/// Journal run holding the history of `connection_id`.
pub fn connection_run_id(connection_id: &str) -> String {
    format!("{CONNECTION_RUN_PREFIX}{connection_id}")
}

/// Identifier of the native connection of `account`, as the connection list
/// names it.
fn native_connection_id(provider: apollia_auth::ConnectorProvider, account: &str) -> String {
    format!("native:{}:{account}", provider.id())
}

/// An event in the life of a connection: who did what to which connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ConnectionEvent {
    /// A connection was added.
    Added {
        /// Connection identifier (`mcp:<server>`, `native:<provider>:<account>`).
        connection_id: String,
        /// Kind of connection.
        kind: ConnectionKind,
        /// Sends data off this machine.
        remote: bool,
        /// Where the connection was found before it was added.
        provenance: Provenance,
        /// Who added it (`api`, `desktop`).
        actor: String,
    },
    /// The configuration of a connection was replaced.
    Modified {
        /// Connection identifier.
        connection_id: String,
        /// Sends data off this machine, after the change.
        remote: bool,
        /// Who changed it.
        actor: String,
    },
    /// A connection was removed.
    Removed {
        /// Connection identifier.
        connection_id: String,
        /// Who removed it.
        actor: String,
    },
    /// The warning shown before a connection is added was accepted.
    WarningAccepted {
        /// Connection identifier.
        connection_id: String,
        /// Who accepted it.
        actor: String,
    },
    /// A secret the connection names was set.
    SecretSet {
        /// Connection identifier.
        connection_id: String,
        /// Name of the secret's key. Never its value.
        key: String,
        /// Who set it.
        actor: String,
    },
    /// A secret the connection names was removed.
    SecretRemoved {
        /// Connection identifier.
        connection_id: String,
        /// Name of the secret's key.
        key: String,
        /// Who removed it.
        actor: String,
    },
    /// A native connector account was connected.
    AccountConnected {
        /// Connection identifier, `native:<provider>:<account>`.
        connection_id: String,
        /// Provider identifier (`google`, `microsoft`).
        provider: String,
        /// Account the connection acts as.
        account: String,
        /// Who connected it.
        actor: String,
    },
    /// A native connector account was disconnected.
    AccountDisconnected {
        /// Connection identifier, `native:<provider>:<account>`.
        connection_id: String,
        /// Provider identifier.
        provider: String,
        /// Account that was disconnected.
        account: String,
        /// Who disconnected it.
        actor: String,
    },
    /// A folder of this machine was added as a connection.
    FolderAdded {
        /// Connection identifier.
        connection_id: String,
        /// Absolute path of the folder.
        path: String,
        /// Who added it.
        actor: String,
    },
}

impl ConnectionEvent {
    /// `account` of `provider` was connected by `actor`.
    pub fn account_connected(
        provider: apollia_auth::ConnectorProvider,
        account: &str,
        actor: &str,
    ) -> Self {
        ConnectionEvent::AccountConnected {
            connection_id: native_connection_id(provider, account),
            provider: provider.id().to_owned(),
            account: account.to_owned(),
            actor: actor.to_owned(),
        }
    }

    /// `account` of `provider` was disconnected by `actor`.
    pub fn account_disconnected(
        provider: apollia_auth::ConnectorProvider,
        account: &str,
        actor: &str,
    ) -> Self {
        ConnectionEvent::AccountDisconnected {
            connection_id: native_connection_id(provider, account),
            provider: provider.id().to_owned(),
            account: account.to_owned(),
            actor: actor.to_owned(),
        }
    }

    /// Connection the event is about.
    pub fn connection_id(&self) -> &str {
        match self {
            ConnectionEvent::Added { connection_id, .. }
            | ConnectionEvent::Modified { connection_id, .. }
            | ConnectionEvent::Removed { connection_id, .. }
            | ConnectionEvent::WarningAccepted { connection_id, .. }
            | ConnectionEvent::SecretSet { connection_id, .. }
            | ConnectionEvent::SecretRemoved { connection_id, .. }
            | ConnectionEvent::AccountConnected { connection_id, .. }
            | ConnectionEvent::AccountDisconnected { connection_id, .. }
            | ConnectionEvent::FolderAdded { connection_id, .. } => connection_id,
        }
    }

    /// Journal kind the event is recorded under.
    pub fn journal_kind(&self) -> JournalEntryKind {
        match self {
            ConnectionEvent::Added { .. } => JournalEntryKind::ConnectionAdded,
            ConnectionEvent::Modified { .. } => JournalEntryKind::ConnectionModified,
            ConnectionEvent::Removed { .. } => JournalEntryKind::ConnectionRemoved,
            ConnectionEvent::WarningAccepted { .. } => JournalEntryKind::ConnectionWarningAccepted,
            ConnectionEvent::SecretSet { .. } => JournalEntryKind::ConnectionSecretSet,
            ConnectionEvent::SecretRemoved { .. } => JournalEntryKind::ConnectionSecretRemoved,
            ConnectionEvent::AccountConnected { .. } => JournalEntryKind::AccountConnected,
            ConnectionEvent::AccountDisconnected { .. } => JournalEntryKind::AccountDisconnected,
            ConnectionEvent::FolderAdded { .. } => JournalEntryKind::FolderAdded,
        }
    }
}

/// Payload of a connection journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRecord {
    /// The recorded event.
    #[serde(flatten)]
    pub event: ConnectionEvent,
}

impl ConnectionRecord {
    /// Builds the journal draft for `event`, stamped at `ts` (RFC3339 UTC).
    pub(crate) fn draft(
        event: &ConnectionEvent,
        ts: String,
    ) -> Result<JournalEntryDraft, GovernanceError> {
        let record = ConnectionRecord {
            event: event.clone(),
        };
        let payload = serde_json::to_value(&record).map_err(GovernanceError::Serialize)?;
        Ok(JournalEntryDraft {
            run_id: connection_run_id(event.connection_id()),
            ts,
            kind: event.journal_kind(),
            payload,
        })
    }
}

/// What the places that add, change or remove a connection hold: the
/// admission policy and the journal.
///
/// The default admits everything and journals nothing, which is what a host
/// without a journal (a test, a degraded boot) runs with.
#[derive(Clone)]
pub struct ConnectionGovernance {
    admission: Arc<dyn ConnectionAdmission>,
    journal: Option<AuditJournalHandle>,
}

impl Default for ConnectionGovernance {
    fn default() -> Self {
        Self::without_policy(None)
    }
}

impl ConnectionGovernance {
    /// Governance over `admission`, journaling into `journal` when there is one.
    pub fn new(
        admission: Arc<dyn ConnectionAdmission>,
        journal: Option<AuditJournalHandle>,
    ) -> Self {
        Self { admission, journal }
    }

    /// Governance in force when no policy is deposited.
    pub fn without_policy(journal: Option<AuditJournalHandle>) -> Self {
        Self::new(Arc::new(NoPolicyAdmission), journal)
    }

    /// Decides whether the connection `request` describes may be added. Fails
    /// closed, see [`admit`].
    #[must_use]
    pub fn admit(&self, request: &AdmissionRequest) -> AdmissionDecision {
        admit(self.admission.as_ref(), request)
    }

    /// Appends `event` to the journal, chained and signed like every entry.
    /// Without a journal the event is only traced.
    pub fn record(&self, event: &ConnectionEvent) -> Result<(), GovernanceError> {
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let draft = ConnectionRecord::draft(event, ts)?;
        tracing::event!(
            tracing::Level::DEBUG,
            connection_id = %event.connection_id(),
            kind = %draft.kind.tag(),
            journaled = self.journal.is_some(),
            "governance.connection.recorded"
        );
        if let Some(journal) = &self.journal {
            journal.append(draft);
        }
        Ok(())
    }

    /// [`Self::record`], with a failure traced rather than returned: the change
    /// it describes has already happened and is not undone.
    pub fn record_or_warn(&self, event: &ConnectionEvent) {
        if let Err(e) = self.record(event) {
            tracing::event!(
                tracing::Level::WARN,
                connection_id = %event.connection_id(),
                error = %e,
                "governance.connection.record_failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::verify::verify_entries;
    use crate::audit_journal::HmacSigner;

    const KEY: &[u8] = b"connection-journal-key-0001";

    fn every_event() -> Vec<ConnectionEvent> {
        let id = "mcp:notion".to_string();
        let actor = "desktop".to_string();
        vec![
            ConnectionEvent::WarningAccepted {
                connection_id: id.clone(),
                actor: actor.clone(),
            },
            ConnectionEvent::SecretSet {
                connection_id: id.clone(),
                key: "NOTION_TOKEN".into(),
                actor: actor.clone(),
            },
            ConnectionEvent::Added {
                connection_id: id.clone(),
                kind: ConnectionKind::Mcp,
                remote: true,
                provenance: Provenance::Selection,
                actor: actor.clone(),
            },
            ConnectionEvent::Modified {
                connection_id: id.clone(),
                remote: true,
                actor: actor.clone(),
            },
            ConnectionEvent::AccountConnected {
                connection_id: id.clone(),
                provider: "google".into(),
                account: "ada@example.com".into(),
                actor: actor.clone(),
            },
            ConnectionEvent::AccountDisconnected {
                connection_id: id.clone(),
                provider: "google".into(),
                account: "ada@example.com".into(),
                actor: actor.clone(),
            },
            ConnectionEvent::FolderAdded {
                connection_id: id.clone(),
                path: "/Users/ada/Documents".into(),
                actor: actor.clone(),
            },
            ConnectionEvent::SecretRemoved {
                connection_id: id.clone(),
                key: "NOTION_TOKEN".into(),
                actor: actor.clone(),
            },
            ConnectionEvent::Removed {
                connection_id: id,
                actor,
            },
        ]
    }

    // Every connection event lands in the journal under its own kind, chained
    // and signed, and the existing verification accepts the chain.
    #[tokio::test]
    async fn test_every_connection_event_is_chained_signed_and_verified() {
        // GIVEN connection governance over a signed journal
        let path =
            std::env::temp_dir().join(format!("apollia_conn_gov_{}.db", uuid::Uuid::new_v4()));
        let journal = AuditJournalHandle::open_with_key_bytes(&path, KEY.to_vec())
            .await
            .expect("open signed journal");
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
        let events = every_event();

        // WHEN each event is recorded
        for event in &events {
            governance.record(event).expect("record");
        }

        // THEN the connection run holds one entry per event, of its own kind,
        // each signed and linked to the previous one
        let run = connection_run_id("mcp:notion");
        let entries = journal.query_run(&run).await;
        let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
        let expected: Vec<JournalEntryKind> = events.iter().map(|e| e.journal_kind()).collect();
        assert_eq!(kinds, expected);
        assert!(entries.iter().all(|e| e.signature.is_some()));
        for pair in entries.windows(2) {
            assert_eq!(pair[1].prev_hash, pair[0].hash);
        }
        // AND the journal's own verification accepts it, signatures required
        let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
        let report = verify_entries(&run, &entries, Some(&signer), true);
        assert!(report.ok, "chain must verify: {report:?}");
        assert_eq!(report.entries_checked, events.len() as u64);
        let whole = journal.verify_journal().await.expect("verify journal");
        assert!(whole.ok);

        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // A secret is journaled by its key, never by a value.
    #[test]
    fn test_secret_payload_names_the_key_only() {
        // GIVEN a secret set on a connection
        let event = ConnectionEvent::SecretSet {
            connection_id: "mcp:github".into(),
            key: "GITHUB_TOKEN".into(),
            actor: "api".into(),
        };
        // WHEN the draft is built
        let draft = ConnectionRecord::draft(&event, "2026-01-01T00:00:00Z".into()).expect("draft");
        // THEN the payload is flat, carries the key name and nothing else
        assert_eq!(draft.run_id, "connection:mcp:github");
        assert_eq!(
            draft.payload,
            serde_json::json!({
                "event": "secret_set",
                "connection_id": "mcp:github",
                "key": "GITHUB_TOKEN",
                "actor": "api",
            })
        );
    }

    // Without a journal, recording succeeds and writes nowhere.
    #[test]
    fn test_default_governance_records_without_a_journal() {
        // GIVEN the default governance
        let governance = ConnectionGovernance::default();
        // WHEN an event is recorded
        let result = governance.record(&every_event()[0]);
        // THEN it succeeds
        assert!(result.is_ok());
    }
}
