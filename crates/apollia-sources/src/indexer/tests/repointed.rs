//! A folder connection pointed at another folder, told to the registry: the
//! old folder is never searched again, and the new one is indexed without
//! waiting for the sweep.

use super::*;
use crate::search::SourcesQuery;

/// The documents a search for `query` in the source `id` finds, and whether
/// the source was counted as not indexed.
async fn found(world: &World, id: &str, query: &str) -> (BTreeSet<String>, bool) {
    let outcome = world
        .registry
        .search(SourcesQuery {
            sources: vec![id.to_owned()],
            query: query.to_owned(),
            vector: None,
            limit: None,
        })
        .await
        .unwrap();
    let documents = outcome.hits.into_iter().map(|h| h.document).collect();
    (documents, outcome.not_indexed.iter().any(|s| s == id))
}

/// A ready folder source on a folder holding `old.txt` about Lyon, and
/// another folder holding `new.txt` about Nantes, canonical.
async fn indexed_with_another_folder(
    stub: &Arc<Stub>,
) -> (World, String, tempfile::TempDir, PathBuf) {
    let world = World::new();
    world.write("old.txt", "The lease of the Lyon office.");
    let other = tempfile::tempdir().unwrap();
    let other_root = std::fs::canonicalize(other.path()).unwrap();
    std::fs::write(
        other_root.join("new.txt"),
        "The lease of the Nantes office.",
    )
    .unwrap();
    world.start(stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    (world, id, other, other_root)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_told_repointed_is_indexed_at_once_and_its_old_content_never_searched() {
    // GIVEN a ready folder source on a folder about Lyon, an indexer that
    // sweeps once an hour and whose embedder now waits to be let through,
    // and another folder about Nantes
    let stub = Stub::new("e5-small");
    let (world, id, _other, other_root) = indexed_with_another_folder(&stub).await;
    assert_eq!(found(&world, &id, "Lyon").await.0.len(), 1);
    stub.take_seen();
    stub.gated.store(true, Ordering::SeqCst);

    // WHEN its connection is pointed at the other folder and the registry
    // told, as the route does
    repoint(&world, &other_root);
    let told = world
        .registry
        .folder_repointed(&format!("folder:{}", first_connection(&world)))
        .await
        .unwrap();
    let right_after = found(&world, &id, "Lyon").await;
    stub.entered.notified().await;
    let while_indexing = found(&world, &id, "Lyon").await;
    stub.gated.store(false, Ordering::SeqCst);
    stub.gate.add_permits(64);
    world.refresh().await;

    // THEN the source is told, never searched in the old folder from the
    // answer on, counted as not indexed while the new folder is read, the
    // new folder read without a sweep, then searched there
    assert_eq!(told, vec![id.clone()]);
    assert_eq!(right_after, (BTreeSet::new(), true));
    assert!(while_indexing.0.is_empty(), "{while_indexing:?}");
    assert!(stub.take_seen().join("\n").contains("Nantes"));
    assert_eq!(found(&world, &id, "Lyon").await, (BTreeSet::new(), false));
    assert_eq!(
        found(&world, &id, "Nantes").await,
        (BTreeSet::from([key(&other_root.join("new.txt"))]), false)
    );
    assert_eq!(
        world.documents(&id),
        BTreeSet::from([key(&other_root.join("new.txt"))])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_the_sweep_already_followed_is_searched_again_once_told() {
    // GIVEN a folder source whose connection was pointed at another folder
    // and already followed by a sweep
    let stub = Stub::new("e5-small");
    let (world, id, _other, other_root) = indexed_with_another_folder(&stub).await;
    repoint(&world, &other_root);
    world.refresh().await;

    // WHEN the registry is told of it afterwards
    world
        .registry
        .folder_repointed(&first_connection(&world))
        .await
        .unwrap();
    let right_after = found(&world, &id, "Nantes").await;

    // THEN the source is still searched in the new folder, never left out
    assert_eq!(
        right_after,
        (BTreeSet::from([key(&other_root.join("new.txt"))]), false)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_told_repointed_without_an_indexer_is_never_searched_in_the_old_one() {
    // GIVEN a folder source with an index of its folder about Lyon, and no
    // indexer running
    let stub = Stub::new("e5-small");
    let (world, id, _other, other_root) = indexed_with_another_folder(&stub).await;
    let fresh = SourceRegistryHandle::open(&world.data.path().join("system.db")).unwrap();
    let world = World {
        registry: fresh,
        ..world
    };

    // WHEN its connection is pointed at another folder and the registry told
    repoint(&world, &other_root);
    world
        .registry
        .folder_repointed(&first_connection(&world))
        .await
        .unwrap();

    // THEN the old index is not answered
    assert_eq!(found(&world, &id, "Lyon").await, (BTreeSet::new(), true));
}

/// The stored identifier of the one folder connection of `world`.
fn first_connection(world: &World) -> String {
    FolderConnectionStore::open(&world.data.path().join("system.db"))
        .unwrap()
        .list()
        .unwrap()
        .remove(0)
        .id
}
