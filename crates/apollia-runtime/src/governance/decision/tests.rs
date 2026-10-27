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

// Line 1: a connection the policy forbade at boot has every tool refused, and
// the tools of the others and those Apollia ships are left alone.
#[test]
fn test_tools_of_a_forbidden_connection_are_refused() {
    // GIVEN a policy that forbade the files server and a Microsoft account
    let mut state = GovernanceState::default();
    state.policy.forbidden_connections = ["mcp:files", "conn:microsoft:bob@example.com"]
        .into_iter()
        .map(str::to_owned)
        .collect();

    // WHEN a tool of each forbidden connection, one of Notion and a native
    // tool are decided
    let other = decide(
        &state,
        &call("mcp:files/read_file", ActionFamily::Read, None),
        NOW,
    );
    let account = decide(
        &state,
        &call(
            "conn:microsoft:bob@example.com/mail.list",
            ActionFamily::Read,
            None,
        ),
        NOW,
    );
    assert!(matches!(
        account,
        AccessDecision::Denied {
            reason: DecisionReason::Policy { .. }
        }
    ));
    let notion = decide(
        &state,
        &call("mcp:notion/search", ActionFamily::Read, None),
        NOW,
    );
    let native = decide(&state, &call("web_search", ActionFamily::Read, None), NOW);

    // THEN only the forbidden ones are refused
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

// A tool nobody classified has no family a consent could stand for: a fine
// rule `allow` naming it lets it run, in developer mode only.
#[test]
fn test_unclassified_runs_on_allow_rule_in_developer_mode_only() {
    // GIVEN an allow rule on a tool of an MCP server nobody classified
    let tool = "mcp:request-mcp:localhost/echo";
    let mut state = GovernanceState {
        rules: vec![rule(5, tool, None, RuleAction::Allow)],
        ..GovernanceState::default()
    };
    let asked = call(tool, ActionFamily::Unclassified, None);

    // WHEN it is decided outside developer mode
    let outside = decide(&state, &asked, NOW);
    // THEN it asks without "always", the rule set aside
    assert_eq!(outside, asked_without_always());

    // WHEN it is decided in developer mode
    state.developer_mode = true;
    let inside = decide(&state, &asked, NOW);
    // THEN the rule lets it run
    assert_eq!(
        inside,
        AccessDecision::Allowed {
            reason: DecisionReason::Rule { rule_id: 5 }
        }
    );

    // WHEN another tool of the same server is decided in developer mode
    let other = call(
        "mcp:request-mcp:localhost/delete",
        ActionFamily::Unclassified,
        None,
    );
    // THEN it still asks without "always"
    assert_eq!(decide(&state, &other, NOW), asked_without_always());
}

// Line 4: in developer mode, "always" is offered on a single simple command
// no rule covers yet, never on a compound one, never outside the mode.
#[test]
fn test_code_offers_always_on_a_simple_command_in_developer_mode_only() {
    // GIVEN no rule, and a simple and a chained shell command
    let mut state = GovernanceState::default();
    let simple = call("bash_executor", ActionFamily::ExecuteCode, Some("ls -la"));
    let chained = call(
        "bash_executor",
        ActionFamily::ExecuteCode,
        Some("ls -la && rm -rf build"),
    );

    // WHEN both are decided outside developer mode, then inside it
    let simple_outside = decide(&state, &simple, NOW);
    let chained_outside = decide(&state, &chained, NOW);
    state.developer_mode = true;
    let simple_inside = decide(&state, &simple, NOW);
    let chained_inside = decide(&state, &chained, NOW);

    // THEN only the simple command in developer mode is offered "always"
    assert_eq!(simple_outside, asked_without_always());
    assert_eq!(chained_outside, asked_without_always());
    assert_eq!(simple_inside, asked_with_all());
    assert_eq!(chained_inside, asked_without_always());
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
        fail_closed: Some(crate::governance::FailClosed::StoreUnreadable {
            detail: "file is not a database".into(),
        }),
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

// A code call the user answered "once" for goes through; a deny rule still
// refuses it, and the grant names one call only.
#[test]
fn test_a_code_call_answered_once_goes_through_and_a_deny_rule_still_refuses() {
    // GIVEN an unattended code call that waits, and the same call under a deny rule
    let mut code = call("bash_executor", ActionFamily::ExecuteCode, Some("ls"));
    code.unattended = true;
    let waits = decide(&GovernanceState::default(), &code, NOW);
    let denied_state = GovernanceState {
        rules: vec![rule(9, "bash_executor", Some("ls"), RuleAction::Deny)],
        ..GovernanceState::default()
    };
    let denied = decide(&denied_state, &code, NOW);

    // WHEN both are read as answered "once"
    let once = answered_once(waits);
    assert!(
        matches!(denied, AccessDecision::Denied { .. }),
        "{denied:?}"
    );
    let still_denied = answered_once(denied.clone());

    // THEN the waiting call goes through on the consent, the refusal stays
    assert_eq!(
        once,
        AccessDecision::Allowed {
            reason: DecisionReason::Consent
        }
    );
    assert_eq!(still_denied, denied);
    // AND the grant of one call is told apart from a grant of the run
    assert!(is_once_grant(&once_grant("bash_executor")));
    assert!(!is_once_grant("bash_executor"));
}

// Line 5: a fact proposed for the profile is asked every time.
#[test]
fn test_a_profile_proposal_is_asked_whatever_consent_stands() {
    // GIVEN an agent holding a standing consent for the writing family, and
    // a run granted for it
    let mut state = GovernanceState::default();
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::WriteOrDelete));
    let mut proposal = call(
        apollia_tools::PROFILE_PROPOSE_TOOL,
        ActionFamily::WriteOrDelete,
        Some("Prefers a table over prose."),
    );
    proposal.granted_for_run = true;
    let write = call("file_write", ActionFamily::WriteOrDelete, None);

    // WHEN the proposal and another write of the same family are decided
    let asked = decide(&state, &proposal, NOW);
    let allowed = decide(&state, &write, NOW);

    // THEN the proposal is put to the person without "always", while the
    // consent still covers the other write
    assert_eq!(asked, asked_without_always());
    assert!(matches!(allowed, AccessDecision::Allowed { .. }));
}

