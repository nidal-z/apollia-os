//! The results of a batch written back to its table: into a copy by default,
//! into the original on request, against a corpus of real workbooks.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use apollia_core::batch_source::{
    dated_backup_path, BatchSource, OriginRef, SourceError, SourceRegistry, SourceSpec,
    WriteSession,
};
use apollia_sources::{
    read_table, register_table_sources, results_copy_path, ResultsDestination, TableFormat,
    TableSource, CSV_KIND, OPTION_DESTINATION, OPTION_LANGUAGE, OPTION_SHEET, SPREADSHEET_KIND,
};
use chrono::{DateTime, Local, TimeZone, Utc};
use rust_xlsxwriter::Workbook;

const RESULT: &str = "Résultat";
/// When set, the corpus test leaves the copies it made in this folder, to be
/// opened by hand in LibreOffice or Excel.
const KEEP_COPIES: &str = "APOLLIA_RESULTS_CORPUS_OUT";

fn taken_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 4, 9, 30, 0).unwrap()
}

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Copies the fixture `name` into `dir`, so that nothing is ever written
/// next to the versioned file.
fn staged(dir: &Path, name: &str) -> PathBuf {
    let source = corpus(name);
    let target = dir.join(source.file_name().unwrap());
    std::fs::copy(&source, &target).unwrap();
    target
}

/// A workbook with a header on row 1 (`Nom`, `Ville`) and three clients.
fn clients_workbook(dir: &Path) -> PathBuf {
    let path = dir.join("clients.xlsx");
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet().set_name("Clients").unwrap();
    for (row, (name, city)) in [
        ("Nom", "Ville"),
        ("Durand", "Lyon"),
        ("Martin", "Nantes"),
        ("Petit", "Lille"),
    ]
    .iter()
    .enumerate()
    {
        let row = u32::try_from(row).unwrap();
        sheet.write_string(row, 0, *name).unwrap();
        sheet.write_string(row, 1, *city).unwrap();
    }
    workbook.save(&path).unwrap();
    path
}

fn open(path: &Path, options: &[(&str, &str)]) -> Result<Box<dyn BatchSource>, SourceError> {
    let mut registry = SourceRegistry::new();
    register_table_sources(&mut registry).unwrap();
    let kind = if TableFormat::of(path) == Some(TableFormat::Csv) {
        CSV_KIND
    } else {
        SPREADSHEET_KIND
    };
    registry.open(
        kind,
        &SourceSpec {
            location: path.display().to_string(),
            options: options
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        },
    )
}

/// Opens a session on `source` and writes `answer <origin>` for every item.
/// Answers the path the session was opened on.
fn write_every_item(source: &mut dyn BatchSource) -> PathBuf {
    let origins: Vec<OriginRef> = source
        .items()
        .unwrap()
        .items
        .map(|item| item.unwrap().origin)
        .collect();
    let mut session = WriteSession::open(source, taken_at()).unwrap();
    for origin in &origins {
        session
            .write(origin, RESULT, &format!("answer {origin}"))
            .unwrap();
    }
    session.backup().path().to_path_buf()
}

fn local_date() -> chrono::NaiveDate {
    taken_at().with_timezone(&Local).date_naive()
}

/// Every entry of an archive: name, then decompressed bytes and raw bytes.
fn entries(path: &Path) -> BTreeMap<String, (Vec<u8>, Vec<u8>)> {
    let bytes = std::fs::read(path).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let mut content = Vec::new();
        let name = {
            let mut entry = archive.by_index(index).unwrap();
            entry.read_to_end(&mut content).unwrap();
            entry.name().to_string()
        };
        let mut raw = Vec::new();
        archive
            .by_index_raw(index)
            .unwrap()
            .read_to_end(&mut raw)
            .unwrap();
        out.insert(name, (content, raw));
    }
    out
}

/// The text of every cell of `sheet`, by `(row, column)`.
fn cells(path: &Path, sheet: &str) -> BTreeMap<(u32, u32), String> {
    read_table(path)
        .unwrap()
        .sheets
        .into_iter()
        .find(|candidate| candidate.name == sheet)
        .unwrap()
        .rows
        .into_iter()
        .flatten()
        .filter(|cell| !cell.text.is_empty())
        .map(|cell| ((cell.location.row, cell.location.column), cell.text))
        .collect()
}

