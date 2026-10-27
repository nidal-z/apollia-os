//! Tests of the folder routes: what is admitted, stored, answered and
//! journaled.

use std::sync::Arc;

use apollia_tools::ActionFamily;

use super::*;
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::{connection_run_id, ConnectionAdmission, PolicyReadError};

struct RefuseFolders;

impl ConnectionAdmission for RefuseFolders {
    fn read_decision(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(
            if request.kind == crate::connections::ConnectionKind::Folder {
                AdmissionDecision::Refused {
                    reason: "folders are not allowed".into(),
                }
            } else {
                AdmissionDecision::Admitted
            },
        )
    }
}

fn body(path: &std::path::Path) -> AddFolderBody {
    AddFolderBody {
        path: path.to_str().unwrap().to_owned(),
        label: None,
    }
}

#[tokio::test]
async fn test_a_folder_is_admitted_stored_answered_and_journaled() {
    // GIVEN governance over a journal, an empty system.db and a folder
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    let store = store(dir.path()).unwrap();

    // WHEN the folder is added
    let connection = add(&governance, &store, &body(folder.path())).unwrap();

    // THEN it is a local folder connection of the selection, kept in the table
    let canonical = std::fs::canonicalize(folder.path()).unwrap();
    assert!(connection.id.starts_with("folder:"));
    assert_eq!(connection.kind, crate::connections::ConnectionKind::Folder);
    assert_eq!(connection.path.as_deref(), canonical.to_str());
    assert!(!connection.remote);
    assert_eq!(store.list().unwrap()[0].path, canonical.to_str().unwrap());
    // AND its connection run records `folder_added` with the path
    let entries = journal.query_run(&connection_run_id(&connection.id)).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, JournalEntryKind::FolderAdded);
    assert_eq!(entries[0].payload["path"], canonical.to_str().unwrap());
    journal.shutdown().await;
}

#[test]
fn test_a_folder_brings_the_native_file_tools_with_their_families() {
    // GIVEN a folder added
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let governance = ConnectionGovernance::without_policy(None);
    let store = store(dir.path()).unwrap();

    // WHEN its connection is built
    let connection = add(&governance, &store, &body(folder.path())).unwrap();

    // THEN reading, listing and searching read and are offered checked;
    // writing and editing write or delete and are offered unchecked; each is
    // granted for this folder alone
    let prefix = format!(
        "conn:folder:{}/",
        connection.id.trim_start_matches("folder:")
    );
    let tools: Vec<(String, ActionFamily, bool)> = connection
        .tools
        .iter()
        .map(|t| (t.id.replace(&prefix, ""), t.family, t.prechecked))
        .collect();
    assert!(connection.tools.iter().all(|t| t.id.starts_with(&prefix)));
    let expected: Vec<(String, ActionFamily, bool)> = [
        ("file_edit", ActionFamily::WriteOrDelete, false),
        ("file_glob", ActionFamily::Read, true),
        ("file_grep", ActionFamily::Read, true),
        ("file_list", ActionFamily::Read, true),
        ("file_read", ActionFamily::Read, true),
        ("file_write", ActionFamily::WriteOrDelete, false),
    ]
    .into_iter()
    .map(|(id, family, checked)| (id.to_owned(), family, checked))
    .collect();
    assert_eq!(tools, expected);
}

#[tokio::test]
async fn test_a_refused_folder_is_neither_stored_nor_journaled() {
    // GIVEN a policy refusing folders, over a journal
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::new(Arc::new(RefuseFolders), Some(journal.clone()));
    let store = store(dir.path()).unwrap();

    // WHEN the folder is added
    let (status, Json(err)) = add(&governance, &store, &body(folder.path())).unwrap_err();

    // THEN the admission's reason is answered as is, and nothing is kept
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "admission_refused");
    assert_eq!(err.error, "folders are not allowed");
    assert!(store.list().unwrap().is_empty());
    journal.shutdown().await;
}

#[test]
fn test_a_path_that_is_not_a_folder_or_is_already_added_is_refused() {
    // GIVEN a folder already added, and a file
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let file = folder.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    let governance = ConnectionGovernance::without_policy(None);
    let store = store(dir.path()).unwrap();
    add(&governance, &store, &body(folder.path())).unwrap();

    // WHEN the folder is added again, then the file
    let (again, Json(again_err)) = add(&governance, &store, &body(folder.path())).unwrap_err();
    let (file_status, Json(file_err)) = add(&governance, &store, &body(&file)).unwrap_err();

    // THEN each answers its status and code
    assert_eq!(again, StatusCode::CONFLICT);
    assert_eq!(again_err.code, "already_added");
    assert_eq!(file_status, StatusCode::BAD_REQUEST);
    assert_eq!(file_err.code, "not_a_folder");
}

