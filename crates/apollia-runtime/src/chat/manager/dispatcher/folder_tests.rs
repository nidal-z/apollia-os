//! The chat path: the file tools of the Apollia agent reach the folders
//! granted to it, and nothing else.

use super::*;
use apollia_core::paths::DataFile;
use apollia_llm::ToolInvoker;

fn config(data_dir: &std::path::Path) -> Arc<ChatToolsConfig> {
    Arc::new(ChatToolsConfig {
        data_dir: data_dir.to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        tool_turn_temperature: None,
        source_registry: None,
        governance: None,
    })
}

/// Store `path` as a folder connection of `data_dir` and answer its id.
fn add_folder(data_dir: &std::path::Path, path: &std::path::Path) -> String {
    apollia_core::FolderConnectionStore::open(&DataFile::System.path(data_dir))
        .expect("open system.db")
        .add(&path.to_string_lossy(), None)
        .expect("add the folder")
        .id
}

/// Set the access of the Apollia agent stored in `data_dir`.
fn grant_apollia(data_dir: &std::path::Path, access: &[String]) {
    apollia_tools::AgentRepository::open(&DataFile::Agents.path(data_dir))
        .expect("open agents.db")
        .set_override(
            apollia_tools::APOLLIA_AGENT_ID,
            apollia_tools::OverrideField::Access,
            &serde_json::json!(access),
        )
        .expect("set the access");
}

/// The invoker of a free-chat message of the Apollia agent over `data_dir`,
/// built as the chat builds it: its access, then its folders.
async fn apollia_invoker(data_dir: &std::path::Path) -> NativeChatToolInvoker {
    let cfg = config(data_dir);
    let access =
        super::super::apollia_agent::apollia_access(Some(&DataFile::Governance.path(data_dir)))
            .expect("a data directory gives an access");
    let tools = match access {
        apollia_tools::AgentAccess::Listed(tools) => tools,
        apollia_tools::AgentAccess::ReadingFamily => vec!["file_read".to_owned()],
    };
    let (_, folders) = granted_folders(tools, Some(&cfg));
    resolve_workspace_for_session(WorkspaceResolutionParams {
        folders,
        hitl: Some(super::tests::preapproved_hitl("s-folders")),
        pending_user_inputs: None,
        mcp_handle: None,
        chat_tools_config: Some(cfg),
        session_id: "s-folders".to_owned(),
        mcp_loading: LoadingMode::Eager,
        mcp_index: Vec::new(),
        tool_search_limit: 20,
        package_tools: Vec::new(),
        sources: apollia_tools::SourceAccess::default(),
        memory_namespace: "apollia".to_owned(),
    })
    .await
    .expect("the invoker is built")
}

async fn read(invoker: &NativeChatToolInvoker, path: &std::path::Path) -> Result<String, String> {
    invoker
        .invoke(
            "file_read",
            &serde_json::json!({ "path": path.to_string_lossy() }),
        )
        .await
}

#[tokio::test]
async fn the_chat_reads_in_the_folder_granted_to_apollia_and_nowhere_else() {
    // GIVEN Apollia granted one folder to read, and a second folder not granted
    let data = tempfile::tempdir().expect("tempdir");
    let granted = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    std::fs::write(granted.path().join("in.txt"), "inside").expect("seed");
    std::fs::write(other.path().join("out.txt"), "outside").expect("seed");
    let id = add_folder(data.path(), granted.path());
    grant_apollia(data.path(), &[format!("conn:folder:{id}/file_read")]);
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat reads in each folder
    let inside = read(&invoker, &granted.path().join("in.txt")).await;
    let outside = read(&invoker, &other.path().join("out.txt")).await;

    // THEN the granted folder is read, the other is refused
    assert!(inside.is_ok_and(|text| text.contains("inside")));
    let refusal = outside.expect_err("a folder not granted is refused");
    assert!(
        refusal.contains("outside every folder granted"),
        "{refusal}"
    );
}

#[tokio::test]
async fn without_a_granted_folder_the_chat_file_tools_refuse_readably() {
    // GIVEN Apollia granted the file tools but no folder
    let data = tempfile::tempdir().expect("tempdir");
    let home_file = tempfile::tempdir().expect("tempdir");
    std::fs::write(home_file.path().join("notes.txt"), "x").expect("seed");
    grant_apollia(
        data.path(),
        &["file_read".to_owned(), "file_write".to_owned()],
    );
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat reads and writes a file
    let read = read(&invoker, &home_file.path().join("notes.txt")).await;
    let write = invoker
        .invoke(
            "file_write",
            &serde_json::json!({ "path": "notes.txt", "content": "y" }),
        )
        .await;

    // THEN both are refused, saying how to grant a folder
    for result in [read, write] {
        let message = result.expect_err("no folder, no file");
        assert!(message.contains("no folder is granted"), "{message}");
    }
}

#[tokio::test]
async fn the_folder_of_a_project_is_not_reached_unless_granted() {
    // GIVEN a session's project folder, not granted, and Apollia granted
    // another folder
    let data = tempfile::tempdir().expect("tempdir");
    let project = tempfile::tempdir().expect("tempdir");
    let granted = tempfile::tempdir().expect("tempdir");
    std::fs::write(project.path().join("plan.md"), "secret").expect("seed");
    let id = add_folder(data.path(), granted.path());
    grant_apollia(data.path(), &[format!("conn:folder:{id}/file_read")]);
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat reads a file of the project folder, by its absolute path
    // and by a path relative to the session
    let absolute = read(&invoker, &project.path().join("plan.md")).await;
    let relative = invoker
        .invoke("file_read", &serde_json::json!({ "path": "plan.md" }))
        .await;

    // THEN both are refused: the project folder is neither a root nor the
    // anchor, which is the granted folder
    assert!(absolute.is_err_and(|m| m.contains("outside every folder granted")));
    assert!(relative.is_err(), "{relative:?}");
    assert_eq!(
        invoker.workspace_path().map(std::path::Path::to_path_buf),
        Some(granted.path().canonicalize().expect("canonical"))
    );
}
