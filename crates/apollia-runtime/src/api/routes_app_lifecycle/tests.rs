use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn daemon_at(data_dir: PathBuf) -> AppState<MockBackend> {
    app_state(data_dir)
}

/// A data directory that is a file: every operation on it fails.
fn data_dir_that_is_a_file(dir: &TempDir) -> PathBuf {
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, "x").unwrap();
    file
}

#[tokio::test]
async fn test_the_onboarding_marked_complete_over_http_writes_the_daemons_flag() {
    // GIVEN a daemon whose data directory holds no flag
    let dir = TempDir::new().unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client marks the onboarding complete
    let (status, body) = call(state, "POST", "/api/v1/onboarding/complete", None).await;

    // THEN the flag is in the daemon's data directory
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["onboarded"], true);
    assert!(dir.path().join(".onboarded").is_file());
}

#[tokio::test]
async fn test_the_onboarding_flag_that_cannot_be_written_is_refused_with_the_reason() {
    // GIVEN a daemon whose data directory is a file
    let dir = TempDir::new().unwrap();
    let state = daemon_at(data_dir_that_is_a_file(&dir));

    // WHEN a client marks the onboarding complete
    let (status, body) = call(state, "POST", "/api/v1/onboarding/complete", None).await;

    // THEN the daemon refuses and says what it could not do
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("failed to create directory"),
        "{body}"
    );
}

#[tokio::test]
async fn test_the_onboarding_reset_over_http_clears_flag_profile_and_its_memory() {
    // GIVEN a daemon with a completed onboarding, a profile of one entry, an
    // onboarding marker and the onboarding agent's memory
    let dir = TempDir::new().unwrap();
    let mut state = daemon_at(dir.path().to_path_buf());
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    repo.set("name", "Nadia", WrittenBy::User).unwrap();
    repo.set_internal("onboarding_phase", "done").unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    std::fs::write(dir.path().join(".onboarded"), "").unwrap();
    std::fs::create_dir_all(dir.path().join("memory")).unwrap();
    std::fs::write(dir.path().join("memory").join("onboarding.db"), "x").unwrap();

    // WHEN a client resets the onboarding
    let (status, body) = call(state.clone(), "POST", "/api/v1/onboarding/reset", None).await;

    // THEN the profile entry is counted, and flag, marker and memory are gone
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed_profile_entries"], 1);
    assert!(!dir.path().join(".onboarded").exists());
    assert!(!dir.path().join("memory").join("onboarding.db").exists());
    let repo = state.user_memory.as_ref().unwrap().lock().unwrap();
    assert_eq!(repo.get_internal("onboarding_phase").unwrap(), None);
}

#[tokio::test]
async fn test_the_onboarding_reset_without_a_profile_store_is_refused_as_unavailable() {
    // GIVEN a daemon whose profile store did not open
    let dir = TempDir::new().unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client resets the onboarding
    let (status, body) = call(state, "POST", "/api/v1/onboarding/reset", None).await;

    // THEN the daemon refuses and names the store
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "user memory repository not initialized");
}

#[tokio::test]
async fn test_the_memory_wiped_over_http_counts_the_daemons_databases() {
    // GIVEN a daemon whose memory directory holds two databases
    let dir = TempDir::new().unwrap();
    let memory = dir.path().join("memory");
    std::fs::create_dir_all(&memory).unwrap();
    std::fs::write(memory.join("agent-a.db"), "x").unwrap();
    std::fs::write(memory.join("agent-b.db"), "x").unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client wipes every memory
    let (status, body) = call(state, "POST", "/api/v1/memory/wipe", None).await;

    // THEN both are counted and gone
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed_databases"], 2);
    assert!(!memory.join("agent-a.db").exists());
}

#[tokio::test]
async fn test_a_memory_directory_that_cannot_be_listed_is_refused_with_the_reason() {
    // GIVEN a daemon whose memory directory is a file
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("memory"), "x").unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client wipes every memory
    let (status, body) = call(state, "POST", "/api/v1/memory/wipe", None).await;

    // THEN the daemon refuses and says what it could not do
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("failed to list memory dir"),
        "{body}"
    );
}

#[tokio::test]
async fn test_the_logs_cleared_over_http_are_the_daemons() {
    // GIVEN a daemon whose logs directory holds the service log
    let dir = TempDir::new().unwrap();
    let logs = dir.path().join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("service.log"), "line").unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client clears the logs, then clears them again
    let (status, body) = call(state.clone(), "POST", "/api/v1/logs/clear", None).await;
    let (_, again) = call(state, "POST", "/api/v1/logs/clear", None).await;

    // THEN the directory is gone, and a second clear has nothing to remove
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], true);
    assert_eq!(again["removed"], false);
    assert!(!logs.exists());
}

#[tokio::test]
async fn test_a_logs_path_that_cannot_be_removed_is_refused_with_the_reason() {
    // GIVEN a daemon whose logs path is a file
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("logs"), "x").unwrap();
    let state = daemon_at(dir.path().to_path_buf());

    // WHEN a client clears the logs
    let (status, body) = call(state, "POST", "/api/v1/logs/clear", None).await;

    // THEN the daemon refuses and says what it could not do
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("failed to remove logs directory"),
        "{body}"
    );
}

#[tokio::test]
async fn test_the_factory_reset_over_http_removes_the_daemons_data_directory() {
    // GIVEN a daemon whose data directory holds a configuration
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join(".apollia");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("config.toml"), "").unwrap();
    let state = daemon_at(data_dir.clone());

    // WHEN a client asks for a factory reset
    let (status, body) = call(state, "POST", "/api/v1/factory-reset", None).await;

    // THEN the data directory is gone, and nothing around it; on Windows,
    // where the daemon holds files inside it, it is marked for the next start
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], true);
    if cfg!(windows) {
        assert!(data_dir.join(".factory-reset-pending").is_file());
    } else {
        assert!(!data_dir.exists());
    }
    assert!(dir.path().exists());
}

#[tokio::test]
async fn test_a_data_directory_that_cannot_be_removed_is_refused_with_the_reason() {
    // GIVEN a daemon whose data directory is a file
    let dir = TempDir::new().unwrap();
    let state = daemon_at(data_dir_that_is_a_file(&dir));

    // WHEN a client asks for a factory reset
    let (status, body) = call(state, "POST", "/api/v1/factory-reset", None).await;

    // THEN the daemon refuses and says what it could not do
    let expected = if cfg!(windows) {
        "failed to record the factory reset"
    } else {
        "failed to remove apollia home"
    };
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"].as_str().unwrap().starts_with(expected),
        "{body}"
    );
}
