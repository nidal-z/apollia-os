//! The natures governance writes to the journal: chained and signed like the
//! run entries, moved from the outbox of `governance.db`, recorded at startup
//! for the policy, and on entering fail-closed.

use std::sync::Arc;

use apollia_permissions::{
    ConsentFamily, ConsentScope, FamilyConsentStore, PermissionScope, PolicyInvalid,
    PolicyInvalidReason, PrefixRule, PrefixRuleEngine, RuleAction, RuleRecord, ValidPolicy,
};
use apollia_tools::ActionFamily;

use super::*;
use crate::audit_journal::verify::verify_entries;
use crate::audit_journal::{HmacSigner, JournalEntry};
use crate::governance::contract::{ConsentAnswer, GovernanceContract};
use crate::governance::decision::GovernanceState;
use crate::governance::record::{governance_run_id, GLOBAL_RULES_RUN_ID, JOURNAL_EXPORT_RUN_ID};
use crate::governance::{GovernanceHandle, GovernanceSources};

const KEY: &[u8] = b"governance-journal-key-0012";

async fn signed_journal(dir: &tempfile::TempDir) -> AuditJournalHandle {
    AuditJournalHandle::open_with_key_bytes(&dir.path().join("audit_journal.db"), KEY.to_vec())
        .await
        .expect("open signed journal")
}

fn sources(dir: &tempfile::TempDir, journal: Option<AuditJournalHandle>) -> GovernanceSources {
    GovernanceSources {
        governance_db: Some(dir.path().join("governance.db")),
        config_path: None,
        policy: Arc::new(PolicyVerdict::Absent),
        journal,
        events: None,
        forbidden_connections: Default::default(),
    }
}

fn valid(digest: &str) -> PolicyVerdict {
    PolicyVerdict::Valid(Box::new(ValidPolicy {
        policy: apollia_permissions::parse_policy(
            "schema_version = 1\nsigned_at = 2026-01-01T00:00:00Z\n",
        )
        .expect("policy"),
        key_fingerprint: "ab12".into(),
        policy_digest: digest.into(),
        publisher: "acme".into(),
        signed_at: "2026-01-01T00:00:00Z".parse().expect("date"),
    }))
}

fn kinds(entries: &[JournalEntry]) -> Vec<JournalEntryKind> {
    entries.iter().map(|e| e.kind.clone()).collect()
}

fn rule_record() -> RuleRecord {
    RuleRecord {
        rule_id: 7,
        tool_name: "bash_executor".into(),
        arg_prefix: Some("git ".into()),
        action: RuleAction::Allow,
        scope: PermissionScope::Agent,
        agent_id: Some("a1".into()),
        created_by: None,
    }
}

