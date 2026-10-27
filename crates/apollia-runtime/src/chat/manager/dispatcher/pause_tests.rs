//! A folder paused during a turn: the file tools built for that turn read the
//! pause at their next call, as the decision on that call does.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use apollia_core::FolderConnection;

use super::*;

/// The secret held by the folder paused during the turn.
const SECRET: &str = "SECRETZZ held by the paused folder";

fn stored(id: &str, path: &Path) -> FolderConnection {
    FolderConnection {
        id: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        label: id.to_owned(),
        added_at: "2026-10-27T00:00:00Z".to_owned(),
    }
}

fn governance(dir: &Path) -> crate::governance::GovernanceHandle {
    crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
        governance_db: Some(dir.join("governance.db")),
        config_path: Some(dir.join("apollia.toml")),
        policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
        journal: None,
        events: None,
        forbidden_connections: Default::default(),
    })
}

/// The text of a call's answer, its error included.
async fn answer(
    dispatcher: &apollia_tools::executor::ToolDispatcher,
    tool: &str,
    input: serde_json::Value,
) -> String {
    match dispatcher.dispatch(tool, input).await {
        Ok(value) => value.to_string(),
        Err(error) => format!("error: {error}"),
    }
}

// GIVEN the dispatcher of a turn over a sound folder `outer` that holds a
//       folder `inner`, nothing in pause when the turn started
// WHEN `inner` is paused during the turn, then the agent greps without a
//      path, globs, lists recursively, reads and writes
// THEN nothing of `inner` is read, named or written, and `outer` is still
//      served
#[tokio::test]
async fn test_a_folder_paused_during_the_turn_is_neither_read_nor_written() {
    // GIVEN
    let data = tempfile::tempdir().expect("tempdir");
    let files = tempfile::tempdir().expect("tempdir");
    let outer = files.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).expect("mkdir");
    std::fs::write(outer.join("o.md"), "outer note").expect("write");
    std::fs::write(inner.join("n.md"), SECRET).expect("write");
    let handle = governance(data.path());
    let access = vec![
        apollia_tools::folder_access::folder_access_name("outer", "file_read"),
        apollia_tools::folder_access::folder_access_name("outer", "file_write"),
    ];
    let paused: HashSet<String> = handle.state().paused.clone();
    let (_, folders) = apollia_tools::resolve_folder_access(
        access,
        &[stored("outer", &outer), stored("inner", &inner)],
        &paused,
    );
    let cfg = Arc::new(ChatToolsConfig {
        data_dir: data.path().to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        tool_turn_temperature: None,
        source_registry: None,
        governance: Some(handle.clone()),
    });
    let dispatcher = build_full_chat_dispatcher(FullDispatcherParams {
        cfg: &cfg,
        session_id: "pause-during-turn",
        folders: &folders,
        pending_user_inputs: &None,
        extra_executors: Vec::new(),
        sources: apollia_tools::SourceAccess::default(),
        memory_namespace: "apollia".to_owned(),
    });

    // WHEN
    assert!(handle
        .set_paused("folder:inner", true)
        .await
        .expect("pause"));
    let grep = answer(
        &dispatcher,
        "file_grep",
        serde_json::json!({ "pattern": "SECRETZZ|outer note" }),
    )
    .await;
    let glob = answer(
        &dispatcher,
        "file_glob",
        serde_json::json!({ "pattern": "**/*.md" }),
    )
    .await;
    let list = answer(
        &dispatcher,
        "file_list",
        serde_json::json!({ "recursive": true }),
    )
    .await;
    let read = answer(
        &dispatcher,
        "file_read",
        serde_json::json!({ "path": "inner/n.md" }),
    )
    .await;
    let missing = answer(
        &dispatcher,
        "file_read",
        serde_json::json!({ "path": "n.md" }),
    )
    .await;
    let write = answer(
        &dispatcher,
        "file_write",
        serde_json::json!({ "path": "inner/w.md", "content": "x" }),
    )
    .await;

    // THEN
    for said in [&grep, &glob, &list, &read, &missing, &write] {
        assert!(!said.contains("SECRETZZ"), "{said}");
    }
    // A refusal repeats the path the agent gave; nothing else names `inner`.
    for said in [&grep, &glob, &list, &missing] {
        assert!(!said.contains("n.md\""), "{said}");
        assert!(!said.contains("inner/"), "{said}");
    }
    assert!(!list.contains("\"inner\""), "{list}");
    assert!(read.contains("paused"), "{read}");
    assert!(write.contains("paused"), "{write}");
    assert!(!inner.join("w.md").exists());
    assert!(grep.contains("outer note"), "{grep}");
    assert!(glob.contains("o.md"), "{glob}");
    assert!(list.contains("o.md"), "{list}");
}
