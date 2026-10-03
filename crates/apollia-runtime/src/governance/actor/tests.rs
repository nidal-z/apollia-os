use super::*;
use apollia_permissions::{PermissionScope, PrefixRule, RuleAction};

use crate::governance::contract::{ConsentAnswer, DecisionReason};

const AGENT: &str = "invoices";

/// A valid policy reading `body` after its header.
fn verdict(body: &str) -> Arc<PolicyVerdict> {
    let text = format!("schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n{body}");
    let policy = apollia_permissions::parse_policy(&text).expect("policy parses");
    Arc::new(PolicyVerdict::Valid(Box::new(
        apollia_permissions::ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "00".into(),
            publisher: "it".into(),
        },
    )))
}

fn sources(dir: &tempfile::TempDir, policy: Arc<PolicyVerdict>) -> GovernanceSources {
    GovernanceSources {
        governance_db: Some(dir.path().join("governance.db")),
        config_path: Some(dir.path().join("apollia.toml")),
        policy,
        journal: None,
        forbidden_connections: Default::default(),
    }
}

fn write_call(access_id: &str) -> AccessCall<'_> {
    AccessCall {
        agent_id: AGENT,
        access_id,
        argument: None,
        family: ActionFamily::WriteOrDelete,
        unattended: false,
        granted_for_run: false,
    }
}

// The actor reads the fine rules and the standing consents of governance.db
// when it starts.
#[tokio::test]
async fn test_spawn_reads_rules_and_consents() {
    // GIVEN a governance.db holding a deny rule and a standing consent
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("governance.db");
    let rule_id = PrefixRuleEngine::new(&db)
        .unwrap()
        .add_rule(&PrefixRule {
            tool_name: "file_delete".into(),
            action: RuleAction::Deny,
            scope: PermissionScope::Global,
            ..PrefixRule::default()
        })
        .unwrap();
    FamilyConsentStore::new(&db)
        .unwrap()
        .grant(
            AGENT,
            ConsentFamily::WriteOrDelete,
            &ConsentScope::Permanent,
            "test",
        )
        .unwrap();

    // WHEN the actor starts and two writing calls are decided
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    let deleted = governance.decide(&write_call("file_delete"));
    let written = governance.decide(&write_call("file_write"));

    // THEN the rule refuses one and the consent lets the other run
    assert_eq!(
        deleted,
        AccessDecision::Denied {
            reason: DecisionReason::Rule { rule_id }
        }
    );
    assert_eq!(
        written,
        AccessDecision::Allowed {
            reason: DecisionReason::Consent
        }
    );
}

// "Always" is written for the agent and the family, and the next decision
// reads it; no rule is written by tool name.
#[tokio::test]
async fn test_grant_always_writes_family_consent_not_rule() {
    // GIVEN the actor over an empty governance.db, and a writing call that asks
    let dir = tempfile::tempdir().unwrap();
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    let before = governance.decide(&write_call("file_write"));
    assert!(before.answers().contains(&ConsentAnswer::Always));

    // WHEN "always" is answered for that call
    let new = governance
        .grant_always(AGENT, ActionFamily::WriteOrDelete)
        .await
        .unwrap();

    // THEN the consent is new, another tool of the family runs at once
    assert!(new);
    assert!(governance.decide(&write_call("file_edit")).is_allowed());
    // AND the store holds the consent for the family, and no rule at all
    let db = dir.path().join("governance.db");
    let consents = FamilyConsentStore::new(&db)
        .unwrap()
        .list_permanent()
        .unwrap();
    assert_eq!(
        consents,
        vec![(AGENT.to_owned(), ConsentFamily::WriteOrDelete)]
    );
    assert!(PrefixRuleEngine::new(&db)
        .unwrap()
        .list_rules()
        .unwrap()
        .is_empty());
}

// No standing consent is written for code, for a tool nobody classified, or
// for reading.
#[tokio::test]
async fn test_grant_always_refuses_code_unclassified_and_read() {
    // GIVEN the actor over an empty governance.db
    let dir = tempfile::tempdir().unwrap();
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));

    for family in [
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
        ActionFamily::Read,
    ] {
        // WHEN "always" is asked for the family
        let written = governance.grant_always(AGENT, family).await;
        // THEN it is refused
        assert!(
            matches!(written, Err(GovernanceWriteError::NotConsentable(f)) if f == family),
            "{family:?}: {written:?}"
        );
    }
}

// A consent written by another process is read by the actor on its own.
#[tokio::test]
async fn test_external_write_is_picked_up() {
    // GIVEN the actor running over governance.db
    let dir = tempfile::tempdir().unwrap();
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    assert!(!governance.decide(&write_call("file_write")).is_allowed());

    // WHEN a consent is written to the database behind its back
    FamilyConsentStore::new(&dir.path().join("governance.db"))
        .unwrap()
        .grant(
            AGENT,
            ConsentFamily::WriteOrDelete,
            &ConsentScope::Permanent,
            "test",
        )
        .unwrap();

    // THEN a decision reads it within a few refresh periods
    let allowed = crate::test_support::poll_until(Duration::from_secs(5), || {
        governance.decide(&write_call("file_write")).is_allowed()
    })
    .await;
    assert!(
        allowed,
        "the actor never read the consent written outside it"
    );
}

// The developer mode is read from apollia.toml, and the policy fixing it wins.
#[tokio::test]
async fn test_developer_mode_reads_config_under_policy() {
    // GIVEN an apollia.toml turning the developer mode on
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("apollia.toml"),
        "[desktop]\ndeveloper_mode = true\n",
    )
    .unwrap();

    // WHEN the state is read without a policy, then under a policy fixing it off
    let free = load_state(&sources(&dir, Arc::new(PolicyVerdict::Absent)));
    let fixed = load_state(&sources(
        &dir,
        verdict("[settings]\n\"desktop.developer_mode\" = false\n"),
    ));

    // THEN the first is in developer mode and the second is not
    assert!(free.developer_mode);
    assert!(!fixed.developer_mode);
}

// The policy's refusals and its signature requirement reach the state.
#[tokio::test]
async fn test_policy_reaches_the_state() {
    // GIVEN a policy refusing a family and a connection and requiring signatures
    let dir = tempfile::tempdir().unwrap();
    let policy = verdict(
        "[connections]\ndeny = [\"mcp:files\"]\n[families]\ndeny = [\"execute_code\"]\n\
         [packages]\nrequire_signature = true\n",
    );

    // WHEN the actor starts
    let governance = GovernanceHandle::spawn(sources(&dir, policy));

    // THEN a tool of the refused connection is refused by the policy
    let decision = governance.decide(&write_call("mcp:files/write_file"));
    assert!(matches!(
        decision,
        AccessDecision::Denied {
            reason: DecisionReason::Policy { .. }
        }
    ));
    // AND a signature is required to install
    assert!(governance.signature_required(&InstallSubject {
        package_id: "crm",
        publisher: None,
    }));
}

// Without an audit journal a record is refused, never dropped.
#[test]
fn test_record_without_journal_is_refused() {
    // GIVEN a governance with no journal
    let governance = GovernanceHandle::detached(GovernanceState::default(), None);

    // WHEN a change of access is recorded
    let recorded = governance.record(&GovernanceEvent::Uninstalled {
        agent_id: AGENT.into(),
        version: "1.0.0".into(),
        memory_cleared: false,
        history_cleared: false,
        actor: "user".into(),
    });

    // THEN the record is refused
    assert!(matches!(recorded, Err(GovernanceError::JournalUnavailable)));
    assert!(!governance.journaled());
}
