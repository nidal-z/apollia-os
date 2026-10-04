//! Scanned pages and pictures in a source: marked while recognition is
//! missing, read once it is there, and never written beside the documents.

use apollia_llm::ocr_data::verified_languages;
use apollia_llm::recommend::OcrCatalogue;
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path as url_path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::index::{UnreadPage, UnreadReason};
use crate::ocr::Ocr;

/// A registry of one language, `fra`, pinned to the digest of `bytes`.
fn catalogue_of(bytes: &[u8]) -> OcrCatalogue {
    OcrCatalogue::from_toml(&format!(
        r#"
[[ocr]]
language = "fra"
label = "French"
publisher = "test"
released = "2017-09-14"
license = "apache-2.0"
repo = "owner/tessdata"
file = "fra.traineddata"
size_bytes = {}
sha256 = "{:x}"
source = "https://example.invalid"
"#,
        bytes.len(),
        Sha256::digest(bytes)
    ))
    .unwrap()
}

/// A server on the loopback that serves `bytes` as the French language data.
async fn registry_serving(bytes: &[u8]) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(url_path("/owner/tessdata/resolve/main/fra.traineddata"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.to_vec()))
        .mount(&server)
        .await;
    server
}

/// The fetch the indexer is started with: `catalogue`, served by `server`.
fn fetch_from(server: &MockServer, catalogue: &OcrCatalogue) -> OcrDataFetch {
    OcrDataFetch {
        catalogue: catalogue.clone(),
        base: server.uri(),
        manager: apollia_llm::DownloadManager::with_client(
            apollia_core::net::loopback_client_builder()
                .build()
                .unwrap(),
        ),
    }
}

/// Has the source indexed again until `reached` holds. The download runs in
/// a task the indexer starts and answers to nobody, so the passes are what
/// the test waits on; a minute of them without `reached` is a failure.
async fn index_until<F: std::future::Future<Output = bool>>(
    world: &World,
    what: &str,
    mut reached: impl FnMut() -> F,
) {
    let started = std::time::Instant::now();
    while !reached().await {
        assert!(started.elapsed() < Duration::from_secs(60), "never: {what}");
        world.refresh().await;
    }
}

