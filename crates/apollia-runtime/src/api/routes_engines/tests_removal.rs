#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_core::config::EngineChoice;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::removal::is_in_service;
use super::*;
use crate::api::routes_llm::{test_app_state_no_llm, MockBackend};
use crate::engine_store::selection::resolve;
use crate::engine_store::{package_dir, Resolved, RECORD_FILE};

/// File a verified build of `llama-server` `tag` `variant` under the store of
/// `data_dir`, as an installation leaves it.
fn file(data_dir: &Path, tag: &str, variant: Variant) -> PathBuf {
    let root = engine_store::store_root(data_dir);
    let dir = package_dir(&root, Engine::LlamaServer, tag, variant);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("llama-server"), b"binary").unwrap();
    let package = engine_store::InstalledPackage {
        engine: Engine::LlamaServer,
        tag: tag.to_owned(),
        variant,
        platform: "linux-x64".to_owned(),
        sha256: "0".repeat(64),
        source: Source::Downloaded,
        released: None,
        path: dir.clone(),
    };
    std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&package).unwrap()).unwrap();
    dir
}

/// A settings file choosing `gguf` for the GGUF engine.
fn choosing(data_dir: &Path, gguf: &str) -> PathBuf {
    let path = data_dir.join("apollia.toml");
    std::fs::write(&path, format!("[engines]\ngguf = \"{gguf}\"\n")).unwrap();
    path
}

