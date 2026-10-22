use axum::http::StatusCode;
use tempfile::TempDir;

use super::*;
use crate::api::test_support::{app_state, call};

#[tokio::test]
async fn test_the_routes_are_mounted_and_refuse_oversized_input_before_any_read() {
    // GIVEN the daemon's router
    let dir = TempDir::new().unwrap();
    let state = app_state(dir.path().to_path_buf());
    let long = "a".repeat(600);

    // WHEN a client browses a folder no Graph id can name, chooses a blank
    // item, and forgets an item of an account no email can name
    let (browse, browse_body) = call(
        state.clone(),
        "GET",
        &format!("/api/v1/connectors/onedrive/ada%40example.test/browse?folder_id={long}"),
        None,
    )
    .await;
    let (choose, choose_body) = call(
        state.clone(),
        "PUT",
        "/api/v1/connectors/onedrive/ada%40example.test/choices/%20",
        None,
    )
    .await;
    let (forget, forget_body) = call(
        state,
        "DELETE",
        &format!("/api/v1/connectors/onedrive/{long}/choices/item-1"),
        None,
    )
    .await;

    // THEN each answers 400 with the reason, before OneDrive is asked
    assert_eq!(browse, StatusCode::BAD_REQUEST, "{browse_body}");
    assert_eq!(browse_body["error"], "the item id is empty or too long");
    assert_eq!(choose, StatusCode::BAD_REQUEST, "{choose_body}");
    assert_eq!(choose_body["error"], "the item id is empty or too long");
    assert_eq!(forget, StatusCode::BAD_REQUEST, "{forget_body}");
    assert_eq!(
        forget_body["error"],
        "the account identifier is empty or too long"
    );
}

#[test]
fn test_each_refusal_carries_its_status() {
    // GIVEN one refusal of each kind a client can act on
    let cases = [
        (
            OneDriveChoiceError::NotChoosable("root".into()),
            StatusCode::BAD_REQUEST,
        ),
        (
            OneDriveChoiceError::NotConnected("no token".into()),
            StatusCode::CONFLICT,
        ),
        (
            OneDriveChoiceError::Graph("503".into()),
            StatusCode::BAD_GATEWAY,
        ),
        (
            OneDriveChoiceError::Malformed("x".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];

    // WHEN each is turned into an answer
    // THEN the status says who must act: the caller, the user, OneDrive, the daemon
    for (error, status) in cases {
        assert_eq!(refusal(error).0, status);
    }
}
