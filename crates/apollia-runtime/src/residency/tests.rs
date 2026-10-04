use std::sync::Arc;
use std::time::Duration;

use apollia_core::{ModelUnloadReason, RuntimeEvent};
use chrono::Utc;
use tokio::sync::broadcast;

use super::*;
use crate::llama_server::LlamaServerSupervisor;
use crate::local_engine::LocalEngine;
use crate::mlx_engine::MlxEngineSupervisor;

const HOUR: Duration = Duration::from_secs(3600);

/// A residency without `system.db` nor `apollia.toml`, announcing on a bus
/// the test reads.
fn residency_with_bus() -> (Arc<Residency>, broadcast::Receiver<RuntimeEvent>) {
    let (sender, receiver) = broadcast::channel(16);
    (Residency::new(None, None, Some(sender)), receiver)
}

/// Stand-in `llama-server` processes for `models`, sharing `residency`.
async fn llama_with(models: &[&str], residency: &Arc<Residency>) -> Arc<LlamaServerSupervisor> {
    let sup = LlamaServerSupervisor::with_stand_in_models(models).await;
    sup.attach_residency(Arc::clone(residency));
    sup
}

async fn resident<E: LocalEngine>(engine: &E) -> Vec<String> {
    let mut paths: Vec<String> = engine
        .loaded_models()
        .await
        .into_iter()
        .map(|m| m.model_path)
        .collect();
    paths.sort();
    paths
}

/// The reasons announced on the bus so far, by model.
fn unloads(receiver: &mut broadcast::Receiver<RuntimeEvent>) -> Vec<(String, ModelUnloadReason)> {
    let mut seen = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let RuntimeEvent::LlmModelUnloaded {
            model_path, reason, ..
        } = event
        {
            seen.push((model_path, reason));
        }
    }
    seen
}

// ── Leases and pins ──────────────────────────────────────────────────────

#[test]
fn test_a_model_is_held_while_one_conversation_designates_it() {
    // GIVEN two conversations of one client on the same model
    let residency = Residency::new(None, None, None);
    residency.open_lease("desk", "c1", "/models/a.gguf");
    residency.open_lease("desk", "c2", "/models/a.gguf");

    // WHEN they leave the screen one after the other
    let first = residency.close_lease("desk", "c1");
    let held_between = residency.is_held("/models/a.gguf");
    let last = residency.close_lease("desk", "c2");

    // THEN the model is held until the last one goes, which releases it
    assert_eq!(first, None);
    assert!(held_between);
    assert_eq!(last.as_deref(), Some("/models/a.gguf"));
    assert!(!residency.is_held("/models/a.gguf"));
}

#[test]
fn test_a_conversation_moved_to_another_model_releases_the_first() {
    // GIVEN a conversation on model A
    let residency = Residency::new(None, None, None);
    residency.open_lease("desk", "c1", "/models/a.gguf");

    // WHEN the same conversation now designates model B
    let released = residency.open_lease("desk", "c1", "/models/b.gguf");

    // THEN A is released and B held, by one lease
    assert_eq!(released.as_deref(), Some("/models/a.gguf"));
    assert_eq!(residency.leases_on("/models/a.gguf"), 0);
    assert_eq!(residency.leases_on("/models/b.gguf"), 1);
}

#[test]
fn test_a_client_gone_releases_its_leases_and_no_other_client_s() {
    // GIVEN two clients, the first alone on A, both on B
    let residency = Residency::new(None, None, None);
    residency.open_lease("desk", "c1", "/models/a.gguf");
    residency.open_lease("desk", "c2", "/models/b.gguf");
    residency.open_lease("other", "c9", "/models/b.gguf");

    // WHEN the first client goes away
    let released = residency.release_client("desk");

    // THEN A is released, B stays held by the other client
    assert_eq!(released, vec!["/models/a.gguf".to_owned()]);
    assert!(residency.is_held("/models/b.gguf"));
}

#[test]
fn test_a_pin_is_kept_in_system_db_across_a_restart() {
    // GIVEN a residency over a system.db, with one model pinned and one
    //       pinned then unpinned
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("system.db");
    let before = Residency::new(Some(db.clone()), None, None);
    before.set_pinned("/models/a.gguf", true).expect("pin");
    before.set_pinned("/models/b.gguf", true).expect("pin");
    before.set_pinned("/models/b.gguf", false).expect("unpin");

    // WHEN the runtime restarts on the same file
    let after = Residency::new(Some(db), None, None);

    // THEN the pin is read back, the unpin too
    assert_eq!(after.pins(), vec!["/models/a.gguf".to_owned()]);
    assert!(after.is_held("/models/a.gguf"));
}

