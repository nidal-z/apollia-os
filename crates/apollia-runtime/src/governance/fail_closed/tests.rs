use super::*;

use crate::governance::decision::GovernanceState;

/// Writes bytes that are not a SQLite database where `governance.db` lives.
fn corrupt_store(dir: &Path) {
    std::fs::write(
        dir.join(apollia_tools::GOVERNANCE_DB_FILENAME),
        vec![0x5a_u8; 4096],
    )
    .unwrap();
}

// Without the actor, a database that cannot be read fails closed and leaves
// only the operator's list switched off.
#[test]
fn test_gate_without_actor_fails_closed_on_unreadable_store() {
    // GIVEN a governance.db that is not a database
    let dir = tempfile::tempdir().unwrap();
    corrupt_store(dir.path());

    // WHEN a run reads its gate
    let gate = tool_gate(None, dir.path(), &["web_search".to_owned()]);

    // THEN it fails closed for that reason, with the operator's list off
    assert!(
        matches!(gate.fail_closed, Some(FailClosed::StoreUnreadable { .. })),
        "{gate:?}"
    );
    assert_eq!(gate.disabled_tools, vec!["web_search".to_owned()]);
}

// Without the actor and without a database, nothing fails: the runtime has
// written nothing yet.
#[test]
fn test_gate_without_actor_and_without_store_is_open() {
    // GIVEN a data directory with no governance.db
    let dir = tempfile::tempdir().unwrap();

    // WHEN a run reads its gate
    let gate = tool_gate(None, dir.path(), &[]);

    // THEN governance decides normally and nothing is switched off
    assert_eq!(gate, ToolGate::default());
}

// With the actor, the gate is the actor's state: the tools it keeps disabled,
// joined to the operator's list, and its fail-closed state.
#[test]
fn test_gate_reads_the_actor_state() {
    // GIVEN an actor failing closed that keeps `file_delete` disabled
    let dir = tempfile::tempdir().unwrap();
    let governance = GovernanceHandle::detached(
        GovernanceState {
            disabled_tools: vec!["file_delete".to_owned()],
            fail_closed: Some(FailClosed::StoreUnreadable {
                detail: "file is not a database".into(),
            }),
            ..GovernanceState::default()
        },
        None,
    );

    // WHEN a run reads its gate, with `web_search` off in the configuration
    let gate = tool_gate(Some(&governance), dir.path(), &["web_search".to_owned()]);

    // THEN both tools are off and the run fails closed
    assert_eq!(
        gate.disabled_tools,
        vec!["file_delete".to_owned(), "web_search".to_owned()]
    );
    assert!(gate.fail_closed.is_some());
}

// A refused policy fails closed with its reason; an absent one does not.
#[test]
fn test_refused_policy_fails_closed_with_its_reason() {
    // GIVEN a policy refused for its signature, and no policy at all
    let refused = PolicyVerdict::Invalid(apollia_permissions::PolicyInvalid {
        reason: apollia_permissions::PolicyInvalidReason::Signature,
        detail: "the signature does not hold".into(),
    });

    // WHEN each is read as a fail-closed state
    let state = FailClosed::from_policy(&refused);
    let free = FailClosed::from_policy(&PolicyVerdict::Absent);

    // THEN the refused one names its cause and reason, the absent one is none
    let state = state.unwrap();
    assert_eq!(state.cause(), "policy_invalid");
    assert!(state.detail().contains("signature"), "{}", state.detail());
    assert_eq!(free, None);
}

// The four sensitive families and a tool nobody classified are refused;
// reading and acting on the conversation continue.
#[test]
fn test_refused_families() {
    // GIVEN every family
    let refused = [
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
    ];
    let open = [ActionFamily::Read, ActionFamily::Exempt];

    // WHEN each is checked THEN only the sensitive ones are refused
    assert!(refused.into_iter().all(refused_when_fail_closed));
    assert!(!open.into_iter().any(refused_when_fail_closed));
}
