use std::sync::Arc;

use apollia_sources::{CheckedSource, SourceRegistryHandle};
use apollia_tools::{AgentOrigin, AgentRepository, StoredAgent};
use axum::http::StatusCode;
use serde_json::json;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::audit_journal::AuditJournalHandle;

const KEY: &[u8] = b"sources-routes-journal-key-00001";

/// A runtime on `dir` with the source registry, and with a signed journal and
/// one stored agent, `veille`, declaring no source, when `agent` is set.
async fn state(dir: &tempfile::TempDir, agent: bool) -> AppState<MockBackend> {
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.source_registry =
        Some(SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap());
    if agent {
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
            .unwrap()
            .save_definition(&StoredAgent {
                name: "veille".into(),
                origin: AgentOrigin::Package,
                package: Some("presse".into()),
                version: "1.0.0".into(),
                definition: json!({ "id": "veille", "name": "Veille", "defaults": {} }),
                enabled: true,
                installed_at: "2026-10-03T00:00:00Z".into(),
                updated_at: "2026-10-03T00:00:00Z".into(),
            })
            .unwrap();
        state.audit_journal = Some(
            AuditJournalHandle::open_with_key_bytes(&dir.path().join("journal.db"), KEY.to_vec())
                .await
                .unwrap(),
        );
    }
    state
}

#[tokio::test]
async fn test_a_source_is_created_listed_and_deleted_over_http() {
    // GIVEN a runtime with an empty registry
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, false).await;

    // WHEN a folder is created, the registry listed, the folder deleted twice
    let (empty_status, empty) = call(state.clone(), "GET", "/api/v1/sources", None).await;
    let (created_status, created) = call(
        state.clone(),
        "POST",
        "/api/v1/sources",
        Some(json!({ "kind": "folder", "label": "Contracts", "reference": "conn-contracts" })),
    )
    .await;
    let id = created["id"].as_str().unwrap_or_default().to_owned();
    let (_, listed) = call(state.clone(), "GET", "/api/v1/sources", None).await;
    let (deleted_status, deleted) = call(
        state.clone(),
        "DELETE",
        &format!("/api/v1/sources/{id}"),
        None,
    )
    .await;
    let (again_status, _) = call(state, "DELETE", &format!("/api/v1/sources/{id}"), None).await;

    // THEN the folder is answered in camelCase, listed, removed once, then not found
    assert_eq!(empty_status, StatusCode::OK);
    assert_eq!(empty, json!([]));
    assert_eq!(created_status, StatusCode::CREATED, "{created}");
    assert_eq!(created["kind"], "folder");
    assert_eq!(created["label"], "Contracts");
    assert_eq!(created["reference"], "conn-contracts");
    assert_eq!(created["group"], serde_json::Value::Null);
    assert_eq!(created["indexState"], "absent");
    assert!(created["addedAt"].is_string());
    assert_eq!(listed, json!([created.clone()]));
    assert_eq!(deleted_status, StatusCode::OK);
    assert_eq!(deleted, created);
    assert_eq!(again_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a_mailbox_without_reference_and_an_unknown_kind_are_refused() {
    // GIVEN a runtime with an empty registry
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, false).await;

    // WHEN a mailbox without reference and a source of an unknown kind are created
    let (mail_status, mail) = call(
        state.clone(),
        "POST",
        "/api/v1/sources",
        Some(json!({ "kind": "mail", "label": "Inbox" })),
    )
    .await;
    let (kind_status, kind) = call(
        state.clone(),
        "POST",
        "/api/v1/sources",
        Some(json!({ "kind": "dossier", "label": "Contracts", "reference": "c" })),
    )
    .await;
    let (_, listed) = call(state, "GET", "/api/v1/sources", None).await;

    // THEN both are refused with 422 and their reason, and nothing is stored
    assert_eq!(mail_status, StatusCode::UNPROCESSABLE_ENTITY, "{mail}");
    assert!(mail["error"]
        .as_str()
        .unwrap()
        .contains("needs a reference"));
    assert_eq!(kind_status, StatusCode::UNPROCESSABLE_ENTITY, "{kind}");
    assert!(kind["error"]
        .as_str()
        .unwrap()
        .contains("unknown source kind"));
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn test_without_a_registry_the_routes_answer_503() {
    // GIVEN a runtime holding no source registry
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );

    // WHEN the registry is listed
    let (status, body) = call(state, "GET", "/api/v1/sources", None).await;

    // THEN the route says it is not available
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}

#[tokio::test]
async fn test_an_override_naming_an_unknown_source_is_refused_at_write() {
    // GIVEN a runtime holding one folder source and the agent `veille`
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, true).await;
    let (_, folder) = call(
        state.clone(),
        "POST",
        "/api/v1/sources",
        Some(json!({ "kind": "folder", "label": "Press", "reference": "conn-presse" })),
    )
    .await;
    let folder_id = folder["id"].as_str().unwrap().to_owned();
    let uri = "/api/v1/agent-definitions/veille/overrides/sources";

    // WHEN the sources override names an unknown source, then the folder, the profile and the memory
    let (unknown_status, unknown) = call(
        state.clone(),
        "PUT",
        uri,
        Some(json!({ "value": ["profile", "documents"] })),
    )
    .await;
    let (_, after_refusal) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-definitions/veille",
        None,
    )
    .await;
    let (known_status, known) = call(
        state,
        "PUT",
        uri,
        Some(json!({ "value": [folder_id, "profile", "memory"] })),
    )
    .await;

    // THEN the unknown name is refused with 422 and nothing is written; the known list is
    assert_eq!(
        unknown_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{unknown}"
    );
    assert!(unknown["error"].as_str().unwrap().contains("documents"));
    assert_eq!(after_refusal["sources"], json!([]));
    assert_eq!(known_status, StatusCode::OK, "{known}");
    assert_eq!(known["sources"], json!([folder_id, "profile", "memory"]));
}

#[tokio::test]
async fn test_the_profile_checked_by_agent_set_is_among_the_checked_sources() {
    // GIVEN the agent `veille`, whose package declares no source
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, true).await;
    let registry = state.source_registry.clone().unwrap();
    let before = crate::agents::sources::checked_sources(dir.path(), &registry, "veille")
        .await
        .unwrap();

    // WHEN the override `apollia-os agent set veille sources profile` sends is written,
    // and the checked sources of the agent are read
    let (status, body) = call(
        state,
        "PUT",
        "/api/v1/agent-definitions/veille/overrides/sources",
        Some(json!({ "value": ["profile"] })),
    )
    .await;
    let after = crate::agents::sources::checked_sources(dir.path(), &registry, "veille")
        .await
        .unwrap();

    // THEN the profile is checked, and the profile guard of the context reads the same list
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(before, Some(vec![]));
    assert_eq!(after, Some(vec![CheckedSource::Profile]));
    assert!(crate::profile::profile_source_checked(dir.path(), "veille"));
}