// ── The idle sweep, on a clock the test sets ─────────────────────────────

#[tokio::test]
async fn test_an_idle_model_nothing_holds_is_unloaded_past_the_delay() {
    // GIVEN a resident model, no lease, no pin, no request
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;

    // WHEN the sweep runs one minute short of the delay, then just past it
    let early = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::minutes(59),
        HOUR,
    )
    .await;
    let late = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::minutes(61),
        HOUR,
    )
    .await;

    // THEN the first sweep keeps it, the second unloads it as idle
    assert!(early.is_empty());
    assert_eq!(late, vec!["/models/a.gguf".to_owned()]);
    assert!(resident(sup.as_ref()).await.is_empty());
    assert_eq!(
        unloads(&mut bus),
        vec![("/models/a.gguf".to_owned(), ModelUnloadReason::Idle)]
    );
}

#[tokio::test]
async fn test_a_pinned_model_is_never_unloaded_by_the_sweep() {
    // GIVEN a pinned resident model, idle for a day
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.set_pinned("/models/a.gguf", true).expect("pin");

    // WHEN the sweep runs
    let unloaded = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        HOUR,
    )
    .await;

    // THEN it stays
    assert!(unloaded.is_empty());
    assert_eq!(resident(sup.as_ref()).await, vec!["/models/a.gguf"]);
    assert!(unloads(&mut bus).is_empty());
}

#[tokio::test]
async fn test_a_model_an_open_conversation_designates_is_not_unloaded_by_the_sweep() {
    // GIVEN a resident model an open conversation designates, idle for a day
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.open_lease("desk", "c1", "/models/a.gguf");

    // WHEN the sweep runs
    let unloaded = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        HOUR,
    )
    .await;

    // THEN it stays
    assert!(unloaded.is_empty());
    assert_eq!(resident(sup.as_ref()).await, vec!["/models/a.gguf"]);
}

#[tokio::test]
async fn test_a_model_answering_a_request_is_unloaded_by_neither_rule() {
    // GIVEN a resident model serving a request, its last conversation open
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.open_lease("desk", "c1", "/models/a.gguf");
    let answering = sup.lease_for_test("/models/a.gguf").await.expect("lease");

    // WHEN its last conversation closes, and the sweep runs a day later
    let released = residency.close_lease("desk", "c1").expect("released");
    let at_release = unload_if_released(
        &residency,
        sup.as_ref(),
        &released,
        ModelUnloadReason::LastConversation,
    )
    .await;
    let swept = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        HOUR,
    )
    .await;

    // THEN it stays while it answers, and the sweep takes it once it is done
    assert!(!at_release);
    assert!(swept.is_empty());
    assert_eq!(resident(sup.as_ref()).await, vec!["/models/a.gguf"]);
    drop(answering);
    let after = unload_idle(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        HOUR,
    )
    .await;
    assert_eq!(after, vec!["/models/a.gguf".to_owned()]);
}

// ── The last conversation closed ─────────────────────────────────────────

#[tokio::test]
async fn test_closing_the_last_conversation_unloads_the_model_at_once() {
    // GIVEN a resident model, just used, one conversation on it
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.open_lease("desk", "c1", "/models/a.gguf");

    // WHEN that conversation leaves the screen
    let released = residency.close_lease("desk", "c1").expect("released");
    let unloaded = unload_if_released(
        &residency,
        sup.as_ref(),
        &released,
        ModelUnloadReason::LastConversation,
    )
    .await;

    // THEN the model is unloaded without waiting for the delay
    assert!(unloaded);
    assert!(resident(sup.as_ref()).await.is_empty());
    assert_eq!(
        unloads(&mut bus),
        vec![(
            "/models/a.gguf".to_owned(),
            ModelUnloadReason::LastConversation
        )]
    );
}

#[tokio::test]
async fn test_closing_the_last_conversation_keeps_a_pinned_model() {
    // GIVEN a pinned resident model, one conversation on it
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.set_pinned("/models/a.gguf", true).expect("pin");
    residency.open_lease("desk", "c1", "/models/a.gguf");

    // WHEN that conversation leaves the screen
    let released = residency.close_lease("desk", "c1").expect("released");
    let unloaded = unload_if_released(
        &residency,
        sup.as_ref(),
        &released,
        ModelUnloadReason::LastConversation,
    )
    .await;

    // THEN it stays
    assert!(!unloaded);
    assert_eq!(resident(sup.as_ref()).await, vec!["/models/a.gguf"]);
}