#[tokio::test]
async fn test_removing_a_folder_connection_removes_its_row_and_is_journaled() {
    // GIVEN a folder added, over a journal
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    let store = store(dir.path()).unwrap();
    let added = add(&governance, &store, &body(folder.path())).unwrap();
    let id = crate::connections::folder_id_of(&added.id).unwrap();

    // WHEN it is removed, then removed again
    let removed = remove(&governance, &store, id).unwrap();
    let (status, Json(err)) = remove(&governance, &store, id).unwrap_err();

    // THEN the table is empty, the removal is journaled, and a second removal
    // finds nothing
    assert_eq!(removed.id, added.id);
    assert!(store.list().unwrap().is_empty());
    let kinds: Vec<JournalEntryKind> = journal
        .query_run(&connection_run_id(&added.id))
        .await
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            JournalEntryKind::FolderAdded,
            JournalEntryKind::ConnectionRemoved
        ]
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err.code, "not_found");
    journal.shutdown().await;
}

/// A runtime on `dir` with its source registry, one folder connection and,
/// on it, one indexed folder source whose index file exists.
pub(crate) async fn folder_with_source(
    dir: &tempfile::TempDir,
    folder: &tempfile::TempDir,
) -> (
    crate::api::server::AppState<crate::api::test_support::MockBackend>,
    String,
    String,
    std::path::PathBuf,
) {
    let mut state = crate::api::test_support::app_state(dir.path().to_path_buf());
    let registry = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let stored = store(dir.path())
        .unwrap()
        .add(folder.path().to_str().unwrap(), Some("Office"))
        .unwrap();
    let source = registry
        .create(apollia_sources::NewSource {
            kind: SourceKind::Folder,
            label: "Office documents".to_owned(),
            group: None,
            reference: Some(stored.id.clone()),
        })
        .await
        .unwrap();
    let index = apollia_sources::index::SourceIndex::open(dir.path(), &source.id).unwrap();
    drop(index);
    let index_file = apollia_core::paths::source_index_path(dir.path(), &source.id).unwrap();
    state.source_registry = Some(registry);
    (state, stored.id, source.id, index_file)
}

#[tokio::test]
async fn test_a_folder_sources_depend_on_is_kept_and_its_sources_listed_without_confirmation() {
    // GIVEN a folder connection with an indexed folder source on it
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let (state, folder_id, source_id, index_file) = folder_with_source(&dir, &folder).await;

    // WHEN the connection is removed without confirmation
    let (status, body) = crate::api::test_support::call(
        state.clone(),
        "DELETE",
        &format!("/api/v1/connections/folders/{folder_id}"),
        None,
    )
    .await;

    // THEN the route refuses with the dependent sources, and removes nothing
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "has_dependent_sources");
    assert_eq!(body["dependent_sources"][0]["id"], source_id.as_str());
    assert_eq!(body["dependent_sources"][0]["label"], "Office documents");
    assert_eq!(body["dependent_sources"][0]["kind"], "folder");
    assert!(store(dir.path())
        .unwrap()
        .get(&folder_id)
        .unwrap()
        .is_some());
    let registry = state.source_registry.clone().unwrap();
    assert_eq!(registry.list().await.unwrap().len(), 1);
    assert!(index_file.is_file());
}

#[tokio::test]
async fn test_a_confirmed_removal_takes_the_sources_and_their_indexes_with_the_folder() {
    // GIVEN a folder connection with an indexed folder source on it
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let (state, folder_id, _source_id, index_file) = folder_with_source(&dir, &folder).await;

    // WHEN the connection is removed with confirmation
    let (status, body) = crate::api::test_support::call(
        state.clone(),
        "DELETE",
        &format!("/api/v1/connections/folders/{folder_id}?confirm=true"),
        None,
    )
    .await;

    // THEN the connection, its source and the source's index are gone
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], format!("folder:{folder_id}"));
    assert!(store(dir.path())
        .unwrap()
        .get(&folder_id)
        .unwrap()
        .is_none());
    let registry = state.source_registry.clone().unwrap();
    assert!(registry.list().await.unwrap().is_empty());
    assert!(!index_file.exists());
}

