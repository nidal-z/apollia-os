use axum::http::StatusCode;
use serde_json::json;

use apollia_tools::tools::{file_read, file_write};

use crate::api::test_support::{app_state, call, seeded_chat, MockBackend};

const OFFER: &str = "/api/v1/agent-drafts/offer";
const CHECK: &str = "/api/v1/agent-drafts";

/// A state whose chat holds a reading tool and a writing tool.
async fn state_with_tools(dir: &tempfile::TempDir) -> crate::api::server::AppState<MockBackend> {
    let state = seeded_chat(dir.path());
    let registry = state.tool_registry_handle.clone().unwrap();
    registry
        .register(file_read::FileRead::descriptor())
        .await
        .unwrap();
    registry
        .register(file_write::FileWrite::descriptor())
        .await
        .unwrap();
    state
}

#[tokio::test]
async fn test_the_offer_lists_what_the_machine_holds() {
    // GIVEN a runtime whose registry holds a reading tool and a writing tool
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_tools(&dir).await;

    // WHEN the offer of a draft is read
    let (status, offer) = call(state, "GET", OFFER, None).await;

    // THEN it lists both tools with their family, the reading one granted by
    // default and the writing one sensitive, and the sources every agent may
    // be given
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        offer["accesses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (
                a["id"].as_str().unwrap(),
                a["family"].as_str().unwrap(),
                a["sensitive"].as_bool().unwrap(),
                a["by_default"].as_bool().unwrap(),
            ))
            .collect::<Vec<_>>(),
        [
            ("file_read", "read", false, true),
            ("file_write", "write_or_delete", true, false)
        ]
    );
    assert_eq!(offer["sources"][0]["id"], "profile");
    assert_eq!(offer["sources"][1]["id"], "memory");
    assert!(offer["models"].is_array());
}

#[tokio::test]
async fn test_a_proposed_sheet_is_checked_and_nothing_is_written() {
    // GIVEN the same runtime
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_tools(&dir).await;

    // WHEN a sheet asking for the writing tool and for a tool nobody
    // installed is checked, then the stored agents are listed
    let (status, draft) = call(
        state.clone(),
        "POST",
        CHECK,
        Some(json!({
            "name": " Quotes ",
            "mission": "Read the quotes.",
            "access": ["file_write", "mail_send"],
            "sources": ["profile", "src-missing"],
        })),
    )
    .await;
    let (_, stored) = call(state, "GET", "/api/v1/agent-definitions", None).await;

    // THEN the draft keeps the writing tool unchecked, adds the reading one
    // as a default, names what the machine lacks, and no agent was created
    // beside the Apollia agent every machine holds
    assert_eq!(status, StatusCode::OK);
    assert_eq!(draft["name"], "Quotes");
    assert_eq!(
        draft["access"],
        json!([
            {"id": "file_write", "checked": false, "sensitive": true, "by_default": false},
            {"id": "file_read", "checked": true, "sensitive": false, "by_default": true},
        ])
    );
    assert_eq!(draft["sources"], json!(["profile"]));
    assert_eq!(
        draft["unavailable"],
        json!(["mail_send", "source:src-missing"])
    );
    let ids: Vec<&str> = stored
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|agent| agent["id"].as_str())
        .collect();
    assert_eq!(ids, ["apollia:chat"], "only the Apollia agent is stored");
}

#[tokio::test]
async fn test_a_sheet_without_a_mission_is_refused() {
    // GIVEN the same runtime
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_tools(&dir).await;

    // WHEN a sheet with a blank mission is checked
    let (status, body) = call(
        state,
        "POST",
        CHECK,
        Some(json!({"name": "Quotes", "mission": "  "})),
    )
    .await;

    // THEN it is refused, the reason naming the field
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["error"].as_str().unwrap().contains("mission"),
        "{body}"
    );
}

#[tokio::test]
async fn test_without_a_chat_the_offer_is_unavailable() {
    // GIVEN a runtime started without a chat
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN the offer is read
    let (status, body) = call(state, "GET", OFFER, None).await;

    // THEN the route says it cannot list what the machine holds
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"].as_str().unwrap().contains("chat"), "{body}");
}
