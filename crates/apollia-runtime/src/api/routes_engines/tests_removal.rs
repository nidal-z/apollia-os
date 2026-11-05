#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_core::config::EngineChoice;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::removal::{is_in_service, weigh, Chosen, Verdict};
use super::*;
use crate::api::routes_llm::{test_app_state_no_llm, MockBackend};
use crate::engine_store::selection::resolve;
use crate::engine_store::{package_dir, EngineChoices, Resolved, RECORD_FILE};

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
async fn test_the_chosen_build_goes_on_confirmation_only_and_an_older_tag_freely() {
    // GIVEN two tags of the Vulkan variant, the variant chosen in the
    // settings: the setting names the newest
    let dir = tempfile::tempdir().unwrap();
    let older = file(dir.path(), "b6000", Variant::Vulkan);
    let newest = file(dir.path(), "b7000", Variant::Vulkan);
    let config = choosing(dir.path(), "vulkan");
    let uri = "/api/v1/engines/llama-server/b7000/vulkan";

    // WHEN the newest is removed without confirmation, then with it, and the
    // older one is removed without confirmation
    let (refused, body) = delete(dir.path(), Some(config.clone()), uri).await;
    let kept = newest.join("llama-server").is_file();
    let (confirmed, _) = delete(
        dir.path(),
        Some(config.clone()),
        &format!("{uri}?confirm=true"),
    )
    .await;
    let (freed, _) = delete(
        dir.path(),
        Some(config.clone()),
        "/api/v1/engines/llama-server/b6000/vulkan",
    )
    .await;

    // THEN the chosen build is refused in 409 `chosen` and stays until the
    // confirmation, which removes it; the older tag, which the choice then
    // names, is refused in its turn, and the setting is left as it was
    assert_eq!(refused, StatusCode::CONFLICT);
    assert_eq!(body["code"], "chosen");
    let message = body["error"].as_str().unwrap();
    assert!(message.contains("b7000") && message.contains("confirm=true"));
    assert!(kept);
    assert_eq!(confirmed, StatusCode::NO_CONTENT);
    assert!(!newest.exists());
    assert_eq!(freed, StatusCode::CONFLICT);
    assert!(older.join("llama-server").is_file());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "[engines]\ngguf = \"vulkan\"\n"
    );
}

