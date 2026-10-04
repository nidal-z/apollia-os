use std::collections::BTreeMap;
use std::path::Path;

use apollia_core::batch_source::BatchSource;

use super::detect::is_text;
use super::*;
use crate::reader::{Cell, CellLocation};

/// A sheet from rows of texts, row 1 and column 1 first.
fn sheet(name: &str, rows: &[&[&str]]) -> Sheet {
    Sheet {
        name: name.to_string(),
        rows: rows
            .iter()
            .zip(1u32..)
            .map(|(cells, row)| {
                cells
                    .iter()
                    .zip(1u32..)
                    .map(|(text, column)| Cell {
                        location: CellLocation {
                            sheet: name.to_string(),
                            row,
                            column,
                        },
                        text: (*text).to_string(),
                    })
                    .collect()
            })
            .collect(),
    }
}

/// The proposal on `sheets`: the sheet's name and the header row number.
fn proposal(sheets: &[Sheet]) -> (Option<String>, Option<u32>) {
    let Some(index) = propose_sheet(sheets) else {
        return (None, None);
    };
    let sheet = &sheets[index];
    let header = propose_header(sheet).map(|row| layout::row_number(sheet, row));
    (Some(sheet.name.clone()), header)
}

/// The sheet and the header row number a layout should be proposed.
type Expected = (Option<&'static str>, Option<u32>);

#[test]
fn test_the_sheet_and_header_proposed_on_the_usual_layouts() {
    // GIVEN four layouts: a title above the header, a header on row 1, an
    // empty cover sheet before the data sheet, and a CSV file with a date
    // and an amount on every row
    let cases: Vec<(&str, Vec<Sheet>, Expected)> = vec![
        (
            "title above the header",
            vec![sheet(
                "Ventes",
                &[
                    &["Quarterly sales"],
                    &[""],
                    &["Client", "Ville", "Montant"],
                    &["Durand", "Lyon", "1200"],
                    &["Martin", "Nantes", "830.5"],
                ],
            )],
            (Some("Ventes"), Some(3)),
        ),
        (
            "header on row 1",
            vec![sheet(
                "Feuil1",
                &[
                    &["Name", "Email"],
                    &["Ada", "ada@example.org"],
                    &["Alan", "alan@example.org"],
                ],
            )],
            (Some("Feuil1"), Some(1)),
        ),
        (
            "empty cover sheet",
            vec![
                sheet("Garde", &[&[""], &["", ""]]),
                sheet("Notes", &[&["Read before use"]]),
                sheet("Data", &[&["Ref", "Qty"], &["A-1", "3"], &["A-2", "5"]]),
            ],
            (Some("Data"), Some(1)),
        ),
        (
            "csv",
            vec![sheet(
                "export",
                &[
                    &["date", "amount", "label"],
                    &["2024-03-12", "12,50", "coffee"],
                    &["13/03/2024", "8", "bread"],
                ],
            )],
            (Some("export"), Some(1)),
        ),
    ];

    for (name, sheets, expected) in cases {
        // WHEN the sheet and the header row are proposed
        let (sheet, header) = proposal(&sheets);

        // THEN each layout gets the sheet and the row a person would pick
        assert_eq!((sheet.as_deref(), header), expected, "case: {name}");
    }
}

#[test]
fn test_without_a_text_row_followed_by_its_width_the_first_non_empty_row_is_proposed() {
    // GIVEN a sheet of numbers only, under two empty rows
    let numbers = sheet("Mesures", &[&[""], &[""], &["1", "2"], &["3", "4"]]);
    // GIVEN a sheet whose only text row is followed by a narrower row
    let narrow = sheet("Libre", &[&["a", "b", "c"], &["x"]]);

    // WHEN the header row of each is proposed
    let first = propose_header(&numbers);
    let second = propose_header(&narrow);

    // THEN the first non-empty row is proposed
    assert_eq!(first, Some(2));
    assert_eq!(second, Some(0));
}

#[test]
fn test_numbers_dates_booleans_and_errors_are_not_text() {
    // GIVEN cell texts of every kind a reader hands over
    let values = [
        "Client",
        "N° client",
        "12",
        "-3.5",
        "12,50",
        "1 200",
        "15 %",
        "2024-03-12",
        "12/03/2024",
        "10:30:00",
        "TRUE",
        "#N/A",
        "A-1",
    ];

    // WHEN each is judged
    let judged: Vec<bool> = values.iter().map(|value| is_text(value)).collect();

    // THEN words are text, the rest is not
    assert_eq!(
        judged,
        [true, true, false, false, false, false, false, false, false, false, false, false, true]
    );
}

#[test]
fn test_column_letters_follow_the_spreadsheet() {
    // GIVEN column numbers around the first rollovers
    let numbers = [1, 26, 27, 52, 703];

    // WHEN each is written as letters
    let letters: Vec<String> = numbers.iter().map(|n| column_letter(*n)).collect();

    // THEN they read as a spreadsheet shows them
    assert_eq!(letters, ["A", "Z", "AA", "AZ", "AAA"]);
}

#[test]
fn test_items_are_the_non_empty_rows_below_the_header_with_the_checked_columns() {
    // GIVEN a table with a title, a blank header cell, a repeated header and
    // a blank row among its data
    let table = Table {
        format: TableFormat::Xlsx,
        sheets: vec![sheet(
            "Clients",
            &[
                &["Customer list"],
                &["Nom", "", "Nom", "Ville"],
                &["Durand", "x", "Paul", "Lyon"],
                &["", "", "", ""],
                &["Martin", "", "Anne", "Nantes"],
            ],
        )],
    };
    let mut options = BTreeMap::new();
    options.insert(OPTION_COLUMNS.to_string(), "a, C,d".to_string());

    // WHEN it is opened as a batch source with three columns checked
    let source = TableSource::from_table(Path::new("/c.xlsx"), &table, &options).unwrap();
    let items = source.items().unwrap();
    let estimated = items.estimated_count;
    let items: Vec<_> = items.items.map(Result::unwrap).collect();

    // THEN the fields are the checked columns, a repeated name made unique,
    // and the blank row is not an item
    assert_eq!(source.fields(), ["Nom", "Nom (2)", "Ville"]);
    assert_eq!(estimated, 2);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].origin.as_str(), "Clients!3");
    assert_eq!(items[1].origin.as_str(), "Clients!5");
    assert_eq!(items[1].fields["Nom"], "Martin");
    assert_eq!(items[1].fields["Nom (2)"], "Anne");
    assert_eq!(items[1].fields["Ville"], "Nantes");
}

