//! Standing consents: "always" for one agent and one action family.
//!
//! A standing consent answers, once, the question a sensitive action asks:
//! may this agent act in this family without asking again? It is stored in
//! the `family_consents` table of `governance.db`, one row per (agent,
//! family, scope). The scope is `permanent`, or `lot:<id>` for a consent that
//! only covers one batch run and is withdrawn in one call when the run ends.
//!
//! The "execute code" family never takes a standing consent: every execution
//! asks, and [`FamilyConsentStore::grant`] refuses it with a typed error.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::journal_outbox::{enqueue, StoreEvent};

/// Agent identifier of Apollia, the conversational agent of the interface.
///
/// Fine rules and standing consents given in the built-in chat are recorded
/// under this identifier.
pub const APOLLIA_AGENT_ID: &str = "apollia:chat";

/// Prefix of a batch-run scope in the `scope` column.
const LOT_SCOPE_PREFIX: &str = "lot:";

/// Action family a standing consent covers.
///
/// The names are those of the tool descriptors' families, so a descriptor's
/// family converts to this one by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentFamily {
    /// Reads non-personal data.
    Read,
    /// Reads personal data held by a connected account.
    ReadPersonal,
    /// Creates, modifies or deletes data.
    WriteOrDelete,
    /// Sends something outside the machine.
    SendExternal,
    /// Executes code. Never granted for always.
    ExecuteCode,
}

impl ConsentFamily {
    /// Textual representation stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            ConsentFamily::Read => "read",
            ConsentFamily::ReadPersonal => "read_personal",
            ConsentFamily::WriteOrDelete => "write_or_delete",
            ConsentFamily::SendExternal => "send_external",
            ConsentFamily::ExecuteCode => "execute_code",
        }
    }

    /// Parses a stored family name.
    ///
    /// # Errors
    ///
    /// [`PermissionError::InvalidConsent`] for a name outside the five families.
    pub fn parse(name: &str) -> Result<Self, PermissionError> {
        match name {
            "read" => Ok(ConsentFamily::Read),
            "read_personal" => Ok(ConsentFamily::ReadPersonal),
            "write_or_delete" => Ok(ConsentFamily::WriteOrDelete),
            "send_external" => Ok(ConsentFamily::SendExternal),
            "execute_code" => Ok(ConsentFamily::ExecuteCode),
            other => Err(PermissionError::InvalidConsent(format!(
                "unknown family '{other}'"
            ))),
        }
    }
}

/// How long a standing consent holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentScope {
    /// Until the user withdraws it.
    Permanent,
    /// For one batch run, identified by its id.
    Lot(String),
}

impl ConsentScope {
    /// Textual representation stored in the database.
    pub fn to_db(&self) -> String {
        match self {
            ConsentScope::Permanent => "permanent".to_string(),
            ConsentScope::Lot(id) => format!("{LOT_SCOPE_PREFIX}{id}"),
        }
    }

    /// Parses a stored scope.
    ///
    /// # Errors
    ///
    /// [`PermissionError::InvalidConsent`] for anything but `permanent` or a
    /// non-empty `lot:<id>`.
    pub fn from_db(value: &str) -> Result<Self, PermissionError> {
        if value == "permanent" {
            return Ok(ConsentScope::Permanent);
        }
        match value.strip_prefix(LOT_SCOPE_PREFIX) {
            Some(id) if !id.trim().is_empty() => Ok(ConsentScope::Lot(id.to_string())),
            _ => Err(PermissionError::InvalidConsent(format!(
                "unknown scope '{value}', expected 'permanent' or 'lot:<id>'"
            ))),
        }
    }

    fn validate(&self) -> Result<(), PermissionError> {
        match self {
            ConsentScope::Lot(id) if id.trim().is_empty() => Err(PermissionError::InvalidConsent(
                "a lot scope needs a non-empty id".to_string(),
            )),
            _ => Ok(()),
        }
    }
}