#[tokio::test]
async fn test_an_older_tag_of_the_chosen_variant_goes_without_confirmation() {
    // GIVEN two tags of the Vulkan variant, the variant chosen in the
    // settings: the setting names the newest
    let dir = tempfile::tempdir().unwrap();
    let older = file(dir.path(), "b6000", Variant::Vulkan);
    let newest = file(dir.path(), "b7000", Variant::Vulkan);
    let config = choosing(dir.path(), "vulkan");

    // WHEN the older is removed without confirmation
    let (status, body) = delete(
        dir.path(),
        Some(config),
        "/api/v1/engines/llama-server/b6000/vulkan",
    )
    .await;

    // THEN it goes, and the newest stays
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(!older.exists());
    assert!(newest.join("llama-server").is_file());
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
    let (confirmed, confirmed_body) = delete(
        dir.path(),
        None,
        "/api/v1/engines/llama-server/embedded/metal?confirm=true",
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

    // THEN the embedded engine is refused in 409, confirmation or not, the
    // three others answer 404, and the store is untouched
    assert_eq!(embedded, StatusCode::CONFLICT);
    assert_eq!(embedded_body["code"], "embedded");
    assert_eq!(confirmed, StatusCode::CONFLICT);
    assert_eq!(confirmed_body["code"], "embedded");
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

#[test]
fn test_a_build_a_process_runs_stays_in_service_even_confirmed() {
    // GIVEN a filed build, the binary inside it, and another binary
    let dir = tempfile::tempdir().unwrap();
    let path = file(dir.path(), "b7000", Variant::Cpu);
    let package = installed_packages(&engine_store::store_root(dir.path()))
        .pop()
        .unwrap();
    let binary = path.join("llama-server");
    let running = std::slice::from_ref(&binary);

    // WHEN its removal is weighed, chosen or not, confirmed or not
    let chosen_running = weigh(&package, running, (None, None), Chosen::Last, true);
    let chosen_alone = weigh(&package, &[], (Some(&binary), None), Chosen::Last, false);
    let chosen_confirmed = weigh(&package, &[], (Some(&binary), None), Chosen::Last, true);
    let replaced_confirmed = weigh(&package, &[], (Some(&binary), None), Chosen::Replaced, true);
    let automatic_next = weigh(&package, &[], (Some(&binary), None), Chosen::No, true);

    // THEN a process running it refuses it whatever the confirmation; the
    // chosen build asks for one and, confirmed, goes, resetting the setting
    // only when nothing replaces it; the build the automatic choice takes
    // next stays in service
    assert_eq!(chosen_running, Verdict::InService);
    assert_eq!(chosen_alone, Verdict::Chosen(Chosen::Last));
    assert_eq!(chosen_confirmed, Verdict::Remove { reset_choice: true });
    assert_eq!(
        replaced_confirmed,
        Verdict::Remove {
            reset_choice: false
        }
    );
    assert_eq!(automatic_next, Verdict::InService);
}

#[tokio::test]
async fn test_the_last_chosen_build_removed_on_confirmation_resets_the_choice_to_auto() {
    // GIVEN one Vulkan build, the variant chosen in the settings, no process
    // running it, and a recorder of the events
    let dir = tempfile::tempdir().unwrap();
    let root = engine_store::store_root(dir.path());
    let vulkan = file(dir.path(), "b7000", Variant::Vulkan);
    let config = choosing(dir.path(), "vulkan");
    let uri = "/api/v1/engines/llama-server/b7000/vulkan";
    let recorder = Recorder::default();
    let _guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(recorder.clone()));

    // WHEN it is removed without confirmation, then with it, and the next
    // launch reads the settings and resolves its engine
    let (refused, body) = delete(dir.path(), Some(config.clone()), uri).await;
    let before = std::fs::read_to_string(&config).unwrap();
    let (removed, _) = delete(
        dir.path(),
        Some(config.clone()),
        &format!("{uri}?confirm=true"),
    )
    .await;
    let choices = EngineChoices::read(Some(&config), &apollia_core::config::PolicySettings::none());
    let next = resolve(Some(&root), Engine::LlamaServer, choices.gguf);
    let former = resolve(Some(&root), Engine::LlamaServer, EngineChoice::Vulkan);

    // THEN the first is refused in 409 `chosen`, saying the choice goes back
    // to auto, and touches neither the build nor the settings; the confirmed
    // one answers 204, the directory is gone, `engines.gguf` reads `auto`,
    // the next launch takes the automatic choice, a launch still holding the
    // former choice falls back to it flagged, and `engine.removed` carries
    // `choice_reset`
    assert_eq!(refused, StatusCode::CONFLICT);
    assert_eq!(body["code"], "chosen");
    assert!(body["error"].as_str().unwrap().contains("back to auto"));
    assert_eq!(before, "[engines]\ngguf = \"vulkan\"\n");
    assert_eq!(removed, StatusCode::NO_CONTENT);
    assert!(!vulkan.exists());
    assert_eq!(choices.gguf, EngineChoice::Auto);
    assert_eq!(next.resolved, Resolved::Automatic);
    assert!(!next.fell_back);
    assert_eq!(former.resolved, Resolved::Automatic);
    assert!(former.fell_back);
    let events = recorder.0.lock().unwrap();
    let removed_event = events
        .iter()
        .find(|fields| {
            fields
                .iter()
                .any(|(name, value)| name == "message" && value == "engine.removed")
        })
        .unwrap();
    let reset = removed_event
        .iter()
        .find(|(name, _)| name == "choice_reset")
        .map(|(_, value)| value.as_str());
    assert_eq!(reset, Some("true"));
}
