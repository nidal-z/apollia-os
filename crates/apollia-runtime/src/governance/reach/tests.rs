use super::*;
use apollia_core::paths::DataFile;
use apollia_core::source_registry::{NewSource, SourceRegistryStore};
use apollia_core::FolderConnectionStore;
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

/// The folders of an agent granted `granted`, read from `data_dir` with
/// `pauses`, as the turn reads them once.
fn folders_of(data_dir: &Path, granted: &[&str], pauses: &HashSet<String>) -> FolderRoots {
    let access = granted
        .iter()
        .map(|connection| {
            let id = crate::connections::folder_id_of(connection).unwrap();
            apollia_tools::folder_access::folder_access_name(id, "file_read")
        })
        .collect();
    apollia_tools::granted_folders(access, data_dir, pauses).1
}

#[test]
fn test_a_file_tool_reaches_the_folder_that_holds_its_path() {
    // GIVEN a sound folder connection granted to the agent, a folder nested
    // in it in pause, and a link from the outer one to the inner one
    let data = tempfile::tempdir().unwrap();
    let outer = tempfile::tempdir().unwrap();
    let inner = outer.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&inner, outer.path().join("link")).unwrap();
    let outer_id = seed_folder(data.path(), &outer.path().canonicalize().unwrap());
    let inner_id = seed_folder(data.path(), &inner.canonicalize().unwrap());
    let pauses = paused(&[&inner_id]);
    let folders = folders_of(data.path(), &[&outer_id], &pauses);
    let caller = Caller {
        agent_id: "dev",
        data_dir: Some(data.path()),
        folders: &folders,
    };
    let reach = |tool: &str, input: Value| call_reaches(&caller, tool, &input, &pauses);

    // WHEN calls with an absolute, a relative, a `..`, a linked path, a new
    // file to write, and no path at all are reached
    let absolute = reach("file_read", json!({ "path": inner.join("n.md") }));
    let relative = reach("file_read", json!({ "path": "inner/n.md" }));
    let parent = reach("file_list", json!({ "dir": "x/../inner/" }));
    #[cfg(unix)]
    let linked = reach("file_read", json!({ "path": "link/n.md" }));
    let write = reach(
        "file_write",
        json!({ "path": inner.join("new/./draft.md"), "content": "x" }),
    );
    let sound = reach("file_read", json!({ "path": "notes.md" }));
    let unnamed = reach("file_grep", json!({ "pattern": "x" }));

    // THEN each path that lands in the folder in pause reaches it, read as
    // the tools read it: a relative path from the first granted folder
    for reached in [&absolute, &relative, &parent, &write] {
        assert_eq!(reached, &vec![inner_id.clone()]);
    }
    #[cfg(unix)]
    assert_eq!(linked, vec![inner_id.clone()]);
    // AND a path of the sound folder, or none, reaches the sound folder
    assert_eq!(sound, vec![outer_id.clone()]);
    assert_eq!(unnamed, vec![outer_id]);
}

#[test]
fn test_nothing_is_resolved_while_no_folder_is_paused() {
    // GIVEN a data directory with no system.db, and only a server in pause
    let data = tempfile::tempdir().unwrap();

    // WHEN a file read is reached
    let reached = call_reaches(
        &Caller {
            agent_id: "dev",
            data_dir: Some(data.path()),
            folders: &FolderRoots::default(),
        },
        "file_read",
        &json!({ "path": "/anywhere/notes.md" }),
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
        &Caller {
            agent_id: "dev",
            data_dir: Some(data.path()),
            folders: &FolderRoots::default(),
        },
        CODE_CONTEXT_TOOL,
        &json!({ "folder": "apollia" }),
        &pauses,
    );
    let unnamed = call_reaches(
        &Caller {
            agent_id: "dev",
            data_dir: Some(data.path()),
            folders: &FolderRoots::default(),
        },
        CODE_CONTEXT_TOOL,
        &json!({}),
        &pauses,
    );
    let other = call_reaches(
        &Caller {
            agent_id: "dev",
            data_dir: Some(data.path()),
            folders: &FolderRoots::default(),
        },
        CODE_CONTEXT_TOOL,
        &json!({ "folder": "elsewhere" }),
        &pauses,
    );

    // THEN the first two reach its connection, the third none
    assert_eq!(named, vec![connection.clone()]);
    assert_eq!(unnamed, vec![connection]);
    assert!(other.is_empty());
}