/// One standing consent, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyConsent {
    /// Row identifier.
    pub id: i64,
    /// Agent the consent is given to.
    pub agent_id: String,
    /// Family it covers.
    pub family: ConsentFamily,
    /// How long it holds.
    pub scope: ConsentScope,
    /// When it was given (Unix epoch, seconds).
    pub granted_at: i64,
    /// Who gave it (a user, `upgrade`, a batch run).
    pub granted_by: String,
}

/// Store of standing consents in `governance.db`.
pub struct FamilyConsentStore {
    db: Connection,
}

impl FamilyConsentStore {
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

    /// Gives `agent_id` a standing consent for `family`, within `scope`.
    ///
    /// Returns `true` when the consent is new, `false` when it already existed.
    /// A new consent for a batch run leaves its event in the journal outbox.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::CodeFamilyRefused`] for [`ConsentFamily::ExecuteCode`].
    /// - [`PermissionError::InvalidConsent`] for an empty agent or lot id.
    /// - [`PermissionError::Database`] on SQLite error.
    pub fn grant(
        &mut self,
        agent_id: &str,
        family: ConsentFamily,
        scope: &ConsentScope,
        granted_by: &str,
    ) -> Result<bool, PermissionError> {
        let tx = self.db.transaction()?;
        let created = insert_consent(
            &tx,
            &NewConsent {
                agent_id,
                family,
                scope,
                granted_by,
                granted_at: crate::prefix_rule_engine::unix_now(),
            },
        )?;
        if let (true, ConsentScope::Lot(batch_id)) = (created, scope) {
            enqueue(
                &tx,
                &StoreEvent::BatchAccessGranted {
                    agent_id: agent_id.to_string(),
                    batch_id: batch_id.clone(),
                    family,
                    granted_by: granted_by.to_string(),
                },
            )?;
        }
        tx.commit()?;
        Ok(created)
    }

    /// Withdraws one standing consent. Returns `true` when a row was removed.
    /// Withdrawing a permanent one leaves its event in the journal outbox.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error.
    pub fn revoke(
        &mut self,
        agent_id: &str,
        family: ConsentFamily,
        scope: &ConsentScope,
    ) -> Result<bool, PermissionError> {
        let tx = self.db.transaction()?;
        let affected = tx.execute(
            "DELETE FROM family_consents WHERE agent_id = ? AND family = ? AND scope = ?",
            params![agent_id, family.as_str(), scope.to_db()],
        )?;
        if affected > 0 && *scope == ConsentScope::Permanent {
            enqueue(
                &tx,
                &StoreEvent::AlwaysWithdrawn {
                    agent_id: agent_id.to_string(),
                    family,
                },
            )?;
        }
        tx.commit()?;
        Ok(affected > 0)
    }

