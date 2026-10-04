use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::UserMemoryRepository;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};

fn state_with_profiles(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    state
}

#[tokio::test]
async fn test_a_profile_created_then_made_default_is_listed_first_and_marked() {
    // GIVEN a daemon with its default profile only
    let dir = TempDir::new().unwrap();
    let state = state_with_profiles(&dir);

    // WHEN a client creates "Formateur" and makes it the default
    let (created, profile) = call(
        state.clone(),
        "POST",
        "/api/v1/profiles",
        Some(json!({ "name": "Formateur" })),
    )
    .await;
    let id = profile["id"].as_str().unwrap().to_owned();
    let (defaulted, _) = call(
        state.clone(),
        "POST",
        &format!("/api/v1/profiles/{id}/default"),
        None,
    )
    .await;
    let (listed, list) = call(state, "GET", "/api/v1/profiles", None).await;

    // THEN the list holds two profiles, "Formateur" first and alone marked
    assert_eq!(created, StatusCode::OK, "{profile}");
    assert_eq!(defaulted, StatusCode::OK);
    assert_eq!(listed, StatusCode::OK, "{list}");
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["name"], "Formateur");
    assert_eq!(list[0]["is_default"], true);
    assert_eq!(list[1]["is_default"], false);
}

#[tokio::test]
async fn test_the_default_profile_cannot_be_deleted_and_says_why() {
    // GIVEN a daemon with its default profile only
    let dir = TempDir::new().unwrap();
    let state = state_with_profiles(&dir);

    // WHEN a client deletes it
    let (status, body) = call(state, "DELETE", "/api/v1/profiles/default", None).await;

    // THEN the refusal is a conflict that names what to do
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("make another profile the default first"));
}

#[tokio::test]
async fn test_fields_and_facts_are_written_to_the_profile_named() {
    // GIVEN a second profile
    let dir = TempDir::new().unwrap();
    let state = state_with_profiles(&dir);
    let (_, profile) = call(
        state.clone(),
        "POST",
        "/api/v1/profiles",
        Some(json!({ "name": "Formateur" })),
    )
    .await;
    let id = profile["id"].as_str().unwrap().to_owned();

    // WHEN its role is written and a fact retained in it
    let (patched, detail) = call(
        state.clone(),
        "PATCH",
        &format!("/api/v1/profiles/{id}"),
        Some(json!({ "fields": { "role": "Formateur SI" } })),
    )
    .await;
    let (put, fact) = call(
        state.clone(),
        "PUT",
        &format!("/api/v1/profiles/{id}/facts/audience"),
        Some(json!({ "value": "adultes" })),
    )
    .await;
    let (_, read) = call(
        state.clone(),
        "GET",
        &format!("/api/v1/profiles/{id}"),
        None,
    )
    .await;
    let (_, default) = call(state, "GET", "/api/v1/profiles/default", None).await;

    // THEN the profile reads its role and its fact, and the default neither
    assert_eq!(patched, StatusCode::OK, "{detail}");
    assert_eq!(put, StatusCode::OK, "{fact}");
    assert_eq!(fact["written_by"], "user");
    let role = read["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "role")
        .unwrap();
    assert_eq!(role["value"], "Formateur SI");
    assert_eq!(read["facts"][0]["key"], "audience");
    assert_eq!(default["facts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_a_reset_empties_the_profile_named_and_a_fact_is_forgotten_once() {
    // GIVEN the default profile with a name and a fact
    let dir = TempDir::new().unwrap();
    let state = state_with_profiles(&dir);
    call(
        state.clone(),
        "PATCH",
        "/api/v1/profiles/default",
        Some(json!({ "fields": { "name": "Claire" } })),
    )
    .await;
    call(
        state.clone(),
        "PUT",
        "/api/v1/profiles/default/facts/timezone",
        Some(json!({ "value": "Paris" })),
    )
    .await;

    // WHEN the fact is forgotten twice, then the profile reset
    let (first, _) = call(
        state.clone(),
        "DELETE",
        "/api/v1/profiles/default/facts/timezone",
        None,
    )
    .await;
    let (second, _) = call(
        state.clone(),
        "DELETE",
        "/api/v1/profiles/default/facts/timezone",
        None,
    )
    .await;
    let (reset, removed) = call(state, "POST", "/api/v1/profiles/default/reset", None).await;

    // THEN the second deletion finds nothing, and the reset removed the name
    assert_eq!(first, StatusCode::OK);
    assert_eq!(second, StatusCode::NOT_FOUND);
    assert_eq!(reset, StatusCode::OK);
    assert_eq!(removed["removed"], 1);
}
