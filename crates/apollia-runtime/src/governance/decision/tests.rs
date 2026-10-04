use super::*;
use apollia_permissions::PermissionScope;

const AGENT: &str = "invoices";
const NOW: i64 = 1_800_000_000;

fn call<'a>(access_id: &'a str, family: ActionFamily, argument: Option<&'a str>) -> AccessCall<'a> {
    AccessCall {
        agent_id: AGENT,
        access_id,
        argument,
        family,
        unattended: false,
        granted_for_run: false,
    }
}

fn rule(id: i64, tool: &str, prefix: Option<&str>, action: RuleAction) -> PrefixRule {
    PrefixRule {
        id,
        tool_name: tool.to_owned(),
        arg_prefix: prefix.map(str::to_owned),
        action,
        scope: PermissionScope::Global,
        ..PrefixRule::default()
    }
}

fn asked_with_all() -> AccessDecision {
    AccessDecision::AskUser {
        reason: DecisionReason::Consent,
        answers: ALL_ANSWERS.to_vec(),
    }
}

fn asked_without_always() -> AccessDecision {
    AccessDecision::AskUser {
        reason: DecisionReason::Consent,
        answers: NO_ALWAYS_ANSWERS.to_vec(),
    }
}

// Line 1: the policy refuses a tool, a connection or a family, whatever the
// consent.
#[test]
fn test_policy_refuses_tool_connection_and_family() {
    // GIVEN a policy refusing one tool, one connection and the family that
    // sends outside, and an agent holding a standing consent for that family
    let mut state = GovernanceState::default();
    state.policy.denied.insert("web_search".into());
    state.policy.denied.insert("mcp:notion".into());
    state.policy.families.insert(PolicyFamily::SendExternal);
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::SendExternal));
    let calls = [
        call("web_search", ActionFamily::Read, None),
        call("mcp:notion/create_page", ActionFamily::WriteOrDelete, None),
        call("send_email", ActionFamily::SendExternal, None),
    ];

    for c in calls {
        // WHEN each call is decided
        let decision = decide(&state, &c, NOW);
        // THEN the policy refuses it, and says so
        assert!(
            matches!(
                &decision,
                AccessDecision::Denied {
                    reason: DecisionReason::Policy { detail }
                } if detail.starts_with("refused by the organisation policy")
            ),
            "{}: {decision:?}",
            c.access_id
        );
    }
}

// Line 1: a non-empty list of allowed connections refuses the tools of every
// other connection, and leaves the tools Apollia ships alone.
#[test]
fn test_policy_allow_list_refuses_other_connections() {
    // GIVEN a policy allowing the Notion server only
    let mut state = GovernanceState::default();
    state.policy.allowed.insert("mcp:notion".into());

    // WHEN a tool of another server, one of Notion and a native tool are decided
    let other = decide(
        &state,
        &call("mcp:files/read_file", ActionFamily::Read, None),
        NOW,
    );
    let notion = decide(
        &state,
        &call("mcp:notion/search", ActionFamily::Read, None),
        NOW,
    );
    let native = decide(&state, &call("web_search", ActionFamily::Read, None), NOW);

    // THEN only the other server is refused
    assert!(matches!(
        other,
        AccessDecision::Denied {
            reason: DecisionReason::Policy { .. }
        }
    ));
    assert!(notion.is_allowed());
    assert!(native.is_allowed());
}

// Line 2: a fine rule `deny` refuses, even a reading tool and even with a
// standing consent.
#[test]
fn test_deny_rule_refuses_before_read_and_consent() {
    // GIVEN deny rules on a reading tool and on a writing one the agent holds
    // a standing consent for
    let mut state = GovernanceState {
        rules: vec![
            rule(7, "file_read", Some("/secret"), RuleAction::Deny),
            rule(8, "file_write", None, RuleAction::Deny),
        ],
        ..GovernanceState::default()
    };
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::WriteOrDelete));

    // WHEN calls matching each rule are decided
    let read = decide(
        &state,
        &call("file_read", ActionFamily::Read, Some("/secret/a")),
        NOW,
    );
    let write = decide(
        &state,
        &call("file_write", ActionFamily::WriteOrDelete, Some("/tmp/a")),
        NOW,
    );

    // THEN both are refused by their rule
    assert_eq!(
        read,
        AccessDecision::Denied {
            reason: DecisionReason::Rule { rule_id: 7 }
        }
    );
    assert_eq!(
        write,
        AccessDecision::Denied {
            reason: DecisionReason::Rule { rule_id: 8 }
        }
    );
}

