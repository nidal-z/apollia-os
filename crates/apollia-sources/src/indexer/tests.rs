use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use apollia_core::source_registry::NewSource;
use tokio::sync::{Notify, Semaphore};

use super::*;
use crate::index::{index_path, SkipReason};
use crate::SourceRegistryHandle;

mod recognition;

/// An embedder that never leaves the process: an eight-dimension vector
/// counted from the bytes of each text, the model named by the test, every
/// text it was asked for recorded. When gated, each call waits for a permit
/// after saying it has started.
struct Stub {
    model: Mutex<String>,
    seen: Mutex<Vec<String>>,
    gated: AtomicBool,
    entered: Notify,
    gate: Semaphore,
}

impl Stub {
    fn new(model: &str) -> Arc<Self> {
        Arc::new(Self {
            model: Mutex::new(model.to_owned()),
            seen: Mutex::new(Vec::new()),
            gated: AtomicBool::new(false),
            entered: Notify::new(),
            gate: Semaphore::new(0),
        })
    }

    fn take_seen(&self) -> Vec<String> {
        std::mem::take(&mut *self.seen.lock().unwrap())
    }
}

struct Shared(Arc<Stub>);

fn vector(text: &str) -> Vec<f32> {
    let mut v = vec![1.0_f32; 8];
    for b in text.bytes() {
        v[usize::from(b) % 8] += 1.0;
    }
    v
}

impl PassageEmbedder for Shared {
    async fn model(&self) -> Result<String, EmbeddingError> {
        Ok(self.0.model.lock().unwrap().clone())
    }

    async fn embed_passages(&self, texts: &[String]) -> Result<ChunkVectors, EmbeddingError> {
        if self.0.gated.load(Ordering::SeqCst) {
            self.0.entered.notify_one();
            self.0.gate.acquire().await.unwrap().forget();
        }
        self.0.seen.lock().unwrap().extend(texts.iter().cloned());
        Ok(ChunkVectors {
            model: self.0.model.lock().unwrap().clone(),
            vectors: texts.iter().map(|t| vector(t)).collect(),
        })
    }
}

/// A data directory, its registry, and a folder of documents connected to
/// it, outside the data directory.
struct World {
    data: tempfile::TempDir,
    docs: tempfile::TempDir,
    registry: SourceRegistryHandle,
}

