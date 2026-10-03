//! The routes the attached desktop reads for the Model Hub: the installed
//! models and the recommendation outcome, through the whole router.

use std::sync::Arc;

use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::test_support::{app_state, call, StemLoader};
use crate::recommendation::RecommendOutcome;

fn state_in(dir: &TempDir) -> crate::api::server::AppState<crate::api::test_support::MockBackend> {
    app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    )
}

#[tokio::test]
async fn test_the_installed_models_are_listed_from_the_models_folder_of_the_daemon() {
    // GIVEN a daemon whose models folder holds one GGUF in a sub-folder
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("models/vendor")).unwrap();
    std::fs::write(dir.path().join("models/vendor/deep.gguf"), b"GGUF").unwrap();

    // WHEN a client lists the installed models
    let (status, body) = call(state_in(&dir), "GET", "/api/v1/llm/installed-models", None).await;

    // THEN the model comes back as the Hub draws it, not in use
    assert_eq!(status, StatusCode::OK, "{body}");
    let models = body.as_array().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["name"], "deep.gguf");
    assert_eq!(models[0]["kind"], "llm");
    assert_eq!(models[0]["in_use"], false);
}

#[tokio::test]
async fn test_a_models_folder_that_cannot_be_read_is_refused_with_its_reason() {
    // GIVEN a daemon whose models folder is a plain file
    let dir = TempDir::new().unwrap();
    std::fs::write(
        apollia_core::config::default_models_dir(dir.path()),
        b"not a folder",
    )
    .unwrap();

    // WHEN a client lists the installed models
    let (status, body) = call(state_in(&dir), "GET", "/api/v1/llm/installed-models", None).await;

    // THEN the daemon refuses with 500 and says the index could not be read
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let error = body["error"].as_str().unwrap();
    assert!(
        error.starts_with("failed to read the model index"),
        "{error}"
    );
}

#[cfg(feature = "cloud")]
#[tokio::test]
async fn test_the_recommendation_outcome_is_served_with_its_status() {
    // GIVEN a ranked answer the daemon already holds for a context window,
    //       so no Hub is asked
    let dir = TempDir::new().unwrap();
    let profile = apollia_llm::hardware::detect();
    let manifest = apollia_llm::recommend::FamilyManifest::embedded().unwrap();
    let outcome = RecommendOutcome::Ok {
        models: Vec::new(),
        hardware: crate::api::routes_model_hub::HardwareResponse::from(profile),
        selection: manifest.editorial_selection(),
    };
    crate::recommendation::remember(4242, &outcome);

    // WHEN a client asks for the recommendation for that window
    let (status, body) = call(
        state_in(&dir),
        "GET",
        "/api/v1/llm/recommendations?n_ctx=4242",
        None,
    )
    .await;

    // THEN the outcome comes back with its status, its hardware and the
    //      editorial selection, as the onboarding step reads it
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");
    assert!(body["hardware"]["total_capacity_gb"].is_number());
    assert_eq!(
        body["selection"].as_array().unwrap().len(),
        manifest.families().len()
    );
}

#[cfg(feature = "cloud")]
#[tokio::test]
async fn test_a_malformed_recommendation_query_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client asks with a context window that is not a number
    let (status, _) = call(
        state_in(&dir),
        "GET",
        "/api/v1/llm/recommendations?n_ctx=large",
        None,
    )
    .await;

    // THEN the daemon refuses the request before asking anything
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
