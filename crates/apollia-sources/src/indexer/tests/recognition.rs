//! Scanned pages and pictures in a source: marked while recognition is
//! missing, read once it is there, and never written beside the documents.

use super::*;
use crate::index::{UnreadPage, UnreadReason};
use crate::ocr::Ocr;

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
