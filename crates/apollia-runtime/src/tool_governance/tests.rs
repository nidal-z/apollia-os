use apollia_permissions::{PermissionScope, PrefixRule, RuleAction};

use super::*;

#[test]
fn test_active_backend_for_web_search() {
    // GIVEN a secret store holding a Brave key, then an empty one
    // WHEN the active backend of the search tool is asked for
    // THEN the key selects Brave, its absence selects the keyless engine, and a tool with no backend answers nothing
    assert_eq!(
        active_backend_for("web_search", &["brave.api_key".to_string()]),
        Some("brave".to_string())
    );
    assert_eq!(
        active_backend_for("web_search", &[]),
        Some("duckduckgo".to_string())
    );
    assert!(active_backend_for("bash_executor", &[]).is_none());
}

#[test]
fn test_parse_scope_unknown_value() {
    // GIVEN the two scope names, the two retired ones, and one that never existed
    // WHEN each is parsed
    // THEN the two known scopes map and the others are rejected
    assert!(parse_scope("user").is_err());
    assert!(parse_scope("session").is_err());
    assert!(parse_scope("project").is_err());
    assert!(matches!(parse_scope("agent"), Ok(PermissionScope::Agent)));
    assert!(matches!(parse_scope("global"), Ok(PermissionScope::Global)));
}

#[test]
fn test_rule_view_serializes_action_and_scope() {
    // GIVEN an agent-scoped deny rule on a command prefix
    let rule = PrefixRule {
        id: 7,
        tool_name: "bash_executor".into(),
        arg_prefix: Some("git".into()),
        action: RuleAction::Deny,
        created_at: 1_700_000_000,
        scope: PermissionScope::Agent,
        agent_id: Some("a1".into()),
        expires_at: None,
        created_by_agent: Some("operator".into()),
    };
    // WHEN it is converted for the front end
    let dto = rule_view(&rule);
    // THEN action, scope, agent and author cross as the strings the UI reads
    assert_eq!(dto.id, 7);
    assert_eq!(dto.action, "deny");
    assert_eq!(dto.scope, "agent");
    assert_eq!(dto.agent_id.as_deref(), Some("a1"));
    assert_eq!(dto.created_by.as_deref(), Some("operator"));
}

#[tokio::test]
async fn test_an_empty_data_directory_is_refused_before_any_file_is_touched() {
    // GIVEN no data directory
    // WHEN the tools are listed
    let refused = list_tools(PathBuf::new()).await;
    // THEN the operation is refused as unavailable
    assert!(matches!(refused, Err(ToolGovernanceError::Unavailable)));
}
