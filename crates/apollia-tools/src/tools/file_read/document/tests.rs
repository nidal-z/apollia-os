//! `file_read` over office documents.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::tools::file_read::{FileRead, FileReadError, FileReadInput};

/// A fixture of the readers, shared with the crate that owns them.
fn reader_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../apollia-sources/tests/fixtures")
        .join(relative)
}

/// Write a PDF of one page per entry of `pages`, each drawing its text.
fn write_pdf(path: &Path, pages: &[&str]) {
    let mut objects: Vec<String> = Vec::new();
    let kids: Vec<String> = (0..pages.len())
        .map(|index| format!("{} 0 R", 4 + 2 * index))
        .collect();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    for (index, text) in pages.iter().enumerate() {
        let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R \
             /Resources << /Font << /F1 3 0 R >> >> >>",
            5 + 2 * index
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", index + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    out.push_str("0000000000 65535 f \n");
    for offset in offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    std::fs::write(path, out).expect("write the PDF");
}

fn input(path: &str) -> FileReadInput {
    FileReadInput {
        path: path.to_string(),
        offset: None,
        limit: None,
        page: None,
        sheet: None,
    }
}

fn three_page_sandbox() -> (TempDir, FileRead) {
    let dir = TempDir::new().expect("temp dir");
    write_pdf(
        &dir.path().join("invoice.pdf"),
        &[
            "Invoice 2026-001",
            "Total due 1840 euros",
            "Payment within 30 days",
        ],
    );
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");
    (dir, tool)
}

#[tokio::test]
async fn test_one_page_of_a_pdf_is_read_alone() {
    // GIVEN a PDF of three pages
    let (_dir, tool) = three_page_sandbox();

    // WHEN file_read asks for page 2
    let output = tool
        .run(FileReadInput {
            page: Some(2),
            ..input("invoice.pdf")
        })
        .await
        .expect("page 2 is read");

    // THEN the text of page 2 comes back, and nothing of pages 1 and 3
    assert!(
        output.content.contains("Total due 1840 euros"),
        "{}",
        output.content
    );
    assert!(!output.content.contains("Invoice 2026-001"));
    assert!(!output.content.contains("Payment within 30 days"));
    assert_eq!(output.parts.as_deref(), Some("3 pages"));
}

#[tokio::test]
async fn test_a_whole_pdf_is_read_page_by_page_under_headings() {
    // GIVEN a PDF of three pages
    let (_dir, tool) = three_page_sandbox();

    // WHEN file_read asks for the file, with no page
    let output = tool.run(input("invoice.pdf")).await.expect("read");

    // THEN every page comes in order, each under its heading
    let content = &output.content;
    let first = content.find("--- page 1 ---").expect("heading 1");
    let second = content.find("--- page 2 ---").expect("heading 2");
    let third = content.find("--- page 3 ---").expect("heading 3");
    assert!(first < second && second < third, "{content}");
    assert!(content.contains("Invoice 2026-001"));
    assert!(content.contains("Payment within 30 days"));
}

#[tokio::test]
async fn test_a_page_past_the_end_names_what_the_document_holds() {
    // GIVEN a PDF of three pages
    let (_dir, tool) = three_page_sandbox();

    // WHEN file_read asks for page 7
    let error = tool
        .run(FileReadInput {
            page: Some(7),
            ..input("invoice.pdf")
        })
        .await
        .expect_err("page 7 is not there");

    // THEN the refusal says the page asked for and the three pages there are
    match error {
        FileReadError::PartNotFound {
            part, available, ..
        } => {
            assert_eq!(part, "page 7");
            assert_eq!(available, "3 pages");
        }
        other => panic!("expected PartNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn test_one_sheet_of_a_workbook_is_read_alone() {
    // GIVEN a workbook with a sheet "issue2" of three rows, among others
    let dir = TempDir::new().expect("temp dir");
    std::fs::copy(
        reader_fixture("spreadsheets/issues.xlsx"),
        dir.path().join("issues.xlsx"),
    )
    .expect("copy the workbook");
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read asks for that sheet
    let output = tool
        .run(FileReadInput {
            sheet: Some("issue2".to_string()),
            ..input("issues.xlsx")
        })
        .await
        .expect("the sheet is read");

    // THEN its three rows come back, cells separated by a tab, and the sheets
    // of the workbook are named
    assert_eq!(output.total_lines, 3);
    assert!(output.content.contains("1\ta"), "{}", output.content);
    assert!(output.content.contains("3\tc"), "{}", output.content);
    let parts = output.parts.expect("the sheets are named");
    assert!(parts.contains("\"issue2\""), "{parts}");
    assert!(parts.contains("\"datatypes\""), "{parts}");
}

#[tokio::test]
async fn test_a_word_document_and_a_presentation_are_read_as_text() {
    // GIVEN a Word document and a presentation of ten slides
    let dir = TempDir::new().expect("temp dir");
    for name in ["report.docx", "deck.pptx"] {
        std::fs::copy(
            reader_fixture(&format!("office/{name}")),
            dir.path().join(name),
        )
        .expect("copy the fixture");
    }
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read reads the document, then slide 2 of the presentation
    let report = tool.run(input("report.docx")).await.expect("report");
    let slide = tool
        .run(FileReadInput {
            page: Some(2),
            ..input("deck.pptx")
        })
        .await
        .expect("slide 2");

    // THEN the document gives its text, and the slide its text and its notes
    assert!(
        report.content.contains("Annual report"),
        "{}",
        report.content
    );
    assert!(
        slide.content.contains("Point for slide 2"),
        "{}",
        slide.content
    );
    assert!(slide.content.contains("Mention the caf"));
    assert!(!slide.content.contains("Quarterly review"));
    assert_eq!(slide.parts.as_deref(), Some("10 slides"));
}

#[tokio::test]
async fn test_a_damaged_document_is_refused_with_its_cause() {
    // GIVEN a file named as a PDF that is not one
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("broken.pdf"), b"\xff\xfe not a pdf").expect("write");
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read reads it
    let error = tool.run(input("broken.pdf")).await.expect_err("refused");

    // THEN the refusal is about the document, not about bytes that are not text
    assert!(
        matches!(error, FileReadError::UnreadableDocument { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn test_a_missing_document_is_not_found() {
    // GIVEN a sandbox without the document
    let dir = TempDir::new().expect("temp dir");
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read reads a PDF that is not there
    let error = tool.run(input("absent.pdf")).await.expect_err("refused");

    // THEN it is reported as not found
    assert!(matches!(error, FileReadError::NotFound { .. }), "{error:?}");
}

#[tokio::test]
async fn test_a_page_of_a_text_file_is_refused() {
    // GIVEN a text file
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("notes.txt"), "one\ntwo").expect("write");
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read asks for its page 1
    let error = tool
        .run(FileReadInput {
            page: Some(1),
            ..input("notes.txt")
        })
        .await
        .expect_err("refused");

    // THEN the refusal says the file has no pages
    assert!(matches!(error, FileReadError::NoParts { .. }), "{error:?}");
}

#[tokio::test]
async fn test_a_csv_file_in_another_encoding_is_still_read() {
    // GIVEN a CSV file saved in Windows-1252, whose accent is not UTF-8
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(
        dir.path().join("clients.csv"),
        b"nom;ville\nDupont;Besan\xe7on\n",
    )
    .expect("write");
    let tool = FileRead::new(dir.path().to_path_buf()).expect("file_read");

    // WHEN file_read reads it
    let output = tool.run(input("clients.csv")).await.expect("read");

    // THEN its rows come back as text, cells separated by a tab
    assert!(output.content.contains("nom\tville"), "{}", output.content);
    assert!(
        output.content.contains("Dupont\tBesan"),
        "{}",
        output.content
    );
}
