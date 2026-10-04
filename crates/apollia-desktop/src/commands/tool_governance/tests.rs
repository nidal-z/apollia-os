use std::path::Path;

use apollia_permissions::{PermissionScope, PrefixRuleEngine, RuleAction};
use apollia_tools::GOVERNANCE_DB_FILENAME;
use tempfile::TempDir;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::AttachedClient;

const TOKEN: &str = "token-of-this-installation";

fn bearer() -> String {
    format!("Bearer {TOKEN}")
}

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

// REASON: test fixture persisting one scoped rule; the arguments are that rule's columns.
#[allow(clippy::too_many_arguments)]
fn persist_scoped_rule(
    base_dir: &Path,
    tool_name: String,
    arg_prefix: Option<String>,
    action: RuleAction,
    scope: PermissionScope,
    agent_id: Option<String>,
) -> Result<i64, String> {
    let rule = PrefixRule {
        tool_name,
        arg_prefix,
        action,
        scope,
        agent_id,
        ..PrefixRule::default()
    };
    apollia_runtime::permission_rules::persist_rule(base_dir, &rule).map_err(|e| e.to_string())
}

#[test]
fn test_add_agent_rule_persisted_with_agent() {
    // GIVEN an empty governance directory
    let dir = TempDir::new().expect("tempdir");

    // WHEN a rule for agent a1 is persisted
    let id = persist_scoped_rule(
        dir.path(),
        "bash_executor".into(),
        Some("git".into()),
        RuleAction::Allow,
        PermissionScope::Agent,
        Some("a1".into()),
    )
    .expect("persist agent");

    // THEN it is stored for that agent
    assert!(id > 0);
    let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
    let engine = PrefixRuleEngine::new(&db_path).expect("engine");
    let rules = engine.list_rules_for_agent("a1").expect("list");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].action, RuleAction::Allow);
}

#[test]
fn test_revoke_rule_removes_from_db() {
    // GIVEN
    let dir = TempDir::new().expect("tempdir");
    let id = persist_scoped_rule(
        dir.path(),
        "bash_executor".into(),
        None,
        RuleAction::Allow,
        PermissionScope::Global,
        None,
    )
    .expect("persist global");
    let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);

    // WHEN
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
    let removed = engine.remove_rule_checked(id).expect("remove");

    // THEN
    assert!(removed);
    assert!(engine.list_rules().expect("list").is_empty());
}

#[test]
fn test_remove_rules_by_scope_only_clears_target() {
    // GIVEN: one global rule and one agent rule
    let dir = TempDir::new().expect("tempdir");
    persist_scoped_rule(
        dir.path(),
        "bash_executor".into(),
        None,
        RuleAction::Allow,
        PermissionScope::Global,
        None,
    )
    .expect("global");
    persist_scoped_rule(
        dir.path(),
        "bash_executor".into(),
        Some("git".into()),
        RuleAction::Allow,
        PermissionScope::Agent,
        Some("a1".into()),
    )
    .expect("agent");
    let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);

    // WHEN: only the global rules are removed
    let mut engine = PrefixRuleEngine::new(&db_path).expect("engine");
    let removed = engine
        .remove_rules_by_scope(PermissionScope::Global)
        .expect("remove globals");

    // THEN: only the agent rule remains
    assert_eq!(removed, 1);
    let remaining = engine.list_rules().expect("list");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].scope, PermissionScope::Agent);
}

// ── Attached to a daemon ─────────────────────────────────────────────────

#[tokio::test]
async fn test_the_tools_are_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon answering its governed tools to the token only
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/governance/tools"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "name": "web_search", "enabled": true, "config": null,
                "credential_keys": [], "active_backend": "duckduckgo",
            }])),
        )
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the tools page lists them through the attached arm
    let tools = list_tools_on(&attached(&server)).await.unwrap();

    // THEN the daemon's view comes back as the page draws it
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "web_search");
    assert_eq!(tools[0].active_backend.as_deref(), Some("duckduckgo"));
}

#[tokio::test]
async fn test_a_tool_is_disabled_on_the_daemon_by_its_encoded_name() {
    // GIVEN a daemon accepting the flag of one tool
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/governance/tools/my%20tool/enabled"))
        .and(header("authorization", bearer().as_str()))
        .and(body_json(serde_json::json!({ "enabled": false })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "enabled": false })),
        )
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the attached arm disables it
    let done = set_tool_enabled_on(&attached(&server), "my tool".into(), false).await;

    // THEN the PUT reached the daemon with the name as one segment
    assert!(done.is_ok(), "{done:?}");
}

#[tokio::test]
async fn test_a_tool_config_is_read_and_written_on_the_daemon() {
    // GIVEN a daemon holding a configuration for the fetch tool, and
    // accepting a new one
    let server = MockServer::start().await;
    let config = serde_json::json!({ "timeout_secs": 12 });
    Mock::given(method("GET"))
        .and(path("/api/v1/governance/tools/http_fetch/config"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "config": config })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/governance/tools/http_fetch/config"))
        .and(header("authorization", bearer().as_str()))
        .and(body_json(serde_json::json!({ "config": config })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "config": config })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = attached(&server);

    // WHEN the attached arm reads it, then writes it
    let read = get_tool_config_on(&link, "http_fetch".into())
        .await
        .unwrap();
    let written = set_tool_config_on(&link, "http_fetch".into(), config.clone()).await;

    // THEN the configuration crossed both ways unchanged
    assert_eq!(read, Some(config));
    assert!(written.is_ok(), "{written:?}");
}