async fn downloads(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// Copies the fixture `name` of `tests/fixtures/ocr` into `folder`.
fn bring(folder: &Path, name: &str) -> PathBuf {
    let dest = folder.join(name);
    std::fs::copy(fixture("ocr").join(name), &dest).unwrap();
    dest
}

fn listing(folder: &Path) -> BTreeSet<String> {
    std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// One pass over `root`, for keywords alone, with `ocr`.
fn pass_with(index: &mut SourceIndex, root: &Path, ocr: &Ocr) -> PassOutcome {
    let no_vectors = |_: &[String]| Err(EmbeddingError::NoModel);
    let means = pass::Means {
        model: None,
        embed: &no_vectors,
        ocr,
    };
    pass::run(index, root, &[root.to_path_buf()], &means).unwrap()
}

fn unread(path: &Path, page: u32, reason: UnreadReason) -> UnreadPage {
    UnreadPage {
        path: key(path),
        page,
        reason,
    }
}

#[cfg(not(feature = "ocr"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_without_recognition_scanned_pages_are_listed_as_waiting_in_the_state_of_the_source() {
    // GIVEN a folder holding a scanned document and a picture, on a machine
    // without the language data of recognition
    let world = World::new();
    let scanned = bring(&world.root(), "scanned-fr.pdf");
    let picture = bring(&world.root(), "recu.png");
    world.start(&Stub::new("e5-small")).await;

    // WHEN the folder becomes a source and is indexed
    let id = world.folder_source().await;
    world.refresh().await;

    // THEN both are in the index, and the state of the source names each
    // page that waits for recognition and the page it cannot read
    let status = world.registry.index_status(&id).await.unwrap();
    assert_eq!(status.documents, 2);
    assert!(status.skipped.is_empty());
    assert_eq!(
        status.unread_pages,
        [
            unread(&picture, 1, UnreadReason::OcrUnavailable),
            unread(&scanned, 1, UnreadReason::OcrUnavailable),
            unread(&scanned, 2, UnreadReason::OcrUnavailable),
            unread(&scanned, 3, UnreadReason::NotRead),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_page_left_waiting_has_the_indexer_fetch_the_pinned_language_data_once() {
    // GIVEN a registry serving the pinned French data, a machine holding
    // none, and a folder holding a picture
    const FRENCH: &[u8] = b"language data, French";
    let server = registry_serving(FRENCH).await;
    let catalogue = catalogue_of(FRENCH);
    let world = World::new();
    bring(&world.root(), "recu.png");
    world
        .start_fetching(&Stub::new("e5-small"), fetch_from(&server, &catalogue))
        .await;
    let tessdata = tessdata_dir(world.data.path());

    // WHEN the folder becomes a source and is indexed, then indexed again
    world.folder_source().await;
    index_until(&world, "the language data is on the disk", || async {
        verified_languages(&tessdata, &catalogue) == ["fra"]
    })
    .await;
    world.refresh().await;

    // THEN the file the registry pins is in the folder the engine reads,
    // under its digest, and it was downloaded once
    assert_eq!(
        std::fs::read(tessdata.join("fra.traineddata")).unwrap(),
        FRENCH
    );
    assert_eq!(downloads(&server).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_language_data_of_another_digest_is_left_off_the_disk_and_the_page_keeps_waiting() {
    // GIVEN a registry serving other bytes than the pinned ones, and a
    // folder holding a picture
    let server = registry_serving(b"tampered").await;
    let catalogue = catalogue_of(b"language data, French");
    let world = World::new();
    let picture = bring(&world.root(), "recu.png");
    world
        .start_fetching(&Stub::new("e5-small"), fetch_from(&server, &catalogue))
        .await;
    let tessdata = tessdata_dir(world.data.path());

    // WHEN the folder becomes a source and is indexed, the download answered
    let id = world.folder_source().await;
    index_until(&world, "the download is answered", || async {
        downloads(&server).await > 0
    })
    .await;
    world.refresh().await;

    // THEN nothing counts as language data, and the state of the source
    // still names the page as waiting for recognition
    assert!(verified_languages(&tessdata, &catalogue).is_empty());
    let status = world.registry.index_status(&id).await.unwrap();
    assert_eq!(
        status.unread_pages,
        [unread(&picture, 1, UnreadReason::OcrUnavailable)]
    );
}

#[cfg(feature = "ocr")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires APOLLIA_TEST_TESSDATA, a folder holding the fra.traineddata the registry pins"]
async fn test_a_scanned_page_is_read_once_the_indexer_has_fetched_the_language_data() {
    // GIVEN a registry serving the French data the environment names, a
    // machine holding none, and a folder holding the scanned French document
    let folder = std::env::var_os("APOLLIA_TEST_TESSDATA").expect("APOLLIA_TEST_TESSDATA");
    let french = std::fs::read(Path::new(&folder).join("fra.traineddata")).unwrap();
    let server = registry_serving(&french).await;
    let catalogue = catalogue_of(&french);
    let world = World::new();
    let scanned = bring(&world.root(), "scanned-fr.pdf");
    world
        .start_fetching(&Stub::new("e5-small"), fetch_from(&server, &catalogue))
        .await;

    // WHEN the folder becomes a source and is indexed
    let id = world.folder_source().await;
    let left = [unread(&scanned, 3, UnreadReason::NotRead)];
    index_until(&world, "the pages that waited are read", || async {
        world.registry.index_status(&id).await.unwrap().unread_pages == left
    })
    .await;

    // THEN the data was downloaded once, and the pages that waited are
    // found by an accented word: only the page of two pictures is left
    assert_eq!(downloads(&server).await, 1);
    let found = world.index(&id).search("Besançon", None, None).unwrap();
    assert!(found.hits.iter().any(|h| h.document == key(&scanned)));
}

#[test]
fn test_pages_that_waited_are_read_once_recognition_is_there_and_nothing_is_written_beside_them() {
    // GIVEN a folder holding a scanned document and a picture in each format,
    // indexed while recognition is missing
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(docs.path()).unwrap();
    let scanned = bring(&root, "scanned-fr.pdf");
    for name in ["recu.png", "recu.jpg", "recu.tiff"] {
        bring(&root, name);
    }
    let before = listing(&root);
    let mut index = SourceIndex::open(data.path(), "src-1").unwrap();
    let waiting = pass_with(&mut index, &root, &Ocr::unavailable());

    // WHEN a pass runs again over the unchanged files, recognition now there
    let ocr = Ocr::with(|picture| Ok(format!("recognised {} wide", picture.width())));
    let read = pass_with(&mut index, &root, &ocr);

    // THEN the five pages that waited are read by the reader the pass calls,
    // their text is in the index, the page of two pictures stays listed
    assert_eq!(waiting.awaiting_ocr, 5);
    assert_eq!((read.indexed, read.awaiting_ocr), (4, 0));
    assert_eq!(
        index.unread_pages().unwrap(),
        [unread(&scanned, 3, UnreadReason::NotRead)]
    );
    let found = index.search("recognised", None, None).unwrap();
    let documents: BTreeSet<&str> = found.hits.iter().map(|h| h.document.as_str()).collect();
    assert_eq!(documents.len(), 4);
    // AND the text lives in the index alone: the folder holds the same files
    assert_eq!(listing(&root), before);
    assert!(index_path(data.path(), "src-1").unwrap().exists());
}

#[cfg(feature = "ocr")]
#[test]
#[ignore = "requires APOLLIA_TEST_TESSDATA, a folder holding the fra.traineddata the registry pins"]
fn test_the_embedded_engine_makes_a_scanned_french_page_searchable_by_its_accented_words() {
    // GIVEN a folder holding the scanned French document and a picture, and
    // the embedded engine over the pinned French data, in the folder the
    // environment names
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(docs.path()).unwrap();
    let scanned = bring(&root, "scanned-fr.pdf");
    let picture = bring(&root, "recu.tiff");
    let before = listing(&root);
    let mut index = SourceIndex::open(data.path(), "src-1").unwrap();
    let tessdata = std::env::var_os("APOLLIA_TEST_TESSDATA").expect("APOLLIA_TEST_TESSDATA");
    let ocr = Ocr::open(Path::new(&tessdata));

    // WHEN the folder is indexed
    let outcome = pass_with(&mut index, &root, &ocr);

    // THEN the page and the picture are found by an accented word, with the
    // page number, and nothing was written beside the documents
    assert_eq!((outcome.indexed, outcome.awaiting_ocr), (2, 0));
    let found = index.search("Besançon", None, None).unwrap();
    let documents: BTreeSet<&str> = found.hits.iter().map(|h| h.document.as_str()).collect();
    assert_eq!(
        documents,
        BTreeSet::from([key(&scanned).as_str(), key(&picture).as_str()])
    );
    assert!(found
        .hits
        .iter()
        .any(|h| h.location == crate::chunk::Location::Page { page: 1 }));
    assert_eq!(listing(&root), before);
}
