//! Loading on demand: a request naming a model of this machine that is not
//! loaded, against a stand-in `llama-server` the supervisor really launches.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::relay::RelayState;
use super::router::compat_router;
use crate::llama_server::{fake_engine, recorded_gguf, LlamaServerSupervisor};
use crate::local_engine::LocalEngine;
use crate::residency::{LoadOrigin, Residency};

const HALF_GIB: u64 = 512 * 1024 * 1024;

/// A model file of its own, in a folder of its own under `dir`.
fn model_in(dir: &Path, name: &str) -> String {
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    recorded_gguf(&folder, HALF_GIB).display().to_string()
}

/// A machine with two language models in its registry, `acme/one` and
/// `acme/two`, none loaded, and an `apollia.toml` holding `settings`.
struct Machine {
    dir: TempDir,
    router: Router,
    supervisor: Arc<LlamaServerSupervisor>,
    residency: Arc<Residency>,
    paths: [String; 2],
}

fn machine(settings: &str) -> Machine {
    let dir = TempDir::new().unwrap();
    let paths = [model_in(dir.path(), "one"), model_in(dir.path(), "two")];
    let entry = |id: &str, path: &str| {
        json!({
            "id": id, "format": "gguf", "path": path, "size_bytes": HALF_GIB,
            "origin": { "kind": "import" }, "category": "language"
        })
    };
    let models_dir = dir.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    let index = json!({
        "version": 1,
        "models": [entry("acme/one", &paths[0]), entry("acme/two", &paths[1])]
    });
    std::fs::write(models_dir.join("index.json"), index.to_string()).unwrap();
    let config = dir.path().join("apollia.toml");
    std::fs::write(&config, settings).unwrap();
    let supervisor = LlamaServerSupervisor::with_uncapped_engine_in_dir_for_test(
        fake_engine(dir.path()),
        dir.path(),
    );
    let residency = Residency::new(None, Some(config.clone()), None);
    supervisor.attach_residency(Arc::clone(&residency));
    let models = LocalModels::new(Some(supervisor.clone()), None, Some(models_dir))
        .with_files(config, dir.path().join("system.db"));
    let router = compat_router(
        None,
        RelayState::new(models).unwrap(),
        ServerLogsHandle::spawn(&crate::eventbus::EventBus::new().0),
        false,
    );
    Machine {
        dir,
        router,
        supervisor,
        residency,
        paths,
    }
}

async fn send(router: &Router, method: &str, path: &str, body: Value) -> Response {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    router.clone().oneshot(request).await.unwrap()
}

async fn json_of(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn chat(router: &Router, model: &str) -> (StatusCode, Value) {
    let body = json!({ "model": model, "input": ["hi"] });
    json_of(send(router, "POST", "/v1/chat/completions", body).await).await
}

/// The resident models, by path, with their origin.
async fn resident(machine: &Machine) -> Vec<(String, LoadOrigin)> {
    let mut resident: Vec<_> = LocalEngine::loaded_models(machine.supervisor.as_ref())
        .await
        .iter()
        .map(|m| (m.model_path.clone(), machine.residency.origin_of(m)))
        .collect();
    resident.sort_by(|a, b| a.0.cmp(&b.0));
    resident
}

async fn listed(router: &Router) -> Vec<String> {
    let (status, body) = json_of(send(router, "GET", "/v1/models", Value::Null).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut ids: Vec<String> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

fn launched(dir: &Path) -> PathBuf {
    dir.join("args.txt")
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_request_loads_the_model_it_names_and_marks_it_as_the_server_s() {
    // GIVEN on-demand loading left at its default, and nothing loaded
    let machine = machine("[server]\nenabled = true\n");

    // WHEN a request names a model of the registry
    let (status, body) = chat(&machine.router, "acme/one").await;

    // THEN the engine was launched for it and answered, and the process
    //      carries the server origin
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(launched(machine.dir.path()).exists());
    assert_eq!(
        resident(&machine).await,
        vec![(machine.paths[0].clone(), LoadOrigin::Server)]
    );
    machine.supervisor.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_with_on_demand_loading_off_a_model_not_loaded_answers_404() {
    // GIVEN on-demand loading turned off, and nothing loaded
    let machine = machine("[server]\njit_loading = false\n");

    // WHEN a request names a model of the registry
    let (status, body) = chat(&machine.router, "acme/one").await;

    // THEN 404 model_not_loaded, and nothing was launched
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "model_not_loaded");
    assert!(!launched(machine.dir.path()).exists());
    assert!(resident(&machine).await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_the_model_list_includes_the_machine_s_models_only_with_on_demand_loading() {
    // GIVEN the same machine, on-demand loading on, then off
    let on = machine("");
    let off = machine("[server]\njit_loading = false\n");

    // WHEN the models are listed
    let with = listed(&on.router).await;
    let without = listed(&off.router).await;

    // THEN the list holds the models of the machine with it on, and only the
    //      loaded ones, none here, with it off
    assert_eq!(with, vec!["acme/one".to_owned(), "acme/two".to_owned()]);
    assert!(without.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_loading_a_second_model_unloads_the_first_the_server_loaded() {
    // GIVEN a model the server loaded for an earlier request, finished
    let machine = machine("");
    let (first, body) = chat(&machine.router, "acme/one").await;
    assert_eq!(first, StatusCode::OK, "{body}");

    // WHEN a request names another model
    let (second, body) = chat(&machine.router, "acme/two").await;

    // THEN only the last one stays loaded
    assert_eq!(second, StatusCode::OK, "{body}");
    assert_eq!(
        resident(&machine).await,
        vec![(machine.paths[1].clone(), LoadOrigin::Server)]
    );
    machine.supervisor.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_without_keep_last_only_both_models_the_server_loaded_stay() {
    // GIVEN keeping only the last model turned off, and the load guardrail
    // off too: with it on, whether a second model fits beside the first is
    // decided by the memory of the machine running the test, and a small one
    // unloads the first to make room
    let machine =
        machine("[server]\njit_keep_last_only = false\n\n[llm.guardrails]\nlevel = \"disabled\"\n");
    let (first, body) = chat(&machine.router, "acme/one").await;
    assert_eq!(first, StatusCode::OK, "{body}");

    // WHEN a request names another model
    let (second, body) = chat(&machine.router, "acme/two").await;

    // THEN both stay loaded, each the server's
    assert_eq!(second, StatusCode::OK, "{body}");
    assert_eq!(
        resident(&machine).await,
        vec![
            (machine.paths[0].clone(), LoadOrigin::Server),
            (machine.paths[1].clone(), LoadOrigin::Server),
        ]
    );
    machine.supervisor.shutdown_in_place().await;
}
