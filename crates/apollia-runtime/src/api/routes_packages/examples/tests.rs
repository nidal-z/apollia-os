//! The example routes through the whole router. The examples the tests
//! propose are the packages of `tests/fixtures/example-packages/`.

use apollia_tools::{AgentRepository, PackageRepository};
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::routes_packages::tests::{agents_db, state};
use crate::api::test_support::call;

const LIST: &str = "/api/v1/packages/examples";
const PREVIEW: &str = "/api/v1/packages/examples/sample-watch/preview";
const INSTALL: &str = "/api/v1/packages/examples/sample-watch/install";

#[tokio::test]
async fn test_examples_route_lists_them_and_installs_none() {
    // GIVEN a fresh runtime
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;

    // WHEN the examples are listed
    let (status, body) = call(state, "GET", LIST, None).await;

    // THEN each comes with its name, its mission and the connections it
    // requires, and neither a package nor an agent is installed
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["name"], "sample-watch");
    assert_eq!(body[0]["agents"][0]["name"], "Sample watch");
    assert!(body[0]["agents"][0]["mission"]
        .as_str()
        .unwrap()
        .starts_with("Watch the topic"));
    assert_eq!(body[0]["requires"][0]["type"], "imap");
    assert_eq!(body[0]["requires"][0]["present"], false);
    assert!(body[0]["installedVersion"].is_null());
    let db = agents_db(&dir);
    assert!(PackageRepository::open(&db)
        .unwrap()
        .list()
        .unwrap()
        .is_empty());
    let stored = AgentRepository::open(&db).unwrap().list_stored().unwrap();
    let names: Vec<&str> = stored.iter().map(|agent| agent.name.as_str()).collect();
    assert_eq!(names, [apollia_tools::APOLLIA_AGENT_ID]);
}

#[tokio::test]
async fn test_example_preview_is_not_flagged_unsigned_and_writes_nothing() {
    // GIVEN a runtime with a journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;

    // WHEN the example is previewed
    let (status, body) = call(state, "POST", PREVIEW, None).await;

    // THEN the preview is the one of a package, with no notice about its
    // signature, and nothing is stored or assembled
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "sample-watch");
    assert_eq!(body["version"], "1.2.0");
    assert_eq!(body["signature"]["status"], "unsigned");
    assert!(body["refusal"].is_null());
    let codes: Vec<&str> = body["notices"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|notice| notice["code"].as_str())
        .collect();
    assert_eq!(codes, ["missing_connection"]);
    assert!(PackageRepository::open(&agents_db(&dir))
        .unwrap()
        .list()
        .unwrap()
        .is_empty());
    assert!(!dir.path().join("agents").join("bundled").exists());
}

#[tokio::test]
async fn test_example_installs_by_choice_then_uninstalls_like_any_package() {
    // GIVEN a runtime with a journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;

    // WHEN the user installs the example
    let (status, body) = call(state.clone(), "POST", INSTALL, Some(json!({}))).await;

    // THEN it is installed, listed as installed, and its archive is gone
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "sample-watch");
    assert_eq!(body["agents"][0], "sample-watch");
    let (_, listed) = call(state.clone(), "GET", LIST, None).await;
    assert_eq!(listed[0]["installedVersion"], "1.2.0");
    assert!(!dir
        .path()
        .join("agents")
        .join("bundled")
        .join("sample-watch.apo")
        .exists());

    // WHEN the user uninstalls it
    let (status, body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/sample-watch/uninstall",
        Some(json!({})),
    )
    .await;

    // THEN it goes like any package, and is proposed again
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, listed) = call(state, "GET", LIST, None).await;
    assert!(listed[0]["installedVersion"].is_null());
}

#[tokio::test]
async fn test_example_routes_answer_404_for_an_unknown_name() {
    // GIVEN a runtime with a journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;

    // WHEN an example the binary does not carry is previewed and installed
    let (previewed, body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/examples/ghost/preview",
        None,
    )
    .await;
    let (installed, _) = call(
        state,
        "POST",
        "/api/v1/packages/examples/ghost/install",
        Some(json!({})),
    )
    .await;

    // THEN both say there is none
    assert_eq!(previewed, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(installed, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_example_install_refuses_without_a_journal() {
    // GIVEN a runtime without an audit journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, false).await;

    // WHEN the example is installed
    let (status, body) = call(state, "POST", INSTALL, Some(json!({}))).await;

    // THEN it is refused: an installation is never left unrecorded
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "journal_unavailable");
}
