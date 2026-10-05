use super::*;
use crate::audit_journal::JournalEntryDraft;
use crate::connections::ConnectionKind;
use apollia_permissions::{ConsentFamily, PermissionScope, RuleAction, RuleRecord};
use apollia_tools::ActionFamily;
use serde_json::json;

const TS: &str = "2026-10-04T09:30:00Z";

fn entry(kind: JournalEntryKind, payload: Value) -> JournalEntry {
    JournalEntry {
        seq: 0,
        run_id: "run-1".into(),
        ts: TS.into(),
        kind,
        payload,
        prev_hash: String::new(),
        hash: String::new(),
        signature: None,
        signing_key_id: None,
    }
}

fn chained(draft: JournalEntryDraft) -> JournalEntry {
    entry(draft.kind, draft.payload)
}

fn governance(event: GovernanceEvent) -> JournalEntry {
    chained(GovernanceRecord::draft(&event, TS.into()).expect("draft"))
}

fn connection(event: ConnectionEvent) -> JournalEntry {
    chained(ConnectionRecord::draft(&event, TS.into()).expect("draft"))
}

#[test]
fn the_start_of_a_tool_call_names_agent_family_tool_and_target() {
    // GIVEN the entry of a call that started, as the subscriber writes it
    let started = entry(
        JournalEntryKind::ToolCallStarted,
        json!({
            "tool_name": "file_write", "agent_id": "quotes", "task_id": "t1",
            "tool_call_id": "c0", "family": "write_or_delete", "target": "/home/u/a.md",
        }),
    );
    // WHEN it is summarised
    let summary = summarize(&started);
    // THEN every part of the line is there, and a start has no result yet
    assert_eq!(
        summary,
        JournalSummary {
            agent: Some("quotes".into()),
            nature: JournalNature::Action,
            family: Some("write_or_delete".into()),
            what: "Tool call started".into(),
            tool: Some("file_write".into()),
            target: Some("/home/u/a.md".into()),
            outcome: None,
            ts: TS.into(),
        }
    );
}

#[test]
fn the_end_of_a_tool_call_says_whether_it_succeeded() {
    for (success, expected) in [
        (true, JournalOutcome::Success),
        (false, JournalOutcome::Failure),
    ] {
        // GIVEN the entry of a call that ended, on a host
        let completed = entry(
            JournalEntryKind::ToolCallCompleted,
            json!({
                "tool_name": "http_fetch", "agent_id": "quotes", "success": success,
                "duration_ms": 12, "tool_call_id": "c0", "family": "send_external",
                "target": "example.org",
            }),
        );
        // WHEN it is summarised
        let summary = summarize(&completed);
        // THEN the result follows the call, and the target is the host
        assert_eq!(summary.nature, JournalNature::Action);
        assert_eq!(summary.outcome, Some(expected));
        assert_eq!(summary.target.as_deref(), Some("example.org"));
        assert_eq!(summary.family.as_deref(), Some("send_external"));
    }
}

#[test]
fn a_tool_call_entry_gives_its_summary_no_argument_but_the_target() {
    // GIVEN the entry of a call whose payload also held, by mistake, its
    // arguments and its output
    let started = entry(
        JournalEntryKind::ToolCallStarted,
        json!({
            "tool_name": "file_write", "agent_id": "quotes", "target": "/home/u/a.md",
            "args_json": "{\"content\":\"SECRET-ARGUMENT\"}", "content": "SECRET-ARGUMENT",
            "output": "SECRET-OUTPUT",
        }),
    );
    // WHEN it is summarised
    let said = serde_json::to_string(&summarize(&started)).expect("serialise");
    // THEN the target is said and nothing else of the call is
    assert!(said.contains("/home/u/a.md"));
    assert!(
        !said.contains("SECRET-ARGUMENT") && !said.contains("SECRET-OUTPUT"),
        "{said}"
    );
}