#[tokio::test]
async fn test_a_folder_no_source_depends_on_is_removed_without_confirmation() {
    // GIVEN a folder connection and a registry holding no source on it
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let registry = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let stored = store(dir.path())
        .unwrap()
        .add(folder.path().to_str().unwrap(), None)
        .unwrap();
    let governance = ConnectionGovernance::without_policy(None);

    // WHEN it is removed without confirmation
    let removed =
        remove_with_sources(&governance, dir.path(), Some(&registry), &stored.id, false).await;

    // THEN it is removed
    assert!(removed.is_ok());
    assert!(store(dir.path())
        .unwrap()
        .get(&stored.id)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_a_folder_connection_chosen_again_keeps_its_identifier_and_is_journaled() {
    // GIVEN a folder connection over a journal, and the folder it moved to
    let dir = tempfile::TempDir::new().unwrap();
    let old = tempfile::TempDir::new().unwrap();
    let new = tempfile::TempDir::new().unwrap();
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    let store = store(dir.path()).unwrap();
    let added = add(&governance, &store, &body(old.path())).unwrap();
    let stored_id = added.id.trim_start_matches("folder:").to_owned();

    // WHEN it is pointed at the new folder
    let moved = repoint(
        &governance,
        &store,
        &stored_id,
        new.path().to_str().unwrap(),
    )
    .unwrap();

    // THEN the connection is the same one, on the new canonical path, and the
    // table still holds one row
    let canonical = std::fs::canonicalize(new.path()).unwrap();
    assert_eq!(moved.id, added.id);
    assert_eq!(moved.path.as_deref(), canonical.to_str());
    assert_eq!(store.list().unwrap().len(), 1);
    // AND its connection run records the addition, then the modification
    let entries = journal.query_run(&connection_run_id(&added.id)).await;
    let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            JournalEntryKind::FolderAdded,
            JournalEntryKind::ConnectionModified
        ]
    );
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_folder_the_admission_refuses_is_not_chosen_again() {
    // GIVEN a folder connection added before a policy that refuses folders
    let dir = tempfile::TempDir::new().unwrap();
    let old = tempfile::TempDir::new().unwrap();
    let new = tempfile::TempDir::new().unwrap();
    let store = store(dir.path()).unwrap();
    let added = add(
        &ConnectionGovernance::without_policy(None),
        &store,
        &body(old.path()),
    )
    .unwrap();
    let stored_id = added.id.trim_start_matches("folder:").to_owned();
    let refusing = ConnectionGovernance::new(Arc::new(RefuseFolders), None);

    // WHEN it is pointed at another folder, and an unknown connection too
    let (status, Json(err)) =
        repoint(&refusing, &store, &stored_id, new.path().to_str().unwrap()).unwrap_err();
    let (unknown, Json(unknown_err)) = repoint(
        &ConnectionGovernance::without_policy(None),
        &store,
        "nope",
        new.path().to_str().unwrap(),
    )
    .unwrap_err();

    // THEN the admission's reason is answered and the row keeps its folder;
    // an unknown connection is not found
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "admission_refused");
    assert_eq!(store.list().unwrap()[0].path, added.path.unwrap());
    assert_eq!(unknown, StatusCode::NOT_FOUND);
    assert_eq!(unknown_err.code, "not_found");
}

/// An embedder with no model: the indexes are built for keywords alone.
struct NoModel;

impl apollia_sources::PassageEmbedder for NoModel {
    async fn model(&self) -> Result<String, apollia_llm::embeddings_client::EmbeddingError> {
        Err(apollia_llm::embeddings_client::EmbeddingError::NoModel)
    }

    async fn embed_passages(
        &self,
        _texts: &[String],
    ) -> Result<apollia_sources::index::ChunkVectors, apollia_llm::embeddings_client::EmbeddingError>
    {
        Err(apollia_llm::embeddings_client::EmbeddingError::NoModel)
    }
}

/// The documents `source_search` finds for `query` in the source `id`.
async fn found(registry: &SourceRegistryHandle, id: &str, query: &str) -> Vec<String> {
    let outcome = registry
        .search(apollia_sources::SourcesQuery {
            sources: vec![id.to_owned()],
            query: query.to_owned(),
            vector: None,
            limit: None,
        })
        .await
        .unwrap();
    outcome.hits.into_iter().map(|h| h.document).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_chosen_again_is_searched_in_its_new_content_at_once() {
    // GIVEN a folder source indexed on a folder about Lyon, an indexer that
    // sweeps once an hour, and another folder about Nantes
    let dir = tempfile::TempDir::new().unwrap();
    let old = tempfile::TempDir::new().unwrap();
    let new = tempfile::TempDir::new().unwrap();
    std::fs::write(old.path().join("old.txt"), "The lease of the Lyon office.").unwrap();
    std::fs::write(
        new.path().join("new.txt"),
        "The lease of the Nantes office.",
    )
    .unwrap();
    let (state, folder_id, source_id, _) = folder_with_source(&dir, &old).await;
    let registry = state.source_registry.clone().unwrap();
    registry
        .start_indexing_every(NoModel, std::time::Duration::from_secs(3600))
        .await
        .unwrap();
    registry.refresh_indexes().await.unwrap().await.unwrap();
    assert_eq!(found(&registry, &source_id, "Lyon").await.len(), 1);

    // WHEN the connection is pointed at the other folder through the route
    let (status, body) = crate::api::test_support::call(
        state.clone(),
        "PUT",
        &format!("/api/v1/connections/folders/{folder_id}"),
        Some(serde_json::json!({ "path": new.path().to_str().unwrap() })),
    )
    .await;
    let right_after = found(&registry, &source_id, "Lyon").await;
    registry.refresh_indexes().await.unwrap().await.unwrap();

    // THEN the old folder is no longer searched as soon as the route
    // answered, and the new one is once its indexing is over
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(right_after.is_empty(), "{right_after:?}");
    assert!(found(&registry, &source_id, "Lyon").await.is_empty());
    assert_eq!(found(&registry, &source_id, "Nantes").await.len(), 1);
}
