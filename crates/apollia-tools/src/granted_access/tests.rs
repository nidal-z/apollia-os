use super::*;
use crate::agent_definition::{AgentOrigin, StoredAgent};
use serde_json::json;

fn manifest(name: &str, required: &[&str], optional: &[&str]) -> AgentManifest {
    serde_json::from_value(json!({
        "name": name,
        "version": "1.0.0",
        "description": "test agent",
        "tools_required": required,
        "tools_optional": optional,
    }))
    .unwrap()
}

fn store(dir: &std::path::Path, manifest: &AgentManifest) -> (PathBuf, AgentRepository) {
    let path = dir.join("agents.db");
    let repo = AgentRepository::open(&path).unwrap();
    repo.save_definition(&StoredAgent {
        name: manifest.name.clone(),
        origin: AgentOrigin::Package,
        package: None,
        version: "1".into(),
        definition: definition_from_manifest(manifest),
        enabled: true,
        installed_at: "2026-10-02T00:00:00Z".into(),
        updated_at: "2026-10-02T00:00:00Z".into(),
    })
    .unwrap();
    (path, repo)
}

// GIVEN an agent whose package declares a native tool, an MCP tool and a
// package tool, stored without override
// WHEN its access is read
// THEN the three are granted, whatever their provenance
#[test]
fn test_declared_access_grants_every_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest(
        "notes",
        &["file_read", "mcp:notion/search"],
        &["pkg:crm/lookup"],
    );
    let (path, _repo) = store(dir.path(), &m);

    let granted = GrantedAccess::new(Some(path), &m).current();

    assert_eq!(
        granted,
        vec!["file_read", "mcp:notion/search", "pkg:crm/lookup"]
    );
}

// GIVEN a stored agent granted a native tool by its package
// WHEN the user overrides its access with another native tool
// THEN the next read returns the override, not the declared tool
#[test]
fn test_native_tool_is_granted_and_revoked_by_the_access_field() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest("notes", &["file_read"], &[]);
    let (path, repo) = store(dir.path(), &m);
    let access = GrantedAccess::new(Some(path), &m);
    assert_eq!(access.current(), vec!["file_read"]);

    repo.set_override("notes", OverrideField::Access, &json!(["file_list"]))
        .unwrap();

    assert_eq!(access.current(), vec!["file_list"]);
}

// GIVEN a stored agent with no MCP tool declared
// WHEN the user grants then revokes an MCP tool through the access field
// THEN the MCP tool appears then disappears
#[test]
fn test_mcp_tool_is_granted_and_revoked_by_the_access_field() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest("notes", &["file_read"], &[]);
    let (path, repo) = store(dir.path(), &m);
    let access = GrantedAccess::new(Some(path), &m);

    repo.set_override(
        "notes",
        OverrideField::Access,
        &json!(["file_read", "mcp:notion/search"]),
    )
    .unwrap();
    let granted = access.current();
    repo.set_override("notes", OverrideField::Access, &json!(["file_read"]))
        .unwrap();
    let revoked = access.current();

    assert_eq!(granted, vec!["file_read", "mcp:notion/search"]);
    assert_eq!(revoked, vec!["file_read"]);
}

// GIVEN a stored agent with no package tool declared
// WHEN the user grants then revokes a package tool through the access field
// THEN the package tool appears then disappears
#[test]
fn test_package_tool_is_granted_and_revoked_by_the_access_field() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest("notes", &[], &[]);
    let (path, repo) = store(dir.path(), &m);
    let access = GrantedAccess::new(Some(path), &m);

    repo.set_override("notes", OverrideField::Access, &json!(["pkg:crm/lookup"]))
        .unwrap();
    let granted = access.current();
    repo.set_override("notes", OverrideField::Access, &json!([]))
        .unwrap();
    let revoked = access.current();

    assert_eq!(granted, vec!["pkg:crm/lookup"]);
    assert!(revoked.is_empty());
}

// GIVEN no agent store, then a store that does not hold the agent
// WHEN the access is read
// THEN it is the declared access, required then optional, each once
#[test]
fn test_unstored_agent_reads_its_declared_access() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest(
        "notes",
        &["file_read", "web_search"],
        &["web_search", "ask_user"],
    );
    let other = manifest("other", &[], &[]);
    let (path, _repo) = store(dir.path(), &other);

    let without_store = GrantedAccess::new(None, &m).current();
    let not_held = GrantedAccess::new(Some(path), &m).current();

    assert_eq!(without_store, vec!["file_read", "web_search", "ask_user"]);
    assert_eq!(not_held, without_store);
}

// GIVEN a file at the store path that is not a database
// WHEN the access is read
// THEN nothing is granted
#[test]
fn test_unreadable_store_grants_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.db");
    std::fs::write(&path, b"not a database").unwrap();
    let m = manifest("notes", &["file_read"], &[]);

    let granted = GrantedAccess::new(Some(path), &m).current();

    assert!(granted.is_empty());
}