#[test]
fn test_results_go_to_a_dated_copy_and_the_original_keeps_its_fingerprint() {
    // GIVEN a workbook of three clients, its bytes and modification time
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let before = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();

    // WHEN it is opened without a destination and every row gets a result
    let mut source = open(&path, &[]).unwrap();
    let written = write_every_item(source.as_mut());

    // THEN the copy is named after the original and the date, the original
    // is byte for byte what it was and was never touched
    let expected = dir.path().join(format!(
        "clients (results {}).xlsx",
        local_date().format("%Y-%m-%d")
    ));
    assert_eq!(written, expected);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    // AND the copy holds the column after the last one, its header named
    let copy = cells(&written, "Clients");
    assert_eq!(copy.get(&(1, 3)).map(String::as_str), Some(RESULT));
    assert_eq!(
        copy.get(&(2, 3)).map(String::as_str),
        Some("answer Clients!2")
    );
    assert_eq!(
        copy.get(&(4, 3)).map(String::as_str),
        Some("answer Clients!4")
    );
    assert_eq!(copy.get(&(4, 1)).map(String::as_str), Some("Petit"));
}

#[test]
fn test_the_copy_is_named_in_the_language_of_the_interface_and_never_overwritten() {
    // GIVEN a workbook, and a copy of results already made the same day
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let first = results_copy_path(&path, TableFormat::Xlsx, local_date(), "fr", 1);
    std::fs::write(&first, b"earlier run").unwrap();

    // WHEN a batch in French writes its results
    let mut source = open(&path, &[(OPTION_LANGUAGE, "fr")]).unwrap();
    let written = write_every_item(source.as_mut());

    // THEN the earlier copy is kept and the new one takes the next rank
    let date = local_date().format("%Y-%m-%d");
    assert_eq!(
        first.file_name().unwrap().to_string_lossy(),
        format!("clients (résultats {date}).xlsx")
    );
    assert_eq!(std::fs::read(&first).unwrap(), b"earlier run");
    assert_eq!(
        written.file_name().unwrap().to_string_lossy(),
        format!("clients (résultats {date}) (2).xlsx")
    );
}

/// One workbook of the corpus, the sheet written and the parts the edit may
/// change.
struct CorpusCase {
    file: &'static str,
    sheet: &'static str,
    feature: &'static str,
}

const CORPUS: [CorpusCase; 6] = [
    CorpusCase {
        file: "results-corpus/WithConditionalFormatting.xlsx",
        sheet: "CF",
        feature: "conditional formatting and a calculation chain",
    },
    CorpusCase {
        file: "results-corpus/WithChart.xlsx",
        sheet: "Sheet1",
        feature: "a chart drawn from the sheet written",
    },
    CorpusCase {
        file: "results-corpus/ExcelPivotTableSample.xlsx",
        sheet: "Tabelle1",
        feature: "a table and two pivot tables reading the sheet written",
    },
    CorpusCase {
        file: "results-corpus/SimpleWithComments.xlsx",
        sheet: "Sheet1",
        feature: "comments on the sheet written",
    },
    CorpusCase {
        file: "results-corpus/70005-countifs.xlsx",
        sheet: "Sheet1",
        feature: "a workbook saved by LibreOffice",
    },
    CorpusCase {
        file: "spreadsheets/issue221.xlsm",
        sheet: "Sheet1",
        feature: "a workbook with macros enabled",
    },
];

#[test]
fn test_on_the_corpus_every_part_but_the_sheet_written_is_copied_byte_for_byte() {
    for case in &CORPUS {
        // GIVEN a real workbook of the corpus
        let dir = tempfile::tempdir().unwrap();
        let path = staged(dir.path(), case.file);
        let original = entries(&path);
        let before = cells(&path, case.sheet);
        let sheet_part = sheet_part(&path, case.sheet);

        // WHEN every row of the sheet gets a result in the copy
        let mut source = open(&path, &[(OPTION_SHEET, case.sheet)]).unwrap();
        let written = write_every_item(source.as_mut());

        // THEN every part but the sheet, the calculation chain and the two
        // parts that name it is identical, stored bytes included
        let copy = entries(&written);
        if let Some(out) = std::env::var_os(KEEP_COPIES) {
            std::fs::create_dir_all(&out).unwrap();
            std::fs::copy(&written, Path::new(&out).join(written.file_name().unwrap())).unwrap();
        }
        let chain = original.contains_key("xl/calcChain.xml");
        for (name, (content, raw)) in &original {
            let edited = name == &sheet_part
                || name == "xl/calcChain.xml"
                || (chain
                    && (name == "[Content_Types].xml" || name == "xl/_rels/workbook.xml.rels"));
            if edited {
                continue;
            }
            let (copied, copied_raw) = copy
                .get(name)
                .unwrap_or_else(|| panic!("{}: {name} missing ({})", case.file, case.feature));
            assert_eq!(copied, content, "{}: {name} changed", case.file);
            assert_eq!(copied_raw, raw, "{}: {name} stored differently", case.file);
        }
        // AND the calculation chain is gone, from the parts that named it too
        assert!(!copy.contains_key("xl/calcChain.xml"), "{}", case.file);
        let types = String::from_utf8(copy["[Content_Types].xml"].0.clone()).unwrap();
        assert!(!types.contains("calcChain"), "{}", case.file);
        let relations = String::from_utf8(copy["xl/_rels/workbook.xml.rels"].0.clone()).unwrap();
        assert!(!relations.contains("calcChain"), "{}", case.file);
        // AND every cell read before reads the same, the results beside them
        let after = cells(&written, case.sheet);
        for (position, text) in &before {
            assert_eq!(
                after.get(position),
                Some(text),
                "{}: {position:?}",
                case.file
            );
        }
        let added: Vec<&String> = after
            .iter()
            .filter(|(position, _)| !before.contains_key(position))
            .map(|(_, text)| text)
            .collect();
        assert!(
            added.iter().any(|text| text.as_str() == RESULT),
            "{}",
            case.file
        );
        assert!(
            added
                .iter()
                .filter(|text| text.starts_with("answer "))
                .count()
                == source.items().unwrap().items.count(),
            "{}",
            case.file
        );
    }
}