#[test]
fn test_every_column_is_a_field_when_none_is_checked_and_a_blank_header_takes_its_letter() {
    // GIVEN a table whose second header cell is blank
    let table = Table {
        format: TableFormat::Csv,
        sheets: vec![sheet("t", &[&["Nom", ""], &["Ada", "1815"]])],
    };

    // WHEN it is opened without options
    let source = TableSource::from_table(Path::new("/t.csv"), &table, &BTreeMap::new()).unwrap();

    // THEN both columns are fields, the blank one named after its letter
    assert_eq!(source.fields(), ["Nom", "B"]);
}

#[test]
fn test_an_option_naming_what_the_file_lacks_is_refused() {
    // GIVEN a one-sheet table
    let table = Table {
        format: TableFormat::Xlsx,
        sheets: vec![sheet("S", &[&["a", "b"], &["1", "2"]])],
    };
    let refused = [
        (OPTION_SHEET, "Absente"),
        (OPTION_HEADER_ROW, "9"),
        (OPTION_HEADER_ROW, "deux"),
        (OPTION_COLUMNS, "A,Z"),
    ];

    for (key, value) in refused {
        // WHEN it is opened with an option the file cannot satisfy
        let mut options = BTreeMap::new();
        options.insert(key.to_string(), value.to_string());
        let result = TableSource::from_table(Path::new("/s.xlsx"), &table, &options);

        // THEN the source is not opened
        assert!(result.is_err(), "{key}={value} was accepted");
    }
}

#[test]
fn test_the_flags_follow_the_format() {
    // GIVEN every table format
    let formats = [
        TableFormat::Xlsx,
        TableFormat::Xlsm,
        TableFormat::Xlsb,
        TableFormat::Xls,
        TableFormat::Ods,
        TableFormat::Csv,
    ];

    // WHEN the two flags are read off each
    let flags: Vec<(bool, bool)> = formats
        .iter()
        .map(|format| (format.can_modify_original(), format.formatting_not_kept()))
        .collect();

    // THEN only xlsx and xlsm may be modified, and xls, xlsb and ods lose
    // their formatting in the copy
    assert_eq!(
        flags,
        [
            (true, false),
            (true, false),
            (false, true),
            (false, true),
            (false, true),
            (false, false)
        ]
    );
}
