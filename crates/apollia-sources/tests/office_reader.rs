//! The Word and PowerPoint readers, against versioned fixtures and written files.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};

use apollia_sources::{
    read_presentation, read_word, ReadError, SegmentKind, OFFICE_MAX_DECOMPRESSED_BYTES,
    OFFICE_MAX_ENTRIES,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/office")
        .join(name)
}

#[test]
fn test_paragraphs_and_table_rows_are_read_in_order_with_their_rank() {
    // GIVEN a document with a paragraph in two runs, an empty paragraph, an
    // accented paragraph, a table of three rows (the last empty), a paragraph
    // with a line break and a text box, and a paragraph in a content control
    let path = fixture("report.docx");

    // WHEN it is read
    let document = read_word(&path).unwrap();

    // THEN each segment comes in document order, empty ones skipped but ranked
    let segments: Vec<(u32, SegmentKind, &str)> = document
        .segments
        .iter()
        .map(|segment| (segment.rank, segment.kind, segment.text.as_str()))
        .collect();
    assert_eq!(
        segments,
        [
            (1, SegmentKind::Paragraph, "Annual report"),
            (
                3,
                SegmentKind::Paragraph,
                "Résumé: the café's naïve figures\t& their « notes »."
            ),
            (4, SegmentKind::TableRow, "Item\tAmount"),
            (5, SegmentKind::TableRow, "Crème brûlée dessert\t12 €"),
            (7, SegmentKind::Paragraph, "Closing\nnote"),
            (8, SegmentKind::Paragraph, "Inside a content control"),
        ]
    );
}

#[test]
fn test_the_page_header_and_the_text_box_are_not_read() {
    // GIVEN a document whose header part and text box hold text
    let path = fixture("report.docx");

    // WHEN it is read
    let document = read_word(&path).unwrap();

    // THEN neither appears in any segment
    for segment in &document.segments {
        assert!(!segment.text.contains("Page header"), "{segment:?}");
        assert!(!segment.text.contains("Floating box"), "{segment:?}");
    }
}

#[test]
fn test_slides_are_read_in_numeric_order_with_their_notes() {
    // GIVEN a ten-slide deck whose archive lists slide 10 before slide 2, with
    // notes on slides 2 and 10 in parts numbered otherwise
    let path = fixture("deck.pptx");

    // WHEN it is read
    let presentation = read_presentation(&path).unwrap();

    // THEN slides come 1 to 10, each with its text and its own notes
    let numbers: Vec<u32> = presentation
        .slides
        .iter()
        .map(|slide| slide.number)
        .collect();
    assert_eq!(numbers, (1..=10).collect::<Vec<u32>>());
    let first = &presentation.slides[0];
    assert_eq!(first.text, "Quarterly review\nPréparé à Genève");
    assert_eq!(first.notes, "");
    let second = &presentation.slides[1];
    assert_eq!(second.text, "Slide 2 title\nPoint for slide 2");
    assert_eq!(second.notes, "Mention the café budget");
    assert_eq!(presentation.slides[3].text, "");
    assert_eq!(
        presentation.slides[8].text,
        "Slide 9 title\nPoint for slide 9"
    );
    let tenth = &presentation.slides[9];
    assert_eq!(tenth.text, "Slide 10 title\nPoint for slide 10");
    assert_eq!(tenth.notes, "Thank the audience");
}

#[test]
fn test_an_archive_inflating_past_the_cap_is_refused() {
    // GIVEN a document of 80 KiB whose main part inflates to 80 MiB
    let path = fixture("inflated.docx");
    assert!(std::fs::metadata(&path).unwrap().len() < 100 * 1024);

    // WHEN it is read
    let error = read_word(&path).unwrap_err();

    // THEN the decompressed-bytes cap refuses it
    assert!(
        matches!(
            error,
            ReadError::ArchiveOverCap { cap: "bytes decompressed", limit, .. }
                if limit == OFFICE_MAX_DECOMPRESSED_BYTES
        ),
        "{error:?}"
    );
}

#[test]
fn test_an_archive_with_too_many_entries_is_refused() {
    // GIVEN a presentation archive with one entry more than the cap
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crowded.pptx");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..=OFFICE_MAX_ENTRIES {
        zip.start_file(format!("ppt/media/blank{index}.txt"), options)
            .unwrap();
    }
    zip.finish().unwrap();

    // WHEN it is read
    let error = read_presentation(&path).unwrap_err();

    // THEN the entry cap refuses it
    assert!(
        matches!(
            error,
            ReadError::ArchiveOverCap { cap: "entries", limit, .. }
                if limit == OFFICE_MAX_ENTRIES as u64
        ),
        "{error:?}"
    );
}

#[test]
fn test_legacy_binary_word_and_powerpoint_files_are_an_unsupported_format() {
    // GIVEN a legacy .doc and a legacy .ppt file
    let dir = tempfile::tempdir().unwrap();
    let doc = write(
        dir.path(),
        "minutes.doc",
        b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1",
    );
    let ppt = write(
        dir.path(),
        "slides.ppt",
        b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1",
    );

    // WHEN each is read by its reader
    let doc = read_word(&doc).unwrap_err();
    let ppt = read_presentation(&ppt).unwrap_err();

    // THEN both are an unsupported format naming what is supported
    assert!(
        matches!(
            doc,
            ReadError::UnsupportedFormat {
                supported: "docx, docm",
                ..
            }
        ),
        "{doc:?}"
    );
    assert!(
        matches!(
            ppt,
            ReadError::UnsupportedFormat {
                supported: "pptx, pptm",
                ..
            }
        ),
        "{ppt:?}"
    );
    assert!(doc
        .to_string()
        .contains("is not a format this reader opens"));
}

#[test]
fn test_a_damaged_or_foreign_file_is_a_typed_error() {
    // GIVEN a .docx that is plain text, a .docx archive without its main part,
    // a .pptx archive without slides or presentation, and a missing file
    let dir = tempfile::tempdir().unwrap();
    let text = write(dir.path(), "text.docx", b"not an archive");
    let hollow = zip_with(dir.path(), "hollow.docx", "word/styles.xml", "<w:styles/>");
    let empty = zip_with(
        dir.path(),
        "empty.pptx",
        "docProps/app.xml",
        "<Properties/>",
    );
    let broken = zip_with(
        dir.path(),
        "broken.docx",
        "word/document.xml",
        "<w:document><w:body><w:p></w:body></w:document>",
    );
    let missing = dir.path().join("absent.docx");

    // WHEN each is read
    let errors = [
        read_word(&text).unwrap_err(),
        read_word(&hollow).unwrap_err(),
        read_presentation(&empty).unwrap_err(),
        read_word(&broken).unwrap_err(),
    ];
    let missing = read_word(&missing).unwrap_err();

    // THEN the first four are damaged files and the last an I/O error
    for error in &errors {
        assert!(matches!(error, ReadError::Corrupt { .. }), "{error:?}");
    }
    assert!(matches!(missing, ReadError::Io { .. }), "{missing:?}");
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn zip_with(dir: &Path, name: &str, part: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(part, SimpleFileOptions::default()).unwrap();
    zip.write_all(content.as_bytes()).unwrap();
    zip.finish().unwrap();
    path
}