// Line 1, after the policy: every tool of a connection in pause is refused, reading ones
// included and whatever consent stands; the reason names the connection, and
// the tools of the other connections and those Apollia ships are left alone.
#[test]
fn test_tools_of_a_paused_connection_are_refused_with_the_pause() {
    // GIVEN a paused MCP server, a paused Google account and a paused folder,
    // and an agent holding a standing consent for sending outside
    let mut state = GovernanceState {
        paused: ["mcp:notion", "native:google:ada@example.com", "folder:f1"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..GovernanceState::default()
    };
    state
        .consents
        .insert((AGENT.into(), ConsentFamily::SendExternal));
    let paused = [
        ("mcp:notion/search", ActionFamily::Read, "mcp:notion"),
        (
            "conn:google:ada@example.com/gmail.send",
            ActionFamily::SendExternal,
            "native:google:ada@example.com",
        ),
        ("conn:folder:f1/read_file", ActionFamily::Read, "folder:f1"),
    ];

    for (tool, family, connection) in paused {
        // WHEN a tool of each is decided
        let decision = decide(&state, &call(tool, family, None), NOW);
        // THEN it is refused because its connection is in pause
        assert_eq!(
            decision,
            AccessDecision::Denied {
                reason: DecisionReason::Paused {
                    connection_id: connection.to_owned()
                }
            },
            "{tool}"
        );
    }
    // AND a tool of another server, of another account, and a native tool run
    assert!(decide(
        &state,
        &call("mcp:files/read", ActionFamily::Read, None),
        NOW
    )
    .is_allowed());
    assert!(decide(
        &state,
        &call(
            "conn:google:bob@example.com/gmail.search",
            ActionFamily::Read,
            None
        ),
        NOW
    )
    .is_allowed());
    assert!(decide(&state, &call("web_search", ActionFamily::Read, None), NOW).is_allowed());
}

// Line 1, the policy before the pause: a connection both forbidden and in pause is refused by
// the policy, which the user cannot lift by resuming it.
#[test]
fn test_the_policy_refuses_before_the_pause() {
    // GIVEN a server the policy forbade and the user paused
    let mut state = GovernanceState::default();
    state
        .policy
        .forbidden_connections
        .insert("mcp:files".into());
    state.paused.insert("mcp:files".into());

    // WHEN one of its tools is decided
    let decision = decide(
        &state,
        &call("mcp:files/read", ActionFamily::Read, None),
        NOW,
    );

    // THEN the policy is the reason
    assert!(matches!(
        decision,
        AccessDecision::Denied {
            reason: DecisionReason::Policy { .. }
        }
    ));
}

// Line 1: `connections.deny` names a connection in the admission vocabulary;
// the call is read in it, so an entry refuses the tools it names.
#[test]
fn test_connections_deny_refuses_the_tools_of_the_kind_it_names() {
    // GIVEN a policy whose `connections.deny` names Google, folders and IMAP,
    // as an organisation writes it, with nothing forbidden at boot
    let mut state = GovernanceState::default();
    for name in ["native:google", "native:folder", "native:imap"] {
        state.policy.denied.insert(name.to_owned());
    }
    let refused = [
        "conn:google:ada@example.com/gmail.search",
        "conn:folder:f1/read_file",
        "imap.search",
    ];

    for tool in refused {
        // WHEN a tool of each kind is decided
        let decision = decide(&state, &call(tool, ActionFamily::Read, None), NOW);
        // THEN the policy refuses it, naming the connection
        assert_eq!(
            decision,
            AccessDecision::Denied {
                reason: DecisionReason::Policy {
                    detail: "refused by the organisation policy: this connection is not allowed"
                        .to_owned()
                }
            },
            "{tool}"
        );
    }
    // AND a Microsoft account and a native tool are left alone
    assert!(decide(
        &state,
        &call(
            "conn:microsoft:bob@example.com/mail.list",
            ActionFamily::Read,
            None
        ),
        NOW
    )
    .is_allowed());
    assert!(decide(&state, &call("web_search", ActionFamily::Read, None), NOW).is_allowed());
}

// The connection of a call, as the connection list names it.
#[test]
fn test_the_listed_connection_of_a_call() {
    // GIVEN a tool of each kind
    // WHEN its connection is named
    // THEN it is named as the connection list names it
    assert_eq!(
        listed_connection_of("mcp:notion/search").as_deref(),
        Some("mcp:notion")
    );
    assert_eq!(
        listed_connection_of("conn:google:ada@example.com/gmail.send").as_deref(),
        Some("native:google:ada@example.com")
    );
    assert_eq!(
        listed_connection_of("conn:folder:f1/read_file").as_deref(),
        Some("folder:f1")
    );
    assert_eq!(listed_connection_of("imap.search"), None);
    assert_eq!(listed_connection_of("web_search"), None);
}

// A mailbox tool is shared by every mailbox: the call is refused by the pause
// of the mailbox it reads, and only of that one.
#[test]
fn test_a_mailbox_tool_is_refused_by_the_pause_of_the_mailbox_it_reads() {
    // GIVEN one mailbox in pause
    let state = GovernanceState {
        paused: ["imap:ada@example.org".to_owned()].into_iter().collect(),
        ..GovernanceState::default()
    };

    // WHEN a search of that mailbox, and one of another, are decided
    let on_paused = decide(
        &state,
        &call(
            "imap.search",
            ActionFamily::ReadPersonal,
            Some("ada@example.org"),
        ),
        NOW,
    );
    let on_other = decide(
        &state,
        &call(
            "imap.search",
            ActionFamily::ReadPersonal,
            Some("bob@example.org"),
        ),
        NOW,
    );

    // THEN the first is refused by the pause of its mailbox, the second is not
    assert_eq!(
        on_paused,
        AccessDecision::Denied {
            reason: DecisionReason::Paused {
                connection_id: "imap:ada@example.org".to_owned()
            }
        }
    );
    assert!(!matches!(on_other, AccessDecision::Denied { .. }));
}
