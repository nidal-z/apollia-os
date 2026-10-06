use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_core::{ModelUnloadReason, RuntimeEvent};
use tokio::sync::broadcast;

use crate::llama_server::test_support::supervisor_with_ceiling;
use crate::llama_server::{fake_engine, recorded_gguf, LlamaServerError, LoadSettings};
use crate::local_engine::LoadConsent;
use crate::residency::Residency;

use super::super::LlamaServerSupervisor;

const HALF_GIB: u64 = 512 * 1024 * 1024;

/// A model file of its own, in a folder of its own under `dir`.
fn model_in(dir: &Path, name: &str) -> String {
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    recorded_gguf(&folder, HALF_GIB).display().to_string()
}

/// A supervisor without a fixed ceiling, launching the stand-in engine, whose
/// guardrail holds every load to `ceiling_of(one model)`: what two models of
/// the size `model_in` writes take, and less than three.
async fn engine_room_for_two(
    dir: &Path,
) -> (
    LlamaServerSupervisor,
    Arc<Residency>,
    broadcast::Receiver<RuntimeEvent>,
    [String; 3],
) {
    let mut sup = supervisor_with_ceiling(1);
    sup.max_loaded = None;
    sup.bin_path = fake_engine(dir);
    let config: PathBuf = dir.join("apollia.toml");
    sup.config_file = Some(config.clone());
    let models = [model_in(dir, "a"), model_in(dir, "b"), model_in(dir, "c")];
    let one = sup
        .estimate_explicit(&models[0], &LoadSettings::default())
        .await
        .expect("an estimate")
        .total_gb;
    std::fs::write(
        &config,
        format!(
            "[llm.guardrails]\nlevel = \"ceiling\"\nceiling_gb = {}\n",
            one * 2.5
        ),
    )
    .unwrap();
    let (events, bus) = broadcast::channel(16);
    let residency = Residency::new(None, None, Some(events));
    sup.attach_residency(Arc::clone(&residency));
    (sup, residency, bus, models)
}

async fn load(sup: &LlamaServerSupervisor, model: &str) -> Result<(), LlamaServerError> {
    sup.ensure_model("m", model.to_owned(), None, LoadConsent::Unconfirmed)
        .await
        .map(drop)
}

async fn resident(sup: &LlamaServerSupervisor) -> Vec<String> {
    let mut paths: Vec<String> = sup
        .loaded_models()
        .await
        .into_iter()
        .map(|m| m.model_path)
        .collect();
    paths.sort();
    paths
}

