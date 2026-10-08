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
        apollia_tools::AgentAccess::Default => vec!["file_read".to_owned()],
    };
    let (_, folders) = granted_folders(tools, Some(&cfg));
    resolve_workspace_for_session(WorkspaceResolutionParams {
        folders,
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

/// The access the chat holds once the reading tools were chosen on the sheet
/// of Apollia: the native names, with no folder among them.
fn reading_natives() -> Vec<String> {
    ["file_glob", "file_grep", "file_list", "file_read"]
        .map(str::to_owned)
        .to_vec()
}

async fn list(
    invoker: &NativeChatToolInvoker,
    dir: &std::path::Path,
    recursive: bool,
) -> Result<String, String> {
    invoker
        .invoke(
            "file_list",
            &serde_json::json!({ "dir": dir.to_string_lossy(), "recursive": recursive }),
        )
        .await
}

#[tokio::test]
async fn a_folder_added_as_a_connection_is_not_reached_until_it_is_granted_to_apollia() {
    // GIVEN a folder stored as a connection, and Apollia holding the reading
    // tools by name but no access to that folder
    let data = tempfile::tempdir().expect("tempdir");
    let downloads = tempfile::tempdir().expect("tempdir");
    std::fs::write(downloads.path().join("invoice.pdf"), "x").expect("seed");
    add_folder(data.path(), downloads.path());
    grant_apollia(data.path(), &reading_natives());
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat lists the folder
    let listing = list(&invoker, downloads.path(), false).await;

    // THEN the call is refused: adding the connection granted it to no agent
    let refusal = listing.expect_err("a folder nobody granted is not listed");
    assert!(refusal.contains("no_folder_granted"), "{refusal}");
}

#[tokio::test]
async fn a_granted_folder_is_listed_and_read_through_every_level_and_never_beyond_its_root() {
    // GIVEN a folder holding a file two levels down, granted to Apollia for
    // its reading tools, and a sibling folder that is not granted
    let data = tempfile::tempdir().expect("tempdir");
    let parent = tempfile::tempdir().expect("tempdir");
    let granted = parent.path().join("Downloads");
    let sibling = parent.path().join("Documents");
    let deep = granted.join("clients").join("2026");
    std::fs::create_dir_all(&deep).expect("seed");
    std::fs::create_dir_all(&sibling).expect("seed");
    std::fs::write(deep.join("quote.txt"), "two levels down").expect("seed");
    std::fs::write(sibling.join("private.txt"), "not granted").expect("seed");
    let id = add_folder(data.path(), &granted);
    let mut access = reading_natives();
    access.extend(
        ["file_read", "file_list"]
            .map(|tool| apollia_tools::folder_access::folder_access_name(&id, tool)),
    );
    grant_apollia(data.path(), &access);
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat lists the root recursively, lists the deepest folder,
    // reads its file by an absolute and by a relative path, then reaches for
    // the sibling by its own path and by climbing out of the root
    let whole = list(&invoker, &granted, true).await;
    let deepest = list(&invoker, &deep, false).await;
    let absolute = read(&invoker, &deep.join("quote.txt")).await;
    let relative = invoker
        .invoke(
            "file_read",
            &serde_json::json!({ "path": "clients/2026/quote.txt" }),
        )
        .await;
    let sibling_list = list(&invoker, &sibling, false).await;
    let sibling_read = read(&invoker, &sibling.join("private.txt")).await;
    let climbed = read(&invoker, &deep.join("../../../Documents/private.txt")).await;

    // THEN every level under the root answers
    assert!(
        whole.as_ref().is_ok_and(|text| text.contains("quote.txt")),
        "{whole:?}"
    );
    assert!(
        deepest
            .as_ref()
            .is_ok_and(|text| text.contains("quote.txt")),
        "{deepest:?}"
    );
    assert!(
        absolute
            .as_ref()
            .is_ok_and(|text| text.contains("two levels down")),
        "{absolute:?}"
    );
    assert!(
        relative
            .as_ref()
            .is_ok_and(|text| text.contains("two levels down")),
        "{relative:?}"
    );
    // AND nothing outside the root does
    for outside in [sibling_list, sibling_read, climbed] {
        let refusal = outside.expect_err("outside the granted root");
        assert!(
            refusal.contains("outside every folder granted"),
            "{refusal}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_inside_a_granted_folder_does_not_lead_out_of_it() {
    // GIVEN a granted folder whose sub-folder holds a symbolic link to a
    // folder that is not granted
    let data = tempfile::tempdir().expect("tempdir");
    let granted = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    let sub = granted.path().join("sub");
    std::fs::create_dir_all(&sub).expect("seed");
    std::fs::write(other.path().join("secret.txt"), "elsewhere").expect("seed");
    std::os::unix::fs::symlink(other.path(), sub.join("escape")).expect("link");
    let id = add_folder(data.path(), granted.path());
    let mut access = reading_natives();
    access.extend(
        ["file_read", "file_list"]
            .map(|tool| apollia_tools::folder_access::folder_access_name(&id, tool)),
    );
    grant_apollia(data.path(), &access);
    let invoker = apollia_invoker(data.path()).await;

    // WHEN the chat lists and reads through the link
    let listed = list(&invoker, &sub.join("escape"), false).await;
    let read = read(&invoker, &sub.join("escape").join("secret.txt")).await;

    // THEN both are refused
    for through_link in [listed, read] {
        let refusal = through_link.expect_err("a link out of the root is refused");
        assert!(
            refusal.contains("outside every folder granted"),
            "{refusal}"
        );
    }
}
