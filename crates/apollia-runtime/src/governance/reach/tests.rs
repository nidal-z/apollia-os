use super::*;
use apollia_core::source_registry::{NewSource, SourceRegistryStore};
use apollia_sources::SourceKind;
use apollia_tools::{AgentOrigin, AgentRepository, OverrideField, StoredAgent};
use serde_json::json;

/// Stores `folder` as a folder connection in the `system.db` of `data_dir`
/// and answers its connection identifier (`folder:<id>`).
pub(crate) fn seed_folder(data_dir: &Path, folder: &Path) -> String {
    let stored = FolderConnectionStore::open(&DataFile::System.path(data_dir))
        .unwrap()
        .add(folder.to_str().unwrap(), Some("code"))
        .unwrap();
    crate::connections::folder_connection_id(&stored.id)
}

/// Stores the agent `agent` in the `agents.db` of `data_dir` with a code
/// folder checked on `folder`, labelled `label`, and answers the identifier
/// of the folder's connection.
pub(crate) fn seed_code_folder(data_dir: &Path, agent: &str, folder: &Path, label: &str) -> String {
    let agents = AgentRepository::open(&DataFile::Agents.path(data_dir)).unwrap();
    agents
        .save_definition(&StoredAgent {
            name: agent.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1".into(),
            definition: json!({ "id": agent, "name": agent, "defaults": {} }),
            enabled: true,
            installed_at: "2026-10-27T00:00:00Z".into(),
            updated_at: "2026-10-27T00:00:00Z".into(),
        })
        .unwrap();
    let connection = seed_folder(data_dir, folder);
    let folder_id = crate::connections::folder_id_of(&connection).unwrap();
    let source = SourceRegistryStore::open(&DataFile::System.path(data_dir))
        .unwrap()
        .create(&NewSource {
            kind: SourceKind::CodeFolder,
            label: label.into(),
            group: None,
            reference: Some(folder_id.to_owned()),
        })
        .unwrap();
    agents
        .set_override(agent, OverrideField::Sources, &json!([source.id]))
        .unwrap();
    connection
}

fn paused(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn test_a_file_tool_reaches_the_folder_that_holds_its_absolute_path() {
    // GIVEN a folder connection, and a folder nested in it, the outer one in pause
    let data = tempfile::tempdir().unwrap();
    let outer = tempfile::tempdir().unwrap();
    let inner = outer.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let outer_id = seed_folder(data.path(), &outer.path().canonicalize().unwrap());
    let inner_id = seed_folder(data.path(), &inner.canonicalize().unwrap());
    let pauses = paused(&[&outer_id]);

    // WHEN a read, a write of a new file, a list and a relative read are reached
    let read = call_reaches(
        "dev",
        "file_read",
        &json!({ "path": outer.path().join("notes.md") }),
        Some(data.path()),
        &pauses,
    );
    let write = call_reaches(
        "dev",
        "file_write",
        &json!({ "path": inner.join("new/./draft.md"), "content": "x" }),
        Some(data.path()),
        &pauses,
    );
    let list = call_reaches(
        "dev",
        "file_list",
        &json!({ "dir": outer.path() }),
        Some(data.path()),
        &pauses,
    );
    let relative = call_reaches(
        "dev",
        "file_read",
        &json!({ "path": "notes.md" }),
        Some(data.path()),
        &pauses,
    );

    // THEN each absolute path reaches the deepest folder that holds it, even
    // through the unresolved temporary directory and a new file
    assert_eq!(read, vec![outer_id.clone()]);
    assert_eq!(write, vec![inner_id]);
    assert_eq!(list, vec![outer_id]);
    // AND a relative path reaches none: a folder in pause is no root
    assert!(relative.is_empty());
}

#[test]
fn test_nothing_is_read_while_no_folder_is_paused() {
    // GIVEN a data directory with no system.db, and only a server in pause
    let data = tempfile::tempdir().unwrap();

    // WHEN a file read is reached
    let reached = call_reaches(
        "dev",
        "file_read",
        &json!({ "path": "/anywhere/notes.md" }),
        Some(data.path()),
        &paused(&["mcp:notion"]),
    );

    // THEN it reaches nothing and no file is created
    assert!(reached.is_empty());
    assert!(!DataFile::System.path(data.path()).exists());
}

#[test]
fn test_code_context_reaches_the_code_folder_it_names_or_every_one() {
    // GIVEN the agent `dev` with one code folder checked
    let data = tempfile::tempdir().unwrap();
    let code = tempfile::tempdir().unwrap();
    let connection = seed_code_folder(data.path(), "dev", code.path(), "apollia");
    let pauses = paused(&[&connection]);

    // WHEN calls naming it, naming nothing, and naming another are reached
    let named = call_reaches(
        "dev",
        CODE_CONTEXT_TOOL,
        &json!({ "folder": "apollia" }),
        Some(data.path()),
        &pauses,
    );
    let unnamed = call_reaches(
        "dev",
        CODE_CONTEXT_TOOL,
        &json!({}),
        Some(data.path()),
        &pauses,
    );
    let other = call_reaches(
        "dev",
        CODE_CONTEXT_TOOL,
        &json!({ "folder": "elsewhere" }),
        Some(data.path()),
        &pauses,
    );

    // THEN the first two reach its connection, the third none
    assert_eq!(named, vec![connection.clone()]);
    assert_eq!(unnamed, vec![connection]);
    assert!(other.is_empty());
}
