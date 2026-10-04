//! The workbook and CSV readers, against versioned fixtures and written files.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use apollia_sources::{
    read_csv, read_workbook, CellLocation, Delimiter, ReadError, Sheet, Workbook,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/spreadsheets")
        .join(name)
}

fn sheet<'a>(workbook: &'a Workbook, name: &str) -> &'a Sheet {
    workbook
        .sheets
        .iter()
        .find(|sheet| sheet.name == name)
        .unwrap_or_else(|| panic!("no sheet {name}"))
}

fn texts(sheet: &Sheet) -> Vec<Vec<&str>> {
    sheet
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.text.as_str()).collect())
        .collect()
}

/// The `datatypes` sheet of the `issues` workbooks: a whole number, a decimal,
/// a formula `CONCATENATE("a","b")`, a formula `A1>A2`, a string and a date.
const DATATYPES: [[&str; 1]; 6] = [["1"], ["1.5"], ["ab"], ["FALSE"], ["test"], ["2016-10-20"]];

#[test]
fn test_each_workbook_format_reads_to_the_same_text() {
    for name in ["issues.xlsx", "issues.xlsb", "issues.xls", "issues.ods"] {
        // GIVEN the same workbook saved in one of four formats
        let path = fixture(name);

        // WHEN it is read
        let workbook = read_workbook(&path).unwrap();

        // THEN numbers lose their `.0`, formulas give their computed value,
        // the boolean and the date are text, and the sheets keep their names
        assert_eq!(texts(sheet(&workbook, "datatypes")), DATATYPES, "{name}");
        assert_eq!(
            texts(sheet(&workbook, "issue2")),
            [["1", "a"], ["2", "b"], ["3", "c"]],
            "{name}"
        );
        assert_eq!(texts(sheet(&workbook, "issue5")), [["0.5"]], "{name}");
    }
}

#[test]
fn test_a_macro_enabled_workbook_is_read() {
    // GIVEN an .xlsm workbook
    let path = fixture("issue221.xlsm");

    // WHEN it is read
    let workbook = read_workbook(&path).unwrap();

    // THEN its one sheet and its cells are there
    assert_eq!(workbook.sheets.len(), 1);
    assert_eq!(
        texts(sheet(&workbook, "Sheet1")),
        [["Cell_A1", "Cell_B1"], ["Cell_A2", "Cell_B2"]]
    );
}

#[test]
fn test_dates_and_durations_are_iso_8601() {
    // GIVEN a workbook holding two dates, a duration of 255 hours, and numbers
    let path = fixture("date.xlsx");

    // WHEN it is read
    let workbook = read_workbook(&path).unwrap();

    // THEN dates and the duration are ISO 8601 text
    assert_eq!(
        texts(&workbook.sheets[0]),
        [
            ["2021-01-01", "15"],
            ["2021-01-02", "16"],
            ["PT255H10M10S", "17"]
        ]
    );
}

#[test]
fn test_an_error_cell_reads_as_its_code() {
    // GIVEN a workbook whose cells hold the seven error values
    let path = fixture("errors.xlsx");

    // WHEN it is read
    let workbook = read_workbook(&path).unwrap();

    // THEN each cell is its error code
    assert_eq!(
        texts(sheet(&workbook, "Feuil1")),
        [
            ["#DIV/0!"],
            ["#NAME?"],
            ["#VALUE!"],
            ["#NULL!"],
            ["#REF!"],
            ["#NUM!"],
            ["#N/A"]
        ]
    );
}

#[test]
fn test_each_workbook_cell_keeps_its_sheet_row_and_column() {
    // GIVEN a workbook whose sheet `Sheet1` starts at A2, not at A1
    let path = fixture("issues.xlsx");

    // WHEN it is read
    let workbook = read_workbook(&path).unwrap();

    // THEN `c` is at issue2!B3, and the first used cell of Sheet1 at A2
    let issue2 = sheet(&workbook, "issue2");
    assert_eq!(issue2.rows[2][1].text, "c");
    assert_eq!(
        issue2.rows[2][1].location,
        CellLocation {
            sheet: "issue2".to_string(),
            row: 3,
            column: 2
        }
    );
    let sheet1 = sheet(&workbook, "Sheet1");
    assert_eq!(
        sheet1.rows[0][0].location,
        CellLocation {
            sheet: "Sheet1".to_string(),
            row: 2,
            column: 1
        }
    );
    for sheet in &workbook.sheets {
        for (row, cells) in sheet.rows.iter().enumerate() {
            for cell in cells {
                assert_eq!(cell.location.sheet, sheet.name);
                assert_eq!(cell.location.row, cells[0].location.row);
                assert_eq!(
                    cell.location.row,
                    sheet.rows[0][0].location.row + row as u32
                );
            }
        }
    }
}

