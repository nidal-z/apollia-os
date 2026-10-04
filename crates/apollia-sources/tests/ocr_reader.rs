#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Scanned pages and pictures, read by recognition.
//!
//! The fixtures under `tests/fixtures/ocr` are text drawn to pictures, as a
//! scanner gives them. The tests of the readers run with a stand-in for the
//! engine; those of the engine itself need the `ocr` feature and the French
//! language data the registry pins, in the folder `APOLLIA_TEST_TESSDATA`
//! names: the file is over the size a commit may add, so it is not kept here.

use std::path::PathBuf;

use apollia_sources::{read_pdf, read_pdf_scanned, read_picture, Ocr, PageText, ReadError};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ocr")
        .join(name)
}

/// A stand-in for the engine: it answers the size of the picture it is given.
fn stand_in() -> Ocr {
    Ocr::with(|picture| Ok(format!("picture {}x{}", picture.width(), picture.height())))
}

fn texts(ocr: &Ocr) -> Vec<PageText> {
    read_pdf_scanned(&fixture("scanned-fr.pdf"), ocr)
        .unwrap()
        .pages
        .into_iter()
        .map(|page| page.text)
        .collect()
}

#[test]
fn test_a_scanned_page_is_read_from_the_picture_it_embeds() {
    // GIVEN a scanned document: two pages of one picture each, and a third
    // page made of two pictures
    let ocr = stand_in();

    // WHEN it is read with recognition
    let pages = texts(&ocr);

    // THEN each single picture reaches the engine at the size the document
    // embeds it, no page being rendered, and the third page is not read
    assert_eq!(
        pages,
        [
            PageText::Text("picture 1654x600".to_owned()),
            PageText::Text("picture 1654x600".to_owned()),
            PageText::NotRead,
        ]
    );
}

#[test]
fn test_without_recognition_a_scanned_page_is_marked_and_the_plain_reader_is_unchanged() {
    // GIVEN the same document and no recognition
    let ocr = Ocr::unavailable();

    // WHEN it is read with recognition asked for, and by the plain reader
    let asked = texts(&ocr);
    let plain = read_pdf(&fixture("scanned-fr.pdf")).unwrap();

    // THEN the single-picture pages wait for recognition, the third page is
    // not read either way, and the plain reader still says "without text"
    assert_eq!(
        asked,
        [
            PageText::OcrUnavailable,
            PageText::OcrUnavailable,
            PageText::NotRead,
        ]
    );
    assert!(plain
        .pages
        .iter()
        .all(|page| page.text == PageText::WithoutText));
}

#[test]
fn test_a_picture_file_is_read_by_recognition_in_each_format() {
    for name in ["recu.png", "recu.jpg", "recu.tiff"] {
        // GIVEN a picture of a format the reader opens
        let path = fixture(name);

        // WHEN it is read with and without recognition
        let read = read_picture(&path, &stand_in()).unwrap();
        let waiting = read_picture(&path, &Ocr::unavailable()).unwrap();

        // THEN it is decoded and handed to the engine, or marked as waiting
        assert_eq!(
            read,
            PageText::Text("picture 1654x600".to_owned()),
            "{name}"
        );
        assert_eq!(waiting, PageText::OcrUnavailable, "{name}");
    }
}

#[test]
fn test_a_damaged_picture_is_a_typed_error() {
    // GIVEN a file named as a picture that is not one
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.png");
    std::fs::write(&path, b"not a picture").unwrap();

    // WHEN it is read
    let error = read_picture(&path, &stand_in()).unwrap_err();

    // THEN the error is typed and names the file
    assert!(matches!(error, ReadError::Corrupt { .. }), "{error:?}");
    assert!(error.to_string().contains("broken.png"), "{error}");
}

#[cfg(feature = "ocr")]
mod engine {
    use super::*;

    /// The embedded engine over the French data in `APOLLIA_TEST_TESSDATA`.
    fn french() -> Ocr {
        let dir = std::env::var_os("APOLLIA_TEST_TESSDATA").expect("APOLLIA_TEST_TESSDATA is set");
        let ocr = Ocr::open(std::path::Path::new(&dir));
        assert!(ocr.is_available(), "the folder must hold the pinned file");
        ocr
    }

    #[test]
    #[ignore = "requires APOLLIA_TEST_TESSDATA, a folder holding the fra.traineddata the registry pins"]
    fn test_a_french_scanned_page_keeps_its_accents() {
        // GIVEN the scanned French document and the embedded engine
        let ocr = french();

        // WHEN it is read
        let pages = texts(&ocr);

        // THEN the first page reads with é, è, à and ç intact
        let first = pages[0].as_text().expect("the first page is read");
        for word in [
            "marché",
            "été",
            "à",
            "Besançon",
            "reçu",
            "après",
            "première",
        ] {
            assert!(first.contains(word), "{word} missing from: {first}");
        }
        let second = pages[1].as_text().expect("the second page is read");
        assert!(second.contains("Deuxième"), "{second}");
        assert_eq!(pages[2], PageText::NotRead);
    }

    #[test]
    #[ignore = "requires APOLLIA_TEST_TESSDATA, a folder holding the fra.traineddata the registry pins"]
    fn test_a_french_picture_is_read_in_each_format() {
        let ocr = french();
        for name in ["recu.png", "recu.jpg", "recu.tiff"] {
            // GIVEN a picture of French text
            // WHEN it is read
            let read = read_picture(&fixture(name), &ocr).unwrap();

            // THEN its text comes back with its accents
            let text = read.as_text().expect("the picture is read");
            assert!(
                text.contains("Besançon") && text.contains("première"),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn test_without_language_data_the_engine_is_not_available() {
        // GIVEN a data folder holding a file of the right name and other bytes
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fra.traineddata"), b"not the pinned file").unwrap();

        // WHEN the engine is opened over it, and over an empty folder
        let altered = Ocr::open(dir.path());
        let empty = Ocr::open(&dir.path().join("absent"));

        // THEN it is not available: an unpinned file never reaches the engine
        assert!(!altered.is_available());
        assert!(!empty.is_available());
    }
}
