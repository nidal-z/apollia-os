//! One document read as the text of its parts, against the readers' fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use apollia_sources::{
    is_office_document, is_readable_document, read_document, PartUnit, ReadError,
};

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

#[test]
fn test_a_pdf_is_read_as_its_pages_a_blank_one_included() {
    // GIVEN a three-page PDF whose second page draws nothing
    let path = fixture("pdf/second-page-blank.pdf");

    // WHEN it is read as a document
    let document = read_document(&path).unwrap();

    // THEN it has three numbered pages, the second with an empty text
    assert_eq!(document.unit, PartUnit::Page);
    let numbers: Vec<u32> = document.parts.iter().map(|part| part.number).collect();
    assert_eq!(numbers, [1, 2, 3]);
    assert!(document.parts[0].text.contains("Facture n° 2026-001"));
    assert_eq!(document.parts[1].text, "");
    assert_eq!(
        document.heading(&document.parts[2]).as_deref(),
        Some("page 3")
    );
}

#[test]
fn test_a_workbook_is_read_as_its_sheets_by_name() {
    // GIVEN a workbook with a sheet "issue2" of three rows of two cells
    let path = fixture("spreadsheets/issues.xlsx");

    // WHEN it is read as a document
    let document = read_document(&path).unwrap();

    // THEN that sheet is a part named after it, its rows as tabbed lines
    assert_eq!(document.unit, PartUnit::Sheet);
    let sheet = document
        .parts
        .iter()
        .find(|part| part.name.as_deref() == Some("issue2"))
        .unwrap();
    assert_eq!(sheet.text, "1\ta\n2\tb\n3\tc");
    assert_eq!(document.heading(sheet).as_deref(), Some("sheet \"issue2\""));
}

#[test]
fn test_a_presentation_is_read_as_its_slides_with_their_notes() {
    // GIVEN a presentation of ten slides, the second with speaker notes
    let path = fixture("office/deck.pptx");

    // WHEN it is read as a document
    let document = read_document(&path).unwrap();

    // THEN it has ten slides, and the notes follow the text of the second
    assert_eq!(document.unit, PartUnit::Slide);
    assert_eq!(document.parts.len(), 10);
    assert_eq!(
        document.parts[1].text,
        "Slide 2 title\nPoint for slide 2\nMention the café budget"
    );
}

#[test]
fn test_a_word_document_is_one_part_and_its_full_text_has_no_heading() {
    // GIVEN a Word document
    let path = fixture("office/report.docx");

    // WHEN it is read as a document
    let document = read_document(&path).unwrap();

    // THEN it is one part, and the full text is that part alone
    assert_eq!(document.unit, PartUnit::Whole);
    assert_eq!(document.parts.len(), 1);
    assert!(document.parts[0].text.starts_with("Annual report"));
    assert_eq!(document.full_text(), document.parts[0].text);
    assert_eq!(
        document.characters(),
        document.parts[0].text.chars().count()
    );
}

#[test]
fn test_the_full_text_of_a_paged_document_heads_each_part() {
    // GIVEN a three-page PDF
    let path = fixture("pdf/second-page-blank.pdf");

    // WHEN its full text is taken
    let text = read_document(&path).unwrap().full_text();

    // THEN each page is introduced by its heading, in order
    let positions: Vec<usize> = ["--- page 1 ---", "--- page 2 ---", "--- page 3 ---"]
        .iter()
        .map(|heading| text.find(heading).unwrap())
        .collect();
    assert!(positions[0] < positions[1] && positions[1] < positions[2]);
}

#[test]
fn test_a_format_no_reader_opens_is_refused_by_its_extension() {
    // GIVEN names of an archive, a legacy Word file and a text file
    let names = ["archive.zip", "old.doc", "notes.txt"];

    for name in names {
        // WHEN each is asked for as a document
        let error = read_document(Path::new(name)).unwrap_err();

        // THEN it is an unsupported format, and no predicate takes it
        assert!(
            matches!(error, ReadError::UnsupportedFormat { .. }),
            "{name}: {error:?}"
        );
        assert!(!is_office_document(Path::new(name)), "{name}");
        assert!(!is_readable_document(Path::new(name)), "{name}");
    }
}

#[test]
fn test_a_csv_file_is_readable_but_is_not_an_office_document() {
    // GIVEN the names of a CSV file and of office documents, in any case
    let csv = Path::new("clients.CSV");
    let office = [
        "a.PDF", "b.docx", "c.pptx", "d.xlsx", "e.xlsm", "f.xls", "g.ods",
    ];

    // WHEN the predicates read them
    // THEN the CSV file is readable without being an office document, and
    // every office name is both
    assert!(is_readable_document(csv));
    assert!(!is_office_document(csv));
    for name in office {
        assert!(is_office_document(Path::new(name)), "{name}");
        assert!(is_readable_document(Path::new(name)), "{name}");
    }
}
