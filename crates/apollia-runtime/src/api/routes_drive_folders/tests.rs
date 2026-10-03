use std::sync::Arc;

use apollia_auth::ConnectorProvider;
use axum::http::StatusCode;
use tempfile::TempDir;

use super::*;
use crate::api::test_support::{app_state, call, StemLoader};
use crate::connectors_bridge::test_support::auth_with_tokens;

#[tokio::test]
async fn test_the_folders_of_the_google_accounts_are_listed_or_refused_with_a_500() {
    // GIVEN two connected Google accounts, one with a folder recorded
    let dir = TempDir::new().unwrap();
    let accounts = [("ada@example.test", "at-1"), ("bob@example.test", "at-2")];
    let manager = auth_with_tokens(dir.path(), ConnectorProvider::Google, &accounts).await;
    let prefs = dir.path().join("drive-prefs.toml");
    drive_folders::set(&prefs, "ada@example.test", "Documents/Apollia").unwrap();

    // WHEN the folders are listed, then listed again over an unreadable index
    let mut folders = drive_folders::list(&manager, &prefs).await.unwrap();
    std::fs::write(dir.path().join("connectors-index.json"), "not json").unwrap();
    let broken = drive_folders::list(&manager, &prefs).await.unwrap_err();

    // THEN the recorded folder is answered and the other account falls back
    // to the default the executors walk; the unreadable index is a 500
    folders.sort_by(|a, b| a.account_id.cmp(&b.account_id));
    assert_eq!(folders[0].folder_path.as_deref(), Some("Documents/Apollia"));
    assert_eq!(folders[0].effective_folder_path, "Documents/Apollia");
    assert_eq!(folders[1].folder_path, None);
    assert_eq!(folders[1].effective_folder_path, "Apollia");
    assert_eq!(refusal(broken).0, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_a_folder_is_recorded_trimmed_then_reset_to_the_default() {
    // GIVEN an empty preferences file
    let dir = TempDir::new().unwrap();
    let prefs = dir.path().join("drive-prefs.toml");

    // WHEN a folder is recorded with surrounding blanks, then reset
    let set = drive_folders::set(&prefs, " ada@example.test ", " Clients/2026 ").unwrap();
    let reset = drive_folders::reset(&prefs, "ada@example.test").unwrap();

    // THEN the folder was recorded trimmed, and the reset restores the default
    assert_eq!(set.account_id, "ada@example.test");
    assert_eq!(set.folder_path.as_deref(), Some("Clients/2026"));
    assert_eq!(reset.folder_path, None);
    assert_eq!(reset.effective_folder_path, "Apollia");
}

#[tokio::test]
async fn test_the_routes_are_mounted_and_refuse_oversized_input_before_any_write() {
    // GIVEN the daemon's router
    let dir = TempDir::new().unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    let long_account = format!("/api/v1/connectors/drive-folders/{}", "a".repeat(400));

    // WHEN a client records a folder longer than any Drive path, then resets
    // the folder of an account no email can name
    let (put, put_body) = call(
        state.clone(),
        "PUT",
        "/api/v1/connectors/drive-folders/ada%40example.test",
        Some(serde_json::json!({ "folder_path": "a".repeat(2000) })),
    )
    .await;
    let (delete, delete_body) = call(state, "DELETE", &long_account, None).await;

    // THEN both answer 400 with the reason
    assert_eq!(put, StatusCode::BAD_REQUEST, "{put_body}");
    assert_eq!(put_body["error"], "the folder path is too long");
    assert_eq!(delete, StatusCode::BAD_REQUEST, "{delete_body}");
    assert_eq!(
        delete_body["error"],
        "the account identifier is empty or too long"
    );
}