#[test]
fn what_is_kept_for_replay_is_summarised_without_its_content() {
    // GIVEN a model response and a tool output, kept whole for replay
    let response = entry(
        JournalEntryKind::LlmCompletion,
        json!({
            "model_id": "local-model", "content": "SECRET-RESPONSE",
            "tool_calls": [{ "id": "c0", "name": "file_write", "arguments": { "content": "SECRET-ARGUMENT" } }],
        }),
    );
    let output = entry(
        JournalEntryKind::ToolOutput,
        json!({ "tool_call_id": "c0", "tool_name": "file_write", "output": "SECRET-OUTPUT", "status": "error" }),
    );
    // WHEN they are summarised
    let response_summary = summarize(&response);
    let output_summary = summarize(&output);
    // THEN each is a system entry that names the model or the tool, never
    // what was said or returned
    assert_eq!(response_summary.nature, JournalNature::System);
    assert_eq!(response_summary.target.as_deref(), Some("local-model"));
    assert_eq!(output_summary.nature, JournalNature::System);
    assert_eq!(output_summary.tool.as_deref(), Some("file_write"));
    assert_eq!(output_summary.outcome, Some(JournalOutcome::Failure));
    let said = format!(
        "{}{}",
        serde_json::to_string(&response_summary).expect("serialise"),
        serde_json::to_string(&output_summary).expect("serialise")
    );
    assert!(!said.contains("SECRET"), "{said}");
}

#[test]
fn a_connection_added_is_an_access_change_on_that_connection() {
    // GIVEN the entry written when an MCP server is added
    let added = connection(ConnectionEvent::Added {
        connection_id: "mcp:notes".into(),
        kind: ConnectionKind::Mcp,
        remote: true,
        provenance: apollia_mcp::Provenance::Custom,
        actor: "desktop".into(),
    });
    // WHEN it is summarised
    let summary = summarize(&added);
    // THEN it reads as a connection added, done, on that connection
    assert_eq!(summary.nature, JournalNature::AccessChange);
    assert_eq!(summary.what, "Connection added");
    assert_eq!(summary.target.as_deref(), Some("mcp:notes"));
    assert_eq!(summary.outcome, Some(JournalOutcome::Success));
    assert_eq!(summary.agent, None);
}

#[test]
fn a_secret_set_names_its_connection_and_shows_no_secret() {
    // GIVEN the entry written when a secret is set on a connection
    let set = connection(ConnectionEvent::SecretSet {
        connection_id: "mcp:notes".into(),
        key: "NOTES_TOKEN".into(),
        actor: "api".into(),
    });
    // WHEN it is summarised
    let summary = summarize(&set);
    // THEN it reads as a secret set on that connection
    assert_eq!(summary.nature, JournalNature::AccessChange);
    assert_eq!(summary.what, "Secret set on a connection");
    assert_eq!(summary.target.as_deref(), Some("mcp:notes"));
    assert_eq!(summary.outcome, Some(JournalOutcome::Success));
}

#[test]
fn an_account_connected_names_the_account_connection() {
    // GIVEN the entry written when a native account is connected
    let connected = connection(ConnectionEvent::AccountConnected {
        connection_id: "native:google:ada@example.org".into(),
        provider: "google".into(),
        account: "ada@example.org".into(),
        actor: "desktop".into(),
    });
    // WHEN it is summarised
    let summary = summarize(&connected);
    // THEN it reads as an account connected, on that account's connection
    assert_eq!(summary.nature, JournalNature::AccessChange);
    assert_eq!(summary.what, "Account connected");
    assert_eq!(
        summary.target.as_deref(),
        Some("native:google:ada@example.org")
    );
}

#[test]
fn a_folder_added_names_its_path() {
    // GIVEN the entry written when a folder is added as a connection
    let folder = connection(ConnectionEvent::FolderAdded {
        connection_id: "folder:1".into(),
        path: "/home/u/Documents".into(),
        actor: "desktop".into(),
    });
    // WHEN it is summarised
    let summary = summarize(&folder);
    // THEN the target is the folder
    assert_eq!(summary.what, "Folder added");
    assert_eq!(summary.target.as_deref(), Some("/home/u/Documents"));
}

