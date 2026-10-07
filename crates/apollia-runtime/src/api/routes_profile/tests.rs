use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::UserMemoryRepository;
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state_with_profile(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = app_state(dir.path().to_path_buf());
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    state
}

#[tokio::test]
async fn test_an_entry_written_over_http_is_read_back_in_the_profile() {
    // GIVEN a daemon whose profile store is open and empty
    let dir = TempDir::new().unwrap();
    let state = state_with_profile(&dir);

    // WHEN a client writes the name, then reads the profile
    let (written, entry) = call(
        state.clone(),
        "PUT",
        "/api/v1/profile/entries/name",
        Some(serde_json::json!({ "value": "Nadia" })),
    )
    .await;
    let (read, profile) = call(state, "GET", "/api/v1/profile", None).await;

    // THEN the entry comes back as stored, written by the user, and the
    // profile lists it among the schema fields
    assert_eq!(written, StatusCode::OK, "{entry}");
    assert_eq!(entry["value"], "Nadia");
    assert_eq!(entry["written_by"], "user");
    assert_eq!(read, StatusCode::OK, "{profile}");
    let names: Vec<&str> = profile["schema_entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["key"].as_str())
        .collect();
    assert_eq!(names, ["name"]);
    assert_eq!(profile["last_updated_at"], entry["updated_at"]);
}

#[tokio::test]
async fn test_a_deleted_entry_is_gone_and_a_second_delete_is_not_found() {
    // GIVEN a profile holding one free-form entry
    let dir = TempDir::new().unwrap();
    let state = state_with_profile(&dir);
    let body = Some(serde_json::json!({ "value": "matin" }));
    call(
        state.clone(),
        "PUT",
        "/api/v1/profile/entries/habitude",
        body,
    )
    .await;

    // WHEN it is deleted twice
    let (first, deleted) = call(
        state.clone(),
        "DELETE",
        "/api/v1/profile/entries/habitude",
        None,
    )
    .await;
    let (second, refusal) = call(state, "DELETE", "/api/v1/profile/entries/habitude", None).await;

    // THEN the first removes it, the second answers 404 with the words the
    // desktop showed when it ran the runtime itself
    assert_eq!(first, StatusCode::OK);
    assert_eq!(deleted["deleted"], "habitude");
    assert_eq!(second, StatusCode::NOT_FOUND);
    assert_eq!(
        refusal["error"],
        "NOT_FOUND: Profile entry not found: habitude"
    );
}

#[tokio::test]
async fn test_a_reset_removes_every_entry_and_counts_them() {
    // GIVEN a profile holding two entries
    let dir = TempDir::new().unwrap();
    let state = state_with_profile(&dir);
    for key in ["name", "role"] {
        let body = Some(serde_json::json!({ "value": "x" }));
        call(
            state.clone(),
            "PUT",
            &format!("/api/v1/profile/entries/{key}"),
            body,
        )
        .await;
    }

    // WHEN the profile is reset
    let (status, reset) = call(state.clone(), "POST", "/api/v1/profile/reset", None).await;
    let (_, profile) = call(state, "GET", "/api/v1/profile", None).await;

    // THEN both are counted and none is left
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset["removed"], 2);
    assert_eq!(profile["entries"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn test_a_daemon_without_a_profile_store_says_so() {
    // GIVEN a daemon whose profile store did not open
    let dir = TempDir::new().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN a client reads the profile
    let (status, refusal) = call(state, "GET", "/api/v1/profile", None).await;

    // THEN it is told the store is unavailable, not handed an empty profile
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(refusal["error"]
        .as_str()
        .unwrap()
        .starts_with("NOT_INITIALIZED"));
}
