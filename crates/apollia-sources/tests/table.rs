//! A workbook or a CSV file as the table of a batch: the registry, the
//! preview and its recalculation, against written files and fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use apollia_core::batch_source::{SourceRegistry, SourceSpec};
use apollia_sources::{
    preview_table, register_table_sources, PreviewError, TableFormat, CSV_KIND, OPTION_COLUMNS,
    SPREADSHEET_KIND,
};
use rust_xlsxwriter::Workbook;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/spreadsheets")
        .join(name)
}

/// A workbook with an empty cover sheet, then a sheet holding a title, a
/// blank row, a header on row 3 and three clients, one blank row among them.
fn clients_workbook(dir: &Path) -> PathBuf {
    let path = dir.join("clients.xlsx");
    let mut workbook = Workbook::new();
    workbook.add_worksheet().set_name("Garde").unwrap();
    let sheet = workbook.add_worksheet().set_name("Clients").unwrap();
    sheet.write_string(0, 0, "Clients 2024").unwrap();
    for (column, name) in ["Nom", "Ville", "Montant"].iter().enumerate() {
        sheet
            .write_string(2, u16::try_from(column).unwrap(), *name)
            .unwrap();
    }
    let rows: [(u32, &str, &str, f64); 3] = [
        (3, "Durand", "Lyon", 1200.0),
        (4, "Martin", "Nantes", 830.5),
        (6, "Petit", "Lille", 75.0),
    ];
    for (row, name, city, amount) in rows {
        sheet.write_string(row, 0, name).unwrap();
        sheet.write_string(row, 1, city).unwrap();
        sheet.write_number(row, 2, amount).unwrap();
    }
    workbook.save(&path).unwrap();
    path
}

fn registry() -> SourceRegistry {
    let mut registry = SourceRegistry::new();
    register_table_sources(&mut registry).unwrap();
    registry
}

#[test]
fn test_the_two_table_kinds_enter_the_registry() {
    // GIVEN an empty batch source registry
    let mut registry = SourceRegistry::new();

    // WHEN the table sources register, then register a second time
    register_table_sources(&mut registry).unwrap();
    let again = register_table_sources(&mut registry);

    // THEN both kinds are listed, once
    assert_eq!(registry.kinds(), [CSV_KIND, SPREADSHEET_KIND]);
    assert!(again.is_err());
}

#[test]
fn test_a_workbook_source_yields_one_item_per_non_empty_row_on_the_proposed_table() {
    // GIVEN a workbook with a cover sheet and a title above its header
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());

    // WHEN it is opened as a spreadsheet source with two columns checked
    let mut options = BTreeMap::new();
    options.insert(OPTION_COLUMNS.to_string(), "A,C".to_string());
    let spec = SourceSpec {
        location: path.display().to_string(),
        options,
    };
    let source = registry().open(SPREADSHEET_KIND, &spec).unwrap();
    let items = source.items().unwrap();
    let estimated = items.estimated_count;
    let items: Vec<_> = items.items.map(Result::unwrap).collect();

    // THEN the data sheet and the row-3 header are used, the blank row is
    // skipped, and each item carries its sheet and row
    assert_eq!(source.fields(), ["Nom", "Montant"]);
    assert_eq!(estimated, 3);
    let origins: Vec<&str> = items.iter().map(|item| item.origin.as_str()).collect();
    assert_eq!(origins, ["Clients!4", "Clients!5", "Clients!7"]);
    assert_eq!(items[1].fields["Nom"], "Martin");
    assert_eq!(items[1].fields["Montant"], "830.5");
}

#[test]
fn test_a_csv_source_reads_its_rows_and_each_kind_refuses_the_other_format() {
    // GIVEN a semicolon CSV file with a blank line, and a workbook
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("achats.csv");
    std::fs::write(&csv, "article;prix\npain;1,20\n;\nlait;0,95\n").unwrap();
    let xlsx = clients_workbook(dir.path());
    let spec = |path: &Path| SourceSpec {
        location: path.display().to_string(),
        options: BTreeMap::new(),
    };

    // WHEN the CSV file is opened as csv, and each file under the other kind
    let source = registry().open(CSV_KIND, &spec(&csv)).unwrap();
    let items: Vec<_> = source.items().unwrap().items.map(Result::unwrap).collect();
    let csv_as_spreadsheet = registry().open(SPREADSHEET_KIND, &spec(&csv));
    let xlsx_as_csv = registry().open(CSV_KIND, &spec(&xlsx));

    // THEN the CSV rows are items with their origin, and the mismatches are refused
    assert_eq!(source.fields(), ["article", "prix"]);
    let origins: Vec<&str> = items.iter().map(|item| item.origin.as_str()).collect();
    assert_eq!(origins, ["achats!2", "achats!4"]);
    assert_eq!(items[1].fields["prix"], "0,95");
    assert!(csv_as_spreadsheet.is_err());
    assert!(xlsx_as_csv.is_err());
}