#[test]
fn an_access_change_names_the_agent_and_the_access() {
    // GIVEN the entry written when an access is revoked from an agent
    let revoked = governance(GovernanceEvent::AccessChanged {
        agent_id: "quotes".into(),
        access_id: "family:send_external".into(),
        change: AccessChange::Revoked,
        actor: "user".into(),
        reason: None,
    });
    // WHEN it is summarised
    let summary = summarize(&revoked);
    // THEN it reads as an access revoked from that agent
    assert_eq!(summary.nature, JournalNature::AccessChange);
    assert_eq!(summary.what, "Access revoked");
    assert_eq!(summary.agent.as_deref(), Some("quotes"));
    assert_eq!(summary.target.as_deref(), Some("family:send_external"));
}

#[test]
fn a_consent_and_the_two_refusals_are_actions_with_their_family() {
    // GIVEN a refused consent, a refusal by the policy and a refusal by a rule
    let cases = [
        (
            GovernanceEvent::ConsentAnswered {
                agent_id: "quotes".into(),
                access_id: "gmail.send".into(),
                family: ActionFamily::SendExternal,
                answer: ConsentAnswer::Refuse,
            },
            "Consent refused",
        ),
        (
            GovernanceEvent::PolicyRefused {
                agent_id: "quotes".into(),
                access_id: "gmail.send".into(),
                family: ActionFamily::SendExternal,
                detail: "sending outside is forbidden".into(),
            },
            "Call refused by the organisation policy",
        ),
        (
            GovernanceEvent::RuleRefused {
                agent_id: "quotes".into(),
                access_id: "gmail.send".into(),
                family: ActionFamily::SendExternal,
                rule_id: 9,
            },
            "Call refused by a rule",
        ),
    ];
    for (event, what) in cases {
        // WHEN each is summarised
        let summary = summarize(&governance(event));
        // THEN it is an action of that agent with that tool, refused
        assert_eq!(summary.nature, JournalNature::Action, "{what}");
        assert_eq!(summary.what, what);
        assert_eq!(summary.agent.as_deref(), Some("quotes"));
        assert_eq!(summary.tool.as_deref(), Some("gmail.send"));
        assert_eq!(summary.family.as_deref(), Some("send_external"));
        assert_eq!(summary.outcome, Some(JournalOutcome::Refused));
        assert_eq!(summary.target, None);
    }
}

#[test]
fn a_consent_given_is_a_success() {
    // GIVEN a consent given for good
    let always = governance(GovernanceEvent::ConsentAnswered {
        agent_id: "quotes".into(),
        access_id: "file_write".into(),
        family: ActionFamily::WriteOrDelete,
        answer: ConsentAnswer::Always,
    });
    // WHEN it is summarised
    let summary = summarize(&always);
    // THEN it reads as given, not refused
    assert_eq!(summary.what, "Consent given for good");
    assert_eq!(summary.outcome, Some(JournalOutcome::Success));
}

#[test]
fn a_rule_names_its_tool_and_never_its_argument_prefix() {
    // GIVEN a rule added on a command prefix
    let rule = RuleRecord {
        rule_id: 4,
        tool_name: "bash_executor".into(),
        arg_prefix: Some("rm -rf /home/u/private".into()),
        action: RuleAction::Deny,
        scope: PermissionScope::Agent,
        agent_id: Some("quotes".into()),
        created_by: None,
    };
    // WHEN its entry is summarised
    let summary = summarize(&governance(GovernanceEvent::RuleAdded(rule)));
    // THEN the rule is an access change on that tool for that agent, and the
    // prefix is not said
    assert_eq!(summary.nature, JournalNature::AccessChange);
    assert_eq!(summary.what, "Rule added");
    assert_eq!(summary.tool.as_deref(), Some("bash_executor"));
    assert_eq!(summary.agent.as_deref(), Some("quotes"));
    let said = serde_json::to_string(&summary).expect("serialise");
    assert!(!said.contains("private"), "{said}");
}

