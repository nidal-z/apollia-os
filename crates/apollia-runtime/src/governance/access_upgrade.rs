//! The access upgrade run at startup, and its journal records.
//!
//! [`apollia_permissions::run_access_upgrade`] converts, once, the rules
//! persisted before standing consents existed. It cannot see the tool
//! descriptors, so this module hands it the family each descriptor states, and
//! records each conversion and removal it reports as a change of access in the
//! audit journal, with [`ACCESS_UPGRADE_REASON`] as the reason.

use std::collections::HashMap;
use std::path::Path;

use apollia_permissions::{
    run_access_upgrade, AccessUpgradeReport, ConsentFamily, PermissionError, UpgradeChange,
};

use crate::governance::contract::{GovernanceContract, GovernanceError};
use crate::governance::record::{AccessChange, GovernanceEvent};

/// Reason recorded on every change of access the upgrade makes.
pub const ACCESS_UPGRADE_REASON: &str = "upgrade to 0.3.0";

/// Actor recorded on every change of access the upgrade makes.
pub const ACCESS_UPGRADE_ACTOR: &str = "upgrade";

/// Errors of the startup access upgrade.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AccessUpgradeError {
    /// The governance database could not be upgraded; nothing was changed.
    #[error("access upgrade failed: {0}")]
    Store(#[from] PermissionError),
    /// A change could not be recorded in the journal.
    #[error("access upgrade not journaled: {0}")]
    Journal(#[from] GovernanceError),
}

/// Families of every tool whose descriptor is known without a running server:
/// the native tools, the connector operations, and the MCP resource tools.
///
/// An MCP server tool is absent: its descriptor exists only while its server
/// runs, and it is published unclassified, so no standing consent comes out
/// of a rule on it.
pub fn descriptor_families() -> HashMap<String, ConsentFamily> {
    let mut descriptors = apollia_tools::tools::native_descriptors();
    descriptors.extend(crate::connectors_bridge::google_tool_descriptors());
    descriptors.extend(crate::connectors_bridge::microsoft_tool_descriptors());
    descriptors.extend(crate::connectors_bridge::imap_tool_descriptors());
    descriptors.extend(apollia_mcp::mcp_resources::mcp_resource_descriptors());
    descriptors
        .into_iter()
        .filter_map(|d| d.family.consent_family().map(|f| (d.name, f)))
        .collect()
}

/// Turns the upgrade report into journal events.
pub fn upgrade_events(report: &AccessUpgradeReport) -> Vec<GovernanceEvent> {
    report
        .entries
        .iter()
        .map(|entry| GovernanceEvent::AccessChanged {
            agent_id: entry.agent_id.clone(),
            access_id: entry.access_id.clone(),
            change: match entry.change {
                UpgradeChange::Granted => AccessChange::Granted,
                UpgradeChange::Revoked => AccessChange::Revoked,
                UpgradeChange::Rescoped => AccessChange::Rescoped,
            },
            actor: ACCESS_UPGRADE_ACTOR.to_string(),
            reason: Some(ACCESS_UPGRADE_REASON.to_string()),
        })
        .collect()
}

/// Runs the access upgrade on `governance_db` and records every change it
/// made through `governance`. Returns the number of changes recorded; zero
/// when the upgrade had already run.
///
/// # Errors
///
/// [`AccessUpgradeError::Store`] when the database cannot be upgraded (it is
/// then left untouched), [`AccessUpgradeError::Journal`] when a record fails.
pub fn upgrade_and_journal(
    governance_db: &Path,
    governance: &dyn GovernanceContract,
) -> Result<usize, AccessUpgradeError> {
    let families = descriptor_families();
    let report = run_access_upgrade(governance_db, &|tool| families.get(tool).copied())?;
    let events = upgrade_events(&report);
    for event in &events {
        governance.record(event)?;
    }
    Ok(events.len())
}

/// Startup step: runs the upgrade on the instance `governance.db` and
/// journals it, or defers it while the journal is unavailable, so a change of
/// access is never made without its record.
pub(crate) fn run_at_startup(data_dir: &Path, governance: &crate::governance::GovernanceHandle) {
    if !governance.journaled() {
        tracing::event!(
            tracing::Level::WARN,
            detail = "audit journal unavailable, upgrade deferred to the next start",
            "governance.access_upgrade.deferred"
        );
        return;
    }
    let db = data_dir.join(apollia_core::paths::DataFile::Governance.file_name());
    match upgrade_and_journal(&db, governance) {
        Ok(changes) => tracing::event!(
            tracing::Level::INFO,
            changes,
            "governance.access_upgrade.done"
        ),
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "governance.access_upgrade.failed"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::verify::verify_entries;
    use crate::audit_journal::{AuditJournalHandle, HmacSigner, JournalEntryKind};
    use crate::governance::record::governance_run_id;
    use crate::governance::NoPolicyGovernance;
    use rusqlite::Connection;

    const KEY: &[u8] = b"access-upgrade-journal-key-01";

    /// A `governance.db` at schema version 1 holding one rule of each kind the
    /// upgrade converts or removes.
    fn legacy_governance_db(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("governance.db");
        let conn = Connection::open(&path).expect("open");
        (apollia_permissions::GOVERNANCE_MIGRATIONS[0])(&conn).expect("v1");
        conn.pragma_update(None, "user_version", 1)
            .expect("stamp v1");
        conn.execute_batch(
            "INSERT INTO permission_rules (tool_name, action, created_at, scope, agent_id)
                 VALUES ('http_fetch', 'allow', 1, 'agent', 'a1');
             INSERT INTO permission_rules (tool_name, action, created_at, scope)
                 VALUES ('bash_executor', 'allow', 1, 'global');
             INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, scope, project_path)
                 VALUES ('file_edit', '/etc/', 'deny', 1, 'project', '/home/u/p');
             UPDATE chat_libre_config SET allowed_tools = '[\"file_write\"]' WHERE id = 1;",
        )
        .expect("legacy rows");
        path
    }

    #[test]
    fn descriptor_families_cover_native_tools_and_skip_the_unclassified() {
        // GIVEN the descriptors known at startup
        // WHEN their families are collected
        let families = descriptor_families();
        // THEN native tools carry the family their descriptor states, and the
        // exempt ask_user carries none
        assert_eq!(
            families.get("http_fetch"),
            Some(&ConsentFamily::SendExternal)
        );
        assert_eq!(
            families.get("file_write"),
            Some(&ConsentFamily::WriteOrDelete)
        );
        assert_eq!(
            families.get("bash_executor"),
            Some(&ConsentFamily::ExecuteCode)
        );
        assert_eq!(families.get("ask_user"), None);
    }

    #[tokio::test]
    async fn the_upgrade_is_journaled_as_access_changes_and_the_chain_verifies() {
        // GIVEN a legacy governance database and a signed journal
        let dir = tempfile::tempdir().expect("tempdir");
        let db = legacy_governance_db(dir.path());
        let journal_path = dir.path().join("audit_journal.db");
        let journal = AuditJournalHandle::open_with_key_bytes(&journal_path, KEY.to_vec())
            .await
            .expect("journal");
        let governance = NoPolicyGovernance::new(journal.clone());

        // WHEN the startup upgrade runs, then runs again
        let recorded = upgrade_and_journal(&db, &governance).expect("upgrade");
        let again = upgrade_and_journal(&db, &governance).expect("second upgrade");

        // THEN each change is an access change with the upgrade reason, and
        // the second run records nothing
        assert_eq!(again, 0);
        let mut entries = Vec::new();
        for run in [
            governance_run_id("a1"),
            governance_run_id(apollia_permissions::ALL_AGENTS),
            governance_run_id(apollia_permissions::APOLLIA_AGENT_ID),
        ] {
            let run_entries = journal.query_run(&run).await;
            // AND every run verifies with the journal's own verification
            let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
            let report = verify_entries(&run, &run_entries, Some(&signer), true);
            assert!(report.ok, "chain of {run} must verify: {report:?}");
            entries.extend(run_entries);
        }
        assert_eq!(entries.len(), recorded);
        assert!(entries
            .iter()
            .all(|e| e.kind == JournalEntryKind::AccessChanged
                && e.payload["reason"] == ACCESS_UPGRADE_REASON
                && e.payload["actor"] == ACCESS_UPGRADE_ACTOR));
        let mut changes: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    e.payload["access_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    e.payload["change"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        changes.sort();
        assert_eq!(
            changes,
            vec![
                ("family:send_external".into(), "granted".into()),
                ("family:write_or_delete".into(), "granted".into()),
                ("rule:bash_executor".into(), "revoked".into()),
                ("rule:file_edit /etc/".into(), "rescoped".into()),
                ("rule:http_fetch".into(), "revoked".into()),
            ]
        );
        // AND the whole journal verifies
        let whole = journal.verify_journal().await.expect("verify journal");
        assert!(whole.ok);
        assert_eq!(whole.entries_checked, recorded as u64);

        journal.shutdown().await;
    }
}