// ── The delay, read at every sweep ───────────────────────────────────────

#[tokio::test]
async fn test_changing_the_key_changes_the_next_decision_without_a_restart() {
    // GIVEN a resident model idle for ninety minutes, and an apollia.toml
    //       without the key
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("apollia.toml");
    std::fs::write(&config, "[llm]\ndefault = \"local\"\n").expect("write");
    let residency = Residency::new(None, Some(config.clone()), None);
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    let now = Utc::now() + chrono::Duration::minutes(90);
    let default_delay = residency.idle_unload();

    // WHEN the key is set to never, a sweep runs, then to two hours, then to
    //      thirty minutes, a sweep running after each
    std::fs::write(&config, "[llm.residency]\nidle_unload_minutes = 0\n").expect("write");
    let never = residency.idle_unload();
    std::fs::write(&config, "[llm.residency]\nidle_unload_minutes = 120\n").expect("write");
    let two_hours = residency.idle_unload().expect("a delay");
    let kept = unload_idle(&residency, sup.as_ref(), now, two_hours).await;
    std::fs::write(&config, "[llm.residency]\nidle_unload_minutes = 30\n").expect("write");
    let half_hour = residency.idle_unload().expect("a delay");
    let unloaded = unload_idle(&residency, sup.as_ref(), now, half_hour).await;

    // THEN the default is an hour, zero means never, and the same idle model
    //      is kept under two hours and unloaded under thirty minutes
    assert_eq!(default_delay, Some(HOUR));
    assert_eq!(never, None);
    assert!(kept.is_empty());
    assert_eq!(unloaded, vec!["/models/a.gguf".to_owned()]);
}

// ── A client that goes away ──────────────────────────────────────────────

#[tokio::test]
async fn test_a_client_that_disconnects_releases_its_leases_and_unloads_the_model() {
    // GIVEN a client holding the only conversation on a resident model, and
    //       the guard its event stream keeps
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &residency).await;
    residency.open_lease("desk", "c1", "/models/a.gguf");
    let guard = ClientLeases::new("desk".to_owned(), Some(Arc::clone(&sup)), None).expect("guard");

    // WHEN its stream ends, and the unload it hands off is announced
    drop(guard);
    let announced = bus.recv().await.expect("an unload is announced");

    // THEN the lease is gone and the model unloaded as its last conversation
    //      closed
    assert!(!residency.is_held("/models/a.gguf"));
    assert!(resident(sup.as_ref()).await.is_empty());
    assert!(
        matches!(
            &announced,
            RuntimeEvent::LlmModelUnloaded {
                reason: ModelUnloadReason::LastConversation,
                ..
            }
        ),
        "{announced:?}"
    );
}

// ── The MLX engine follows the same rules ────────────────────────────────

#[tokio::test]
async fn test_mlx_models_follow_the_same_residency_rules() {
    // GIVEN three stand-in MLX models sharing the residency: one pinned, one
    //       a conversation designates, one answering a request, and one held
    //       by nothing
    let (residency, mut bus) = residency_with_bus();
    let mlx = MlxEngineSupervisor::with_stand_in_models(&[
        "/models/pinned-mlx",
        "/models/open-mlx",
        "/models/busy-mlx",
        "/models/idle-mlx",
    ])
    .await;
    mlx.attach_residency(Arc::clone(&residency));
    residency
        .set_pinned("/models/pinned-mlx", true)
        .expect("pin");
    residency.open_lease("desk", "c1", "/models/open-mlx");
    let answering = mlx.lease_for_test("/models/busy-mlx").await.expect("lease");

    // WHEN the sweep runs a day later, then the open conversation closes
    let swept = unload_idle(
        &residency,
        mlx.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        HOUR,
    )
    .await;
    let released = residency.close_lease("desk", "c1").expect("released");
    let closed = unload_if_released(
        &residency,
        mlx.as_ref(),
        &released,
        ModelUnloadReason::LastConversation,
    )
    .await;

    // THEN the sweep takes the idle one only, the closed conversation takes
    //      its model at once, and the pinned and busy ones stay
    assert_eq!(swept, vec!["/models/idle-mlx".to_owned()]);
    assert!(closed);
    assert_eq!(
        resident(mlx.as_ref()).await,
        vec!["/models/busy-mlx", "/models/pinned-mlx"]
    );
    assert_eq!(
        unloads(&mut bus),
        vec![
            ("/models/idle-mlx".to_owned(), ModelUnloadReason::Idle),
            (
                "/models/open-mlx".to_owned(),
                ModelUnloadReason::LastConversation
            ),
        ]
    );
    drop(answering);
}
