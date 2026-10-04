//! The outbox of governance events written by the stores of this crate.
//!
//! The signed audit journal has one writer, the runtime. A fine rule or a
//! standing consent, though, is written by whoever opens `governance.db`: the
//! runtime, the desktop, or the command line with no runtime running. Each
//! store therefore leaves an event in the `journal_outbox` table, in the same
//! transaction as the change it describes, and the runtime moves the events to
//! the journal: at once when it runs, at its next start otherwise.
//!
//! An event names what changed and who it concerns, never the argument of a
//! call. Its row carries the version of the event format, so a binary never
//! reads an event a newer one wrote: [`JournalOutbox::pending`] leaves such a
//! row in place.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::family_consent::ConsentFamily;
use crate::prefix_rule_engine::{PermissionScope, PrefixRule, RuleAction};

/// Version of the event format this binary writes and reads.
pub const OUTBOX_EVENT_VERSION: u32 = 1;

/// A fine rule, as an outbox event describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRecord {
    /// Identifier of the rule in `governance.db`.
    pub rule_id: i64,
    /// Tool the rule binds.
    pub tool_name: String,
    /// Argument prefix the rule matches, `None` for any argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg_prefix: Option<String>,
    /// What the rule does.
    pub action: RuleAction,
    /// One agent, or every agent.
    pub scope: PermissionScope,
    /// The agent of an agent rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Who created the rule, when it says (`None` for the operator).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl RuleRecord {
    /// The record of `rule`, persisted under `rule_id`.
    #[must_use]
    pub fn of(rule_id: i64, rule: &PrefixRule) -> Self {
        Self {
            rule_id,
            tool_name: rule.tool_name.clone(),
            arg_prefix: rule.arg_prefix.clone(),
            action: rule.action.clone(),
            scope: rule.scope,
            agent_id: rule.agent_id.clone(),
            created_by: rule.created_by_agent.clone(),
        }
    }
}

/// A change a store of `governance.db` made, waiting for the journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StoreEvent {
    /// A permanent standing consent ("always") was withdrawn.
    AlwaysWithdrawn {
        /// Agent the consent was given to.
        agent_id: String,
        /// Family it covered.
        family: ConsentFamily,
    },
    /// A family was granted to an agent for one batch run.
    BatchAccessGranted {
        /// Agent the family was granted to.
        agent_id: String,
        /// The batch run the grant holds for.
        batch_id: String,
        /// Family granted.
        family: ConsentFamily,
        /// Who granted it.
        granted_by: String,
    },
    /// A fine rule was added.
    RuleAdded(RuleRecord),
    /// A fine rule was removed.
    RuleRemoved(RuleRecord),
}

/// One event waiting in the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// Row identifier, increasing in the order the events were written.
    pub id: i64,
    /// When the change was made (Unix epoch, seconds).
    pub recorded_at: i64,
    /// The event.
    pub event: StoreEvent,
}

/// Leaves `event` in the outbox of `conn`, inside the caller's transaction.
pub(crate) fn enqueue(conn: &Connection, event: &StoreEvent) -> Result<(), PermissionError> {
    let json = serde_json::to_string(event).map_err(PermissionError::Event)?;
    conn.execute(
        "INSERT INTO journal_outbox (version, event, recorded_at) VALUES (?, ?, ?)",
        params![
            OUTBOX_EVENT_VERSION,
            json,
            crate::prefix_rule_engine::unix_now()
        ],
    )?;
    Ok(())
}

/// The reading side of the outbox, for the runtime that empties it.
pub struct JournalOutbox {
    db: Connection,
}

impl JournalOutbox {
    /// Opens (or creates) `governance.db` at `db_path` and brings its schema
    /// to the current version.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] or [`PermissionError::Schema`] when the
    /// file cannot be opened or migrated.
    pub fn new(db_path: &Path) -> Result<Self, PermissionError> {
        let db = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::governance_schema::open_governance_schema(&db)?;
        Ok(Self { db })
    }