#[tokio::test]
async fn test_the_rules_are_listed_on_the_daemon_with_the_filter_as_query() {
    // GIVEN a daemon holding one agent rule
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/governance/rules"))
        .and(query_param("scope", "agent"))
        .and(query_param("tool_name", "bash_executor"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 3, "tool_name": "bash_executor", "arg_prefix": "git",
                "action": "allow", "scope": "agent", "agent_id": "a1",
                "expires_at": null, "created_at": "2026-10-03T00:00:00+00:00",
                "created_by": null,
            }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let filter = PermissionRuleFilter {
        scope: Some("agent".into()),
        tool_name: Some("bash_executor".into()),
    };

    // WHEN the permissions page lists the agent rules of the shell tool
    let rules = list_permission_rules_on(&attached(&server), filter)
        .await
        .unwrap();

    // THEN the daemon's rule comes back as the page draws it
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].agent_id.as_deref(), Some("a1"));
}

#[tokio::test]
async fn test_a_rule_is_revoked_on_the_daemon_and_its_refusal_shown_as_is() {
    // GIVEN a daemon that revokes rule 3 and knows no rule 4
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/governance/rules/3"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "revoked": 3 })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/governance/rules/4"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({ "error": "permission rule 4 not found" })),
        )
        .mount(&server)
        .await;
    let link = attached(&server);

    // WHEN the attached arm revokes both
    let revoked = revoke_permission_rule_on(&link, 3).await;
    let refused = revoke_permission_rule_on(&link, 4).await;

    // THEN the first is done, the second reads in the daemon's own words
    assert!(revoked.is_ok(), "{revoked:?}");
    assert_eq!(refused.unwrap_err(), "permission rule 4 not found");
}

#[tokio::test]
async fn test_revoke_all_reaches_the_daemon_and_answers_its_count() {
    // GIVEN a daemon clearing two global rules
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/governance/rules/revoke-all"))
        .and(header("authorization", bearer().as_str()))
        .and(body_json(serde_json::json!({ "scope": null })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "removed": 2 })))
        .expect(1)
        .mount(&server)
        .await;

    // WHEN "revoke all" is pressed while attached
    let removed = revoke_all_rules_on(&attached(&server), None).await.unwrap();

    // THEN the daemon's count comes back
    assert_eq!(removed, 2);
}

#[tokio::test]
async fn test_revoke_all_of_an_unknown_scope_shows_the_daemon_refusal() {
    // GIVEN a daemon refusing a scope it does not know
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/governance/rules/revoke-all"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "unknown scope 'project', expected 'agent' | 'global'",
        })))
        .mount(&server)
        .await;

    // WHEN the attached arm asks to clear that scope
    let refused = revoke_all_rules_on(&attached(&server), Some("project".into())).await;

    // THEN the interface reads the daemon's words
    assert_eq!(
        refused.unwrap_err(),
        "unknown scope 'project', expected 'agent' | 'global'"
    );
}

#[tokio::test]
async fn test_the_audit_is_paged_on_the_daemon_with_the_token() {
    // GIVEN a daemon holding one decision on the shell tool
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/governance/audit"))
        .and(query_param("tool_name", "bash_executor"))
        .and(query_param("limit", "20"))
        .and(query_param("offset", "40"))
        .and(header("authorization", bearer().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 1, "tool_name": "bash_executor", "first_arg": "git",
                "decision": "AutoAllowedSafeList", "scope": null, "rule_id": null,
                "agent": null, "decided_at": "2026-10-03T00:00:00+00:00",
            }])),
        )
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the audit page asks for its third page of twenty
    let entries = list_audit_on(
        &attached(&server),
        Some("bash_executor".into()),
        Some(20),
        Some(40),
    )
    .await
    .unwrap();

    // THEN the daemon's entry comes back
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].decision, "AutoAllowedSafeList");
}

#[tokio::test]
async fn test_a_daemon_refusal_of_the_tools_is_shown_as_is() {
    // GIVEN a daemon started without a data directory
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/governance/tools"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "error": "the governance database has no data directory",
        })))
        .mount(&server)
        .await;

    // WHEN the tools page lists them
    let refused = list_tools_on(&attached(&server)).await;

    // THEN the interface reads the daemon's words, not an unavailability
    assert_eq!(
        refused.unwrap_err(),
        "the governance database has no data directory"
    );
}

#[test]
fn test_a_query_string_keeps_the_present_pairs_encoded() {
    // GIVEN a filter with one value holding a space and one absent
    let pairs = [("tool_name", Some("my tool".to_owned())), ("scope", None)];
    // WHEN the query string is built
    let query = query_string(&pairs);
    // THEN only the present pair crosses, encoded, and no pair gives nothing
    assert_eq!(query, "?tool_name=my%20tool");
    assert_eq!(query_string(&[("scope", None)]), "");
}

#[test]
fn test_the_nine_governance_commands_pass_the_gate_attached() {
    // GIVEN the commands of the tools and permissions pages
    let commands = [
        "governance_list_tools",
        "governance_get_tool_config",
        "governance_set_tool_config",
        "governance_set_tool_enabled",
        "governance_list_audit",
        "governance_list_permission_rules",
        "governance_revoke_permission_rule",
        "governance_revoke_all_rules",
        "governance_state",
    ];
    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|c| crate::attached::gate(crate::attached::LinkMode::Attached, c).is_err())
        .collect();
    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}