/// The part of the sheet named `sheet`, read the way a person would: the
/// workbook names a relationship, the relationship names the part.
fn sheet_part(path: &Path, sheet: &str) -> String {
    let all = entries(path);
    let workbook = String::from_utf8(all["xl/workbook.xml"].0.clone()).unwrap();
    let relations = String::from_utf8(all["xl/_rels/workbook.xml.rels"].0.clone()).unwrap();
    let tag = workbook
        .split("<sheet ")
        .skip(1)
        .find(|tag| tag.contains(&format!("name=\"{sheet}\"")))
        .unwrap();
    let id = tag
        .split(":id=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let relation = relations
        .split("<Relationship ")
        .skip(1)
        .find(|relation| relation.contains(&format!("Id=\"{id}\"")))
        .unwrap();
    let target = relation
        .split("Target=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    format!("xl/{}", target.trim_start_matches("/xl/"))
}

#[test]
fn test_on_request_the_original_is_written_after_a_dated_backup() {
    // GIVEN a workbook of three clients and its bytes
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let before = std::fs::read(&path).unwrap();

    // WHEN it is opened to write into the original, and every row is written
    let mut source = open(&path, &[(OPTION_DESTINATION, ResultsDestination::ORIGINAL)]).unwrap();
    let backup = write_every_item(source.as_mut());

    // THEN the dated backup holds the original bytes, and the original holds
    // the results beside its cells
    assert_eq!(backup, dated_backup_path(&path, taken_at()).unwrap());
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    let after = cells(&path, "Clients");
    assert_eq!(after.get(&(1, 3)).map(String::as_str), Some(RESULT));
    assert_eq!(
        after.get(&(3, 3)).map(String::as_str),
        Some("answer Clients!3")
    );
    assert_eq!(after.get(&(3, 1)).map(String::as_str), Some("Martin"));
    // AND no copy of results was made
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn test_without_a_successful_backup_nothing_is_written_into_the_original() {
    // GIVEN a workbook whose dated backup name is already taken
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let before = std::fs::read(&path).unwrap();
    let backup = dated_backup_path(&path, taken_at()).unwrap();
    std::fs::write(&backup, b"someone else's file").unwrap();

    // WHEN a session is opened to write into the original
    let mut source = open(&path, &[(OPTION_DESTINATION, ResultsDestination::ORIGINAL)]).unwrap();
    let opened = WriteSession::open(source.as_mut(), taken_at());

    // THEN the session is refused, the original and the other file untouched
    assert!(matches!(opened, Err(SourceError::BackupFailed { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(&backup).unwrap(), b"someone else's file");
}

#[test]
fn test_the_destination_is_fixed_at_opening_and_the_original_of_other_formats_refused() {
    // GIVEN a workbook, a CSV file and the legacy workbooks of the fixtures
    let dir = tempfile::tempdir().unwrap();
    let workbook = clients_workbook(dir.path());
    let csv = dir.path().join("clients.csv");
    std::fs::write(&csv, "Nom;Ville\nDurand;Lyon\n").unwrap();
    let legacy: Vec<PathBuf> = ["issues.xls", "issues.xlsb", "issues.ods"]
        .iter()
        .map(|name| staged(dir.path(), &format!("spreadsheets/{name}")))
        .collect();

    // WHEN the workbook is opened once for a copy and once for the
    // original, and each session is opened
    let mut original = BTreeMap::new();
    original.insert(
        OPTION_DESTINATION.to_string(),
        ResultsDestination::ORIGINAL.to_string(),
    );
    let mut for_copy = TableSource::open(&workbook, &BTreeMap::new()).unwrap();
    let mut in_place = TableSource::open(&workbook, &original).unwrap();
    let copy_session = WriteSession::open(&mut for_copy, taken_at())
        .unwrap()
        .backup()
        .path()
        .to_path_buf();
    let in_place_session = WriteSession::open(&mut in_place, taken_at())
        .unwrap()
        .backup()
        .path()
        .to_path_buf();
    let mut unknown = BTreeMap::new();
    unknown.insert(OPTION_DESTINATION.to_string(), "elsewhere".to_string());

    // THEN each session follows the destination asked at opening, through
    // the unchanged batch source contract
    assert_eq!(
        copy_session,
        results_copy_path(&workbook, TableFormat::Xlsx, local_date(), "", 1)
    );
    assert_eq!(
        in_place_session,
        dated_backup_path(&workbook, taken_at()).unwrap()
    );
    assert!(TableSource::open(&workbook, &unknown).is_err());
    // AND the original of a CSV file or a legacy workbook is never offered
    for path in std::iter::once(&csv).chain(&legacy) {
        let refused = TableSource::open(path, &original).unwrap_err();
        assert!(
            refused.to_string().contains(&path.display().to_string()),
            "{refused}"
        );
    }
}

#[test]
fn test_a_csv_file_gets_a_copy_with_its_column_added() {
    // GIVEN a CSV file separated by semicolons, with a byte order mark
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clients.csv");
    let before = "\u{feff}Nom;Ville\nDurand;Lyon\nMartin;\"Nantes; centre\"\n";
    std::fs::write(&path, before).unwrap();

    // WHEN every row gets a result
    let mut source = open(&path, &[]).unwrap();
    let written = write_every_item(source.as_mut());

    // THEN the original is unchanged and the copy keeps the separator, the
    // mark and the quoted field, with the column added
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert_eq!(written.extension().unwrap(), "csv");
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        "\u{feff}Nom;Ville;Résultat\nDurand;Lyon;answer clients!2\nMartin;\"Nantes; centre\";answer clients!3\n"
    );
}

#[test]
fn test_a_legacy_workbook_gets_a_new_xlsx_copy_with_its_values() {
    for name in ["issues.xls", "issues.xlsb", "issues.ods"] {
        // GIVEN a workbook whose formatting cannot be kept
        let dir = tempfile::tempdir().unwrap();
        let path = staged(dir.path(), &format!("spreadsheets/{name}"));
        let before = std::fs::read(&path).unwrap();
        let table = read_table(&path).unwrap();
        let sheet = table.sheets[0].name.clone();

        // WHEN every row of its first sheet gets a result
        let mut source = open(&path, &[(OPTION_SHEET, sheet.as_str())]).unwrap();
        let written = write_every_item(source.as_mut());

        // THEN the copy is a new .xlsx workbook holding the values and the
        // results, and the original is unchanged
        assert_eq!(written.extension().unwrap(), "xlsx", "{name}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
        let copied = read_table(&written).unwrap();
        assert_eq!(copied.sheets.len(), table.sheets.len(), "{name}");
        let after = cells(&written, &sheet);
        let original: BTreeMap<(u32, u32), String> = table.sheets[0]
            .rows
            .iter()
            .flatten()
            .filter(|cell| !cell.text.is_empty())
            .map(|cell| ((cell.location.row, cell.location.column), cell.text.clone()))
            .collect();
        for (position, text) in &original {
            assert_eq!(after.get(position), Some(text), "{name}: {position:?}");
        }
        assert!(after.values().any(|text| text == RESULT), "{name}");
    }
}

#[test]
fn test_a_write_to_a_row_the_table_does_not_hold_is_refused() {
    // GIVEN a session on a workbook of three clients
    let dir = tempfile::tempdir().unwrap();
    let path = clients_workbook(dir.path());
    let mut source = open(&path, &[]).unwrap();
    let mut session = WriteSession::open(source.as_mut(), taken_at()).unwrap();

    // WHEN a value is written to the header row, and to another sheet
    let header = session.write(&OriginRef::new("Clients!1"), RESULT, "x");
    let elsewhere = session.write(&OriginRef::new("Other!2"), RESULT, "x");

    // THEN both are refused
    assert!(matches!(header, Err(SourceError::UnknownOrigin { .. })));
    assert!(matches!(elsewhere, Err(SourceError::UnknownOrigin { .. })));
}
