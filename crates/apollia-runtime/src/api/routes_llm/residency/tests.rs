use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request};
use tower::ServiceExt;

use super::super::tests::{test_app_state_no_llm, MockBackend};
use super::*;
use crate::llama_server::LlamaServerSupervisor;

/// The LLM routes over stand-in `models`, sharing a residency whose pins live
/// in a `system.db` under `dir`.
async fn router_over(
    models: &[&str],
    dir: &std::path::Path,
) -> (Router, Arc<LlamaServerSupervisor>) {
    let sup = LlamaServerSupervisor::with_stand_in_models(models).await;
    sup.attach_residency(Residency::new(Some(dir.join("system.db")), None, None));
    let mut state = test_app_state_no_llm();
    state.llama_server_supervisor = Some(Arc::clone(&sup));
    (
        crate::api::routes_llm::llm_routes::<MockBackend>().with_state(state),
        sup,
    )
}

async fn send(
    router: Router,
    method: Method,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .expect("build request");
    let response = router.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn resident(sup: &LlamaServerSupervisor) -> Vec<String> {
    sup.loaded_models()
        .await
        .into_iter()
        .map(|m| m.model_path)
        .collect()
}

#[tokio::test]
async fn test_releasing_the_last_lease_over_http_unloads_the_model() {
    // GIVEN a resident model two conversations designate
    let dir = tempfile::tempdir().expect("temp dir");
    let (router, sup) = router_over(&["/models/a.gguf"], dir.path()).await;
    for conversation in ["c1", "c2"] {
        let body = format!(
            r#"{{"clientId":"desk","conversationId":"{conversation}","modelPath":"/models/a.gguf"}}"#
        );
        let (status, _) = send(router.clone(), Method::POST, "/api/v1/llm/leases", &body).await;
        assert_eq!(status, StatusCode::OK);
    }

    // WHEN one conversation leaves the screen, then the other
    let (_, first) = send(
        router.clone(),
        Method::POST,
        "/api/v1/llm/leases/release",
        r#"{"clientId":"desk","conversationId":"c1"}"#,
    )
    .await;
    let still_loaded = resident(&sup).await;
    let (status, last) = send(
        router,
        Method::POST,
        "/api/v1/llm/leases/release",
        r#"{"clientId":"desk","conversationId":"c2"}"#,
    )
    .await;

    // THEN the first release keeps the model, the second unloads it
    assert_eq!(first["unloaded"], false);
    assert_eq!(still_loaded, vec!["/models/a.gguf"]);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(last["released"], "/models/a.gguf");
    assert_eq!(last["unloaded"], true);
    assert!(resident(&sup).await.is_empty());
}

#[tokio::test]
async fn test_a_pin_set_over_http_is_listed_kept_in_system_db_and_keeps_the_model() {
    // GIVEN a resident model with one open conversation
    let dir = tempfile::tempdir().expect("temp dir");
    let (router, sup) = router_over(&["/models/a.gguf"], dir.path()).await;
    send(
        router.clone(),
        Method::POST,
        "/api/v1/llm/leases",
        r#"{"clientId":"desk","conversationId":"c1","modelPath":"/models/a.gguf"}"#,
    )
    .await;

    // WHEN the model is pinned, then its conversation leaves the screen
    let (status, pinned) = send(
        router.clone(),
        Method::PUT,
        "/api/v1/llm/pins",
        r#"{"modelPath":"/models/a.gguf","pinned":true}"#,
    )
    .await;
    let (_, released) = send(
        router.clone(),
        Method::POST,
        "/api/v1/llm/leases/release",
        r#"{"clientId":"desk","conversationId":"c1"}"#,
    )
    .await;
    let (_, listed) = send(router, Method::GET, "/api/v1/llm/pins", "").await;

    // THEN the pin is saved, listed, written to system.db, and the model stays
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pinned["pinned"], true);
    assert_eq!(released["unloaded"], false);
    assert_eq!(listed["pins"], serde_json::json!(["/models/a.gguf"]));
    let repo =
        apollia_core::LlmBackendRepository::open(&dir.path().join("system.db")).expect("system.db");
    assert_eq!(repo.pinned_models().expect("pins"), vec!["/models/a.gguf"]);
    assert_eq!(resident(&sup).await, vec!["/models/a.gguf"]);
}

#[tokio::test]
async fn test_a_blank_field_is_refused_and_a_host_without_engine_answers_503() {
    // GIVEN the routes over an engine, and over no engine at all
    let dir = tempfile::tempdir().expect("temp dir");
    let (router, _sup) = router_over(&[], dir.path()).await;
    let bare =
        crate::api::routes_llm::llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

    // WHEN a lease without a client is opened, and the pins are read without
    // an engine
    let (blank, body) = send(
        router,
        Method::POST,
        "/api/v1/llm/leases",
        r#"{"clientId":" ","conversationId":"c1","modelPath":"/models/a.gguf"}"#,
    )
    .await;
    let (absent, _) = send(bare, Method::GET, "/api/v1/llm/pins", "").await;

    // THEN the first is a 400 naming the field, the second a 503
    assert_eq!(blank, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .is_some_and(|e| e.contains("clientId")));
    assert_eq!(absent, StatusCode::SERVICE_UNAVAILABLE);
}
