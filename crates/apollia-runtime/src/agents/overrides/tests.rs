use std::path::Path;
use std::sync::Mutex;

use apollia_tools::{AgentOrigin, StoredAgent};
use serde_json::json;

use super::*;
use crate::governance::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, InstallSubject,
};

/// A governance whose journal is a list in memory.
#[derive(Default)]
struct RecordingGovernance {
    records: Mutex<Vec<GovernanceEvent>>,
}

impl RecordingGovernance {
    fn records(&self) -> Vec<GovernanceEvent> {
        self.records.lock().unwrap().clone()
    }
}

impl GovernanceContract for RecordingGovernance {
    fn effective_accesses(&self, _: &str, _: &[DeclaredAccess]) -> Vec<EffectiveAccess> {
        Vec::new()
    }

    fn decide(&self, _: &AccessCall<'_>) -> AccessDecision {
        AccessDecision::Allowed {
            reason: crate::governance::DecisionReason::NoConsentNeeded,
        }
    }

    fn signature_required(&self, _: &InstallSubject<'_>) -> bool {
        false
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        self.records.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn repo() -> AgentRepository {
    let repo = AgentRepository::open(Path::new(":memory:")).unwrap();
    repo.save_definition(&StoredAgent {
        name: "invoices".into(),
        origin: AgentOrigin::Package,
        package: None,
        version: "1.0.0".into(),
        definition: json!({
            "id": "invoices",
            "name": "Invoices",
            "mission": "Extract the totals.",
            "defaults": {"access": ["file_read", "file_write"]}
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    })
    .unwrap();
    repo
}

fn writer<'a>(repo: &'a AgentRepository, journal: &'a RecordingGovernance) -> OverrideWriter<'a> {
    OverrideWriter {
        repo,
        governance: journal,
        actor: "user",
    }
}

fn override_changed(field: &str, change: OverrideChange) -> GovernanceEvent {
    GovernanceEvent::OverrideChanged {
        agent_id: "invoices".into(),
        field: field.into(),
        change,
        actor: "user".into(),
    }
}

fn access_changed(access: &str, change: AccessChange) -> GovernanceEvent {
    GovernanceEvent::AccessChanged {
        agent_id: "invoices".into(),
        access_id: access.into(),
        change,
        actor: "user".into(),
        reason: None,
    }
}

// GIVEN an agent and a recording journal
// WHEN its mission is overridden
// THEN one override record is written, and no access record
#[test]
fn test_set_mission_is_recorded() {
    let repo = repo();
    let journal = RecordingGovernance::default();

    writer(&repo, &journal)
        .set("invoices", OverrideField::Mission, &json!("Mine."))
        .unwrap();

    assert_eq!(
        journal.records(),
        vec![override_changed("mission", OverrideChange::Set)]
    );
}

// GIVEN an agent granted file_read and file_write by its package
// WHEN the user sets its access to file_read and web_fetch
// THEN the override is recorded, then web_fetch granted and file_write revoked
#[test]
fn test_set_access_records_what_was_granted_and_revoked() {
    let repo = repo();
    let journal = RecordingGovernance::default();

    writer(&repo, &journal)
        .set(
            "invoices",
            OverrideField::Access,
            &json!(["file_read", "web_fetch"]),
        )
        .unwrap();

    assert_eq!(
        journal.records(),
        vec![
            override_changed("access", OverrideChange::Set),
            access_changed("web_fetch", AccessChange::Granted),
            access_changed("file_write", AccessChange::Revoked),
        ]
    );
}

// GIVEN an overridden access
// WHEN the override is removed
// THEN the removal is recorded with the access given back and taken away
#[test]
fn test_remove_override_is_recorded() {
    let repo = repo();
    let journal = RecordingGovernance::default();
    repo.set_override("invoices", OverrideField::Access, &json!(["web_fetch"]))
        .unwrap();

    let removed = writer(&repo, &journal)
        .remove("invoices", OverrideField::Access)
        .unwrap();

    assert!(removed);
    assert_eq!(
        journal.records(),
        vec![
            override_changed("access", OverrideChange::Removed),
            access_changed("file_read", AccessChange::Granted),
            access_changed("file_write", AccessChange::Granted),
            access_changed("web_fetch", AccessChange::Revoked),
        ]
    );
}

// GIVEN a field that is not overridden
// WHEN its override is removed
// THEN nothing changes and nothing is recorded
#[test]
fn test_remove_absent_override_records_nothing() {
    let repo = repo();
    let journal = RecordingGovernance::default();

    let removed = writer(&repo, &journal)
        .remove("invoices", OverrideField::Mission)
        .unwrap();

    assert!(!removed);
    assert!(journal.records().is_empty());
}

// GIVEN an agent with a mission and a name overridden
// WHEN it goes back to defaults
// THEN each removed override is recorded
#[test]
fn test_reset_records_each_removed_override() {
    let repo = repo();
    let journal = RecordingGovernance::default();
    repo.set_override("invoices", OverrideField::Mission, &json!("Mine."))
        .unwrap();
    repo.set_override("invoices", OverrideField::Name, &json!("Bills"))
        .unwrap();

    let fields = writer(&repo, &journal).reset("invoices").unwrap();

    assert_eq!(fields, vec![OverrideField::Mission, OverrideField::Name]);
    assert_eq!(
        journal.records(),
        vec![
            override_changed("mission", OverrideChange::Removed),
            override_changed("name", OverrideChange::Removed),
        ]
    );
}

// GIVEN an unknown agent
// WHEN an override is written
// THEN it is refused and nothing is recorded
#[test]
fn test_unknown_agent_records_nothing() {
    let repo = repo();
    let journal = RecordingGovernance::default();

    let err = writer(&repo, &journal)
        .set("ghost", OverrideField::Mission, &json!("x"))
        .unwrap_err();

    assert!(matches!(
        err,
        AgentOverrideError::Store(AgentRepositoryError::NotFound(_))
    ));
    assert!(journal.records().is_empty());
}
