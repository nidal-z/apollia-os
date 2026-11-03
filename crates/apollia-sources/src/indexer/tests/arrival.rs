//! A language that arrives in the models folder in force: the pages that
//! waited are read at the next sweep, without a restart, and a language there
//! from the start reads nothing twice.

use std::sync::atomic::AtomicUsize;

use super::*;
use crate::index::{UnreadPage, UnreadReason};
use crate::ocr::Ocr;

/// The file a simulated engine needs to open.
const LANGUAGE: &str = "fra.traineddata";

/// Counts the pictures a simulated engine read.
#[derive(Default)]
struct Engine {
    reads: AtomicUsize,
}

impl Engine {
    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

/// An engine that opens when [`LANGUAGE`] is in the folder it is handed,
/// and reads every picture as a receipt from a bakery in Lyon.
fn simulated(engine: &Arc<Engine>) -> OpenOcr {
    let engine = Arc::clone(engine);
    Arc::new(move |tessdata: &Path| {
        if !tessdata.join(LANGUAGE).is_file() {
            return Ocr::unavailable();
        }
        let engine = Arc::clone(&engine);
        Ocr::with(move |_| {
            engine.reads.fetch_add(1, Ordering::SeqCst);
            Ok("Receipt from a bakery in Lyon".to_owned())
        })
    })
}

/// Records the folder of every `sources.indexer.full_pass` event of the test
/// binary. No subscriber crate is a dependency here, and this needs none.
#[derive(Default, Clone)]
struct FullPasses(Arc<Mutex<Vec<String>>>);

#[derive(Default)]
struct PassFields {
    message: Option<String>,
    root: Option<String>,
}

impl tracing::field::Visit for PassFields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = Some(format!("{value:?}")),
            "root" => self.root = Some(format!("{value:?}")),
            _ => {}
        }
    }
}

impl tracing::Subscriber for FullPasses {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = PassFields::default();
        event.record(&mut fields);
        if let (Some(message), Some(root)) = (fields.message, fields.root) {
            if message == "sources.indexer.full_pass" {
                self.0.lock().unwrap().push(root);
            }
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// The one recorder of the test binary, installed as the global subscriber
/// on first use: tracing caches the interest of a call site for the whole
/// process, and the indexer emits from the threads of the runtime. Each test
/// finds its own passes by the folder, a temporary directory of its own.
fn recorder() -> &'static FullPasses {
    static PASSES: std::sync::OnceLock<FullPasses> = std::sync::OnceLock::new();
    PASSES.get_or_init(|| {
        let passes = FullPasses::default();
        tracing::subscriber::set_global_default(passes.clone())
            .expect("no other global subscriber in this test binary");
        passes
    })
}

/// The full passes recorded over the folder `root`.
fn full_passes(root: &Path) -> usize {
    let root = root.display().to_string();
    recorder()
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|r| **r == root)
        .count()
}

/// Puts the language the simulated engine needs into `models/tessdata`.
fn place_language(models: &Path) {
    let tessdata = tessdata_dir(models);
    std::fs::create_dir_all(&tessdata).unwrap();
    std::fs::write(tessdata.join(LANGUAGE), b"language data, French").unwrap();
}

