#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::api::routes_llm::{test_app_state_no_llm, MockBackend};
use crate::engine_store::{import_from_dir, MANIFEST_FILE, SIGNATURE_FILE};

async fn call(
    data_dir: &Path,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut state = test_app_state_no_llm();
    state.data_dir = data_dir.to_path_buf();
    let app = engines_router::<MockBackend>().with_state(state);
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(b) => Body::from(serde_json::to_vec(&b).unwrap()),
            None => Body::empty(),
        })
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/engine_store")
            .join(name),
    )
    .unwrap()
}

#[tokio::test]
async fn test_settings_default_to_stable_and_persist_through_the_route() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN the settings are read, changed, then read again
    let (s1, first) = call(dir.path(), "GET", "/api/v1/engines/settings", None).await;
    let (s2, _) = call(
        dir.path(),
        "PUT",
        "/api/v1/engines/settings",
        Some(json!({"channel": "preview", "base_url": "https://mirror.example.org"})),
    )
    .await;
    let (_, again) = call(dir.path(), "GET", "/api/v1/engines/settings", None).await;

    // THEN stable is the default, and the change persisted
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(first["channel"], "stable");
    assert_eq!(again["channel"], "preview");
    assert_eq!(again["base_url"], "https://mirror.example.org");
}

#[tokio::test]
async fn test_settings_refuse_a_non_http_mirror() {
    // GIVEN a mirror with another scheme
    let dir = tempfile::tempdir().unwrap();

    // WHEN it is saved
    let (status, body) = call(
        dir.path(),
        "PUT",
        "/api/v1/engines/settings",
        Some(json!({"channel": "stable", "base_url": "ftp://mirror"})),
    )
    .await;

    // THEN it is refused with a readable error
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("ftp://mirror"));
}

