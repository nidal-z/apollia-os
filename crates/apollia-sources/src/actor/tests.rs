use super::*;
use apollia_core::source_registry::{IndexState, SourceKind};

fn mailbox(reference: Option<&str>) -> NewSource {
    NewSource {
        kind: SourceKind::Mail,
        label: "Office inbox".to_owned(),
        group: None,
        reference: reference.map(str::to_owned),
    }
}

#[tokio::test]
async fn test_the_actor_adds_lists_and_removes_through_cloned_handles() {
    // GIVEN a registry actor on a fresh system.db, and a clone of its handle
    let dir = tempfile::tempdir().unwrap();
    let handle = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let other = handle.clone();

    // WHEN one handle adds a mailbox and the other lists, then removes it
    let added = handle.create(mailbox(Some("imap-cabinet"))).await.unwrap();
    let listed = other.list().await.unwrap();
    let removed = other.delete(&added.id).await.unwrap();
    let after = handle.list().await.unwrap();

    // THEN both handles see the same registry
    assert_eq!(added.kind, SourceKind::Mail);
    assert_eq!(added.index_state, IndexState::Absent);
    assert_eq!(listed, vec![added.clone()]);
    assert_eq!(removed, added);
    assert!(after.is_empty());
}

#[tokio::test]
async fn test_the_actor_answers_refusals_as_typed_errors() {
    // GIVEN a registry actor on a fresh system.db
    let dir = tempfile::tempdir().unwrap();
    let handle = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();

    // WHEN a mailbox without reference is added, an absent source removed,
    // and a list naming an unknown source checked
    let missing = handle.create(mailbox(None)).await;
    let absent = handle.delete("src-absent").await;
    let unknown = handle
        .check_entries(vec!["profile".to_owned(), "crm".to_owned()])
        .await;

    // THEN each comes back as the registry's typed error
    assert!(matches!(
        missing,
        Err(SourcesError::Registry(
            SourceRegistryError::MissingReference(SourceKind::Mail)
        ))
    ));
    assert!(matches!(
        absent,
        Err(SourcesError::Registry(SourceRegistryError::NotFound(_)))
    ));
    assert!(matches!(
        unknown,
        Err(SourcesError::Registry(SourceRegistryError::UnknownSource(e))) if e == "crm"
    ));
}

#[tokio::test]
async fn test_the_actor_reads_an_agent_list_against_the_registry() {
    // GIVEN a registry holding one mailbox
    let dir = tempfile::tempdir().unwrap();
    let handle = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let inbox = handle.create(mailbox(Some("imap"))).await.unwrap();

    // WHEN an agent's list naming it, the profile and a stale name is read
    let checked = handle
        .checked(
            "tri",
            vec![
                inbox.id.clone(),
                "profile".to_owned(),
                "src-gone".to_owned(),
            ],
        )
        .await
        .unwrap();

    // THEN the mailbox and the profile are checked, the stale name is not
    assert_eq!(
        checked,
        vec![CheckedSource::Registered(inbox), CheckedSource::Profile]
    );
}

#[test]
fn test_a_file_written_by_a_newer_binary_is_refused_at_open() {
    // GIVEN a system.db stamped beyond this binary's schema
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("system.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 1_000)
        .unwrap();

    // WHEN the actor is started on it
    let opened = SourceRegistryHandle::open(&path);

    // THEN it does not start, and says why
    assert!(matches!(
        opened,
        Err(SourcesError::Registry(SourceRegistryError::Schema(_)))
    ));
}

#[tokio::test]
async fn test_removing_a_source_erases_its_index_file() {
    // GIVEN a folder source of the registry with an index on disk, and the
    //       index of another source beside it
    let dir = tempfile::tempdir().unwrap();
    let handle = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let folder = handle
        .create(NewSource {
            kind: SourceKind::Folder,
            label: "Contracts".to_owned(),
            group: None,
            reference: Some("folder-contracts".to_owned()),
        })
        .await
        .unwrap();
    let kept = handle.create(mailbox(Some("imap-cabinet"))).await.unwrap();
    for id in [&folder.id, &kept.id] {
        drop(crate::index::SourceIndex::open(dir.path(), id).unwrap());
    }
    let path = crate::index::index_path(dir.path(), &folder.id).unwrap();
    let other = crate::index::index_path(dir.path(), &kept.id).unwrap();
    assert!(path.exists());

    // WHEN the folder source is removed
    handle.delete(&folder.id).await.unwrap();

    // THEN its index file and journals are gone, and the other index stays
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with(&folder.id))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(other.exists());
}

#[tokio::test]
async fn test_the_actor_searches_the_indexes_beside_its_registry() {
    // GIVEN a registry on a system.db, and the index of one of its folder
    //       sources beside it, holding one passage
    let dir = tempfile::tempdir().unwrap();
    let handle = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
    let folder = handle
        .create(NewSource {
            kind: SourceKind::Folder,
            label: "Contracts".to_owned(),
            group: None,
            reference: Some("folder-contracts".to_owned()),
        })
        .await
        .unwrap();
    let mut index = crate::index::SourceIndex::open(dir.path(), &folder.id).unwrap();
    index
        .upsert_document(
            &crate::index::DocumentRecord {
                path: "lease.txt".to_owned(),
                fingerprint: "f".to_owned(),
                modified_at: "2026-10-04T00:00:00Z".to_owned(),
            },
            &[crate::chunk::Chunk {
                text: "The lease ends in June.".to_owned(),
                location: crate::chunk::Location::Lines { first: 1, last: 1 },
            }],
            None,
        )
        .unwrap();
    drop(index);

    // WHEN the handle is asked to search that source
    let outcome = handle
        .search(SourcesQuery {
            sources: vec![folder.id.clone()],
            query: "lease".to_owned(),
            vector: None,
            limit: None,
        })
        .await
        .unwrap();

    // THEN the passage comes back, with its document and its source
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].document, "lease.txt");
    assert_eq!(outcome.hits[0].source, folder.id);
}
