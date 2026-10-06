use std::path::Path;
use std::sync::Arc;

use apollia_llm::model_index::{IndexedModel, ModelOrigin};
use axum::body::Body;
use axum::http::{Method, Request};
use serde_json::json;
use tower::ServiceExt;

use super::super::tests::{test_app_state_no_llm, MockBackend};
use super::*;
use crate::llama_server::{
    fake_engine, fake_engine_launches, recorded_gguf, LlamaServerSupervisor,
};

const MODEL_ID: &str = "Qwen2.5-0.5B-Instruct";
const ROW: &str = "qwen2-5-0-5b-instruct";
const HALF_GIB: u64 = 512 * 1024 * 1024;

/// A data directory holding one GGUF language model in its index, a
/// `system.db`, and a stand-in engine. Returns the model's path.
fn home(dir: &Path) -> String {
    let models = crate::installed_models::models_dir(dir);
    std::fs::create_dir_all(&models).unwrap();
    let path = recorded_gguf(&models, HALF_GIB);
    ModelIndex {
        models: vec![IndexedModel {
            id: MODEL_ID.to_owned(),
            format: ModelFormat::Gguf,
            path: path.clone(),
            parts: Vec::new(),
            annexes: Vec::new(),
            size_bytes: HALF_GIB,
            origin: ModelOrigin::Import,
            sha256: None,
            origin_verified: false,
            category: ModelCategory::Language,
            parameters: None,
            modified_at: None,
            pinned_at: None,
            category_override: None,
            source_file_override: None,
            capabilities: None,
        }],
        ..ModelIndex::default()
    }
    .save(&models.join(INDEX_FILE_NAME))
    .unwrap();
    path.display().to_string()
}

/// The LLM routes over `dir`, as a runtime started there serves them.
fn state_over(dir: &Path, engine: &Arc<LlamaServerSupervisor>) -> AppState<MockBackend> {
    let db = dir.join("system.db");
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.to_path_buf();
    state.config_path = Some(dir.join("apollia.toml"));
    state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(
        LlmBackendRepository::open(&db).unwrap(),
    )));
    state.llama_server_supervisor = Some(Arc::clone(engine));
    state
}

fn engine(dir: &Path) -> Arc<LlamaServerSupervisor> {
    LlamaServerSupervisor::with_engine_in_dir_for_test(fake_engine(dir), dir)
}

fn router(state: AppState<MockBackend>) -> Router {
    crate::api::routes_llm::llm_routes::<MockBackend>().with_state(state)
}

