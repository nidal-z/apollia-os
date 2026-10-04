//! The PDF reader, against versioned fixtures and written files.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use apollia_sources::{read_pdf, PageText, PdfDocument, ReadError};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf")
        .join(name)
}

fn numbers(document: &PdfDocument) -> Vec<u32> {
    document.pages.iter().map(|page| page.number).collect()
}

#[test]
fn test_each_page_is_read_with_its_number_and_its_accents() {
    // GIVEN a three-page document whose pages one and three hold accented text
    let path = fixture("second-page-blank.pdf");

    // WHEN it is read
    let document = read_pdf(&path).unwrap();

    // THEN every page comes back in order, numbered from 1, accents intact
    assert_eq!(numbers(&document), [1, 2, 3]);
    let first = document.pages[0].text.as_text().unwrap();
    assert!(first.contains("Facture n° 2026-001"), "{first}");
    assert!(
        first.contains("Prestation réalisée à Besançon, payée à réception."),
        "{first}"
    );
    let third = document.pages[2].text.as_text().unwrap();
    assert!(
        third.contains("Troisième page : clôture de l'exercice."),
        "{third}"
    );
}

#[test]
fn test_a_page_without_text_is_marked_without_text() {
    // GIVEN a document whose second page draws nothing
    let path = fixture("second-page-blank.pdf");

    // WHEN it is read
    let document = read_pdf(&path).unwrap();

    // THEN that page, and only that one, is marked without text
    let marks: Vec<bool> = document
        .pages
        .iter()
        .map(|page| page.text == PageText::WithoutText)
        .collect();
    assert_eq!(marks, [false, true, false]);
    assert_eq!(document.pages[1].text.as_text(), None);
}

#[test]
fn test_a_document_locked_with_an_owner_password_only_is_read() {
    // GIVEN an encrypted document whose user password is empty
    let path = fixture("owner-password-only.pdf");

    // WHEN it is read
    let document = read_pdf(&path).unwrap();

    // THEN its text comes back as from the plain document
    assert_eq!(numbers(&document), [1, 2, 3]);
    let first = document.pages[0].text.as_text().unwrap();
    assert!(first.contains("payée à réception"), "{first}");
}

#[test]
fn test_a_document_that_needs_a_password_is_a_typed_error() {
    // GIVEN a document encrypted with a user password
    let path = fixture("user-password.pdf");

    // WHEN it is read
    let error = read_pdf(&path).unwrap_err();

    // THEN the error says so and names the file
    assert!(
        matches!(error, ReadError::PasswordProtected { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("user-password.pdf"), "{error}");
}

#[test]
fn test_a_damaged_document_is_a_typed_error_never_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let whole = std::fs::read(fixture("second-page-blank.pdf")).unwrap();
    let header_only = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut no_trailer = whole.clone();
    no_trailer.truncate(whole.len() - 120);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("garbage.pdf", b"this is not a PDF document".to_vec()),
        ("empty.pdf", Vec::new()),
        ("zeros.pdf", vec![0u8; 512]),
        ("header-only.pdf", header_only),
        ("truncated.pdf", whole[..whole.len() / 3].to_vec()),
    ];
    for (name, bytes) in cases {
        // GIVEN a file named as a PDF whose content is broken
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();

        // WHEN it is read
        let error = read_pdf(&path).unwrap_err();

        // THEN the error is typed and readable
        assert!(
            matches!(error, ReadError::Corrupt { .. }),
            "{name}: {error:?}"
        );
        assert!(error.to_string().contains(name), "{error}");
    }

    // GIVEN a document cut short of its cross-reference table
    let path = dir.path().join("no-trailer.pdf");
    std::fs::write(&path, no_trailer).unwrap();

    // WHEN it is read
    let read = read_pdf(&path);

    // THEN it is either rebuilt by the parser or a typed error, never a panic
    if let Err(error) = read {
        assert!(matches!(error, ReadError::Corrupt { .. }), "{error:?}");
    }
}

#[test]
fn test_a_missing_document_is_a_typed_error() {
    // GIVEN a PDF path that does not exist
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.pdf");

    // WHEN it is read
    let error = read_pdf(&missing).unwrap_err();

    // THEN it is an I/O error
    assert!(matches!(error, ReadError::Io { .. }), "{error:?}");
}
