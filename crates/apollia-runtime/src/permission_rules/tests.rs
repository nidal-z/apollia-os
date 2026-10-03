use apollia_permissions::prefix_rule_engine::PrefixRuleEngine;
use tempfile::TempDir;

use super::*;

fn stored_rules(data_dir: &Path) -> Vec<PrefixRule> {
    PrefixRuleEngine::new(&data_dir.join(GOVERNANCE_DB_FILENAME))
        .unwrap()
        .list_rules()
        .unwrap()
}

#[test]
fn parse_proposals_extracts_fields() {
    // GIVEN two proposals as the agent emits them, one with a prefix and one without
    let raw = r#"[
      {"tool_name":"http_fetch","action":"deny","arg_prefix":"https://","scope":"global"},
      {"tool_name":"file_read","action":"allow","scope":"global"}
    ]"#;
    // WHEN they are parsed for the front end
    let views = parse_proposals(raw);
    // THEN each keeps its index, its tool, its action, and its prefix or nothing
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].index, 0);
    assert_eq!(views[0].tool_name, "http_fetch");
    assert_eq!(views[0].action, "deny");
    assert_eq!(views[0].arg_prefix.as_deref(), Some("https://"));
    assert_eq!(views[1].index, 1);
    assert_eq!(views[1].arg_prefix, None);
}

#[test]
fn parse_proposals_tolerates_invalid_items() {
    // GIVEN a list mixing one valid proposal with a string and a number
    let raw = r#"[{"tool_name":"ok","action":"allow"}, "garbage", 42]"#;
    // WHEN it is parsed
    let views = parse_proposals(raw);
    // THEN the valid one survives and the rest is dropped rather than losing the whole list
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].tool_name, "ok");
}

#[test]
fn parse_proposals_empty_on_garbage() {
    // GIVEN a payload that is not JSON at all
    // WHEN it is parsed
    // THEN the list is empty rather than a panic
    assert!(parse_proposals("not json").is_empty());
}

#[tokio::test]
async fn test_an_applied_proposal_becomes_a_rule_by_the_onboarding_agent_and_leaves_the_list() {
    // GIVEN two proposals stored by the onboarding agent
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &serde_json::json!([
            {"tool_name": "file_read", "action": "allow", "scope": "global"},
            {"tool_name": "http_fetch", "action": "deny", "arg_prefix": "http://"}
        ]),
    );

    // WHEN the second is applied
    let id = apply_proposal(dir.path().to_path_buf(), 1).await.unwrap();

    // THEN it is a stored rule stamped by the agent, and only the first remains
    let rules = stored_rules(dir.path());
    let rule = rules.iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.tool_name, "http_fetch");
    assert_eq!(rule.arg_prefix.as_deref(), Some("http://"));
    assert_eq!(rule.created_by_agent.as_deref(), Some("onboarding-agent"));
    let left = list_proposals(dir.path().to_path_buf()).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].tool_name, "file_read");
}

#[tokio::test]
async fn test_a_dismissed_last_proposal_empties_the_list_and_stores_no_rule() {
    // GIVEN one proposal
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &serde_json::json!([{"tool_name": "file_read", "action": "allow"}]),
    );

    // WHEN it is dismissed, then dismissed again
    dismiss_proposal(dir.path().to_path_buf(), 0).await.unwrap();
    let again = dismiss_proposal(dir.path().to_path_buf(), 0).await;

    // THEN the list is empty, the second dismissal says so, and no rule exists
    assert!(list_proposals(dir.path().to_path_buf())
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(again, Err(PermissionRuleError::NoProposals)));
    assert!(!dir.path().join(GOVERNANCE_DB_FILENAME).exists());
}

#[tokio::test]
async fn test_a_proposal_with_a_project_scope_is_refused_and_kept() {
    // GIVEN a proposal scoped to a project, which no rule can target
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &serde_json::json!([{"tool_name": "file_read", "action": "allow", "scope": "project"}]),
    );

    // WHEN it is applied
    let refused = apply_proposal(dir.path().to_path_buf(), 0).await;

    // THEN it is refused with the embedded words and stays in the list
    let error = refused.unwrap_err();
    assert_eq!(error.to_string(), "scope 'project' not persistable");
    assert_eq!(
        list_proposals(dir.path().to_path_buf())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_a_global_rule_by_hand_is_stored_and_a_project_scope_refused() {
    // GIVEN an empty data directory
    let dir = TempDir::new().unwrap();

    // WHEN a global rule is added, then one scoped to a project
    let id = add_global_rule(
        dir.path().to_path_buf(),
        "file_write".into(),
        Some("/tmp/".into()),
        "allow".into(),
        "global".into(),
    )
    .await
    .unwrap();
    let refused = add_global_rule(
        dir.path().to_path_buf(),
        "file_write".into(),
        None,
        "allow".into(),
        "project".into(),
    )
    .await;

    // THEN the first is stored for every agent, the second refused before any write
    let rules = stored_rules(dir.path());
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, id);
    assert_eq!(rules[0].scope, PermissionScope::Global);
    assert!(matches!(refused, Err(PermissionRuleError::InvalidRule(_))));
}
