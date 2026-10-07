use axum::http::StatusCode;
use serde_json::json;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state(dir: &tempfile::TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

/// A workbook whose first sheet is an empty cover and whose second holds a
/// title on row 1, a header on row 3 and two clients.
fn workbook(dir: &tempfile::TempDir) -> String {
    let path = dir.path().join("clients.xlsx");
    let mut workbook = rust_xlsxwriter::Workbook::new();
    workbook.add_worksheet().set_name("Garde").unwrap();
    let sheet = workbook.add_worksheet().set_name("Clients").unwrap();
    sheet.write_string(0, 0, "Clients 2024").unwrap();
    sheet.write_string(2, 0, "Nom").unwrap();
    sheet.write_string(2, 1, "Ville").unwrap();
    sheet.write_string(3, 0, "Durand").unwrap();
    sheet.write_string(3, 1, "Lyon").unwrap();
    sheet.write_string(4, 0, "Martin").unwrap();
    sheet.write_string(4, 1, "Nantes").unwrap();
    workbook.save(&path).unwrap();
    path.display().to_string()
}

#[tokio::test]
async fn test_the_preview_proposes_the_table_and_follows_the_choice_over_http() {
    // GIVEN a runtime and a workbook with a cover sheet and a title
    let dir = tempfile::tempdir().unwrap();
    let path = workbook(&dir);

    // WHEN it is previewed as proposed, then on header row 4
    let (status, proposed) = call(
        state(&dir),
        "POST",
        "/api/v1/sources/preview",
        Some(json!({ "path": path })),
    )
    .await;
    let (moved_status, moved) = call(
        state(&dir),
        "POST",
        "/api/v1/sources/preview",
        Some(json!({ "path": path, "sheet": "Clients", "headerRow": 4 })),
    )
    .await;

    // THEN the data sheet and row 3 are proposed in camelCase, with the rows
    // from the top and the flags of an xlsx; the second answer follows row 4
    assert_eq!(status, StatusCode::OK, "{proposed}");
    assert_eq!(proposed["format"], "xlsx");
    assert_eq!(proposed["sourceType"], "spreadsheet");
    assert_eq!(
        proposed["sheets"],
        json!([{ "name": "Garde", "nonEmptyRows": 0 }, { "name": "Clients", "nonEmptyRows": 4 }])
    );
    assert_eq!(proposed["proposedSheet"], "Clients");
    assert_eq!(proposed["proposedHeaderRow"], 3);
    assert_eq!(proposed["headerRow"], 3);
    assert_eq!(
        proposed["columns"],
        json!([{ "letter": "A", "name": "Nom" }, { "letter": "B", "name": "Ville" }])
    );
    assert_eq!(
        proposed["rows"][0],
        json!({ "row": 1, "cells": ["Clients 2024", ""] })
    );
    assert_eq!(proposed["estimatedItems"], 2);
    assert_eq!(proposed["canModifyOriginal"], true);
    assert_eq!(proposed["formattingNotKept"], false);
    assert_eq!(moved_status, StatusCode::OK, "{moved}");
    assert_eq!(moved["headerRow"], 4);
    assert_eq!(moved["proposedHeaderRow"], 3);
    assert_eq!(moved["estimatedItems"], 1);
}

#[tokio::test]
async fn test_the_preview_refuses_by_status_what_it_cannot_read() {
    // GIVEN a runtime, a text file and a workbook
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, "hello").unwrap();
    let path = workbook(&dir);
    let missing = dir.path().join("absent.xlsx").display().to_string();
    let cases = [
        (json!({ "path": "relative.xlsx" }), StatusCode::BAD_REQUEST),
        (json!({ "path": missing }), StatusCode::NOT_FOUND),
        (
            json!({ "path": text.display().to_string() }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "path": path, "sheet": "Absente" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "path": path, "headerRow": 99 }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];

    for (body, expected) in cases {
        // WHEN each is previewed
        let (status, answer) = call(
            state(&dir),
            "POST",
            "/api/v1/sources/preview",
            Some(body.clone()),
        )
        .await;

        // THEN each is refused with its status and a message
        assert_eq!(status, expected, "{body}: {answer}");
        assert!(answer["error"].is_string(), "{body}: {answer}");
    }
}