// Line 3: reading runs without asking, with no consent at all.
#[test]
fn test_read_is_allowed_without_prompt() {
    // GIVEN an empty state
    let state = GovernanceState::default();

    // WHEN a reading call and a call that acts on the conversation alone are decided
    let read = decide(&state, &call("web_search", ActionFamily::Read, None), NOW);
    let exempt = decide(&state, &call("ask_user", ActionFamily::Exempt, None), NOW);

    // THEN both run, no consent needed
    for decision in [read, exempt] {
        assert_eq!(
            decision,
            AccessDecision::Allowed {
                reason: DecisionReason::NoConsentNeeded
            }
        );
    }
}

// Line 4: executing code runs on an allow rule for a single simple command,
// in developer mode only; otherwise it asks, without "always".
#[test]
fn test_code_runs_on_allow_rule_in_developer_mode_only() {
    // GIVEN an allow rule on `git` for the shell executor
    let mut state = GovernanceState {
        rules: vec![rule(3, "bash_executor", Some("git"), RuleAction::Allow)],
        ..GovernanceState::default()
    };
    let simple = call(
        "bash_executor",
        ActionFamily::ExecuteCode,
        Some("git status"),
    );
    let chained = call(
        "bash_executor",
        ActionFamily::ExecuteCode,
        Some("git status; rm -rf /"),
    );

    // WHEN a simple command is decided outside developer mode
    let outside = decide(&state, &simple, NOW);
    // THEN it asks without "always"
    assert_eq!(outside, asked_without_always());

    // WHEN the same command is decided in developer mode
    state.developer_mode = true;
    let inside = decide(&state, &simple, NOW);
    // THEN the rule lets it run
    assert_eq!(
        inside,
        AccessDecision::Allowed {
            reason: DecisionReason::Rule { rule_id: 3 }
        }
    );

    // WHEN a chained command is decided in developer mode
    let compound = decide(&state, &chained, NOW);
    // THEN it asks without "always"
    assert_eq!(compound, asked_without_always());
}

// Line 4: no standing consent and no grant of the run lets code run.
#[test]
fn test_code_ignores_consents_and_run_grants() {
    // GIVEN a standing consent recorded for code, against the rules, and a
    // call the run granted
    let mut state = GovernanceState::default();
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::ExecuteCode));
    let mut c = call(
        "python_executor",
        ActionFamily::Unclassified,
        Some("print(1)"),
    );
    c.granted_for_run = true;

    // WHEN the call is decided
    let decision = decide(&state, &c, NOW);

    // THEN it asks without "always", the executor being code by its name
    assert_eq!(decision, asked_without_always());
}

// Line 5: a standing consent of the agent for the family, or a grant of the
// run, lets the call run.
#[test]
fn test_standing_consent_or_run_grant_allows() {
    // GIVEN a standing consent of the agent for writing
    let mut state = GovernanceState::default();
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::WriteOrDelete));

    // WHEN a writing call and a sending call the run granted are decided
    let write = decide(
        &state,
        &call("file_write", ActionFamily::WriteOrDelete, None),
        NOW,
    );
    let mut granted = call("send_email", ActionFamily::SendExternal, None);
    granted.granted_for_run = true;
    let send = decide(&state, &granted, NOW);

    // THEN both run on consent
    for decision in [write, send] {
        assert_eq!(
            decision,
            AccessDecision::Allowed {
                reason: DecisionReason::Consent
            }
        );
    }
}