// One entry of each nature lands in the journal, chained and signed, and the
// journal's own verification accepts it.
#[tokio::test]
async fn test_each_nature_is_chained_and_signed() {
    // GIVEN governance over a signed journal, and one event of each nature
    // for the same agent
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = signed_journal(&dir).await;
    let gov = GovernanceHandle::detached(GovernanceState::default(), Some(journal.clone()));
    let agent_events = [
        GovernanceEvent::ConsentAnswered {
            agent_id: "a1".into(),
            access_id: "send_email".into(),
            family: ActionFamily::SendExternal,
            answer: ConsentAnswer::Once,
        },
        GovernanceEvent::ConsentAnswered {
            agent_id: "a1".into(),
            access_id: "send_email".into(),
            family: ActionFamily::SendExternal,
            answer: ConsentAnswer::Always,
        },
        GovernanceEvent::ConsentAnswered {
            agent_id: "a1".into(),
            access_id: "send_email".into(),
            family: ActionFamily::SendExternal,
            answer: ConsentAnswer::Refuse,
        },
        GovernanceEvent::AlwaysWithdrawn {
            agent_id: "a1".into(),
            family: ConsentFamily::SendExternal,
        },
        GovernanceEvent::PolicyRefused {
            agent_id: "a1".into(),
            access_id: "bash_executor".into(),
            family: ActionFamily::ExecuteCode,
            detail: "refused by the policy".into(),
        },
        GovernanceEvent::RuleRefused {
            agent_id: "a1".into(),
            access_id: "file_write".into(),
            family: ActionFamily::WriteOrDelete,
            rule_id: 3,
        },
        GovernanceEvent::RuleAdded(rule_record()),
        GovernanceEvent::RuleRemoved(rule_record()),
        GovernanceEvent::BatchAccessGranted {
            agent_id: "a1".into(),
            batch_id: "b1".into(),
            family: ConsentFamily::WriteOrDelete,
            granted_by: "batch".into(),
        },
    ];
    let policy_events = [
        GovernanceEvent::PolicyChanged {
            key_fingerprint: "ab12".into(),
            policy_digest: "d2".into(),
            previous_digest: "d1".into(),
        },
        GovernanceEvent::PolicyApplied {
            key_fingerprint: "ab12".into(),
            policy_digest: "d2".into(),
            publisher: "acme".into(),
            signed_at: "2026-01-01T00:00:00Z".into(),
        },
        GovernanceEvent::PolicyUnreadable {
            reason: "signature".into(),
            detail: "the signature does not hold".into(),
        },
        GovernanceEvent::FailClosedTriggered {
            cause: "policy_invalid".into(),
            detail: "the policy is refused".into(),
        },
    ];
    let export = GovernanceEvent::JournalExported {
        format: "csv".into(),
        entries: 12,
        actor: "user".into(),
    };

    // WHEN each event is recorded
    for event in agent_events.iter().chain(&policy_events).chain([&export]) {
        gov.record(event).expect("record");
    }

    // THEN the agent run holds its natures, in order, signed and linked
    let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
    let agent_run = journal.query_run(&governance_run_id("a1")).await;
    assert_eq!(
        kinds(&agent_run),
        vec![
            JournalEntryKind::ConsentAnswered,
            JournalEntryKind::ConsentAnswered,
            JournalEntryKind::ConsentAnswered,
            JournalEntryKind::AlwaysWithdrawn,
            JournalEntryKind::PolicyRefused,
            JournalEntryKind::RuleRefused,
            JournalEntryKind::RuleAdded,
            JournalEntryKind::RuleRemoved,
            JournalEntryKind::BatchAccessGranted,
        ]
    );
    // AND the policy run holds the four policy natures
    let policy_run = journal.query_run(POLICY_RUN_ID).await;
    assert_eq!(
        kinds(&policy_run),
        vec![
            JournalEntryKind::PolicyChanged,
            JournalEntryKind::PolicyApplied,
            JournalEntryKind::PolicyUnreadable,
            JournalEntryKind::FailClosedTriggered,
        ]
    );
    let export_run = journal.query_run(JOURNAL_EXPORT_RUN_ID).await;
    assert_eq!(kinds(&export_run), vec![JournalEntryKind::JournalExported]);
    // AND every run verifies with signatures required, and so does the whole
    // journal
    for (run, entries) in [
        (governance_run_id("a1"), &agent_run),
        (POLICY_RUN_ID.to_owned(), &policy_run),
        (JOURNAL_EXPORT_RUN_ID.to_owned(), &export_run),
    ] {
        assert!(entries.iter().all(|e| e.signature.is_some()));
        let report = verify_entries(&run, entries, Some(&signer), true);
        assert!(report.ok, "{run} must verify: {report:?}");
    }
    let whole = journal.verify_journal().await.expect("verify journal");
    assert!(whole.ok);
    assert_eq!(whole.entries_checked, 14);
    journal.shutdown().await;
}

// The rules and consents written while no runtime ran wait in the outbox,
// and land in the journal when governance starts, with their own date.
#[tokio::test]
async fn test_outbox_written_offline_is_journaled_at_start() {
    // GIVEN a governance.db where, with no runtime running, a global rule was
    // added then removed, an agent rule added, a family granted for a batch,
    // and a standing consent given then withdrawn
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("governance.db");
    let mut engine = PrefixRuleEngine::new(&db).expect("engine");
    let global = engine
        .add_rule(&PrefixRule {
            tool_name: "web_fetch".into(),
            action: RuleAction::Deny,
            ..PrefixRule::default()
        })
        .expect("add");
    engine.remove_rule_checked(global).expect("remove");
    engine
        .add_rule(&PrefixRule {
            tool_name: "file_write".into(),
            action: RuleAction::Allow,
            scope: PermissionScope::Agent,
            agent_id: Some("a1".into()),
            ..PrefixRule::default()
        })
        .expect("add agent rule");
    let mut consents = FamilyConsentStore::new(&db).expect("consents");
    consents
        .grant(
            "a1",
            ConsentFamily::WriteOrDelete,
            &ConsentScope::Lot("b1".into()),
            "batch",
        )
        .expect("lot");
    consents
        .grant(
            "a1",
            ConsentFamily::SendExternal,
            &ConsentScope::Permanent,
            "user",
        )
        .expect("always");
    consents
        .revoke("a1", ConsentFamily::SendExternal, &ConsentScope::Permanent)
        .expect("withdraw");
    let journal = signed_journal(&dir).await;

    // WHEN governance starts over the database and the journal
    let gov = GovernanceHandle::spawn(sources(&dir, Some(journal.clone())));
    gov.refresh().await;

    // THEN the global rule's history is in the rules run
    assert_eq!(
        kinds(&journal.query_run(GLOBAL_RULES_RUN_ID).await),
        vec![JournalEntryKind::RuleAdded, JournalEntryKind::RuleRemoved]
    );
    // AND the agent's run holds its rule, its batch grant and the withdrawal
    let agent_run = journal.query_run(&governance_run_id("a1")).await;
    assert_eq!(
        kinds(&agent_run),
        vec![
            JournalEntryKind::RuleAdded,
            JournalEntryKind::BatchAccessGranted,
            JournalEntryKind::AlwaysWithdrawn,
        ]
    );
    assert_eq!(agent_run[2].payload["family"], "send_external");
    // AND the outbox is empty
    let left = apollia_permissions::JournalOutbox::new(&db)
        .expect("outbox")
        .pending()
        .expect("pending");
    assert!(left.is_empty());
    let whole = journal.verify_journal().await.expect("verify");
    assert!(whole.ok);
    journal.shutdown().await;
}