/// Waits, without asking the indexer for anything, until `reached` holds;
/// `within` without it is a failure.
async fn until<F: std::future::Future<Output = bool>>(
    what: &str,
    within: Duration,
    mut reached: impl FnMut() -> F,
) {
    let started = std::time::Instant::now();
    while !reached().await {
        assert!(started.elapsed() < within, "never: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn unread_pages(world: &World, id: &str) -> Vec<UnreadPage> {
    world.registry.index_status(id).await.unwrap().unread_pages
}

fn waiting(path: &Path) -> Vec<UnreadPage> {
    vec![UnreadPage {
        path: key(path),
        page: 1,
        reason: UnreadReason::OcrUnavailable,
    }]
}

fn found_in(world: &World, id: &str, word: &str) -> BTreeSet<String> {
    world
        .index(id)
        .search(word, None, None)
        .unwrap()
        .hits
        .into_iter()
        .map(|h| h.document)
        .collect()
}

/// Copies the receipt picture of the fixtures into the documents folder.
fn bring_picture(world: &World) -> PathBuf {
    let dest = world.root().join("recu.png");
    std::fs::copy(fixture("ocr/recu.png"), &dest).unwrap();
    dest
}

/// Scanned pages read through the simulated `engine`, in `models_dir`, no
/// language data fetched.
fn simulated_in(models_dir: PathBuf, engine: &Arc<Engine>) -> Recognition {
    Recognition {
        models_dir,
        fetch: None,
        open: simulated(engine),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_the_language_is_read_in_the_models_folder_the_settings_name() {
    // GIVEN an apollia.toml whose [models] dir names a folder outside
    // <data_dir>/models, a language in its tessdata, nothing under
    // <data_dir>/models, and a folder holding a picture
    let world = World::new();
    let moved = tempfile::tempdir().unwrap();
    std::fs::write(
        world.data.path().join("apollia.toml"),
        format!("[models]\ndir = {:?}\n", moved.path().to_str().unwrap()),
    )
    .unwrap();
    let models = apollia_core::config::models_dir(world.data.path());
    assert_eq!(models, moved.path());
    place_language(&models);
    let picture = bring_picture(&world);
    let engine = Arc::new(Engine::default());
    let recognition = simulated_in(models, &engine);
    world
        .start_with(
            &Stub::new("e5-small"),
            Duration::from_secs(3600),
            recognition,
        )
        .await;

    // WHEN the folder becomes a source and a pass runs
    let id = world.folder_source().await;
    world.refresh().await;

    // THEN the engine is found there: no page waits, the picture is found
    assert!(!tessdata_dir(&world.models()).exists());
    assert_eq!(unread_pages(&world, &id).await, []);
    assert_eq!(
        found_in(&world, &id, "Lyon"),
        BTreeSet::from([key(&picture)])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_page_that_waits_is_read_at_the_next_sweep_once_a_language_is_copied_in() {
    // GIVEN an indexer sweeping every 200 ms, no language on the disk, and a
    // source whose picture waits for recognition
    recorder();
    let world = World::new();
    let picture = bring_picture(&world);
    let engine = Arc::new(Engine::default());
    let recognition = simulated_in(world.models(), &engine);
    world
        .start_with(
            &Stub::new("e5-small"),
            Duration::from_millis(200),
            recognition,
        )
        .await;
    let id = world.folder_source().await;
    until("the picture waits", Duration::from_secs(30), || async {
        unread_pages(&world, &id).await == waiting(&picture)
    })
    .await;
    let before = full_passes(&world.root());

    // WHEN a language is copied into the models folder, nothing restarted
    // and nothing asked of the indexer
    place_language(&world.models());

    // THEN within a few sweeps a full pass runs again, the picture no longer
    // waits, and its text is found
    until(
        "the picture is read by a sweep",
        Duration::from_secs(10),
        || async {
            full_passes(&world.root()) > before
                && unread_pages(&world, &id).await.is_empty()
                && found_in(&world, &id, "Lyon") == BTreeSet::from([key(&picture)])
        },
    )
    .await;
    assert_eq!(engine.reads(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_language_there_from_the_start_runs_no_second_full_pass() {
    // GIVEN a language on the disk before the start, a source on a folder
    // holding a picture and a source on an empty folder, both created before
    // the indexer, which sweeps once an hour
    recorder();
    let world = World::new();
    place_language(&world.models());
    let picture = bring_picture(&world);
    let id = world.folder_source().await;
    let empty = tempfile::tempdir().unwrap();
    let other = other_folder_source(&world, empty.path()).await;
    let engine = Arc::new(Engine::default());
    let recognition = simulated_in(world.models(), &engine);

    // WHEN the indexer starts, its first sweep is over, and the registry
    // changes, which makes it sweep once more
    world
        .start_with(
            &Stub::new("e5-small"),
            Duration::from_secs(3600),
            recognition,
        )
        .await;
    until(
        "both sources are ready",
        Duration::from_secs(30),
        || async {
            world.state(&id).await == IndexState::Ready
                && world.state(&other).await == IndexState::Ready
        },
    )
    .await;
    world.registry.indexer_served().await.await.unwrap();
    world.registry.delete(&other).await.unwrap();
    world.registry.indexer_served().await.await.unwrap();

    // THEN the source was read by one full pass, at the start, and its
    // picture once
    assert_eq!(full_passes(&world.root()), 1);
    assert_eq!(engine.reads(), 1);
    assert_eq!(
        found_in(&world, &id, "Lyon"),
        BTreeSet::from([key(&picture)])
    );
}

#[cfg(feature = "ocr")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires APOLLIA_TEST_TESSDATA, a folder holding the fra.traineddata the registry pins"]
async fn test_the_embedded_engine_reads_a_waiting_scanned_page_once_the_language_is_copied_in() {
    // GIVEN an indexer over the embedded engine sweeping every 200 ms, no
    // language on the disk, and a source whose scanned French pages wait
    let folder = std::env::var_os("APOLLIA_TEST_TESSDATA").expect("APOLLIA_TEST_TESSDATA");
    let world = World::new();
    let scanned = world.root().join("scanned-fr.pdf");
    std::fs::copy(fixture("ocr/scanned-fr.pdf"), &scanned).unwrap();
    let recognition = Recognition {
        models_dir: world.models(),
        fetch: None,
        open: Arc::new(Ocr::open),
    };
    world
        .start_with(
            &Stub::new("e5-small"),
            Duration::from_millis(200),
            recognition,
        )
        .await;
    let id = world.folder_source().await;
    let is_waiting = |pages: &[UnreadPage]| {
        pages
            .iter()
            .any(|p| p.reason == UnreadReason::OcrUnavailable)
    };
    until(
        "the scanned pages wait",
        Duration::from_secs(60),
        || async { is_waiting(&unread_pages(&world, &id).await) },
    )
    .await;

    // WHEN the pinned French data is copied into the models folder
    let tessdata = tessdata_dir(&world.models());
    std::fs::create_dir_all(&tessdata).unwrap();
    std::fs::copy(Path::new(&folder).join(LANGUAGE), tessdata.join(LANGUAGE)).unwrap();

    // THEN at the next sweeps no page waits for recognition, and the page is
    // found by an accented word
    until(
        "the scanned pages are read",
        Duration::from_secs(120),
        || async {
            !is_waiting(&unread_pages(&world, &id).await)
                && found_in(&world, &id, "Besançon").contains(&key(&scanned))
        },
    )
    .await;
}

/// A folder source on `folder`, through a folder connection of its own.
async fn other_folder_source(world: &World, folder: &Path) -> String {
    let connections = FolderConnectionStore::open(&world.data.path().join("system.db")).unwrap();
    let connection = connections
        .add(folder.to_str().unwrap(), Some("Empty"))
        .unwrap();
    world
        .registry
        .create(NewSource {
            kind: SourceKind::Folder,
            label: "Empty".to_owned(),
            group: None,
            reference: Some(connection.id),
        })
        .await
        .unwrap()
        .id
}