async fn send(router: &Router, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn launches(dir: &Path) -> Vec<String> {
    fake_engine_launches(dir)
        .into_iter()
        .filter(|line| line.contains("--port"))
        .collect()
}

/// The value a launch line gives `flag`.
fn arg(line: &str, flag: &str) -> Option<u64> {
    let mut words = line.split_whitespace();
    words.find(|w| *w == flag)?;
    words.next()?.parse().ok()
}

/// The window of one request a launch line carries: `-c` is the window of
/// every slot together.
fn window(line: &str) -> u64 {
    arg(line, "-c").unwrap() / arg(line, "-np").unwrap_or(1)
}

const ESTIMATE: &str = "/api/v1/llm/loaded-models/estimate";
const LOAD: &str = "/api/v1/llm/loaded-models";

#[tokio::test]
async fn test_the_estimate_reads_the_recorded_header_at_the_window_asked() {
    // GIVEN a model of the index whose header is recorded, and no guardrail
    // section in apollia.toml
    let dir = tempfile::tempdir().unwrap();
    home(dir.path());
    let sup = engine(dir.path());
    let routes = router(state_over(dir.path(), &sup));

    // WHEN its load is estimated at 4 096 and at 32 768 tokens
    let (small_status, small) = send(
        &routes,
        Method::POST,
        ESTIMATE,
        json!({ "modelId": MODEL_ID, "nCtx": 4096 }),
    )
    .await;
    let (_, large) = send(
        &routes,
        Method::POST,
        ESTIMATE,
        json!({ "modelId": MODEL_ID, "nCtx": 32768, "advanced": { "n_threads": 4 } }),
    )
    .await;

    // THEN each answers the window it would launch, the largest window and
    // the quantisation the file declares, the memory on the accelerator and
    // in total sized from the header, and the balanced guardrail's verdict;
    // nothing was launched
    assert_eq!(small_status, StatusCode::OK, "{small}");
    assert_eq!(small["maxCtx"], 32768);
    assert_eq!(small["quantization"], "Q8_0");
    assert_eq!(small["engine"], "llama-server");
    assert_eq!(small["configurable"], true);
    assert_eq!(small["nCtx"], 4096);
    assert_eq!(large["nCtx"], 32768);
    assert_eq!(small["totalLayers"], 24);
    assert_eq!(small["memory"]["kvCacheMeasured"], true);
    let total = |v: &Value| v["memory"]["totalGb"].as_f64().unwrap();
    assert!(total(&large) > total(&small), "{small} {large}");
    assert!(small["memory"]["gpuGb"].as_f64().is_some());
    assert_eq!(small["guardrail"]["level"], "balanced");
    assert!(small["guardrail"]["verdict"].is_string());
    assert!(launches(dir.path()).is_empty());
}

#[tokio::test]
async fn test_the_estimate_refuses_what_it_cannot_load() {
    // GIVEN a model of the index
    let dir = tempfile::tempdir().unwrap();
    home(dir.path());
    let routes = router(state_over(dir.path(), &engine(dir.path())));

    // WHEN a model outside the index, an unknown setting and a system prompt
    // are asked
    let (ghost, _) = send(
        &routes,
        Method::POST,
        ESTIMATE,
        json!({ "modelId": "ghost" }),
    )
    .await;
    let (unknown, body) = send(
        &routes,
        Method::POST,
        ESTIMATE,
        json!({ "modelId": MODEL_ID, "advanced": { "turbo": true } }),
    )
    .await;
    let (prompt, _) = send(
        &routes,
        Method::POST,
        LOAD,
        json!({ "modelId": MODEL_ID, "advanced": { "system_prompt": "x" } }),
    )
    .await;

    // THEN the first is not found, the others are refused before any launch
    assert_eq!(ghost, StatusCode::NOT_FOUND);
    assert_eq!(unknown, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("turbo"), "{body}");
    assert_eq!(prompt, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(launches(dir.path()).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_load_launches_the_window_and_offload_asked_and_lists_the_instance() {
    // GIVEN a model of the index and a stand-in engine
    let dir = tempfile::tempdir().unwrap();
    let path = home(dir.path());
    let sup = engine(dir.path());
    let routes = router(state_over(dir.path(), &sup));

    // WHEN it is loaded with a window and an offload of its own
    let (status, body) = send(
        &routes,
        Method::POST,
        LOAD,
        json!({ "modelId": MODEL_ID, "nCtx": 16384, "nGpuLayers": 12 }),
    )
    .await;

    // THEN the engine was launched with them, and the answer is the instance
    assert_eq!(status, StatusCode::OK, "{body}");
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert_eq!(window(&lines[0]), 16384, "{}", lines[0]);
    assert_eq!(arg(&lines[0], "-ngl"), Some(12), "{}", lines[0]);
    assert_eq!(body["modelId"], MODEL_ID);
    assert_eq!(body["loaded"]["modelPath"], path);
    assert_eq!(body["loaded"]["nCtx"], 16384);
    assert_eq!(body["loaded"]["nGpuLayers"], 12);
    assert!(body.get("rememberedPresetId").is_none());
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_load_the_guardrail_refuses_carries_the_estimate_and_the_budget() {
    // GIVEN a guardrail holding every load to a tenth of a gigabyte
    let dir = tempfile::tempdir().unwrap();
    home(dir.path());
    std::fs::write(
        dir.path().join("apollia.toml"),
        "[llm.guardrails]\nlevel = \"ceiling\"\nceiling_gb = 0.1\n",
    )
    .unwrap();
    let sup = engine(dir.path());
    let routes = router(state_over(dir.path(), &sup));

    // WHEN the model is estimated, loaded, then loaded with confirmation
    let (_, estimate) = send(
        &routes,
        Method::POST,
        ESTIMATE,
        json!({ "modelId": MODEL_ID }),
    )
    .await;
    let (refused_status, refusal) =
        send(&routes, Method::POST, LOAD, json!({ "modelId": MODEL_ID })).await;
    let launched_before = launches(dir.path()).len();
    let (confirmed, _) = send(
        &routes,
        Method::POST,
        LOAD,
        json!({ "modelId": MODEL_ID, "confirmed": true }),
    )
    .await;

    // THEN the estimate says refused against that budget, the load is refused
    // with the estimate and the budget and launches nothing, and the
    // confirmed load goes ahead
    assert_eq!(estimate["guardrail"]["verdict"], "refused", "{estimate}");
    assert_eq!(estimate["guardrail"]["budgetGb"], 0.1);
    assert_eq!(refused_status, StatusCode::CONFLICT, "{refusal}");
    assert_eq!(refusal["code"], "guardrail_refused");
    assert_eq!(refusal["budgetGb"], 0.1);
    assert!(refusal["estimatedGb"].as_f64().unwrap() > 0.1);
    assert_eq!(launched_before, 0);
    assert_eq!(confirmed, StatusCode::OK);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_remembered_settings_are_the_ones_a_lazy_load_takes_next() {
    // GIVEN a model loaded with "remember these settings"
    let dir = tempfile::tempdir().unwrap();
    let path = home(dir.path());
    let sup = engine(dir.path());
    let routes = router(state_over(dir.path(), &sup));
    let (status, body) = send(
        &routes,
        Method::POST,
        LOAD,
        json!({ "modelId": MODEL_ID, "nCtx": 8192, "advanced": { "n_threads": 3 }, "remember": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // WHEN it is unloaded, then asked for by a request that names no window
    sup.unload_model(&path).await.unwrap();
    let lease = sup
        .ensure_model(ROW, path.clone(), None, LoadConsent::Unconfirmed)
        .await
        .unwrap();

    // THEN the default preset of the model carries the settings, no system
    // prompt, and the lazy load launched under them
    let preset_id = body["rememberedPresetId"].as_i64().unwrap();
    let preset = LlmBackendRepository::open(&dir.path().join("system.db"))
        .unwrap()
        .find_preset(preset_id)
        .unwrap()
        .unwrap();
    assert!(preset.is_model_default);
    assert_eq!(preset.model_id, ROW);
    assert_eq!(
        preset.load,
        json!({ "n_ctx": 8192, "n_threads": 3 })
            .as_object()
            .cloned()
            .unwrap()
    );
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert_eq!(window(&lines[1]), 8192, "{}", lines[1]);
    assert!(lines[1].contains(" -t 3 "), "{}", lines[1]);
    assert_eq!(lease.n_ctx(), 8192);
    drop(lease);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_the_last_model_survives_a_restart_and_reloads_with_its_settings() {
    // GIVEN no model loaded by hand yet, then one loaded at 12 288 tokens
    let dir = tempfile::tempdir().unwrap();
    let path = home(dir.path());
    let first = engine(dir.path());
    let before_restart = router(state_over(dir.path(), &first));
    let (empty, _) = send(
        &before_restart,
        Method::GET,
        "/api/v1/llm/loaded-models/last",
        Value::Null,
    )
    .await;
    let (status, _) = send(
        &before_restart,
        Method::POST,
        LOAD,
        json!({ "modelId": MODEL_ID, "nCtx": 12288 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    first.shutdown_in_place().await;

    // WHEN the runtime starts again over the same data directory, reads the
    // last model, and reloads it
    let second = engine(dir.path());
    let after_restart = router(state_over(dir.path(), &second));
    let (_, last) = send(
        &after_restart,
        Method::GET,
        "/api/v1/llm/loaded-models/last",
        Value::Null,
    )
    .await;
    let (reload_status, reloaded) = send(
        &after_restart,
        Method::POST,
        "/api/v1/llm/loaded-models/last/reload",
        json!({}),
    )
    .await;

    // THEN there was none at first; after the restart the model and its
    // settings are read back, and the reload launches it under them
    assert_eq!(empty, StatusCode::NOT_FOUND);
    assert_eq!(last["modelId"], MODEL_ID);
    assert_eq!(last["load"], json!({ "n_ctx": 12288 }));
    assert!(last["loadedAt"].is_string());
    assert_eq!(reload_status, StatusCode::OK, "{reloaded}");
    assert_eq!(reloaded["loaded"]["modelPath"], path);
    assert_eq!(reloaded["loaded"]["nCtx"], 12288);
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert_eq!(window(&lines[1]), 12288, "{}", lines[1]);
    second.shutdown_in_place().await;
}
