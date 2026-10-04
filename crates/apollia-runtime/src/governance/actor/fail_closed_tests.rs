//! The actor fails closed: a database it cannot read, or a refused policy,
//! never becomes a permission.

use super::*;
use apollia_permissions::{PermissionScope, PrefixRule, RuleAction};

use crate::governance::contract::DecisionReason;

const AGENT: &str = "invoices";

fn sources(
    dir: &tempfile::TempDir,
    policy: Arc<PolicyVerdict>,
    events: Option<EventBusSender>,
) -> GovernanceSources {
    GovernanceSources {
        governance_db: Some(dir.path().join("governance.db")),
        config_path: None,
        policy,
        journal: None,
        events,
    }
}

fn call(access_id: &str, family: ActionFamily) -> AccessCall<'_> {
    AccessCall {
        agent_id: AGENT,
        access_id,
        argument: None,
        family,
        unattended: false,
        granted_for_run: false,
    }
}

/// Overwrites `governance.db` with bytes that are not a database, and drops
/// its write-ahead files.
fn corrupt_store(dir: &tempfile::TempDir) {
    let db = dir.path().join("governance.db");
    for suffix in ["-wal", "-shm"] {
        let mut side = db.clone().into_os_string();
        side.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(side));
    }
    std::fs::write(&db, vec![0x5a_u8; 4096]).unwrap();
}

/// A governance.db holding a deny rule on a reading tool, a deny rule on a
/// writing tool, a standing consent, and a disabled reading tool.
fn seed_store(dir: &tempfile::TempDir) -> (i64, i64) {
    let db = dir.path().join("governance.db");
    apollia_tools::GovernanceDb::open(dir.path()).unwrap();
    let mut rules = PrefixRuleEngine::new(&db).unwrap();
    let mut deny = |tool: &str| {
        rules
            .add_rule(&PrefixRule {
                tool_name: tool.into(),
                action: RuleAction::Deny,
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .unwrap()
    };
    let read_rule = deny("file_read");
    let write_rule = deny("file_delete");
    FamilyConsentStore::new(&db)
        .unwrap()
        .grant(
            AGENT,
            ConsentFamily::WriteOrDelete,
            &ConsentScope::Permanent,
            "test",
        )
        .unwrap();
    apollia_tools::NativeToolRegistry::new(&db)
        .unwrap()
        .set_enabled("file_grep", false)
        .unwrap();
    (read_rule, write_rule)
}

fn refused_fail_closed(decision: &AccessDecision) -> bool {
    *decision
        == AccessDecision::Denied {
            reason: DecisionReason::FailClosed,
        }
}

// A database that stops being readable keeps its last state in force: a deny
// rule still refuses, never asks; a disabled tool stays disabled; the consent
// no longer opens a sensitive call; reading continues.
#[tokio::test]
async fn test_unreadable_store_keeps_the_last_state_and_refuses() {
    // GIVEN the actor started over a readable governance.db
    let dir = tempfile::tempdir().unwrap();
    let (read_rule, write_rule) = seed_store(&dir);
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent), None));
    assert!(governance
        .decide(&call("file_write", ActionFamily::WriteOrDelete))
        .is_allowed());

    // WHEN the database becomes unreadable and the actor reads it again
    corrupt_store(&dir);
    governance.refresh().await;
    let state = governance.state();

    // THEN governance fails closed for that reason
    assert!(
        matches!(state.fail_closed, Some(FailClosed::StoreUnreadable { .. })),
        "{:?}",
        state.fail_closed
    );
    // AND the deny rules still refuse, the reading one included
    assert_eq!(
        governance.decide(&call("file_delete", ActionFamily::WriteOrDelete)),
        AccessDecision::Denied {
            reason: DecisionReason::Rule {
                rule_id: write_rule
            }
        }
    );
    assert_eq!(
        governance.decide(&call("file_read", ActionFamily::Read)),
        AccessDecision::Denied {
            reason: DecisionReason::Rule { rule_id: read_rule }
        }
    );
    // AND the disabled tool stays disabled
    assert_eq!(state.disabled_tools, vec!["file_grep".to_owned()]);
    // AND a sensitive call the consent covered is refused, reading runs
    assert!(refused_fail_closed(
        &governance.decide(&call("file_write", ActionFamily::WriteOrDelete))
    ));
    assert!(governance
        .decide(&call("file_list", ActionFamily::Read))
        .is_allowed());
}

