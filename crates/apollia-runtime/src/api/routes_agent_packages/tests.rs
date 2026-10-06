use std::path::Path;

use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use apollia_tools::{InstalledPackage, PackageRepository};

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::audit_journal::AuditJournalHandle;

fn state(dir: &Path) -> AppState<MockBackend> {
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    app_state(data_dir)
}

/// Records the package `veille` as an installation leaves it: its row, its
/// folder and the agent it brought.
fn installed_package(state: &AppState<MockBackend>) {
    let root = state.data_dir.join("agents/packages/veille");
    std::fs::create_dir_all(&root).unwrap();
    let db = apollia_core::paths::DataFile::Agents.path(&state.data_dir);
    let agents = apollia_tools::AgentRepository::open(&db).unwrap();
    agents
        .save_definition(&apollia_tools::StoredAgent {
            name: "veille-agent".into(),
            origin: apollia_tools::AgentOrigin::Package,
            package: Some("veille".into()),
            version: "1.2.0".into(),
            definition: json!({
                "id": "veille-agent", "name": "Veille", "mission": "Watch the morning news.",
                "defaults": {},
            }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    let packages = PackageRepository::open(&db).unwrap();
    packages
        .save(&InstalledPackage {
            name: "veille".into(),
            version: "1.2.0".into(),
            root_path: root,
            manifest_json: json!({
                "package": {
                    "name": "veille", "version": "1.2.0",
                    "description": "Morning watch", "author": "Ada",
                },
                "agents": [{"id": "veille-agent", "name": "Veille",
                            "mission": "Watch the morning news."}],
            })
            .to_string(),
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    packages.link_agent("veille", "veille-agent").unwrap();
}

#[tokio::test]
async fn test_an_installed_package_is_listed_and_read_in_full() {
    // GIVEN an installed package that brought one agent
    let dir = TempDir::new().unwrap();
    let state = state(dir.path());
    installed_package(&state);

    // WHEN the packages are listed, then this one is read
    let (listed, list) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed",
        None,
    )
    .await;
    let (read, detail) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;

    // THEN the listing and the detail read the package back, with its agent
    assert_eq!(listed, StatusCode::OK, "{list}");
    assert_eq!(list[0]["name"], "veille");
    assert_eq!(list[0]["description"], "Morning watch");
    assert_eq!(list[0]["root_missing"], false);
    assert_eq!(list[0]["counts"]["agents"], 1);
    assert_eq!(read, StatusCode::OK, "{detail}");
    assert_eq!(detail["author"], "Ada");
    assert_eq!(detail["content"]["agents"][0]["id"], "veille-agent");
    assert_eq!(detail["manifest"]["package"]["version"], "1.2.0");
}

#[tokio::test]
async fn test_installing_a_folder_is_no_longer_a_route() {
    // GIVEN a runtime
    let dir = TempDir::new().unwrap();
    let state = state(dir.path());

    // WHEN a folder is posted to the two routes that installed and previewed one
    let (install, _) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({ "path": dir.path().to_string_lossy() })),
    )
    .await;
    let (preview, _) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/preview",
        Some(json!({ "path": dir.path().to_string_lossy() })),
    )
    .await;

    // THEN neither exists, and nothing was written
    assert_eq!(install, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(preview, StatusCode::NOT_FOUND);
    assert!(!state.data_dir.join("agents").exists());
}

#[tokio::test]
async fn test_an_unknown_package_is_a_404_and_an_unreadable_store_a_500() {
    // GIVEN a runtime with no package, and one whose data directory is a file
    let dir = TempDir::new().unwrap();
    let empty = state(dir.path());
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    let broken = app_state(file);

    // WHEN a package is read from the first and the list from the second
    let (missing, body) = call(
        empty,
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;
    let (failed, failure) = call(broken, "GET", "/api/v1/agent-packages/installed", None).await;

    // THEN the absent package is a 404 in the words of the embedded mode,
    // the store a 500
    assert_eq!(missing, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "package 'veille' not found");
    assert_eq!(failed, StatusCode::INTERNAL_SERVER_ERROR, "{failure}");
}

#[tokio::test]
async fn test_a_package_is_uninstalled_and_a_name_that_is_a_path_refused() {
    // GIVEN an installed package, and the audit journal every uninstallation
    // is recorded in
    let dir = TempDir::new().unwrap();
    let mut state = state(dir.path());
    let journal = AuditJournalHandle::open_with_key_bytes(
        &dir.path().join("journal.db"),
        b"agent-packages-journal-key-0001".to_vec(),
    )
    .await
    .unwrap();
    state.governance = Some(crate::governance::GovernanceHandle::detached(
        crate::governance::GovernanceState::default(),
        Some(journal.clone()),
    ));
    state.audit_journal = Some(journal);
    installed_package(&state);

    // WHEN it is uninstalled, then a name climbing out of the data directory is
    let (removed, body) = call(
        state.clone(),
        "DELETE",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;
    let (_, list) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed",
        None,
    )
    .await;
    let (refused, refusal) = call(
        state.clone(),
        "DELETE",
        "/api/v1/agent-packages/installed/%2E%2E",
        None,
    )
    .await;

    // THEN the package, its row and its folder are gone, and the path is a
    // 400 before anything is touched
    assert_eq!(removed, StatusCode::OK, "{body}");
    assert_eq!(body["uninstalled"], "veille");
    assert_eq!(list, json!([]));
    assert!(!state.data_dir.join("agents/packages/veille").exists());
    assert_eq!(refused, StatusCode::BAD_REQUEST, "{refusal}");
    assert!(state.data_dir.exists());
}