#[test]
fn test_the_preview_shows_the_proposal_and_the_first_rows() {
    // GIVEN a workbook with a cover sheet and a title above its header
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());

    // WHEN it is previewed without a choice
    let preview = preview_table(&path, None, None).unwrap();

    // THEN both sheets are listed, the data sheet and row 3 are proposed and
    // used, the first rows are shown from the top, and an xlsx may be modified
    let sheets: Vec<(&str, usize)> = preview
        .sheets
        .iter()
        .map(|sheet| (sheet.name.as_str(), sheet.non_empty_rows))
        .collect();
    assert_eq!(sheets, [("Garde", 0), ("Clients", 5)]);
    assert_eq!(preview.proposed_sheet.as_deref(), Some("Clients"));
    assert_eq!(preview.sheet.as_deref(), Some("Clients"));
    assert_eq!(preview.proposed_header_row, Some(3));
    assert_eq!(preview.header_row, Some(3));
    let columns: Vec<(&str, &str)> = preview
        .columns
        .iter()
        .map(|column| (column.letter.as_str(), column.name.as_str()))
        .collect();
    assert_eq!(columns, [("A", "Nom"), ("B", "Ville"), ("C", "Montant")]);
    assert_eq!(preview.rows.len(), 7);
    assert_eq!(preview.rows[0].row, 1);
    assert_eq!(preview.rows[0].cells, ["Clients 2024", "", ""]);
    assert_eq!(preview.rows[3].cells, ["Durand", "Lyon", "1200"]);
    assert_eq!(preview.estimated_items, 3);
    assert_eq!(preview.format, TableFormat::Xlsx);
    assert!(preview.can_modify_original);
    assert!(!preview.formatting_not_kept);
}

#[test]
fn test_the_preview_follows_the_sheet_and_the_header_row_the_user_picks() {
    // GIVEN a workbook previewed once on its proposal
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let proposed = preview_table(&path, None, None).unwrap();

    // WHEN the header row is moved to row 4, then the cover sheet is picked
    let moved = preview_table(&path, Some("Clients"), Some(4)).unwrap();
    let cover = preview_table(&path, Some("Garde"), None).unwrap();

    // THEN the columns and the item count follow the new header row, the
    // proposal stays what it was, and the empty sheet shows no table
    assert_eq!(proposed.estimated_items, 3);
    assert_eq!(moved.header_row, Some(4));
    assert_eq!(moved.proposed_header_row, Some(3));
    let names: Vec<&str> = moved.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Durand", "Lyon", "1200"]);
    assert_eq!(moved.estimated_items, 2);
    assert_eq!(cover.sheet.as_deref(), Some("Garde"));
    assert_eq!(cover.proposed_sheet.as_deref(), Some("Clients"));
    assert_eq!(cover.header_row, None);
    assert!(cover.columns.is_empty());
    assert_eq!(cover.estimated_items, 0);
}

#[test]
fn test_the_preview_refuses_a_sheet_or_a_row_the_file_lacks() {
    // GIVEN a workbook
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());

    // WHEN an absent sheet, then an absent row, is asked for
    let sheet = preview_table(&path, Some("Absente"), None);
    let row = preview_table(&path, Some("Clients"), Some(99));

    // THEN each is refused by name
    assert!(matches!(sheet, Err(PreviewError::UnknownSheet(name)) if name == "Absente"));
    assert!(matches!(
        row,
        Err(PreviewError::UnknownHeaderRow { row: 99, .. })
    ));
}

#[test]
fn test_the_preview_flags_follow_the_file_format() {
    // GIVEN the same workbook in five formats, and a CSV file
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("t.csv");
    std::fs::write(&csv, "a,b\n1,2\n").unwrap();
    let files = [
        fixture("issues.xlsx"),
        fixture("issue221.xlsm"),
        fixture("issues.xlsb"),
        fixture("issues.xls"),
        fixture("issues.ods"),
        csv,
    ];

    for path in files {
        // WHEN each is previewed
        let preview = preview_table(&path, None, None).unwrap();

        // THEN only xlsx and xlsm may be modified in place, and only xls,
        // xlsb and ods lose their formatting in the copy
        let extension = preview.format.extension();
        assert_eq!(
            preview.can_modify_original,
            matches!(extension, "xlsx" | "xlsm"),
            "{extension}"
        );
        assert_eq!(
            preview.formatting_not_kept,
            matches!(extension, "xls" | "xlsb" | "ods"),
            "{extension}"
        );
        assert!(preview.sheet.is_some(), "{extension}");
    }
}