    /// Withdraws every consent given for the batch run `lot_id`, whatever the
    /// agent and the family. Returns the number of rows removed.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error.
    pub fn revoke_lot(&mut self, lot_id: &str) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM family_consents WHERE scope = ?",
            params![ConsentScope::Lot(lot_id.to_string()).to_db()],
        )?;
        Ok(affected as u32)
    }

    // TEST-ONLY: the listing of one agent's consents, batch runs included.
    /// Lists the consents of `agent_id`, oldest first.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error, or
    /// [`PermissionError::InvalidConsent`] for a row this binary cannot read.
    pub fn list_for_agent(&self, agent_id: &str) -> Result<Vec<FamilyConsent>, PermissionError> {
        self.consents_where("agent_id = ?1", params![agent_id])
    }

    /// Lists every permanent consent in full, whatever the agent, oldest
    /// first: what the authorizations screen shows and lets the user
    /// withdraw. The consents bound to a batch run are left out.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error, or
    /// [`PermissionError::InvalidConsent`] for a row this binary cannot read.
    pub fn list_permanent_consents(&self) -> Result<Vec<FamilyConsent>, PermissionError> {
        self.consents_where("scope = 'permanent'", params![])
    }

    fn consents_where(
        &self,
        condition: &str,
        values: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<FamilyConsent>, PermissionError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT id, agent_id, family, scope, granted_at, granted_by \
             FROM family_consents WHERE {condition} ORDER BY id ASC"
        ))?;
        let rows = stmt
            .query_map(values, |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, agent_id, family, scope, granted_at, granted_by)| {
                Ok(FamilyConsent {
                    id,
                    agent_id,
                    family: ConsentFamily::parse(&family)?,
                    scope: ConsentScope::from_db(&scope)?,
                    granted_at,
                    granted_by,
                })
            })
            .collect()
    }

    /// Lists every permanent consent, whatever the agent, as (agent, family)
    /// pairs, oldest first. The consents bound to a batch run are left out.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error, or
    /// [`PermissionError::InvalidConsent`] for a row this binary cannot read.
    pub fn list_permanent(&self) -> Result<Vec<(String, ConsentFamily)>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT agent_id, family FROM family_consents \
             WHERE scope = 'permanent' ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(agent_id, family)| Ok((agent_id, ConsentFamily::parse(&family)?)))
            .collect()
    }

    /// Whether `agent_id` holds a standing consent for `family`: a permanent
    /// one, or one for the batch run `lot_id` when the call belongs to a run.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] on SQLite error.
    pub fn is_granted(
        &self,
        agent_id: &str,
        family: ConsentFamily,
        lot_id: Option<&str>,
    ) -> Result<bool, PermissionError> {
        if family == ConsentFamily::ExecuteCode {
            return Ok(false);
        }
        let lot_scope = lot_id.map(|id| ConsentScope::Lot(id.to_string()).to_db());
        let hit: Option<i64> = self
            .db
            .query_row(
                "SELECT id FROM family_consents \
                 WHERE agent_id = ?1 AND family = ?2 \
                   AND (scope = 'permanent' OR (?3 IS NOT NULL AND scope = ?3)) \
                 LIMIT 1",
                params![agent_id, family.as_str(), lot_scope],
                |row| row.get(0),
            )
            .optional()?;
        Ok(hit.is_some())
    }
}

/// A consent about to be written.
pub(crate) struct NewConsent<'a> {
    pub(crate) agent_id: &'a str,
    pub(crate) family: ConsentFamily,
    pub(crate) scope: &'a ConsentScope,
    pub(crate) granted_by: &'a str,
    pub(crate) granted_at: i64,
}

