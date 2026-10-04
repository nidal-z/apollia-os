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