#[tokio::test]
async fn test_list_names_installed_engines_with_their_source() {
    // GIVEN a CUDA engine imported into the store
    let dir = tempfile::tempdir().unwrap();
    let usb = tempfile::tempdir().unwrap();
    for (to, from) in [
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ] {
        std::fs::write(usb.path().join(to), fixture(from)).unwrap();
    }
    let store = EngineStore {
        root: engine_store::store_root(dir.path()),
        trusted_key: String::from_utf8(fixture("test-dist.key.pub")).unwrap(),
        platform: "linux-x64".to_owned(),
    };
    import_from_dir(&store, usb.path()).unwrap();

    // WHEN the engines are listed
    let view = engines_view(
        dir.path(),
        Vec::new(),
        Some("NVIDIA GeForce RTX 4070".to_owned()),
    )
    .unwrap();
    let (status, body) = call(dir.path(), "GET", "/api/v1/engines", None).await;

    // THEN the engine is there with its tag, variant, path and source, the
    //      channel is stated, and CUDA is not proposed since it is installed
    assert_eq!(status, StatusCode::OK);
    let engine = &body["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["source"] == "imported")
        .unwrap()
        .clone();
    assert_eq!(engine["engine"], "llama-server");
    assert_eq!(engine["tag"], "b6000");
    assert_eq!(engine["variant"], "cuda");
    assert!(engine["path"].as_str().unwrap().ends_with("cuda"));
    assert_eq!(body["channel"], "stable");
    assert!(!view.cuda_proposable);
}

#[test]
fn test_cuda_is_proposable_only_with_a_card_and_without_the_engine() {
    // GIVEN an empty store
    let dir = tempfile::tempdir().unwrap();

    // WHEN the view is built with and without an NVIDIA card
    let with_card = engines_view(
        dir.path(),
        Vec::new(),
        Some("NVIDIA GeForce RTX 4070".to_owned()),
    )
    .unwrap();
    let without = engines_view(dir.path(), Vec::new(), None).unwrap();

    // THEN CUDA is proposed only with the card
    assert!(with_card.cuda_proposable);
    assert!(!without.cuda_proposable);
    assert!(with_card.updates.is_empty());
    assert_eq!(
        with_card.nvidia_card.as_deref(),
        Some("NVIDIA GeForce RTX 4070")
    );
    assert_eq!(without.nvidia_card, None);
}

#[tokio::test]
async fn test_import_without_signature_is_refused_by_the_route() {
    // GIVEN a directory with a manifest and no signature
    let dir = tempfile::tempdir().unwrap();
    let usb = tempfile::tempdir().unwrap();
    std::fs::write(usb.path().join(MANIFEST_FILE), fixture("stable-5.json")).unwrap();

    // WHEN it is imported through the route
    let (status, body) = call(
        dir.path(),
        "POST",
        "/api/v1/engines/import",
        Some(json!({"path": usb.path().display().to_string()})),
    )
    .await;

    // THEN it is refused: no key in this build, or no signature on the disk
    assert!(
        status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::UNPROCESSABLE_ENTITY,
        "{status} {body}"
    );
    assert!(body["error"].as_str().is_some());
}

#[tokio::test]
async fn test_node_state_route_reads_absent_then_ready() {
    // GIVEN an empty store, then Node.js imported into it
    let dir = tempfile::tempdir().unwrap();
    let (s1, before) = call(dir.path(), "GET", "/api/v1/engines/nodejs", None).await;
    let usb = tempfile::tempdir().unwrap();
    for (to, from) in [
        (MANIFEST_FILE, "nodejs-7.json"),
        (SIGNATURE_FILE, "nodejs-7.json.minisig"),
        ("node-v22.11.0-linux-x64.zip", "node-v22.11.0-linux-x64.zip"),
    ] {
        std::fs::write(usb.path().join(to), fixture(from)).unwrap();
    }
    let store = EngineStore {
        root: engine_store::store_root(dir.path()),
        trusted_key: String::from_utf8(fixture("node-dist.key.pub")).unwrap(),
        platform: "linux-x64".to_owned(),
    };
    import_from_dir(&store, usb.path()).unwrap();

    // WHEN the state is read through the route, then Node.js asked for
    let (s2, after) = call(dir.path(), "GET", "/api/v1/engines/nodejs", None).await;
    let (s3, ensured) = call(dir.path(), "POST", "/api/v1/engines/nodejs", None).await;

    // THEN it is absent, then ready with its tag; asking again for a ready
    //      Node.js answers ready, or 503 in a build without a distribution key
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(before, json!({"state": "absent"}));
    assert_eq!(after["state"], "ready");
    assert_eq!(after["tag"], "v22.11.0");
    assert!(
        (s3 == StatusCode::OK && ensured["state"] == "ready")
            || s3 == StatusCode::SERVICE_UNAVAILABLE,
        "{s3} {ensured}"
    );
}

#[tokio::test]
async fn test_listed_engines_carry_the_version_they_state() {
    // GIVEN an MLX environment with its package metadata, and a llama-server
    //      package whose binary is not there to state anything
    let dir = tempfile::tempdir().unwrap();
    let mlx = dir.path().join("mlx");
    let info = mlx.join("site-packages/vllm_mlx-0.9.1.dist-info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(info.join("METADATA"), "Name: vllm-mlx\nVersion: 0.9.1\n").unwrap();
    let mute = dir.path().join("llama/b7001/metal");
    std::fs::create_dir_all(&mute).unwrap();
    let entry = |engine, path: &Path| EngineEntry {
        engine,
        tag: Some("b7001".to_owned()),
        variant: Some(Variant::Metal),
        path: path.display().to_string(),
        source: Source::Downloaded,
        version: None,
        in_service: false,
    };

    // WHEN their versions are read
    let listed = with_versions(vec![
        entry(Engine::VllmMlx, &mlx),
        entry(Engine::LlamaServer, &mute),
    ])
    .await;

    // THEN the environment carries the version its metadata states, and the
    //      engine that states nothing carries none, not its store tag
    let versions: Vec<Option<&str>> = listed.iter().map(|e| e.version.as_deref()).collect();
    assert_eq!(versions, vec![Some("0.9.1"), None]);
}

#[test]
fn test_the_engines_the_runtime_starts_are_marked_in_service() {
    // GIVEN an embedded llama-server binary, a CUDA package of the store
    //      holding its own, an MLX environment and Node.js
    let dir = tempfile::tempdir().unwrap();
    let name = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let embedded = dir.path().join("app").join(&name);
    let cuda = dir.path().join("llama/b7001/cuda");
    let mlx = dir.path().join("mlx");
    let node = dir.path().join("node");
    for d in [
        embedded.parent().unwrap(),
        &cuda,
        &mlx.join("site-packages"),
        &node,
    ] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(&embedded, b"").unwrap();
    std::fs::write(cuda.join(&name), b"").unwrap();
    let entry = |engine, path: &Path, source| EngineEntry {
        engine,
        tag: None,
        variant: None,
        path: path.display().to_string(),
        source,
        version: None,
        in_service: false,
    };
    let mut entries = vec![
        entry(Engine::LlamaServer, &embedded, Source::Embedded),
        entry(Engine::LlamaServer, &cuda, Source::Downloaded),
        entry(Engine::VllmMlx, &mlx, Source::Embedded),
        entry(Engine::NodeJs, &node, Source::Downloaded),
    ];

    // WHEN the runtime starts the CUDA binary and that MLX environment
    mark_in_service(
        &mut entries,
        Some(&cuda.join(&name)),
        Some(&mlx.join("site-packages")),
    );

    // THEN the CUDA package and the MLX environment are in service, the
    //      embedded llama-server it passed over and Node.js are not
    let marks: Vec<bool> = entries.iter().map(|e| e.in_service).collect();
    assert_eq!(marks, vec![false, true, true, false]);

    // AND with nothing in reach, nothing is marked
    mark_in_service(&mut entries, None, None);
    assert!(entries.iter().all(|e| !e.in_service));
}

#[tokio::test]
async fn test_the_installation_route_answers_a_refusal_as_a_state_then_forgets_it() {
    // GIVEN an empty data directory and a directory without signature
    let dir = tempfile::tempdir().unwrap();
    let usb = tempfile::tempdir().unwrap();
    std::fs::write(usb.path().join(MANIFEST_FILE), fixture("stable-5.json")).unwrap();
    let route = "/api/v1/engines/installation";

    // WHEN the installation is read, an import started and followed to its
    //      end, then dismissed
    let (s1, idle) = call(dir.path(), "GET", route, None).await;
    let (s2, started) = call(
        dir.path(),
        "POST",
        "/api/v1/engines/installation/import",
        Some(json!({"path": usb.path().display().to_string()})),
    )
    .await;
    let root = engine_store::store_root(dir.path());
    let ended = match engine_store::install_job::subscribe(&root) {
        Some(mut rx) => {
            let done = rx.wait_for(|state| !state.is_running()).await.unwrap();
            serde_json::to_value(&*done).unwrap()
        }
        None => started.clone(),
    };
    let (_, read) = call(dir.path(), "GET", route, None).await;
    let (s3, dismissed) = call(dir.path(), "DELETE", route, None).await;
    let (_, after) = call(dir.path(), "GET", route, None).await;

    // THEN every answer is a state: idle, then the refusal with its code (no
    //      key in this build, or no signature on the disk), kept until dismissed
    assert_eq!(
        (s1, s2, s3),
        (StatusCode::OK, StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(idle, json!({"state": "idle"}));
    assert_eq!(ended["state"], "failed", "{ended}");
    assert_eq!(ended["kind"], "import");
    assert_eq!(ended["archive_requested"], false);
    let code = ended["code"].as_str().unwrap();
    assert!(
        code == "no_trusted_key" || code == "signature_missing",
        "{ended}"
    );
    assert_eq!(read, ended);
    assert_eq!(dismissed, json!({"state": "idle"}));
    assert_eq!(after, json!({"state": "idle"}));
}

#[tokio::test]
async fn test_a_download_without_a_key_or_a_mirror_ends_as_a_refused_state() {
    // GIVEN a store whose mirror is an address nothing listens on
    let dir = tempfile::tempdir().unwrap();
    let (_, _) = call(
        dir.path(),
        "PUT",
        "/api/v1/engines/settings",
        Some(json!({"channel": "stable", "base_url": "http://127.0.0.1:9"})),
    )
    .await;

    // WHEN the CUDA engine is asked for and the installation followed
    let (status, started) = call(
        dir.path(),
        "POST",
        "/api/v1/engines/installation",
        Some(json!({"engine": "llama-server", "variant": "cuda"})),
    )
    .await;
    let root = engine_store::store_root(dir.path());
    let ended = match engine_store::install_job::subscribe(&root) {
        Some(mut rx) => {
            let done = rx.wait_for(|state| !state.is_running()).await.unwrap();
            serde_json::to_value(&*done).unwrap()
        }
        None => started.clone(),
    };

    // THEN the answer is a state, refused before any archive: no key in this
    //      build, or a mirror that cannot be reached
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ended["state"], "failed", "{ended}");
    assert_eq!(ended["kind"], "install");
    assert_eq!(ended["archive_requested"], false);
    let code = ended["code"].as_str().unwrap();
    assert!(code == "no_trusted_key" || code == "network", "{ended}");
}