impl World {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let registry = SourceRegistryHandle::open(&data.path().join("system.db")).unwrap();
        Self {
            data,
            docs,
            registry,
        }
    }

    fn root(&self) -> PathBuf {
        std::fs::canonicalize(self.docs.path()).unwrap()
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Adds the documents folder as a folder connection, then a folder
    /// source on it.
    async fn folder_source(&self) -> String {
        let connections = FolderConnectionStore::open(&self.data.path().join("system.db")).unwrap();
        let connection = connections
            .add(self.docs.path().to_str().unwrap(), Some("Office"))
            .unwrap();
        self.registry
            .create(NewSource {
                kind: SourceKind::Folder,
                label: "Office".to_owned(),
                group: None,
                reference: Some(connection.id),
            })
            .await
            .unwrap()
            .id
    }

    async fn start(&self, stub: &Arc<Stub>) {
        self.registry
            .start_indexing_every(Shared(Arc::clone(stub)), Duration::from_secs(3600))
            .await
            .unwrap();
    }

    /// [`Self::start`], the language data of recognition fetched through
    /// `ocr_data`.
    async fn start_fetching(&self, stub: &Arc<Stub>, ocr_data: OcrDataFetch) {
        self.registry
            .start_indexing_fetching(
                Shared(Arc::clone(stub)),
                Duration::from_secs(3600),
                Some(ocr_data),
            )
            .await
            .unwrap();
    }

    async fn refresh(&self) {
        self.registry
            .refresh_indexes()
            .await
            .unwrap()
            .await
            .unwrap();
    }

    fn index(&self, id: &str) -> SourceIndex {
        SourceIndex::open(self.data.path(), id).unwrap()
    }

    fn documents(&self, id: &str) -> BTreeSet<String> {
        self.index(id)
            .documents()
            .unwrap()
            .into_iter()
            .map(|d| d.path)
            .collect()
    }

    async fn state(&self, id: &str) -> IndexState {
        self.registry.index_status(id).await.unwrap().state
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

/// Moves the modification date of `path` an hour ahead, so a change is seen
/// whatever the resolution of the file system's clock.
fn bump(path: &Path) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(3600))
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_source_is_indexed_in_the_background_from_indexing_to_ready() {
    // GIVEN a folder holding a text file, a CSV file, a workbook, a PDF, a
    // Word document, a presentation and an archive no reader opens, and an
    // indexer whose embedder waits to be let through
    let world = World::new();
    world.write("notes.txt", "The lease of the Lyon office runs until 2027.");
    world.write("clients.csv", "name;city\nDupont;Lyon\nMartin;Nantes\n");
    for (from, to) in [
        ("spreadsheets/date.xlsx", "table.xlsx"),
        ("pdf-corpus/tarifs-reseau-bus.pdf", "tarifs.pdf"),
        ("office/report.docx", "report.docx"),
        ("office/deck.pptx", "deck.pptx"),
    ] {
        std::fs::copy(fixture(from), world.root().join(to)).unwrap();
    }
    std::fs::write(world.root().join("backup.zip"), b"PK\x03\x04").unwrap();
    let stub = Stub::new("e5-small");
    stub.gated.store(true, Ordering::SeqCst);
    world.start(&stub).await;

    // WHEN a folder source is added on it
    let id = world.folder_source().await;
    stub.entered.notified().await;
    let during = world.state(&id).await;
    stub.gated.store(false, Ordering::SeqCst);
    stub.gate.add_permits(64);
    world.refresh().await;

    // THEN the index is built in the background: `indexing` while it reads,
    // `ready` once done, every readable format indexed, the archive ignored
    let status = world.registry.index_status(&id).await.unwrap();
    let names: BTreeSet<String> = world
        .documents(&id)
        .iter()
        .filter_map(|p| Path::new(p).file_name()?.to_str().map(str::to_owned))
        .collect();
    assert_eq!(during, IndexState::Indexing);
    assert_eq!(status.state, IndexState::Ready);
    assert_eq!(
        names,
        BTreeSet::from(
            [
                "clients.csv",
                "deck.pptx",
                "notes.txt",
                "report.docx",
                "table.xlsx",
                "tarifs.pdf"
            ]
            .map(str::to_owned)
        )
    );
    assert!(status.skipped.is_empty());
    assert_eq!(
        world.index(&id).embedding_model().unwrap().as_deref(),
        Some("e5-small")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_changed_file_alone_is_read_again() {
    // GIVEN a ready index of two files
    let world = World::new();
    let kept = world.write("kept.txt", "The maintenance contract is signed.");
    let edited = world.write("edited.txt", "First version of the quote.");
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    let before = world.index(&id).document(&key(&kept)).unwrap();
    stub.take_seen();

    // WHEN one of them is rewritten, the other only touched, and the index
    // brought in step with the disk
    std::fs::write(&edited, "Second version of the quote, ten percent off.").unwrap();
    bump(&edited);
    bump(&kept);
    world.refresh().await;

    // THEN only the rewritten file's passages are embedded again; the other
    // keeps its fingerprint and takes its new date without being read
    let seen = stub.take_seen();
    let after = world.index(&id).document(&key(&kept)).unwrap().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("Second version"));
    assert_eq!(before.as_ref().unwrap().fingerprint, after.fingerprint);
    assert_ne!(before.unwrap().modified_at, after.modified_at);
    assert_eq!(world.state(&id).await, IndexState::Ready);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_the_watcher_follows_a_file_added_modified_and_deleted() {
    // GIVEN a ready index of two files, watched, with no sweep to come
    let world = World::new();
    let edited = world.write("edited.txt", "Former address: Lilac Street.");
    let deleted = world.write("deleted.txt", "Outdated document.");
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    stub.take_seen();

    // WHEN a file is added, one rewritten and one deleted
    let added = world.write("sub/added.md", "New address: Saint-Antoine quay.");
    std::fs::write(&edited, "Corrected address: Bellecour square.").unwrap();
    std::fs::remove_file(&deleted).unwrap();

    // THEN the index follows each change without being asked
    let expected = BTreeSet::from([key(&added), key(&edited)]);
    let followed = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let fresh = world
                .index(&id)
                .document(&key(&edited))
                .unwrap()
                .is_some_and(|d| d.fingerprint == pass_fingerprint(&edited));
            if world.documents(&id) == expected && fresh {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(followed.is_ok(), "the index never followed the files");
    let seen = stub.take_seen().join("\n");
    assert!(seen.contains("Saint-Antoine quay"));
    assert!(seen.contains("Bellecour square"));
    assert!(!seen.contains("Outdated document"));
}

/// The fingerprint the pass computes for `path`.
fn pass_fingerprint(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_restart_compares_the_index_with_the_disk() {
    // GIVEN a ready index of two files, and Apollia stopped
    let world = World::new();
    let edited = world.write("edited.txt", "Before the stop.");
    let deleted = world.write("deleted.txt", "Deleted while stopped.");
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    let World {
        data,
        docs,
        registry,
    } = world;
    drop(registry);
    // The indexer stops with its registry and lets go of its embedder; no
    // watch of the stopped instance may see what follows.
    while Arc::strong_count(&stub) > 1 {
        tokio::task::yield_now().await;
    }

    // WHEN, while it is stopped, a file is rewritten, one deleted and one
    // added, then it starts again
    std::fs::write(&edited, "After the restart.").unwrap();
    bump(&edited);
    std::fs::remove_file(&deleted).unwrap();
    let registry = SourceRegistryHandle::open(&data.path().join("system.db")).unwrap();
    let world = World {
        data,
        docs,
        registry,
    };
    let added = world.write("added.txt", "Added while stopped.");
    let restarted = Stub::new("e5-small");
    world.start(&restarted).await;
    world.refresh().await;

    // THEN the index matches the disk, and only the two changed files were
    // read again
    let seen = restarted.take_seen().join("\n");
    assert_eq!(
        world.documents(&id),
        BTreeSet::from([key(&added), key(&edited)])
    );
    assert!(seen.contains("After the restart"));
    assert!(seen.contains("Added while stopped"));
    assert_eq!(world.state(&id).await, IndexState::Ready);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_file_over_fifty_megabytes_is_skipped_and_listed() {
    // GIVEN a folder holding a small file and a text file one byte over the
    // cap (sparse, so the test writes nothing of its size)
    let world = World::new();
    world.write("small.txt", "Minutes of the meeting.");
    let big = world.root().join("archive.txt");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(MAX_INDEXED_FILE_BYTES + 1)
        .unwrap();
    let stub = Stub::new("e5-small");
    world.start(&stub).await;

    // WHEN the folder becomes a source and is indexed
    let id = world.folder_source().await;
    world.refresh().await;

    // THEN the large file is left out of the index and listed in the state
    // of the source with its size and the reason
    let status = world.registry.index_status(&id).await.unwrap();
    assert_eq!(status.documents, 1);
    assert_eq!(status.skipped.len(), 1);
    assert_eq!(status.skipped[0].path, key(&big));
    assert_eq!(status.skipped[0].size_bytes, MAX_INDEXED_FILE_BYTES + 1);
    assert_eq!(status.skipped[0].reason, SkipReason::TooLarge);
    assert!(!world.documents(&id).contains(&key(&big)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_change_of_default_embeddings_model_rebuilds_every_index() {
    // GIVEN a folder source and a file source, both ready with model one
    let world = World::new();
    world.write("a.txt", "First document.");
    let single = world.write("single/b.txt", "Second document.");
    let stub = Stub::new("model-one");
    world.start(&stub).await;
    let folder = world.folder_source().await;
    let file = world
        .registry
        .create(NewSource {
            kind: SourceKind::File,
            label: "b".to_owned(),
            group: None,
            reference: Some(key(&single)),
        })
        .await
        .unwrap()
        .id;
    world.refresh().await;
    stub.take_seen();

    // WHEN the default model becomes model two and the indexer sweeps
    *stub.model.lock().unwrap() = "model-two".to_owned();
    world.refresh().await;

    // THEN every index is rebuilt with model two, every passage embedded
    // again
    let seen = stub.take_seen().join("\n");
    for id in [&folder, &file] {
        assert_eq!(
            world.index(id).embedding_model().unwrap().as_deref(),
            Some("model-two")
        );
        assert_eq!(world.state(id).await, IndexState::Ready);
    }
    assert!(seen.contains("First document"));
    assert!(seen.contains("Second document"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_the_indexes_live_under_the_data_directory_never_beside_the_documents() {
    // GIVEN a folder of documents, listed before any indexing
    let world = World::new();
    world.write("agreement.txt", "Rental agreement.");
    world.write("appendix/annex.md", "Appendix to the agreement.");
    let listing = |root: &Path| -> BTreeSet<PathBuf> {
        let mut out = BTreeSet::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.insert(path);
            }
        }
        out
    };
    let before = listing(&world.root());
    let stub = Stub::new("e5-small");
    world.start(&stub).await;

    // WHEN it is indexed
    let id = world.folder_source().await;
    world.refresh().await;

    // THEN not one file was added beside the documents, and the index is a
    // file of the data directory
    assert_eq!(listing(&world.root()), before);
    assert!(
        apollia_core::paths::source_index_path(world.data.path(), &id)
            .unwrap()
            .is_file()
    );
    assert_eq!(world.documents(&id).len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_source_removed_stops_being_indexed_and_loses_its_index() {
    // GIVEN a ready folder source
    let world = World::new();
    world.write("a.txt", "A document.");
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    let index = apollia_core::paths::source_index_path(world.data.path(), &id).unwrap();
    assert!(index.is_file());

    // WHEN it is removed and the indexer has read the registry again
    world.registry.delete(&id).await.unwrap();
    world.refresh().await;

    // THEN its index file is gone
    assert!(!index.exists());
}

/// Points the one folder connection of `world` at `to`, as
/// `PUT /api/v1/connections/folders/{id}` does.
fn repoint(world: &World, to: &Path) {
    let connections = FolderConnectionStore::open(&world.data.path().join("system.db")).unwrap();
    let connection = connections.list().unwrap().remove(0);
    connections
        .repoint(&connection.id, to.to_str().unwrap())
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_repointed_folder_is_indexed_on_its_new_folder_and_drops_the_old_one() {
    // GIVEN a ready folder source on a folder holding `old.txt`, and another
    // folder holding `new.txt`
    let world = World::new();
    world.write("old.txt", "The lease of the Lyon office.");
    let other = tempfile::tempdir().unwrap();
    let other_root = std::fs::canonicalize(other.path()).unwrap();
    std::fs::write(
        other_root.join("new.txt"),
        "The lease of the Nantes office.",
    )
    .unwrap();
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    assert_eq!(
        world.documents(&id),
        BTreeSet::from([key(&world.root().join("old.txt"))])
    );

    // WHEN its connection is pointed at the other folder and the index
    // brought in step, without a restart
    repoint(&world, &other_root);
    world.refresh().await;

    // THEN the index holds the new folder's file and no longer the old one
    assert_eq!(
        world.documents(&id),
        BTreeSet::from([key(&other_root.join("new.txt"))])
    );
    assert_eq!(world.state(&id).await, IndexState::Ready);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_folder_emptied_then_repointed_indexes_the_new_folder() {
    // GIVEN a ready folder source whose folder is emptied, and another
    // folder holding `new.txt`
    let world = World::new();
    let old = world.write("old.txt", "The lease of the Lyon office.");
    let other = tempfile::tempdir().unwrap();
    let other_root = std::fs::canonicalize(other.path()).unwrap();
    std::fs::write(
        other_root.join("new.txt"),
        "The lease of the Nantes office.",
    )
    .unwrap();
    let stub = Stub::new("e5-small");
    world.start(&stub).await;
    let id = world.folder_source().await;
    world.refresh().await;
    std::fs::remove_file(&old).unwrap();
    world.refresh().await;
    assert!(world.documents(&id).is_empty());

    // WHEN its connection is pointed at the other folder and the index
    // brought in step
    repoint(&world, &other_root);
    world.refresh().await;

    // THEN the new folder's file is indexed
    assert_eq!(
        world.documents(&id),
        BTreeSet::from([key(&other_root.join("new.txt"))])
    );
    assert_eq!(world.state(&id).await, IndexState::Ready);
}

#[test]
fn test_reported_paths_become_scopes_under_the_root_hidden_ones_left_out() {
    // GIVEN the root of a source and paths a watcher reported
    let root = Path::new("/data/docs");
    let paths = vec![
        PathBuf::from("/data/docs/a.txt"),
        PathBuf::from("/data/docs/.git/index"),
        PathBuf::from("/elsewhere/b.txt"),
        PathBuf::from("/data/docs/a.txt"),
        PathBuf::from("/data/docs/sub/c.md"),
    ];

    // WHEN they are turned into scopes, then with the root among them
    let scopes = scopes_of(root, paths.clone());
    let mut with_root = paths;
    with_root.push(root.to_path_buf());
    let whole = scopes_of(root, with_root);

    // THEN each path under the root counts once, hidden and foreign ones
    // left out, and the root alone stands for everything
    assert_eq!(
        scopes,
        vec![
            PathBuf::from("/data/docs/a.txt"),
            PathBuf::from("/data/docs/sub/c.md")
        ]
    );
    assert_eq!(whole, vec![root.to_path_buf()]);
}