async fn delete(data_dir: &Path, config: Option<PathBuf>, uri: &str) -> (StatusCode, Value) {
    let mut state = test_app_state_no_llm();
    state.data_dir = data_dir.to_path_buf();
    state.config_path = config;
    let app = engines_router::<MockBackend>().with_state(state);
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
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

#[tokio::test]
async fn test_removing_a_build_out_of_service_deletes_its_directory() {
    // GIVEN two variants of one tag in the store, the automatic choice in
    // force on a machine where neither is the one the next launch takes
    let dir = tempfile::tempdir().unwrap();
    let vulkan = file(dir.path(), "b7000", Variant::Vulkan);
    let cpu = file(dir.path(), "b7000", Variant::Cpu);
    let config = choosing(dir.path(), "auto");

    // WHEN the Vulkan build is removed
    let (status, body) = delete(
        dir.path(),
        Some(config),
        "/api/v1/engines/llama-server/b7000/vulkan",
    )
    .await;

    // THEN the route answers 204 with no body, the Vulkan directory is gone,
    // and the other variant of the tag is left where it was
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(body, Value::Null);
    assert!(!vulkan.exists());
    assert!(cpu.join("llama-server").is_file());
    let left: Vec<Variant> = installed_packages(&engine_store::store_root(dir.path()))
        .into_iter()
        .map(|p| p.variant)
        .collect();
    assert_eq!(left, [Variant::Cpu]);
}

#[tokio::test]
async fn test_the_build_the_next_launch_takes_is_refused_and_an_older_tag_is_not() {
    // GIVEN two tags of the Vulkan variant, the variant chosen in the
    // settings: the next launch takes the newest
    let dir = tempfile::tempdir().unwrap();
    let older = file(dir.path(), "b6000", Variant::Vulkan);
    let newest = file(dir.path(), "b7000", Variant::Vulkan);
    let config = choosing(dir.path(), "vulkan");

    // WHEN each is removed
    let (refused, body) = delete(
        dir.path(),
        Some(config.clone()),
        "/api/v1/engines/llama-server/b7000/vulkan",
    )
    .await;
    let (removed, _) = delete(
        dir.path(),
        Some(config),
        "/api/v1/engines/llama-server/b6000/vulkan",
    )
    .await;

    // THEN the build in service is refused in 409 with its stable code and
    // stays, and the older tag goes
    assert_eq!(refused, StatusCode::CONFLICT);
    assert_eq!(body["code"], "in_service");
    assert!(body["error"].as_str().unwrap().contains("b7000"));
    assert!(newest.join("llama-server").is_file());
    assert_eq!(removed, StatusCode::NO_CONTENT);
    assert!(!older.exists());
}

#[tokio::test]
async fn test_the_embedded_engine_is_refused_and_an_unknown_build_is_not_found() {
    // GIVEN a store holding one Vulkan build
    let dir = tempfile::tempdir().unwrap();
    let vulkan = file(dir.path(), "b7000", Variant::Vulkan);

    // WHEN the embedded engine, a tag never installed, a variant this build
    // does not know and an unknown engine are removed
    let (embedded, embedded_body) = delete(
        dir.path(),
        None,
        "/api/v1/engines/llama-server/embedded/metal",
    )
    .await;
    let (absent, absent_body) = delete(
        dir.path(),
        None,
        "/api/v1/engines/llama-server/b9999/vulkan",
    )
    .await;
    let (unknown_variant, _) =
        delete(dir.path(), None, "/api/v1/engines/llama-server/b7000/turbo").await;
    let (unknown_engine, _) = delete(dir.path(), None, "/api/v1/engines/ollama/b7000/vulkan").await;

    // THEN the embedded engine is refused in 409, the three others answer
    // 404, and the store is untouched
    assert_eq!(embedded, StatusCode::CONFLICT);
    assert_eq!(embedded_body["code"], "embedded");
    assert_eq!(absent, StatusCode::NOT_FOUND);
    assert_eq!(absent_body["code"], "not_found");
    assert_eq!(unknown_variant, StatusCode::NOT_FOUND);
    assert_eq!(unknown_engine, StatusCode::NOT_FOUND);
    assert!(vulkan.join("llama-server").is_file());
}

#[test]
fn test_a_build_a_process_runs_now_is_in_service() {
    // GIVEN a filed build and the binary inside it
    let dir = tempfile::tempdir().unwrap();
    let path = file(dir.path(), "b7000", Variant::Cpu);
    let package = installed_packages(&engine_store::store_root(dir.path()))
        .pop()
        .unwrap();
    let binary = path.join("llama-server");
    let elsewhere = dir.path().join("other").join("llama-server");

    // WHEN it is weighed against the processes running and the next launch
    let running = is_in_service(&package, &[elsewhere.clone(), binary.clone()], None, None);
    let next = is_in_service(&package, &[], Some(&binary), None);
    let idle = is_in_service(
        &package,
        std::slice::from_ref(&elsewhere),
        Some(&elsewhere),
        None,
    );

    // THEN a process running it, or the next launch taking it, keeps it in
    // service, and neither leaves it free
    assert!(running);
    assert!(next);
    assert!(!idle);
}

#[tokio::test]
async fn test_after_its_removal_a_chosen_variant_falls_back_to_auto_signalled() {
    // GIVEN the Vulkan variant chosen in the settings, two tags of it filed,
    // so the older can go through the route, and the newest removed from the
    // store as a purge removes one
    let dir = tempfile::tempdir().unwrap();
    let root = engine_store::store_root(dir.path());
    file(dir.path(), "b6000", Variant::Vulkan);
    file(dir.path(), "b7000", Variant::Vulkan);
    let config = choosing(dir.path(), "vulkan");
    let (status, _) = delete(
        dir.path(),
        Some(config),
        "/api/v1/engines/llama-server/b6000/vulkan",
    )
    .await;
    let newest = engine_store::newest_installed(&root, Engine::LlamaServer, Variant::Vulkan);
    engine_store::purge::remove_build(&newest.unwrap()).unwrap();

    // WHEN the choice is resolved the way the next launch resolves it
    let next = resolve(Some(&root), Engine::LlamaServer, EngineChoice::Vulkan);

    // THEN no Vulkan build is left, and the launch takes the automatic
    // choice and says so
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(installed_packages(&root).is_empty());
    assert_eq!(next.resolved, Resolved::Automatic);
    assert!(next.fell_back);
}

/// The fields of one event, by name, its message among them.
type EventFields = Vec<(String, String)>;

/// The events emitted on this thread, each as its fields.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<EventFields>>>);

struct Fields(EventFields);

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn test_a_removal_is_logged_with_engine_tag_and_variant() {
    // GIVEN a CPU build out of service and a recorder of the events
    let dir = tempfile::tempdir().unwrap();
    file(dir.path(), "b7000", Variant::Cpu);
    let config = choosing(dir.path(), "auto");
    let recorder = Recorder::default();
    let _guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(recorder.clone()));

    // WHEN it is removed
    let (status, _) = delete(
        dir.path(),
        Some(config),
        "/api/v1/engines/llama-server/b7000/cpu",
    )
    .await;

    // THEN one `engine.removed` event names the engine, the tag and the
    // variant, as structured fields
    assert_eq!(status, StatusCode::NO_CONTENT);
    let events = recorder.0.lock().unwrap();
    let removed: Vec<&EventFields> = events
        .iter()
        .filter(|fields| {
            fields
                .iter()
                .any(|(name, value)| name == "message" && value == "engine.removed")
        })
        .collect();
    assert_eq!(removed.len(), 1, "{events:?}");
    let field = |name: &str| {
        removed[0]
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(field("engine"), Some("llama-server"));
    assert_eq!(field("tag"), Some("b7000"));
    assert_eq!(field("variant"), Some("cpu"));
}