// A database unreadable at startup has no state to keep: every sensitive
// family and an unclassified tool are refused, never asked; reading continues.
#[tokio::test]
async fn test_unreadable_store_at_startup_refuses_every_sensitive_family() {
    // GIVEN a governance.db that is not a database
    let dir = tempfile::tempdir().unwrap();
    corrupt_store(&dir);

    // WHEN the actor starts
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent), None));

    // THEN each sensitive family is refused for that reason
    for family in [
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
    ] {
        let decision = governance.decide(&call("some_tool", family));
        assert!(refused_fail_closed(&decision), "{family:?}: {decision:?}");
    }
    // AND reading runs
    assert!(governance
        .decide(&call("file_read", ActionFamily::Read))
        .is_allowed());
}

// A refused policy puts the actor in the same state: the four sensitive
// families refused, reading continues, whatever the database grants.
#[tokio::test]
async fn test_invalid_policy_refuses_every_sensitive_family() {
    // GIVEN a readable governance.db with a standing consent, and a policy
    // refused for its owner
    let dir = tempfile::tempdir().unwrap();
    seed_store(&dir);
    let refused = Arc::new(PolicyVerdict::Invalid(apollia_permissions::PolicyInvalid {
        reason: apollia_permissions::PolicyInvalidReason::Owner,
        detail: "the folder can be modified by another user".into(),
    }));

    // WHEN the actor starts
    let governance = GovernanceHandle::spawn(sources(&dir, refused, None));

    // THEN it fails closed naming the policy
    let state = governance.state();
    assert!(
        matches!(&state.fail_closed, Some(FailClosed::PolicyInvalid { reason, .. }) if reason == "owner"),
        "{:?}",
        state.fail_closed
    );
    // AND every sensitive family is refused, the consented one included
    for family in [
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
    ] {
        let decision = governance.decide(&call("file_write", family));
        assert!(refused_fail_closed(&decision), "{family:?}: {decision:?}");
    }
    // AND reading runs
    assert!(governance
        .decide(&call("file_list", ActionFamily::Read))
        .is_allowed());
}

// Entering and leaving fail-closed are each announced once on the event bus.
#[tokio::test]
async fn test_fail_closed_changes_are_announced() {
    // GIVEN the actor over a readable governance.db, listening to the bus
    let dir = tempfile::tempdir().unwrap();
    seed_store(&dir);
    let (events, mut received) = tokio::sync::broadcast::channel(16);
    let governance =
        GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent), Some(events)));

    // WHEN the database becomes unreadable, then disappears
    corrupt_store(&dir);
    governance.refresh().await;
    std::fs::remove_file(dir.path().join("governance.db")).unwrap();
    governance.refresh().await;

    // THEN one event says it fails closed and why, the next that it recovered
    let announced: Vec<RuntimeEvent> = std::iter::from_fn(|| received.try_recv().ok())
        .filter(|event| matches!(event, RuntimeEvent::GovernanceFailClosed { .. }))
        .collect();
    assert_eq!(announced.len(), 2, "{announced:?}");
    assert!(matches!(
        &announced[0],
        RuntimeEvent::GovernanceFailClosed { active: true, cause: Some(cause), detail: Some(_) }
            if cause == "store_unreadable"
    ));
    assert!(matches!(
        &announced[1],
        RuntimeEvent::GovernanceFailClosed {
            active: false,
            cause: None,
            detail: None
        }
    ));
    assert!(governance.state().fail_closed.is_none());
}