#[test]
fn a_standing_consent_withdrawn_and_a_batch_grant_carry_their_family() {
    // GIVEN a standing consent withdrawn and a family granted for one batch
    let withdrawn = governance(GovernanceEvent::AlwaysWithdrawn {
        agent_id: "quotes".into(),
        family: ConsentFamily::WriteOrDelete,
    });
    let batch = governance(GovernanceEvent::BatchAccessGranted {
        agent_id: "quotes".into(),
        batch_id: "batch-7".into(),
        family: ConsentFamily::ReadPersonal,
        granted_by: "user".into(),
    });
    // WHEN they are summarised
    let withdrawn = summarize(&withdrawn);
    let batch = summarize(&batch);
    // THEN both are access changes carrying the family, the batch as target
    assert_eq!(withdrawn.nature, JournalNature::AccessChange);
    assert_eq!(withdrawn.family.as_deref(), Some("write_or_delete"));
    assert_eq!(batch.nature, JournalNature::AccessChange);
    assert_eq!(batch.family.as_deref(), Some("read_personal"));
    assert_eq!(batch.target.as_deref(), Some("batch-7"));
}

#[test]
fn an_installation_is_an_access_change_of_its_agent_or_package() {
    // GIVEN an agent installed and a package installed
    let agent = governance(GovernanceEvent::Installed {
        agent_id: "quotes".into(),
        version: "1.2.0".into(),
        signed: true,
        actor: "user".into(),
    });
    let package = governance(GovernanceEvent::PackageInstalled {
        package_id: "billing".into(),
        version: "1.0.0".into(),
        publisher: "Acme".into(),
        key_fingerprint: None,
        signed: false,
        actor: "user".into(),
    });
    // WHEN they are summarised
    let agent = summarize(&agent);
    let package = summarize(&package);
    // THEN each names what was installed
    assert_eq!(agent.nature, JournalNature::AccessChange);
    assert_eq!(agent.what, "Agent installed");
    assert_eq!(agent.agent.as_deref(), Some("quotes"));
    assert_eq!(package.nature, JournalNature::AccessChange);
    assert_eq!(package.what, "Package installed");
    assert_eq!(package.target.as_deref(), Some("billing"));
}

#[test]
fn the_policy_applied_refused_and_fail_closed_are_policy_entries() {
    // GIVEN the three entries the policy writes at startup
    let applied = governance(GovernanceEvent::PolicyApplied {
        key_fingerprint: "ab12".into(),
        policy_digest: "d1".into(),
        publisher: "Acme IT".into(),
        signed_at: TS.into(),
    });
    let unreadable = governance(GovernanceEvent::PolicyUnreadable {
        reason: "signature".into(),
        detail: "the signature does not match".into(),
    });
    let fail_closed = governance(GovernanceEvent::FailClosedTriggered {
        cause: "policy_invalid".into(),
        detail: "the deposited policy is refused".into(),
    });
    // WHEN they are summarised
    let applied = summarize(&applied);
    let unreadable = summarize(&unreadable);
    let fail_closed = summarize(&fail_closed);
    // THEN all three are policy entries; the applied one succeeded, the other
    // two record a failure
    for summary in [&applied, &unreadable, &fail_closed] {
        assert_eq!(summary.nature, JournalNature::Policy);
        assert_eq!(summary.agent, None);
    }
    assert_eq!(applied.outcome, Some(JournalOutcome::Success));
    assert_eq!(applied.target.as_deref(), Some("Acme IT"));
    assert_eq!(unreadable.outcome, Some(JournalOutcome::Failure));
    assert_eq!(fail_closed.outcome, Some(JournalOutcome::Failure));
}

