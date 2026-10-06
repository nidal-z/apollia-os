use std::path::Path;

use chrono::NaiveDate;

use super::ooxml::{edit_sheet, reference_column, resolve, stretched_range};
use super::*;

fn grid(cells: &[(u32, u32, &str)]) -> Grid {
    let mut grid = Grid::new();
    for (row, column, text) in cells {
        grid.entry(*row)
            .or_default()
            .push((*column, (*text).to_string()));
    }
    grid
}

fn edited(xml: &str, cells: &[(u32, u32, &str)], last: u32) -> String {
    String::from_utf8(edit_sheet(xml.as_bytes(), &grid(cells), last).unwrap()).unwrap()
}

#[test]
fn test_a_cell_reference_gives_its_column_number() {
    // GIVEN references of one, two and three letters, absolute or not
    let references = ["A1", "Z9", "AA10", "$C$3", "XFD1048576", "12"];

    // WHEN their columns are read
    let columns: Vec<Option<u32>> = references.iter().map(|r| reference_column(r)).collect();

    // THEN they are counted from 1, and a reference without letters has none
    assert_eq!(
        columns,
        [Some(1), Some(26), Some(27), Some(3), Some(16_384), None]
    );
}

#[test]
fn test_the_dimension_is_widened_to_the_added_columns_only() {
    // GIVEN ranges narrower and wider than column 4
    // WHEN they are stretched to column 4
    let stretched = [
        stretched_range("A1:B9", 4),
        stretched_range("A1", 4),
        stretched_range("A1:F9", 4),
    ];

    // THEN only the narrower ones change
    assert_eq!(stretched, ["A1:D9", "A1:D1", "A1:F9"]);
}

#[test]
fn test_cells_are_appended_to_their_rows_and_missing_rows_inserted_in_order() {
    // GIVEN a sheet whose rows 1 and 3 exist, with spans, and row 5 does not
    let xml = r#"<worksheet><dimension ref="A1:B3"/><sheetData><row r="1" spans="1:2"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row><row r="3" spans="1:2" ht="20"><c r="A3"><v>3</v></c></row></sheetData></worksheet>"#;

    // WHEN column C receives a header on row 1 and values on rows 2, 3 and 5
    let out = edited(
        xml,
        &[(1, 3, "R"), (2, 3, "a"), (3, 3, "b & <c>"), (5, 3, " d ")],
        3,
    );

    // THEN each cell is an inline string at the end of its row, rows 2 and 5
    // are inserted where they belong, and only the touched rows lose spans
    assert_eq!(
        out,
        concat!(
            r#"<worksheet><dimension ref="A1:C3"/><sheetData>"#,
            r#"<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1" t="inlineStr"><is><t xml:space="preserve">R</t></is></c></row>"#,
            r#"<row r="2"><c r="C2" t="inlineStr"><is><t xml:space="preserve">a</t></is></c></row>"#,
            r#"<row r="3" ht="20"><c r="A3"><v>3</v></c><c r="C3" t="inlineStr"><is><t xml:space="preserve">b &amp; &lt;c&gt;</t></is></c></row>"#,
            r#"<row r="5"><c r="C5" t="inlineStr"><is><t xml:space="preserve"> d </t></is></c></row>"#,
            r#"</sheetData></worksheet>"#
        )
    );
}

#[test]
fn test_an_empty_sheet_data_and_a_prefixed_sheet_are_written() {
    // GIVEN an empty sheet, and a sheet whose elements carry a prefix
    let empty = r#"<worksheet><sheetData/></worksheet>"#;
    let prefixed = r#"<x:worksheet xmlns:x="ns"><x:sheetData><x:row><x:c><x:v>1</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;

    // WHEN a cell is added to row 1 of each
    let from_empty = edited(empty, &[(1, 1, "v")], 1);
    let from_prefixed = edited(prefixed, &[(1, 2, "v")], 2);

    // THEN the empty sheet gets its row, and the prefixed one keeps its prefix
    // on a row whose number was implicit
    assert_eq!(
        from_empty,
        r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t xml:space="preserve">v</t></is></c></row></sheetData></worksheet>"#
    );
    assert_eq!(
        from_prefixed,
        r#"<x:worksheet xmlns:x="ns"><x:sheetData><x:row><x:c><x:v>1</x:v></x:c><x:c r="B1" t="inlineStr"><x:is><x:t xml:space="preserve">v</x:t></x:is></x:c></x:row></x:sheetData></x:worksheet>"#
    );
}

#[test]
fn test_a_relationship_target_resolves_from_its_folder_or_the_root() {
    // GIVEN targets relative, parent-relative and absolute
    // WHEN they are resolved from the folder of the workbook
    let parts = [
        resolve("xl", "worksheets/sheet1.xml"),
        resolve("xl", "../customXml/item1.xml"),
        resolve("xl", "/xl/worksheets/sheet2.xml"),
    ];

    // THEN each names a part from the root of the archive
    assert_eq!(
        parts,
        [
            "xl/worksheets/sheet1.xml",
            "customXml/item1.xml",
            "xl/worksheets/sheet2.xml"
        ]
    );
}

#[test]
fn test_a_cell_value_drops_what_xml_refuses_and_stops_at_the_cell_limit() {
    // GIVEN a text with control characters, and a text too long for a cell
    let controlled = "a\u{0}b\u{1b}c\td\ne";
    let long = "x".repeat(CELL_MAX_CHARS + 10);

    // WHEN they become cell values
    let (clean, cut) = (cell_value(controlled), cell_value(&long));

    // THEN the refused characters are gone, tabs and line breaks kept, and the
    // long text is cut
    assert_eq!(clean, "abc\td\ne");
    assert_eq!(cut.chars().count(), CELL_MAX_CHARS);
}

#[test]
fn test_a_legacy_workbook_is_copied_as_xlsx_under_the_same_name() {
    // GIVEN a .xls workbook and a CSV file
    let date = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
    let xls = Path::new("/data/ventes.xls");
    let csv = Path::new("/data/ventes.csv");

    // WHEN their copies of results are named
    let names = [
        results_copy_path(xls, TableFormat::Xls, date, "en", 1),
        results_copy_path(csv, TableFormat::Csv, date, "FR", 3),
    ];

    // THEN the workbook becomes a .xlsx, the CSV stays a CSV
    assert_eq!(
        names,
        [
            Path::new("/data/ventes (results 2026-10-04).xlsx").to_path_buf(),
            Path::new("/data/ventes (résultats 2026-10-04) (3).csv").to_path_buf(),
        ]
    );
}

#[test]
fn test_a_destination_is_read_from_its_option_value() {
    // GIVEN the two option values and an unknown one
    // WHEN they are parsed
    let parsed = [
        ResultsDestination::parse("copy"),
        ResultsDestination::parse(" original "),
        ResultsDestination::parse("both"),
    ];

    // THEN the known values name their destination
    assert_eq!(
        parsed,
        [
            Some(ResultsDestination::Copy),
            Some(ResultsDestination::Original),
            None
        ]
    );
}