// Line 6: otherwise the call asks, with the three answers; a consent of
// another agent does not count.
#[test]
fn test_otherwise_asks_with_three_answers() {
    // GIVEN a consent held by another agent
    let mut state = GovernanceState::default();
    state
        .consents
        .insert(("other".into(), ConsentFamily::ReadPersonal));

    // WHEN a call reading personal data is decided
    let decision = decide(
        &state,
        &call("gmail_search", ActionFamily::ReadPersonal, None),
        NOW,
    );

    // THEN it asks with once, always and refuse
    assert_eq!(decision, asked_with_all());
    assert_eq!(
        decision.answers(),
        &[
            ConsentAnswer::Once,
            ConsentAnswer::Always,
            ConsentAnswer::Refuse
        ]
    );
}

// A tool nobody classified follows the last line, without "always", and no
// grant of the run covers it.
#[test]
fn test_unclassified_asks_without_always() {
    // GIVEN a call to an unclassified tool the run granted
    let state = GovernanceState::default();
    let mut c = call("mcp:files/move", ActionFamily::Unclassified, None);
    c.granted_for_run = true;

    // WHEN it is decided
    let decision = decide(&state, &c, NOW);

    // THEN it asks, once or refuse
    assert_eq!(decision, asked_without_always());
}

// Unattended, a call that would ask waits instead, with the same answers.
#[test]
fn test_unattended_ask_becomes_pending() {
    // GIVEN an unattended run
    let state = GovernanceState::default();
    let mut write = call("file_write", ActionFamily::WriteOrDelete, None);
    write.unattended = true;
    let mut code = call("bash_executor", ActionFamily::ExecuteCode, Some("ls"));
    code.unattended = true;

    // WHEN a writing call and a code call are decided
    let w = decide(&state, &write, NOW);
    let c = decide(&state, &code, NOW);

    // THEN both wait, with the answers they would have offered
    assert_eq!(
        w,
        AccessDecision::Pending {
            reason: DecisionReason::Consent,
            answers: ALL_ANSWERS.to_vec(),
        }
    );
    assert_eq!(
        c,
        AccessDecision::Pending {
            reason: DecisionReason::Consent,
            answers: NO_ALWAYS_ANSWERS.to_vec(),
        }
    );
}

// Failing closed refuses what is sensitive and lets reading run.
#[test]
fn test_fail_closed_refuses_sensitive_only() {
    // GIVEN governance told it cannot read its state, and a standing consent
    let mut state = GovernanceState {
        fail_closed: true,
        ..GovernanceState::default()
    };
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::WriteOrDelete));

    // WHEN a writing call and a reading call are decided
    let write = decide(
        &state,
        &call("file_write", ActionFamily::WriteOrDelete, None),
        NOW,
    );
    let read = decide(&state, &call("file_read", ActionFamily::Read, None), NOW);

    // THEN the write is refused for that reason and the read runs
    assert_eq!(
        write,
        AccessDecision::Denied {
            reason: DecisionReason::FailClosed
        }
    );
    assert!(read.is_allowed());
}

// An expired rule and a rule of another agent decide nothing.
#[test]
fn test_expired_and_foreign_rules_are_ignored() {
    // GIVEN an expired deny rule and a deny rule of another agent
    let mut expired = rule(1, "file_write", None, RuleAction::Deny);
    expired.expires_at = Some(NOW - 1);
    let mut foreign = rule(2, "file_write", None, RuleAction::Deny);
    foreign.scope = PermissionScope::Agent;
    foreign.agent_id = Some("other".into());
    let state = GovernanceState {
        rules: vec![expired, foreign],
        ..GovernanceState::default()
    };

    // WHEN a writing call is decided
    let decision = decide(
        &state,
        &call("file_write", ActionFamily::WriteOrDelete, None),
        NOW,
    );

    // THEN neither refuses it: it asks
    assert_eq!(decision, asked_with_all());
}