    /// The events waiting, oldest first. A row this binary cannot read (a
    /// newer format, or a body that does not parse) is left in place and
    /// reported in the log, and the rows after it wait behind it, so the
    /// journal never receives the events out of order.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error.
    pub fn pending(&self) -> Result<Vec<OutboxEntry>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT id, version, event, recorded_at FROM journal_outbox ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut entries = Vec::with_capacity(rows.len());
        for (id, version, json, recorded_at) in rows {
            let parsed = (version == OUTBOX_EVENT_VERSION)
                .then(|| serde_json::from_str::<StoreEvent>(&json).ok())
                .flatten();
            let Some(event) = parsed else {
                tracing::event!(
                    tracing::Level::WARN,
                    id,
                    version,
                    "governance.outbox.unreadable"
                );
                break;
            };
            entries.push(OutboxEntry {
                id,
                recorded_at,
                event,
            });
        }
        Ok(entries)
    }

    /// Removes the events up to `last_id` included, once they are in the
    /// journal. Returns the number of rows removed.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error.
    pub fn acknowledge(&mut self, last_id: i64) -> Result<u32, PermissionError> {
        let removed = self
            .db
            .execute("DELETE FROM journal_outbox WHERE id <= ?", params![last_id])?;
        Ok(removed as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family_consent::{ConsentScope, FamilyConsentStore};
    use crate::prefix_rule_engine::PrefixRuleEngine;
    use tempfile::NamedTempFile;

    fn rule(tool: &str, prefix: Option<&str>) -> PrefixRule {
        PrefixRule {
            tool_name: tool.to_string(),
            arg_prefix: prefix.map(str::to_string),
            action: RuleAction::Allow,
            created_at: 1_700_000_000,
            ..PrefixRule::default()
        }
    }

    // Each write of a store leaves its event, in the order written.
    #[test]
    fn test_store_writes_leave_their_events_in_order() {
        // GIVEN an empty governance.db
        let file = NamedTempFile::new().expect("tempfile");
        let mut engine = PrefixRuleEngine::new(file.path()).expect("engine");
        let mut consents = FamilyConsentStore::new(file.path()).expect("consents");

        // WHEN a rule is added then removed, a family is granted for a batch,
        // and a permanent consent is granted then withdrawn
        let id = engine
            .add_rule(&rule("bash_executor", Some("git ")))
            .expect("add");
        assert!(engine.remove_rule_checked(id).expect("remove"));
        consents
            .grant(
                "a1",
                ConsentFamily::SendExternal,
                &ConsentScope::Lot("b1".into()),
                "batch",
            )
            .expect("grant lot");
        consents
            .grant(
                "a1",
                ConsentFamily::WriteOrDelete,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant always");
        assert!(consents
            .revoke("a1", ConsentFamily::WriteOrDelete, &ConsentScope::Permanent)
            .expect("revoke"));

        // THEN the outbox holds four events, in the order of the writes
        let outbox = JournalOutbox::new(file.path()).expect("outbox");
        let events: Vec<StoreEvent> = outbox
            .pending()
            .expect("pending")
            .into_iter()
            .map(|e| e.event)
            .collect();
        let record = RuleRecord {
            rule_id: id,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git ".into()),
            action: RuleAction::Allow,
            scope: PermissionScope::Global,
            agent_id: None,
            created_by: None,
        };
        assert_eq!(
            events,
            vec![
                StoreEvent::RuleAdded(record.clone()),
                StoreEvent::RuleRemoved(record),
                StoreEvent::BatchAccessGranted {
                    agent_id: "a1".into(),
                    batch_id: "b1".into(),
                    family: ConsentFamily::SendExternal,
                    granted_by: "batch".into(),
                },
                StoreEvent::AlwaysWithdrawn {
                    agent_id: "a1".into(),
                    family: ConsentFamily::WriteOrDelete,
                },
            ]
        );
    }

    // A write that changes nothing leaves no event.
    #[test]
    fn test_writes_that_change_nothing_leave_no_event() {
        // GIVEN an empty governance.db
        let file = NamedTempFile::new().expect("tempfile");
        let mut engine = PrefixRuleEngine::new(file.path()).expect("engine");
        let mut consents = FamilyConsentStore::new(file.path()).expect("consents");

        // WHEN an unknown rule and an absent consent are removed, and the same
        // batch grant is given twice
        assert!(!engine.remove_rule_checked(42).expect("remove"));
        engine.remove_rule(43).expect("remove silent");
        assert!(!consents
            .revoke("a1", ConsentFamily::SendExternal, &ConsentScope::Permanent)
            .expect("revoke"));
        for _ in 0..2 {
            consents
                .grant(
                    "a1",
                    ConsentFamily::SendExternal,
                    &ConsentScope::Lot("b1".into()),
                    "batch",
                )
                .expect("grant");
        }

        // THEN only the first batch grant is waiting
        let outbox = JournalOutbox::new(file.path()).expect("outbox");
        assert_eq!(outbox.pending().expect("pending").len(), 1);
    }

    // Removing rules in bulk leaves one event per rule removed.
    #[test]
    fn test_bulk_removal_leaves_one_event_per_rule() {
        // GIVEN two global rules and one agent rule
        let file = NamedTempFile::new().expect("tempfile");
        let mut engine = PrefixRuleEngine::new(file.path()).expect("engine");
        engine.add_rule(&rule("web_fetch", None)).expect("add");
        engine.add_rule(&rule("read_file", None)).expect("add");
        engine
            .add_rule(&PrefixRule {
                scope: PermissionScope::Agent,
                agent_id: Some("a1".into()),
                created_by_agent: Some("onboarding".into()),
                ..rule("send_email", None)
            })
            .expect("add");
        let mut outbox = JournalOutbox::new(file.path()).expect("outbox");
        let added = outbox.pending().expect("pending");
        outbox
            .acknowledge(added.last().expect("three").id)
            .expect("ack");

        // WHEN the global scope is cleared, then the rules of a creator
        let by_scope = engine
            .remove_rules_by_scope(PermissionScope::Global)
            .expect("by scope");
        let by_creator = engine
            .remove_rules_by_creator("onboarding")
            .expect("by creator");

        // THEN each removed rule has its event
        assert_eq!((by_scope, by_creator), (2, 1));
        let removed: Vec<String> = outbox
            .pending()
            .expect("pending")
            .into_iter()
            .map(|e| match e.event {
                StoreEvent::RuleRemoved(r) => r.tool_name,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(removed, vec!["web_fetch", "read_file", "send_email"]);
    }

    // Acknowledged events leave; a row of a newer format stops the reading.
    #[test]
    fn test_acknowledge_and_newer_format() {
        // GIVEN two events, the second written by a newer binary
        let file = NamedTempFile::new().expect("tempfile");
        let mut engine = PrefixRuleEngine::new(file.path()).expect("engine");
        engine.add_rule(&rule("web_fetch", None)).expect("add");
        let conn = Connection::open(file.path()).expect("open");
        conn.execute(
            "INSERT INTO journal_outbox (version, event, recorded_at) VALUES (?, '{}', 0)",
            params![OUTBOX_EVENT_VERSION + 1],
        )
        .expect("newer row");
        let mut outbox = JournalOutbox::new(file.path()).expect("outbox");

        // WHEN the readable event is acknowledged
        let first = outbox.pending().expect("pending");
        assert_eq!(first.len(), 1);
        let removed = outbox.acknowledge(first[0].id).expect("ack");

        // THEN it is gone, and the newer row stays, unread
        assert_eq!(removed, 1);
        assert!(outbox.pending().expect("pending").is_empty());
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM journal_outbox", [], |r| r.get(0))
            .expect("count");
        assert_eq!(left, 1);
    }
}
