use super::*;
use apollia_core::source_registry::NewSource;
use apollia_tools::{AgentOrigin, OverrideField, StoredAgent};
use serde_json::json;

/// A data directory holding the agent `dev`, a folder connection on `code`
/// and, when `kind` is given, a source of that kind on the connection,
/// checked for `dev`.
fn data_dir_with(kind: Option<SourceKind>) -> (tempfile::TempDir, tempfile::TempDir) {
    let data = tempfile::tempdir().unwrap();
    let code = tempfile::tempdir().unwrap();
    std::fs::write(code.path().join("APOLLIA.md"), "Tests first.").unwrap();
    let agents = AgentRepository::open(&DataFile::Agents.path(data.path())).unwrap();
    agents
        .save_definition(&StoredAgent {
            name: "dev".into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1".into(),
            definition: json!({ "id": "dev", "name": "Dev", "defaults": {} }),
            enabled: true,
            installed_at: "2026-10-04T00:00:00Z".into(),
            updated_at: "2026-10-04T00:00:00Z".into(),
        })
        .unwrap();
    let system = DataFile::System.path(data.path());
    let connection = FolderConnectionStore::open(&system)
        .unwrap()
        .add(code.path().to_str().unwrap(), Some("apollia"))
        .unwrap();
    let mut checked = vec![json!("profile")];
    if let Some(kind) = kind {
        let source = SourceRegistryStore::open(&system)
            .unwrap()
            .create(&NewSource {
                kind,
                label: "apollia".into(),
                group: None,
                reference: Some(connection.id),
            })
            .unwrap();
        checked.push(json!(source.id));
    }
    agents
        .set_override("dev", OverrideField::Sources, &json!(checked))
        .unwrap();
    (data, code)
}

#[tokio::test]
async fn test_a_checked_code_folder_gives_the_agent_its_code_context() {
    // GIVEN the agent `dev` with a code folder checked on a folder connection
    let (data, _code) = data_dir_with(Some(SourceKind::CodeFolder));

    // WHEN its code context is built, then called
    let tool = code_context_for(data.path(), "dev").expect("a code folder is checked");
    let out = apollia_tools::executor::ToolExecutor::execute(&tool, json!({ "parts": ["rules"] }))
        .await
        .unwrap();

    // THEN the tool reads the folder of the connection
    assert_eq!(out["folders"][0]["label"], "apollia");
    assert!(out["folders"][0]["sections"][0]["content"]
        .as_str()
        .unwrap()
        .contains("Tests first."));
}

#[test]
fn test_without_a_code_folder_checked_there_is_no_code_context() {
    // GIVEN the agent `dev` with an indexed folder on the same connection,
    // another with nothing but the profile, and an agent nobody installed
    let (with_folder, _a) = data_dir_with(Some(SourceKind::Folder));
    let (profile_only, _b) = data_dir_with(None);

    // WHEN the code context of each is asked
    let folder = code_context_for(with_folder.path(), "dev");
    let profile = code_context_for(profile_only.path(), "dev");
    let absent = code_context_for(profile_only.path(), "nobody");

    // THEN none has the tool
    assert!(folder.is_none());
    assert!(profile.is_none());
    assert!(absent.is_none());
}
