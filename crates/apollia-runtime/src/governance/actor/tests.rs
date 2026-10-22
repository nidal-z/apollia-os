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
            policy_digest: "00".into(),
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
        events: None,
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
    let free = load_state(&sources(&dir, Arc::new(PolicyVerdict::Absent)), None);
    let fixed = load_state(
        &sources(
            &dir,
            verdict("[settings]\n\"desktop.developer_mode\" = false\n"),
        ),
        None,
    );

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

fn code_call<'a>(access_id: &'a str, command: &'a str) -> AccessCall<'a> {
    AccessCall {
        agent_id: AGENT,
        access_id,
        argument: Some(command),
        family: ActionFamily::ExecuteCode,
        unattended: false,
        granted_for_run: false,
    }
}

// In developer mode, "always" on a simple command writes a fine allow rule
// for this agent on the whole command, never a grant by tool name, and the
// next identical call runs without asking.
#[tokio::test]
async fn test_allow_command_writes_an_agent_rule_on_the_whole_command() {
    // GIVEN the actor in developer mode over an empty governance.db
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("apollia.toml"),
        "[desktop]\ndeveloper_mode = true\n",
    )
    .unwrap();
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    let command = "git log --oneline -n 20";
    let before = governance.decide(&code_call("bash_executor", command));
    assert!(before.answers().contains(&ConsentAnswer::Always));

    // WHEN "always" is answered for that command
    let rule_id = governance
        .allow_command(AGENT, "bash_executor", command)
        .await
        .unwrap();

    // THEN the same command runs on that rule
    assert_eq!(
        governance.decide(&code_call("bash_executor", command)),
        AccessDecision::Allowed {
            reason: DecisionReason::Rule { rule_id }
        }
    );
    // AND the rule is an allow on the whole command, scoped to this agent
    let rules = PrefixRuleEngine::new(&dir.path().join("governance.db"))
        .unwrap()
        .list_rules()
        .unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].tool_name, "bash_executor");
    assert_eq!(rules[0].arg_prefix.as_deref(), Some(command));
    assert_eq!(rules[0].action, RuleAction::Allow);
    assert_eq!(rules[0].scope, PermissionScope::Agent);
    assert_eq!(rules[0].agent_id.as_deref(), Some(AGENT));
    // AND a chained command starting with it still asks, without "always"
    let chained = governance.decide(&code_call(
        "bash_executor",
        "git log --oneline -n 20; rm -rf ~",
    ));
    assert!(!chained.is_allowed());
    assert!(!chained.answers().contains(&ConsentAnswer::Always));
    // AND another agent is still asked
    let mut other = code_call("bash_executor", command);
    other.agent_id = "someone-else";
    assert!(!governance.decide(&other).is_allowed());
}

// No rule is written for a compound command, outside developer mode, or for a
// tool that does not run code.
#[tokio::test]
async fn test_allow_command_refuses_compound_off_mode_and_other_tools() {
    // GIVEN the actor over an empty governance.db, developer mode off
    let dir = tempfile::tempdir().unwrap();
    let governance = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));

    // WHEN "always" is asked for a simple command with the mode off
    let off = governance.allow_command(AGENT, "bash_executor", "ls").await;
    // AND, the mode turned on, for a compound command and a non-code tool
    std::fs::write(
        dir.path().join("apollia.toml"),
        "[desktop]\ndeveloper_mode = true\n",
    )
    .unwrap();
    governance.refresh().await;
    let compound = governance
        .allow_command(AGENT, "bash_executor", "ls && rm -rf build")
        .await;
    let not_code = governance
        .allow_command(AGENT, "file_write", "notes.txt")
        .await;

    // THEN each is refused and the store holds no rule
    assert!(matches!(off, Err(GovernanceWriteError::NotACommandRule(_))));
    assert!(matches!(
        compound,
        Err(GovernanceWriteError::NotACommandRule(_))
    ));
    assert!(matches!(
        not_code,
        Err(GovernanceWriteError::NotACommandRule(_))
    ));
    let db = dir.path().join("governance.db");
    let rules = if db.exists() {
        PrefixRuleEngine::new(&db).unwrap().list_rules().unwrap()
    } else {
        Vec::new()
    };
    assert!(rules.is_empty());
}

// A pause is written to governance.db and read by the next decision, without
// a restart; a resume lets the tools run again; a new actor over the same
// file reads the pause it left.
#[tokio::test]
async fn test_a_pause_is_live_persisted_and_resumed() {
    // GIVEN a running actor and a reading tool of the Notion server
    let dir = tempfile::tempdir().unwrap();
    let handle = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    let read = AccessCall {
        family: ActionFamily::Read,
        ..write_call("mcp:notion/search")
    };
    assert!(handle.decide(&read).is_allowed());

    // WHEN the server is paused
    assert!(handle.set_paused("mcp:notion", true).await.unwrap());

    // THEN the next call is refused with the pause, and a second pause
    // changes nothing
    assert_eq!(
        handle.decide(&read),
        AccessDecision::Denied {
            reason: DecisionReason::Paused {
                connection_id: "mcp:notion".into()
            }
        }
    );
    assert!(!handle.set_paused("mcp:notion", true).await.unwrap());
    // AND an actor started later over the same file reads the pause
    let later = GovernanceHandle::spawn(sources(&dir, Arc::new(PolicyVerdict::Absent)));
    assert!(later.state().paused.contains("mcp:notion"));

    // WHEN it is resumed
    assert!(handle.set_paused("mcp:notion", false).await.unwrap());

    // THEN the call runs again
    assert!(handle.decide(&read).is_allowed());
}

// A handle with no database refuses to pause.
#[tokio::test]
async fn test_a_detached_handle_refuses_to_pause() {
    // GIVEN a handle with no actor behind it
    let handle = GovernanceHandle::detached(GovernanceState::default(), None);
    // WHEN a connection is paused
    let written = handle.set_paused("mcp:notion", true).await;
    // THEN it is refused
    assert!(matches!(written, Err(GovernanceWriteError::NoDatabase)));
}