#[test]
fn test_a_password_protected_workbook_is_a_typed_error() {
    for name in ["pass_protected.xlsx", "pass_protected.ods"] {
        // GIVEN an encrypted workbook
        let path = fixture(name);

        // WHEN it is read
        let error = read_workbook(&path).unwrap_err();

        // THEN the error says so, naming the file
        assert!(
            matches!(error, ReadError::PasswordProtected { .. }),
            "{name}: {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("protected by a password"), "{message}");
        assert!(message.contains(name), "{message}");
    }
}

#[test]
fn test_a_damaged_workbook_is_a_typed_error_never_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let whole = std::fs::read(fixture("issues.xlsx")).unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("garbage.xlsx", b"this is not a workbook".to_vec()),
        ("truncated.xlsx", whole[..whole.len() / 2].to_vec()),
        ("garbage.xlsm", vec![0u8; 512]),
        ("garbage.xlsb", b"PK\x03\x04 broken".to_vec()),
        (
            "garbage.xls",
            vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0],
        ),
        ("garbage.ods", Vec::new()),
        ("zip-as-xls.xls", whole.clone()),
    ];
    for (name, bytes) in cases {
        // GIVEN a file named as a workbook whose content is broken
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();

        // WHEN it is read
        let error = read_workbook(&path).unwrap_err();

        // THEN the error is typed and readable
        assert!(
            matches!(error, ReadError::Corrupt { .. }),
            "{name}: {error:?}"
        );
        assert!(error.to_string().contains(name), "{error}");
    }
}

#[test]
fn test_a_missing_file_and_a_foreign_extension_are_typed_errors() {
    // GIVEN a workbook path that does not exist, and a file that is no workbook
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.xlsx");
    let foreign = dir.path().join("notes.txt");
    std::fs::write(&foreign, "text").unwrap();

    // WHEN each is read
    let missing = read_workbook(&missing).unwrap_err();
    let foreign = read_workbook(&foreign).unwrap_err();

    // THEN one is an I/O error and the other an unsupported format
    assert!(matches!(missing, ReadError::Io { .. }), "{missing:?}");
    assert!(
        matches!(foreign, ReadError::UnsupportedFormat { .. }),
        "{foreign:?}"
    );
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn test_a_semicolon_csv_with_quoted_line_breaks_is_read() {
    // GIVEN a `;` file with a BOM, quoted fields holding a line break, a `;`
    // and a doubled quote, and decimal commas
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "accounts.csv",
        "\u{feff}name;address;balance\r\n\
         Smith;\"12 Lilac Street\r\nSpringfield\";1,5\r\n\
         \"Jones; sons\";\"known as \"\"the elder\"\"\";-3\r\n\
         Brown;Leeds;0\r\n"
            .as_bytes(),
    );

    // WHEN it is read
    let file = read_csv(&path).unwrap();

    // THEN the separator is `;`, the BOM is gone, and each quoted field is one cell
    assert_eq!(file.delimiter, Delimiter::Semicolon);
    assert!(!file.invalid_bytes_replaced);
    assert_eq!(file.sheet.name, "accounts");
    assert_eq!(
        texts(&file.sheet),
        [
            vec!["name", "address", "balance"],
            vec!["Smith", "12 Lilac Street\r\nSpringfield", "1,5"],
            vec!["Jones; sons", "known as \"the elder\"", "-3"],
            vec!["Brown", "Leeds", "0"],
        ]
    );
}

#[test]
fn test_each_csv_cell_keeps_its_row_and_column() {
    // GIVEN a comma file whose second record spans two lines
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "notes.csv", b"a,b\n\"one\ntwo\",c\nd,e\n");

    // WHEN it is read
    let file = read_csv(&path).unwrap();

    // THEN rows count records, not lines: `e` is row 3, column 2
    assert_eq!(file.delimiter, Delimiter::Comma);
    let cell = &file.sheet.rows[2][1];
    assert_eq!(cell.text, "e");
    assert_eq!(
        cell.location,
        CellLocation {
            sheet: "notes".to_string(),
            row: 3,
            column: 2
        }
    );
}

#[test]
fn test_a_tab_separated_file_is_read() {
    // GIVEN a tab-separated file
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "table.tsv", b"x\ty\n1\t2\n");

    // WHEN it is read
    let file = read_csv(&path).unwrap();

    // THEN the tabulation is the separator
    assert_eq!(file.delimiter, Delimiter::Tab);
    assert_eq!(texts(&file.sheet), [["x", "y"], ["1", "2"]]);
}

#[test]
fn test_a_csv_that_is_not_utf8_is_read_and_flagged() {
    // GIVEN a file saved in Windows-1252: `ö` and `è` are the single bytes 0xF6 and 0xE8
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "legacy.csv",
        b"city;country\nMalm\xF6;Sweden\nS\xE8te;France\n",
    );

    // WHEN it is read
    let file = read_csv(&path).unwrap();

    // THEN it is read, each invalid byte replaced, and the result says so
    assert!(file.invalid_bytes_replaced);
    assert_eq!(file.delimiter, Delimiter::Semicolon);
    assert_eq!(
        texts(&file.sheet),
        [
            ["city", "country"],
            ["Malm\u{fffd}", "Sweden"],
            ["S\u{fffd}te", "France"]
        ]
    );
}

#[test]
fn test_a_missing_csv_is_a_typed_error() {
    // GIVEN a CSV path that does not exist
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.csv");

    // WHEN it is read
    let error = read_csv(&path).unwrap_err();

    // THEN the error is an I/O error naming the file
    assert!(matches!(error, ReadError::Io { .. }), "{error:?}");
    assert!(error.to_string().contains("absent.csv"), "{error}");
}
