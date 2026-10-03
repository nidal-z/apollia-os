use std::sync::Arc;

use apollia_memory::semantic::{RememberInput, SemanticMemory};
use apollia_memory::store::MemoryStore;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};

/// A daemon rooted at `dir`, whose namespace `agent-a` holds two facts.
fn daemon_with_memory(dir: &TempDir) -> (AppState<MockBackend>, Vec<String>) {
    let memory = dir.path().join("memory");
    std::fs::create_dir_all(&memory).unwrap();
    let store = MemoryStore::open(&memory.join("agent-a.db")).unwrap();
    let facts = SemanticMemory::new(&store);
    let ids = ["client.budget", "client.city"]
        .into_iter()
        .map(|key| {
            facts
                .remember(RememberInput {
                    namespace: "agent-a",
                    key,
                    value: &json!("x"),
                    confidence: 1.0,
                    source: None,
                    expires_at: None,
                })
                .unwrap()
        })
        .collect();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    (state, ids)
}

#[tokio::test]
async fn test_a_namespace_cleared_over_http_counts_what_it_removed() {
    // GIVEN a namespace holding two facts
    let dir = TempDir::new().unwrap();
    let (state, _) = daemon_with_memory(&dir);

    // WHEN the facts are cleared, then cleared again
    let body = Some(json!({ "memoryType": "semantic" }));
    let (first, removed) = call(
        state.clone(),
        "POST",
        "/api/v1/memory/namespaces/agent-a/clear",
        body.clone(),
    )
    .await;
    let (_, again) = call(
        state,
        "POST",
        "/api/v1/memory/namespaces/agent-a/clear",
        body,
    )
    .await;

    // THEN both are counted the first time and none is left
    assert_eq!(first, StatusCode::OK, "{removed}");
    assert_eq!(removed["removed"], 2);
    assert_eq!(again["removed"], 0);
}

#[tokio::test]
async fn test_a_namespace_naming_a_path_is_refused_by_every_route() {
    // GIVEN a daemon with memory, and a namespace that climbs out of its folder
    let dir = TempDir::new().unwrap();
    let (state, _) = daemon_with_memory(&dir);
    let base = "/api/v1/memory/namespaces/..%2Fagents";

    // WHEN each route is called with it
    let calls = [
        ("POST", format!("{base}/clear"), Some(json!({}))),
        (
            "POST",
            format!("{base}/purge"),
            Some(json!({ "olderThanDays": 0 })),
        ),
        ("DELETE", format!("{base}/entries/e-1"), None),
        ("GET", format!("{base}/export"), None),
    ];
    for (method, uri, body) in calls {
        let (status, refusal) = call(state.clone(), method, &uri, body).await;

        // THEN it is refused as a bad request, with the words of the
        // embedded mode
        assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {uri}");
        assert_eq!(
            refusal["error"],
            "VALIDATION_ERROR: invalid memory namespace '../agents'"
        );
    }
}

#[tokio::test]
async fn test_a_purge_over_http_keeps_entries_younger_than_the_threshold() {
    // GIVEN a namespace holding two facts written now
    let dir = TempDir::new().unwrap();
    let (state, _) = daemon_with_memory(&dir);

    // WHEN entries older than thirty days are purged
    let (status, purged) = call(
        state,
        "POST",
        "/api/v1/memory/namespaces/agent-a/purge",
        Some(json!({ "olderThanDays": 30 })),
    )
    .await;

    // THEN nothing is removed
    assert_eq!(status, StatusCode::OK, "{purged}");
    assert_eq!(purged["removed"], 0);
}

#[tokio::test]
async fn test_an_entry_deleted_over_http_is_gone_and_an_unknown_namespace_is_not_found() {
    // GIVEN a namespace holding two facts
    let dir = TempDir::new().unwrap();
    let (state, ids) = daemon_with_memory(&dir);

    // WHEN the first is deleted twice, and an entry of a namespace without a
    // file is deleted
    let uri = format!("/api/v1/memory/namespaces/agent-a/entries/{}", ids[0]);
    let (first, deleted) = call(state.clone(), "DELETE", &uri, None).await;
    let (_, again) = call(state.clone(), "DELETE", &uri, None).await;
    let (missing, refusal) = call(
        state,
        "DELETE",
        "/api/v1/memory/namespaces/nobody/entries/e-1",
        None,
    )
    .await;

    // THEN the first removes it, the second finds nothing, the third is a
    // 404 with the words of the embedded mode
    assert_eq!(first, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["deleted"], true);
    assert_eq!(again["deleted"], false);
    assert_eq!(missing, StatusCode::NOT_FOUND);
    assert_eq!(refusal["error"], "namespace 'nobody' not found");
}

#[tokio::test]
async fn test_an_export_read_over_http_imports_back_into_another_namespace() {
    // GIVEN a namespace holding two facts
    let dir = TempDir::new().unwrap();
    let (state, _) = daemon_with_memory(&dir);

    // WHEN it is exported, then the export is imported into another namespace
    let (exported, content) = call(
        state.clone(),
        "GET",
        "/api/v1/memory/namespaces/agent-a/export",
        None,
    )
    .await;
    let (imported, answer) = call(
        state,
        "POST",
        "/api/v1/memory/namespaces/agent-b/import",
        Some(json!({ "mode": "replace", "content": content })),
    )
    .await;

    // THEN the export carries both facts and both go in
    assert_eq!(exported, StatusCode::OK, "{content}");
    assert_eq!(content["format_version"], 1);
    assert_eq!(content["semantic"].as_array().map(Vec::len), Some(2));
    assert_eq!(imported, StatusCode::OK, "{answer}");
    assert_eq!(answer["imported"], 2);
    assert!(dir.path().join("memory").join("agent-b.db").exists());
}

#[tokio::test]
async fn test_an_import_of_an_unknown_format_version_is_refused() {
    // GIVEN an export written by a future format
    let dir = TempDir::new().unwrap();
    let (state, _) = daemon_with_memory(&dir);
    let content = json!({
        "format_version": 9,
        "namespace": "agent-a",
        "exported_at": "2026-10-03T00:00:00Z",
        "episodic": [],
        "semantic": [],
        "procedural": [],
    });

    // WHEN it is imported
    let (status, refusal) = call(
        state,
        "POST",
        "/api/v1/memory/namespaces/agent-a/import",
        Some(json!({ "mode": "merge", "content": content })),
    )
    .await;

    // THEN it is a bad request naming the version
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refusal["error"],
        "import failed: unsupported format version: 9"
    );
}