// A rule another process adds while the runtime runs reaches the journal
// without a restart.
#[tokio::test]
async fn test_rule_added_while_running_is_journaled() {
    // GIVEN governance running over an empty database and a journal
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("governance.db");
    let journal = signed_journal(&dir).await;
    let gov = GovernanceHandle::spawn(sources(&dir, Some(journal.clone())));
    gov.refresh().await;

    // WHEN the command line adds an agent rule
    PrefixRuleEngine::new(&db)
        .expect("engine")
        .add_rule(&PrefixRule {
            tool_name: "send_email".into(),
            action: RuleAction::Deny,
            scope: PermissionScope::Agent,
            agent_id: Some("a2".into()),
            ..PrefixRule::default()
        })
        .expect("add");
    gov.refresh().await;

    // THEN the agent's run records it
    let entries = journal.query_run(&governance_run_id("a2")).await;
    assert_eq!(kinds(&entries), vec![JournalEntryKind::RuleAdded]);
    assert_eq!(entries[0].payload["tool_name"], "send_email");
    journal.shutdown().await;
}

// Without a journal the events wait in the outbox.
#[tokio::test]
async fn test_outbox_waits_without_a_journal() {
    // GIVEN a rule added to the database
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("governance.db");
    PrefixRuleEngine::new(&db)
        .expect("engine")
        .add_rule(&PrefixRule {
            tool_name: "web_fetch".into(),
            ..PrefixRule::default()
        })
        .expect("add");

    // WHEN governance starts with no journal
    let gov = GovernanceHandle::spawn(sources(&dir, None));
    gov.refresh().await;

    // THEN the event is still waiting
    let left = apollia_permissions::JournalOutbox::new(&db)
        .expect("outbox")
        .pending()
        .expect("pending");
    assert_eq!(left.len(), 1);
}

// At each start the policy is applied; it is changed when its digest
// differs from the last one applied; a refused one is unreadable.
#[tokio::test]
async fn test_policy_applied_changed_and_unreadable_at_start() {
    // GIVEN a signed journal
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = signed_journal(&dir).await;

    // WHEN the runtime starts three times with a valid policy, the third time
    // a different one, then once with a refused one, then once with none
    record_policy_at_startup(&journal, &valid("d1")).await;
    record_policy_at_startup(&journal, &valid("d1")).await;
    record_policy_at_startup(&journal, &valid("d2")).await;
    record_policy_at_startup(
        &journal,
        &PolicyVerdict::Invalid(PolicyInvalid {
            reason: PolicyInvalidReason::Signature,
            detail: "the signature does not hold for policy.toml".into(),
        }),
    )
    .await;
    record_policy_at_startup(&journal, &PolicyVerdict::Absent).await;

    // THEN the policy run reads applied, applied, changed then applied,
    // unreadable, and nothing for the start without a policy
    let entries = journal.query_run(POLICY_RUN_ID).await;
    assert_eq!(
        kinds(&entries),
        vec![
            JournalEntryKind::PolicyApplied,
            JournalEntryKind::PolicyApplied,
            JournalEntryKind::PolicyChanged,
            JournalEntryKind::PolicyApplied,
            JournalEntryKind::PolicyUnreadable,
        ]
    );
    // AND applied carries the key's fingerprint, changed both digests, and
    // unreadable its reason
    assert_eq!(entries[0].payload["key_fingerprint"], "ab12");
    assert_eq!(entries[0].payload["policy_digest"], "d1");
    assert_eq!(entries[2].payload["previous_digest"], "d1");
    assert_eq!(entries[2].payload["policy_digest"], "d2");
    assert_eq!(entries[4].payload["reason"], "signature");
    journal.shutdown().await;
}

// Entering fail-closed is journaled, once.
#[tokio::test]
async fn test_fail_closed_is_journaled() {
    // GIVEN a governance.db that is not a database, and a journal
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("governance.db"), vec![0x5a_u8; 4096]).expect("corrupt");
    let journal = signed_journal(&dir).await;

    // WHEN governance starts, then reads its sources again
    let gov = GovernanceHandle::spawn(sources(&dir, Some(journal.clone())));
    gov.refresh().await;

    // THEN one entry records the entry into fail-closed, with its cause
    assert!(gov.state().fail_closed.is_some());
    let entries = journal.query_run(POLICY_RUN_ID).await;
    assert_eq!(kinds(&entries), vec![JournalEntryKind::FailClosedTriggered]);
    assert_eq!(entries[0].payload["cause"], "store_unreadable");
    journal.shutdown().await;
}
