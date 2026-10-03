//! The date, long text and file inputs: checked before a run, and shown as
//! their own field.

use serde_json::{json, Value};

use super::fields::{input_fields, InputKind};
use super::inputs::{validate, InputErrorCode};

/// Inputs as the SDK writes them for a workflow with a date, a long text and
/// a file, the file optional.
fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "invoice": {"type": "string", "format": "file-path", "nullable": true},
            "due": {"type": "string", "format": "date"},
            "message": {"type": "string", "format": "long-text"}
        },
        "required": ["due"],
        "additionalProperties": false,
        "x-order": ["invoice", "due", "message"]
    })
}

#[test]
fn a_day_of_the_calendar_and_a_file_that_opens_are_accepted() {
    // GIVEN a file on disk, a leap day and a long text over several lines
    let dir = tempfile::tempdir().unwrap();
    let invoice = dir.path().join("f-12.pdf");
    std::fs::write(&invoice, b"%PDF").unwrap();
    let inputs = json!({
        "invoice": invoice.to_str().unwrap(),
        "due": "2028-02-29",
        "message": "Hello,\nyour invoice is late."
    });

    // WHEN they are validated
    let checked = validate(Some(&schema()), &inputs).unwrap();

    // THEN they pass unchanged
    assert_eq!(checked, inputs);
}

#[test]
fn a_malformed_date_is_refused_with_its_code() {
    for due in [
        "2026-02-30",
        "03/10/2026",
        "2026-1-3",
        "2026-10-03T10:00",
        "",
    ] {
        // GIVEN a date that is not a day written YYYY-MM-DD
        let inputs = json!({ "due": due });

        // WHEN validated
        let err = validate(Some(&schema()), &inputs).unwrap_err();

        // THEN it is refused before the run, the field named, the code typed
        assert_eq!(err.code, InputErrorCode::MalformedDate, "{due}");
        assert_eq!(err.field, "due");
    }
}

#[test]
fn a_file_that_is_absent_or_not_absolute_is_refused_with_its_code() {
    // GIVEN a folder holding no `missing.pdf`, and a subfolder
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("missing.pdf");
    let cases = [
        absent.to_str().unwrap().to_string(),
        dir.path().to_str().unwrap().to_string(),
        "invoices/f-12.pdf".to_string(),
    ];
    for invoice in cases {
        let inputs = json!({ "due": "2026-10-03", "invoice": invoice });

        // WHEN validated
        let err = validate(Some(&schema()), &inputs).unwrap_err();

        // THEN it is refused before the run, the field named, the code typed
        assert_eq!(err.code, InputErrorCode::MissingFile, "{invoice}");
        assert_eq!(err.field, "invoice");
    }
}

#[test]
fn the_older_refusals_carry_their_code_too() {
    // GIVEN inputs missing the date, then naming an undeclared input
    let missing = json!({});
    let unknown = json!({ "due": "2026-10-03", "colour": "red" });

    // WHEN validated
    let missing = validate(Some(&schema()), &missing).unwrap_err();
    let unknown = validate(Some(&schema()), &unknown).unwrap_err();

    // THEN each says why with its own code
    assert_eq!(missing.code, InputErrorCode::Required);
    assert_eq!(unknown.code, InputErrorCode::Unknown);
}

#[test]
fn the_fields_follow_the_declared_order_with_their_kind() {
    // GIVEN the inputs of the SDK's example workflow, as describe writes them
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk/tests/fixtures/invoice_pkg_described.toml");
    let manifest =
        apollia_tools::apollia_manifest::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let schema = manifest.workflows[0].inputs.clone();

    // WHEN its fields are read
    let fields = input_fields(schema.as_ref());

    // THEN each input comes in the declared order with the field it takes
    let kinds: Vec<(&str, InputKind, bool)> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.kind, f.required))
        .collect();
    assert_eq!(
        kinds,
        [
            ("folder", InputKind::Text, true),
            ("notify", InputKind::Boolean, false),
            ("closing_day", InputKind::Date, false),
            ("note", InputKind::LongText, false),
            ("ledger", InputKind::File, false),
        ]
    );
    assert_eq!(
        fields[3].description.as_deref(),
        Some("a note filed with the totals")
    );
}

#[test]
fn a_choice_and_a_list_carry_their_values_and_item_kind() {
    // GIVEN two choices, a list of numbers and a type no field carries
    let schema = json!({
        "type": "object",
        "properties": {
            "channel": {"type": "string", "enum": ["mail", "letter"]},
            "copies": {"type": "integer", "enum": [1, 2]},
            "amounts": {"type": "array", "items": {"type": "number"}},
            "raw": {"type": "object"}
        }
    });

    // WHEN its fields are read
    let fields = input_fields(Some(&schema));

    // THEN a choice lists its values as text, the list its item kind, the
    //      rest is other
    let by = |n: &str| fields.iter().find(|f| f.name == n).unwrap();
    assert_eq!(by("channel").kind, InputKind::Choice);
    assert_eq!(by("channel").choices, ["mail", "letter"]);
    assert_eq!(by("copies").choices, ["1", "2"]);
    assert_eq!(by("amounts").kind, InputKind::List);
    assert_eq!(by("amounts").item_kind, Some(InputKind::Number));
    assert_eq!(by("raw").kind, InputKind::Other);
    assert!(input_fields(None).is_empty());
}

#[test]
fn each_code_is_written_as_the_api_serialises_it() {
    // GIVEN every refusal code
    let codes = [
        InputErrorCode::NotAnObject,
        InputErrorCode::Required,
        InputErrorCode::Unknown,
        InputErrorCode::Null,
        InputErrorCode::WrongType,
        InputErrorCode::NotAChoice,
        InputErrorCode::MalformedDate,
        InputErrorCode::MissingFile,
    ];
    for code in codes {
        // WHEN it is written as text and serialised
        let serialised = serde_json::to_value(code).unwrap();

        // THEN both agree, so a message and a body name it the same way
        assert_eq!(serialised, json!(code.as_str()));
    }
}
