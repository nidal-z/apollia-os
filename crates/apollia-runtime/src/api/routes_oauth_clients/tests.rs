use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};

fn state_in(dir: &TempDir) -> AppState<MockBackend> {
    app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    )
}

async fn google_row(state: AppState<MockBackend>) -> serde_json::Value {
    let (status, rows) = call(state, "GET", "/api/v1/oauth/clients", None).await;
    assert_eq!(status, StatusCode::OK, "{rows}");
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["provider"] == "google")
        .cloned()
        .unwrap()
}

#[tokio::test]
async fn test_the_clients_are_listed_one_row_per_provider() {
    // GIVEN a daemon whose data directory holds no OAuth client file
    let dir = TempDir::new().unwrap();

    // WHEN a client lists the OAuth clients
    let (status, rows) = call(state_in(&dir), "GET", "/api/v1/oauth/clients", None).await;

    // THEN Google and Microsoft each come back, Google asking for a secret
    assert_eq!(status, StatusCode::OK, "{rows}");
    let providers: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["provider"].as_str())
        .collect();
    assert_eq!(providers, ["google", "microsoft"]);
    assert_eq!(rows[0]["requires_client_secret"], true);
    assert_eq!(rows[1]["requires_client_secret"], false);
}

#[tokio::test]
async fn test_a_client_id_put_over_http_is_read_back_from_the_file() {
    // GIVEN a daemon with an empty data directory
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);

    // WHEN a client puts a Google client id
    let (status, written) = call(
        state.clone(),
        "PUT",
        "/api/v1/oauth/clients/google/client-id",
        Some(json!({ "value": "1-a.apps.googleusercontent.com" })),
    )
    .await;

    // THEN it is written under the data directory and listed as the override
    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["provider"], "google");
    assert!(dir.path().join("oauth-clients.toml").exists());
    let row = google_row(state).await;
    assert_eq!(row["override_client_id"], "1-a.apps.googleusercontent.com");
}

#[tokio::test]
async fn test_a_secret_and_a_key_put_over_http_show_as_present_never_as_values() {
    // GIVEN a daemon with an empty data directory
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);

    // WHEN a client puts a Google client secret and API key
    let (secret, _) = call(
        state.clone(),
        "PUT",
        "/api/v1/oauth/clients/google/client-secret",
        Some(json!({ "value": "GOCSPX-hidden" })),
    )
    .await;
    let (key, _) = call(
        state.clone(),
        "PUT",
        "/api/v1/oauth/clients/google/api-key",
        Some(json!({ "value": "AIza-hidden" })),
    )
    .await;

    // THEN both are accepted, listed as present, and neither value comes back
    assert_eq!(secret, StatusCode::OK);
    assert_eq!(key, StatusCode::OK);
    let row = google_row(state).await;
    assert_eq!(row["has_client_secret"], true);
    assert_eq!(row["has_api_key"], true);
    let text = row.to_string();
    assert!(
        !text.contains("GOCSPX-hidden") && !text.contains("AIza-hidden"),
        "{text}"
    );
}

#[tokio::test]
async fn test_a_write_for_an_unknown_provider_is_refused_with_its_name() {
    // GIVEN a daemon with an empty data directory
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);

    // WHEN a client writes each field for a provider Apollia does not know
    let mut answers = Vec::new();
    for field in ["client-id", "client-secret", "api-key"] {
        answers.push(
            call(
                state.clone(),
                "PUT",
                &format!("/api/v1/oauth/clients/yahoo/{field}"),
                Some(json!({ "value": "x" })),
            )
            .await,
        );
    }

    // THEN each is a 400 with the words the desktop showed in its own process,
    // and no file is created
    for (status, body) in answers {
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unknown provider: yahoo");
    }
    assert!(!dir.path().join("oauth-clients.toml").exists());
}

#[tokio::test]
async fn test_a_client_without_id_tests_as_not_configured() {
    // GIVEN a daemon whose data directory holds no Google client
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for the Google client to be tested
    let (status, verdict) = call(
        state_in(&dir),
        "POST",
        "/api/v1/oauth/clients/google/test",
        None,
    )
    .await;

    // THEN the verdict is a failed check that names the missing id, not an error
    assert_eq!(status, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["ok"], false);
    assert_eq!(verdict["detail"], "OAuth client id is not configured");
}

#[tokio::test]
async fn test_testing_an_unknown_provider_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for an unknown provider to be tested
    let (status, body) = call(
        state_in(&dir),
        "POST",
        "/api/v1/oauth/clients/yahoo/test",
        None,
    )
    .await;

    // THEN it is a 400 naming the provider
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "unknown provider: yahoo");
}

#[tokio::test]
async fn test_an_export_imported_over_http_writes_the_id_and_the_secret() {
    // GIVEN a daemon, and the text of a Google Desktop client export
    let dir = TempDir::new().unwrap();
    let state = state_in(&dir);
    let export = r#"{"installed": {"client_id": "7-q.apps.googleusercontent.com", "client_secret": "GOCSPX-q"}}"#;

    // WHEN a client posts it for Google
    let (status, written) = call(
        state.clone(),
        "POST",
        "/api/v1/oauth/clients/google/import",
        Some(json!({ "client_json": export })),
    )
    .await;

    // THEN both halves are in the file the connectors read
    assert_eq!(status, StatusCode::OK, "{written}");
    let row = google_row(state).await;
    assert_eq!(row["override_client_id"], "7-q.apps.googleusercontent.com");
    assert_eq!(row["has_client_secret"], true);
}

#[tokio::test]
async fn test_a_text_that_is_not_an_export_is_refused_as_unprocessable() {
    // GIVEN a daemon, and a service-account key in place of a client export
    let dir = TempDir::new().unwrap();
    let body = json!({ "client_json": r#"{"type": "service_account"}"# });

    // WHEN a client posts it for Google
    let (status, refusal) = call(
        state_in(&dir),
        "POST",
        "/api/v1/oauth/clients/google/import",
        Some(body),
    )
    .await;

    // THEN it is a 422 that says what was expected, and nothing is written
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refusal["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid OAuth client file: expected an \"installed\""),
        "{refusal}"
    );
    assert!(!dir.path().join("oauth-clients.toml").exists());
}