#[cfg(unix)]
#[tokio::test]
async fn test_two_models_that_fit_stay_loaded_and_a_third_is_refused_by_the_guardrail() {
    // GIVEN room under the guardrail for two models, and two conversations
    //       each designating one of them
    let dir = tempfile::tempdir().unwrap();
    let (sup, residency, _bus, [a, b, c]) = engine_room_for_two(dir.path()).await;
    residency.open_lease("desk", "c1", &a);
    residency.open_lease("desk", "c2", &b);

    // WHEN both are loaded, then a third
    load(&sup, &a).await.expect("the first fits");
    load(&sup, &b).await.expect("the second fits beside it");
    let third = load(&sup, &c).await;

    // THEN the first two stay loaded together, and the third is refused by
    //      the guardrail rather than by a fixed count
    assert!(
        matches!(third, Err(LlamaServerError::GuardrailRefused(_))),
        "{third:?}"
    );
    let mut both = vec![a, b];
    both.sort();
    assert_eq!(resident(&sup).await, both);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_resident_model_nothing_holds_gives_its_memory_to_a_new_one() {
    // GIVEN room for two models, both loaded, the first used least recently,
    //       neither designated by a conversation nor pinned
    let dir = tempfile::tempdir().unwrap();
    let (sup, _residency, mut bus, [a, b, c]) = engine_room_for_two(dir.path()).await;
    load(&sup, &a).await.expect("the first fits");
    load(&sup, &b).await.expect("the second fits beside it");

    // WHEN a third is loaded
    load(&sup, &c).await.expect("room is made");

    // THEN the least recently used one was unloaded to make room, and only it
    let mut kept = vec![b, c];
    kept.sort();
    assert_eq!(resident(&sup).await, kept);
    let announced = bus.try_recv().expect("an unload is announced");
    assert!(
        matches!(
            &announced,
            RuntimeEvent::LlmModelUnloaded { model_path, reason: ModelUnloadReason::MakeRoom, .. }
                if *model_path == a
        ),
        "{announced:?}"
    );
    sup.shutdown_in_place().await;
}

/// Turn `[llm.residency] unload_before_load` on in the file the engine reads.
fn unload_before_load_on(dir: &Path) {
    let config = dir.join("apollia.toml");
    let mut content = std::fs::read_to_string(&config).unwrap();
    content.push_str("\n[llm.residency]\nunload_before_load = true\n");
    std::fs::write(&config, content).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn test_unload_before_load_unloads_the_unpinned_models_before_a_load() {
    // GIVEN room for two models, both loaded, the first pinned, the second
    //       designated by an open conversation, and the rule turned on
    let dir = tempfile::tempdir().unwrap();
    let (sup, residency, mut bus, [a, b, c]) = engine_room_for_two(dir.path()).await;
    residency.set_pinned(&a, true).expect("pinned in memory");
    residency.open_lease("desk", "c1", &b);
    load(&sup, &a).await.expect("the first fits");
    load(&sup, &b).await.expect("the second fits beside it");
    unload_before_load_on(dir.path());

    // WHEN a third is loaded, which would fit beside one of them only
    load(&sup, &c).await.expect("the third loads");

    // THEN the unpinned one was unloaded first, although it fitted and a
    //      conversation designated it, and the pinned one stays
    let mut kept = vec![a, c];
    kept.sort();
    assert_eq!(resident(&sup).await, kept);
    let announced = bus.try_recv().expect("an unload is announced");
    assert!(
        matches!(
            &announced,
            RuntimeEvent::LlmModelUnloaded { model_path, reason: ModelUnloadReason::MakeRoom, .. }
                if *model_path == b
        ),
        "{announced:?}"
    );
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_unload_before_load_leaves_a_model_answering_a_request() {
    // GIVEN one model loaded and answering a request, and the rule turned on
    let dir = tempfile::tempdir().unwrap();
    let (sup, _residency, _bus, [a, b, _c]) = engine_room_for_two(dir.path()).await;
    let answering = sup
        .ensure_model("m", a.clone(), None, LoadConsent::Unconfirmed)
        .await
        .expect("the first fits");
    unload_before_load_on(dir.path());

    // WHEN a second is loaded
    load(&sup, &b).await.expect("the second fits beside it");

    // THEN the one in the middle of its answer was not stopped
    let mut both = vec![a, b];
    both.sort();
    assert_eq!(resident(&sup).await, both);
    drop(answering);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_unload_before_load_stops_nothing_when_the_new_model_is_refused() {
    // GIVEN two pinned models that fill the guardrail, an unpinned third
    //       being impossible beside them, and the rule turned on
    let dir = tempfile::tempdir().unwrap();
    let (sup, residency, _bus, [a, b, c]) = engine_room_for_two(dir.path()).await;
    residency.set_pinned(&a, true).expect("pinned in memory");
    residency.set_pinned(&b, true).expect("pinned in memory");
    load(&sup, &a).await.expect("the first fits");
    load(&sup, &b).await.expect("the second fits beside it");
    unload_before_load_on(dir.path());

    // WHEN the third is asked for
    let third = load(&sup, &c).await;

    // THEN it is refused by the guardrail, the pinned ones counted
    assert!(
        matches!(third, Err(LlamaServerError::GuardrailRefused(_))),
        "{third:?}"
    );
    let mut both = vec![a, b];
    both.sort();
    assert_eq!(resident(&sup).await, both);
    sup.shutdown_in_place().await;
}