/// Inserts one consent on `conn`; shared by the store and the access upgrade,
/// which writes inside its own transaction.
pub(crate) fn insert_consent(
    conn: &Connection,
    consent: &NewConsent<'_>,
) -> Result<bool, PermissionError> {
    let NewConsent {
        agent_id,
        family,
        scope,
        granted_by,
        granted_at,
    } = *consent;
    if family == ConsentFamily::ExecuteCode {
        return Err(PermissionError::CodeFamilyRefused {
            agent_id: agent_id.to_string(),
        });
    }
    if agent_id.trim().is_empty() {
        return Err(PermissionError::InvalidConsent(
            "agent_id must not be empty".to_string(),
        ));
    }
    scope.validate()?;
    let affected = conn.execute(
        "INSERT OR IGNORE INTO family_consents \
         (agent_id, family, scope, granted_at, granted_by) VALUES (?, ?, ?, ?, ?)",
        params![
            agent_id,
            family.as_str(),
            scope.to_db(),
            granted_at,
            granted_by
        ],
    )?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn store() -> (FamilyConsentStore, NamedTempFile) {
        let file = NamedTempFile::new().expect("tempfile");
        let store = FamilyConsentStore::new(file.path()).expect("store");
        (store, file)
    }

    fn lot(id: &str) -> ConsentScope {
        ConsentScope::Lot(id.to_string())
    }

    #[test]
    fn grant_records_a_consent_once() {
        // GIVEN an empty store
        let (mut store, _f) = store();

        // WHEN the same consent is granted twice
        let first = store
            .grant(
                "a1",
                ConsentFamily::SendExternal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant");
        let second = store
            .grant(
                "a1",
                ConsentFamily::SendExternal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant again");

        // THEN it is stored once, with who gave it
        assert!(first);
        assert!(!second);
        let listed = store.list_for_agent("a1").expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].family, ConsentFamily::SendExternal);
        assert_eq!(listed[0].scope, ConsentScope::Permanent);
        assert_eq!(listed[0].granted_by, "user");
    }

    #[test]
    fn grant_refuses_the_execute_code_family() {
        // GIVEN an empty store
        let (mut store, _f) = store();

        // WHEN a standing consent is asked for executing code
        let err = store
            .grant(
                "a1",
                ConsentFamily::ExecuteCode,
                &ConsentScope::Permanent,
                "user",
            )
            .unwrap_err();

        // THEN the store refuses it with its own error, and stores nothing
        assert!(matches!(
            err,
            PermissionError::CodeFamilyRefused { ref agent_id } if agent_id == "a1"
        ));
        assert!(store.list_for_agent("a1").expect("list").is_empty());
    }

    #[test]
    fn grant_refuses_an_empty_agent_or_lot() {
        // GIVEN an empty store
        let (mut store, _f) = store();

        // WHEN a consent names no agent, or an empty lot
        let no_agent = store.grant(" ", ConsentFamily::Read, &ConsentScope::Permanent, "user");
        let no_lot = store.grant("a1", ConsentFamily::Read, &lot(""), "user");

        // THEN both are refused as invalid
        assert!(matches!(no_agent, Err(PermissionError::InvalidConsent(_))));
        assert!(matches!(no_lot, Err(PermissionError::InvalidConsent(_))));
    }

    #[test]
    fn revoke_removes_only_the_named_consent() {
        // GIVEN two consents of one agent
        let (mut store, _f) = store();
        for family in [ConsentFamily::SendExternal, ConsentFamily::WriteOrDelete] {
            store
                .grant("a1", family, &ConsentScope::Permanent, "user")
                .expect("grant");
        }

        // WHEN one is withdrawn, then withdrawn again
        let removed = store
            .revoke("a1", ConsentFamily::SendExternal, &ConsentScope::Permanent)
            .expect("revoke");
        let again = store
            .revoke("a1", ConsentFamily::SendExternal, &ConsentScope::Permanent)
            .expect("revoke again");

        // THEN the first call removes it, the second finds nothing, the other stays
        assert!(removed);
        assert!(!again);
        let left = store.list_for_agent("a1").expect("list");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].family, ConsentFamily::WriteOrDelete);
    }

    #[test]
    fn list_for_agent_returns_that_agent_only() {
        // GIVEN consents for two agents
        let (mut store, _f) = store();
        store
            .grant("a1", ConsentFamily::Read, &ConsentScope::Permanent, "user")
            .expect("grant");
        store
            .grant("a2", ConsentFamily::ReadPersonal, &lot("run-7"), "run-7")
            .expect("grant");

        // WHEN each agent's consents are listed
        let a1 = store.list_for_agent("a1").expect("list a1");
        let a2 = store.list_for_agent("a2").expect("list a2");

        // THEN each sees its own, with its scope
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].agent_id, "a1");
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0].scope, lot("run-7"));
    }

    #[test]
    fn is_granted_tests_agent_family_and_optional_lot() {
        // GIVEN a permanent consent and a consent for one batch run
        let (mut store, _f) = store();
        store
            .grant(
                "a1",
                ConsentFamily::SendExternal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant");
        store
            .grant("a1", ConsentFamily::WriteOrDelete, &lot("run-1"), "run-1")
            .expect("grant");

        // WHEN the store is asked about several calls
        // THEN the permanent consent holds with or without a run
        assert!(store
            .is_granted("a1", ConsentFamily::SendExternal, None)
            .expect("q"));
        assert!(store
            .is_granted("a1", ConsentFamily::SendExternal, Some("run-9"))
            .expect("q"));
        // AND the run consent holds in its run only
        assert!(store
            .is_granted("a1", ConsentFamily::WriteOrDelete, Some("run-1"))
            .expect("q"));
        assert!(!store
            .is_granted("a1", ConsentFamily::WriteOrDelete, Some("run-2"))
            .expect("q"));
        assert!(!store
            .is_granted("a1", ConsentFamily::WriteOrDelete, None)
            .expect("q"));
        // AND nothing holds for another agent or another family
        assert!(!store
            .is_granted("a2", ConsentFamily::SendExternal, None)
            .expect("q"));
        assert!(!store
            .is_granted("a1", ConsentFamily::ReadPersonal, None)
            .expect("q"));
    }

    #[test]
    fn is_granted_never_holds_for_execute_code() {
        // GIVEN a row for the execute_code family written behind the store's back
        let (store, f) = store();
        let conn = Connection::open(f.path()).expect("open");
        conn.execute(
            "INSERT INTO family_consents (agent_id, family, scope, granted_at, granted_by)
             VALUES ('a1', 'execute_code', 'permanent', 1, 'user')",
            [],
        )
        .expect("raw insert");

        // WHEN the store is asked whether the agent may execute code
        let granted = store
            .is_granted("a1", ConsentFamily::ExecuteCode, None)
            .expect("q");

        // THEN it says no
        assert!(!granted);
    }

    #[test]
    fn revoke_lot_withdraws_the_whole_run_in_one_call() {
        // GIVEN consents of two agents for one run, one for another run, and a
        // permanent one
        let (mut store, _f) = store();
        store
            .grant("a1", ConsentFamily::SendExternal, &lot("run-1"), "run-1")
            .expect("grant");
        store
            .grant("a2", ConsentFamily::WriteOrDelete, &lot("run-1"), "run-1")
            .expect("grant");
        store
            .grant("a1", ConsentFamily::WriteOrDelete, &lot("run-2"), "run-2")
            .expect("grant");
        store
            .grant(
                "a1",
                ConsentFamily::ReadPersonal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant");

        // WHEN the run ends
        let removed = store.revoke_lot("run-1").expect("revoke lot");

        // THEN its two consents are gone and the others stay
        assert_eq!(removed, 2);
        assert!(store.list_for_agent("a2").expect("list").is_empty());
        let a1: Vec<ConsentScope> = store
            .list_for_agent("a1")
            .expect("list")
            .into_iter()
            .map(|c| c.scope)
            .collect();
        assert_eq!(a1, vec![lot("run-2"), ConsentScope::Permanent]);
    }

    #[test]
    fn family_and_scope_round_trip_through_their_stored_form() {
        // GIVEN every family and both kinds of scope
        let families = [
            ConsentFamily::Read,
            ConsentFamily::ReadPersonal,
            ConsentFamily::WriteOrDelete,
            ConsentFamily::SendExternal,
            ConsentFamily::ExecuteCode,
        ];
        // WHEN each is written and read back
        // THEN it comes back unchanged, and an unknown value is refused
        for f in families {
            assert_eq!(ConsentFamily::parse(f.as_str()).expect("parse"), f);
        }
        for s in [ConsentScope::Permanent, lot("x")] {
            assert_eq!(ConsentScope::from_db(&s.to_db()).expect("parse"), s);
        }
        assert!(ConsentFamily::parse("unclassified").is_err());
        assert!(ConsentScope::from_db("session").is_err());
        assert!(ConsentScope::from_db("lot:").is_err());
    }

    #[test]
    fn list_permanent_consents_gives_every_agent_and_leaves_batch_runs_out() {
        // GIVEN two agents with a permanent consent each, and one bound to a
        // batch run
        let (mut store, _f) = store();
        store
            .grant(
                "a1",
                ConsentFamily::SendExternal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant");
        store
            .grant(
                "a2",
                ConsentFamily::WriteOrDelete,
                &ConsentScope::Permanent,
                "upgrade",
            )
            .expect("grant");
        store
            .grant("a1", ConsentFamily::WriteOrDelete, &lot("nightly"), "batch")
            .expect("grant");

        // WHEN the permanent consents are listed in full
        let listed = store.list_permanent_consents().expect("list");

        // THEN both agents are there with who gave the consent, oldest first,
        // and the batch run's consent is not
        let seen: Vec<(&str, ConsentFamily, &str)> = listed
            .iter()
            .map(|c| (c.agent_id.as_str(), c.family, c.granted_by.as_str()))
            .collect();
        assert_eq!(
            seen,
            vec![
                ("a1", ConsentFamily::SendExternal, "user"),
                ("a2", ConsentFamily::WriteOrDelete, "upgrade"),
            ]
        );
        assert!(listed.iter().all(|c| c.scope == ConsentScope::Permanent));
    }
}