#[test]
fn every_kind_is_said_in_words_and_carries_the_nature_of_its_kind() {
    // GIVEN one entry of every kind the journal can hold, its payload empty
    let tags = [
        "tool_call_started",
        "tool_call_completed",
        "llm_call_started",
        "llm_call_completed",
        "llm_completion",
        "tool_output",
        "clock_sample",
        "random_sample",
        "plan_mutation",
        "agent_started",
        "agent_stopped",
        "escalation_triggered",
        "message_sent",
        "message_delivered",
        "message_dropped",
        "access_changed",
        "agent_installed",
        "agent_updated",
        "agent_uninstalled",
        "agent_override_changed",
        "package_installed",
        "package_updated",
        "package_uninstalled",
        "connection_added",
        "connection_modified",
        "connection_removed",
        "connection_warning_accepted",
        "connection_secret_set",
        "connection_secret_removed",
        "account_connected",
        "account_disconnected",
        "folder_added",
        "consent_answered",
        "always_withdrawn",
        "policy_refused",
        "rule_refused",
        "rule_added",
        "rule_removed",
        "batch_access_granted",
        "policy_applied",
        "policy_changed",
        "policy_unreadable",
        "fail_closed_triggered",
        "journal_exported",
        "journal_purged",
        "journal_unsigned",
        "journal_export_failed",
        "ChatResponseCompleted",
    ];
    for tag in tags {
        let kind = JournalEntryKind::from_tag(tag);
        // WHEN it is summarised, even though its payload cannot be read
        let summary = summarize(&entry(kind.clone(), json!({})));
        // THEN it still says something, dated, under the nature of its kind
        assert!(!summary.what.is_empty(), "{tag}");
        assert_eq!(summary.nature, nature_of(&kind), "{tag}");
        assert_eq!(summary.ts, TS, "{tag}");
    }
}

#[test]
fn natures_and_outcomes_round_trip_through_their_wire_names() {
    // GIVEN every nature and every outcome
    let natures = [
        (JournalNature::Action, "action"),
        (JournalNature::AccessChange, "access_change"),
        (JournalNature::Policy, "policy"),
        (JournalNature::System, "system"),
    ];
    let outcomes = [
        (JournalOutcome::Success, "success"),
        (JournalOutcome::Failure, "failure"),
        (JournalOutcome::Refused, "refused"),
    ];
    // WHEN each is written, serialised and parsed back
    // THEN the wire name is the documented one, in code and in JSON
    for (nature, wire) in natures {
        assert_eq!(nature.as_str(), wire);
        assert_eq!(serde_json::to_value(nature).expect("serialise"), wire);
        assert_eq!(JournalNature::parse(wire), Some(nature));
    }
    for (outcome, wire) in outcomes {
        assert_eq!(outcome.as_str(), wire);
        assert_eq!(serde_json::to_value(outcome).expect("serialise"), wire);
        assert_eq!(JournalOutcome::parse(wire), Some(outcome));
    }
    assert_eq!(JournalNature::parse("other"), None);
    assert_eq!(JournalOutcome::parse("other"), None);
}

#[test]
fn a_server_call_names_its_endpoint_its_target_and_how_it_was_answered() {
    // GIVEN the entries of three calls: an agent answered, a model refused
    // for want of a token, and a model whose engine did not answer
    let call = |target: &str, kind: &str, status: u16| {
        entry(
            JournalEntryKind::ServerCall,
            json!({
                "event": "server_call", "method": "POST",
                "endpoint": "/v1/chat/completions", "target": target,
                "target_kind": kind, "status": status, "duration_ms": 12,
            }),
        )
    };
    // WHEN each is summarised
    let answered = summarize(&call("apollia/quotes", "agent", 200));
    let refused = summarize(&call("acme/chat", "model", 401));
    let failed = summarize(&call("acme/chat", "model", 502));
    // THEN the line is an action naming the endpoint and the target, the
    // agent by its own name, and the status reads as its outcome
    assert_eq!(
        answered,
        JournalSummary {
            agent: Some("quotes".into()),
            nature: JournalNature::Action,
            family: None,
            what: "Server call: POST /v1/chat/completions".into(),
            tool: None,
            target: Some("apollia/quotes".into()),
            outcome: Some(JournalOutcome::Success),
            ts: TS.into(),
        }
    );
    assert_eq!(refused.agent, None);
    assert_eq!(refused.outcome, Some(JournalOutcome::Refused));
    assert_eq!(failed.outcome, Some(JournalOutcome::Failure));
}
