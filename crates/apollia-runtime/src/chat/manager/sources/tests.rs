//! The source tools of a turn follow the sources checked for its agent.

use std::path::Path;
use std::sync::Arc;

use apollia_core::paths::DataFile;
use apollia_sources::{NewSource, SourceKind, SourceRegistryHandle};
use apollia_tools::tools::source_search::{SourceSearchError, SourceSearchInput};

use super::*;

fn config(data_dir: &Path, registry: Option<SourceRegistryHandle>) -> Arc<ChatToolsConfig> {
    Arc::new(ChatToolsConfig {
        data_dir: data_dir.to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        tool_turn_temperature: None,
        source_registry: registry,
        governance: None,
    })
}

/// Checks `sources` for the Apollia agent stored under `data_dir`.
fn check_for_apollia(data_dir: &Path, sources: &[&str]) {
    apollia_tools::AgentRepository::open(&DataFile::Agents.path(data_dir))
        .unwrap()
        .set_override(
            APOLLIA_CHAT_AGENT_ID,
            apollia_tools::OverrideField::Sources,
            &serde_json::json!(sources),
        )
        .unwrap();
}

async fn folder(registry: &SourceRegistryHandle, label: &str) -> String {
    registry
        .create(NewSource {
            kind: SourceKind::Folder,
            label: label.to_owned(),
            group: None,
            reference: Some(format!("folder-{label}")),
        })
        .await
        .unwrap()
        .id
}

fn granted() -> Vec<String> {
    ["file_read", "memory_search", "source_search", "web_search"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

#[tokio::test]
async fn test_apollia_with_a_folder_and_its_memory_checked_is_offered_both_tools() {
    // GIVEN a registry with two folders, and Apollia checking the first one
    //       and its memory
    let dir = tempfile::tempdir().unwrap();
    let registry = SourceRegistryHandle::open(&DataFile::System.path(dir.path())).unwrap();
    let contracts = folder(&registry, "Contracts").await;
    let _private = folder(&registry, "Private").await;
    check_for_apollia(dir.path(), &[contracts.as_str(), "memory"]);
    let cfg = config(dir.path(), Some(registry));

    // WHEN the sources of a turn of Apollia are read
    let access = source_access(Some(&cfg), APOLLIA_CHAT_AGENT_ID).await;

    // THEN both source tools are offered, the search bounded to the folder
    assert!(access.memory);
    let search = access.search.as_ref().unwrap();
    assert_eq!(search.sources(), [contracts]);
    assert_eq!(offered(granted(), &access), granted());
}

#[tokio::test]
async fn test_apollia_cannot_search_a_folder_it_did_not_check() {
    // GIVEN a registry with two folders, Apollia checking the first only
    let dir = tempfile::tempdir().unwrap();
    let registry = SourceRegistryHandle::open(&DataFile::System.path(dir.path())).unwrap();
    let contracts = folder(&registry, "Contracts").await;
    let private = folder(&registry, "Private").await;
    check_for_apollia(dir.path(), &[contracts.as_str()]);
    let cfg = config(dir.path(), Some(registry));
    let access = source_access(Some(&cfg), APOLLIA_CHAT_AGENT_ID).await;

    // WHEN its source_search names the second folder
    let refused = access
        .search
        .as_ref()
        .unwrap()
        .run(SourceSearchInput {
            query: "lease".to_owned(),
            sources: Some(vec![private.clone()]),
            limit: None,
        })
        .await;

    // THEN the call is refused, naming the folder
    match refused {
        Err(SourceSearchError::SourceNotChecked(id)) => assert_eq!(id, private),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn test_apollia_with_nothing_checked_is_offered_no_source_tool() {
    // GIVEN a registry with one folder, and Apollia checking only its profile
    let dir = tempfile::tempdir().unwrap();
    let registry = SourceRegistryHandle::open(&DataFile::System.path(dir.path())).unwrap();
    let _contracts = folder(&registry, "Contracts").await;
    check_for_apollia(dir.path(), &["profile"]);
    let cfg = config(dir.path(), Some(registry));

    // WHEN the sources of a turn of Apollia are read
    let access = source_access(Some(&cfg), APOLLIA_CHAT_AGENT_ID).await;

    // THEN neither memory_search nor source_search is offered
    assert!(!access.memory);
    assert!(access.search.is_none());
    assert_eq!(
        offered(granted(), &access),
        vec!["file_read".to_owned(), "web_search".to_owned()]
    );
}

#[tokio::test]
async fn test_without_a_registry_only_the_memory_can_be_checked() {
    // GIVEN Apollia checking its memory, with no registry of sources
    let dir = tempfile::tempdir().unwrap();
    check_for_apollia(dir.path(), &["memory"]);
    let cfg = config(dir.path(), None);

    // WHEN the sources of a turn are read, and of a turn without configuration
    let access = source_access(Some(&cfg), APOLLIA_CHAT_AGENT_ID).await;
    let bare = source_access(None, APOLLIA_CHAT_AGENT_ID).await;

    // THEN the memory is open and nothing is searchable; without
    //      configuration nothing is open
    assert!(access.memory);
    assert!(access.search.is_none());
    assert!(!bare.memory && bare.search.is_none());
}

#[test]
fn test_the_memory_of_apollia_is_one_space_across_its_conversations() {
    // GIVEN two conversations of Apollia and one of another agent
    // WHEN the memory namespace of each is read
    let first = memory_namespace(APOLLIA_CHAT_AGENT_ID, "s-1");
    let second = memory_namespace(APOLLIA_CHAT_AGENT_ID, "s-2");
    let other = memory_namespace("veille", "s-3");

    // THEN Apollia keeps one space, named after it, never the session's
    assert_eq!(first, APOLLIA_MEMORY_NAMESPACE);
    assert_eq!(first, second);
    assert!(!first.contains("s-1"));
    assert_eq!(other, "apollia:chat:s-3");
}
