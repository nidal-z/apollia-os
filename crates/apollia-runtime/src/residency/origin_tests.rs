//! The models the compatible server loaded: their own idle delay, and keeping
//! only the last one, on a clock the test sets and stand-in processes.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{ModelUnloadReason, RuntimeEvent};
use chrono::Utc;
use tokio::sync::broadcast;

use super::*;
use crate::llama_server::LlamaServerSupervisor;
use crate::local_engine::LocalEngine;

const MINUTE: Duration = Duration::from_secs(60);

/// The delays of the defaults: an hour for the server's models, two for the
/// others, so a sweep between the two tells them apart.
const DELAYS: IdleDelays = IdleDelays {
    general: Some(Duration::from_secs(2 * 3600)),
    server: Some(Duration::from_secs(3600)),
};

fn residency_with_bus() -> (Arc<Residency>, broadcast::Receiver<RuntimeEvent>) {
    let (sender, receiver) = broadcast::channel(16);
    (Residency::new(None, None, Some(sender)), receiver)
}

/// Stand-in processes for `models`, sharing `residency`, the ones named in
/// `by_server` marked as loaded by the compatible server.
async fn llama_with(
    models: &[&str],
    by_server: &[&str],
    residency: &Arc<Residency>,
) -> Arc<LlamaServerSupervisor> {
    let sup = LlamaServerSupervisor::with_stand_in_models(models).await;
    sup.attach_residency(Arc::clone(residency));
    for model in LocalEngine::loaded_models(sup.as_ref()).await {
        if by_server.contains(&model.model_path.as_str()) {
            residency.mark_loaded_by_server(&model.model_path, model.loaded_at);
        }
    }
    sup
}

async fn resident(sup: &LlamaServerSupervisor) -> Vec<String> {
    let mut paths: Vec<String> = LocalEngine::loaded_models(sup)
        .await
        .into_iter()
        .map(|m| m.model_path)
        .collect();
    paths.sort();
    paths
}

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

/// A residency reading the `apollia.toml` written with `content` in `dir`.
fn residency_reading(dir: &std::path::Path, content: &str) -> Arc<Residency> {
    let file = dir.join("apollia.toml");
    std::fs::write(&file, content).expect("write apollia.toml");
    Residency::new(None, Some(file), None)
}

// ── Origin ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_the_origin_is_the_server_s_only_for_the_process_it_loaded() {
    // GIVEN a model the server loaded, and one the application loaded
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(
        &["/models/a.gguf", "/models/b.gguf"],
        &["/models/a.gguf"],
        &residency,
    )
    .await;

    // WHEN their origins are read, then the server's model is relaunched
    let mut origins: Vec<(String, LoadOrigin)> = LocalEngine::loaded_models(sup.as_ref())
        .await
        .iter()
        .map(|m| (m.model_path.clone(), residency.origin_of(m)))
        .collect();
    origins.sort_by(|a, b| a.0.cmp(&b.0));
    let mut relaunched = LocalEngine::loaded_models(sup.as_ref()).await[0].clone();
    relaunched.loaded_at += chrono::Duration::seconds(1);

    // THEN each carries its own, and a later process of the same model is no
    //      longer the server's
    assert_eq!(
        origins,
        vec![
            ("/models/a.gguf".to_owned(), LoadOrigin::Server),
            ("/models/b.gguf".to_owned(), LoadOrigin::App),
        ]
    );
    assert_eq!(residency.origin_of(&relaunched), LoadOrigin::App);
}

// ── The server's idle delay ──────────────────────────────────────────────

#[tokio::test]
async fn test_a_model_the_server_loaded_is_unloaded_after_the_server_delay() {
    // GIVEN a model the server loaded and one the application loaded, both
    //       idle, nothing holding them
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(
        &["/models/a.gguf", "/models/b.gguf"],
        &["/models/a.gguf"],
        &residency,
    )
    .await;

    // WHEN the sweep runs past the server's delay, short of the general one
    let swept = unload_idle_by_origin(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::minutes(61),
        DELAYS,
    )
    .await;

    // THEN only the server's model goes, as idle
    assert_eq!(swept, vec!["/models/a.gguf".to_owned()]);
    assert_eq!(resident(&sup).await, vec!["/models/b.gguf"]);
    assert_eq!(
        unloads(&mut bus),
        vec![("/models/a.gguf".to_owned(), ModelUnloadReason::Idle)]
    );
}

#[tokio::test]
async fn test_a_pinned_model_the_server_loaded_is_never_unloaded_by_the_sweep() {
    // GIVEN a model the server loaded, pinned, idle for a day
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &["/models/a.gguf"], &residency).await;
    residency.set_pinned("/models/a.gguf", true).expect("pin");

    // WHEN the sweep runs
    let swept = unload_idle_by_origin(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        DELAYS,
    )
    .await;

    // THEN it stays
    assert!(swept.is_empty());
    assert_eq!(resident(&sup).await, vec!["/models/a.gguf"]);
}

#[tokio::test]
async fn test_a_model_the_server_loaded_that_is_answering_is_never_unloaded_by_the_sweep() {
    // GIVEN a model the server loaded, serving a response
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &["/models/a.gguf"], &residency).await;
    let answering = sup.lease_for_test("/models/a.gguf").await.expect("lease");

    // WHEN the sweep runs a day later, then once the answer is done
    let during = unload_idle_by_origin(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        DELAYS,
    )
    .await;
    drop(answering);
    let after = unload_idle_by_origin(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        DELAYS,
    )
    .await;

    // THEN it stays while it answers, and goes once it is done
    assert!(during.is_empty());
    assert_eq!(after, vec!["/models/a.gguf".to_owned()]);
}

#[tokio::test]
async fn test_a_model_the_server_loaded_that_a_conversation_holds_is_not_unloaded() {
    // GIVEN a model the server loaded, then designated by an open
    //       conversation of the application
    let (residency, _bus) = residency_with_bus();
    let sup = llama_with(&["/models/a.gguf"], &["/models/a.gguf"], &residency).await;
    residency.open_lease("desk", "c1", "/models/a.gguf");

    // WHEN the sweep runs a day later
    let swept = unload_idle_by_origin(
        &residency,
        sup.as_ref(),
        Utc::now() + chrono::Duration::days(1),
        DELAYS,
    )
    .await;

    // THEN it stays, though its origin is still the server's
    assert!(swept.is_empty());
    let model = &LocalEngine::loaded_models(sup.as_ref()).await[0];
    assert_eq!(residency.origin_of(model), LoadOrigin::Server);
}

#[test]
fn test_the_server_delay_is_read_from_the_server_section() {
    // GIVEN a file setting the server's delay to five minutes, the general
    //       one to thirty
    let dir = tempfile::tempdir().expect("temp dir");
    let residency = residency_reading(
        dir.path(),
        "[llm.residency]\nidle_unload_minutes = 30\n[server]\njit_idle_ttl_minutes = 5\n",
    );

    // WHEN the delays are read
    let delays = residency.idle_delays();

    // THEN each origin has its own
    assert_eq!(delays.server, Some(5 * MINUTE));
    assert_eq!(delays.general, Some(30 * MINUTE));
}

#[test]
fn test_the_server_delay_defaults_to_an_hour() {
    // GIVEN a file that names neither delay
    let dir = tempfile::tempdir().expect("temp dir");
    let residency = residency_reading(dir.path(), "[server]\nenabled = true\n");

    // WHEN the delays are read
    let delays = residency.idle_delays();

    // THEN the server's models go after sixty minutes
    assert_eq!(delays.server, Some(60 * MINUTE));
}

#[test]
fn test_without_auto_unload_the_server_s_models_follow_the_general_delay() {
    // GIVEN a file turning the server's own delay off
    let dir = tempfile::tempdir().expect("temp dir");
    let residency = residency_reading(
        dir.path(),
        "[llm.residency]\nidle_unload_minutes = 30\n\
         [server]\njit_auto_unload = false\njit_idle_ttl_minutes = 5\n",
    );

    // WHEN the delays are read
    let delays = residency.idle_delays();

    // THEN the server's models wait as long as any other
    assert_eq!(delays, IdleDelays::uniform(Some(30 * MINUTE)));
}

// ── Keeping only the last model the server loaded ────────────────────────

#[tokio::test]
async fn test_loading_for_the_server_unloads_its_other_models_and_never_the_app_s() {
    // GIVEN two models the server loaded and one the application loaded,
    //       nothing holding them
    let (residency, mut bus) = residency_with_bus();
    let sup = llama_with(
        &["/models/a.gguf", "/models/b.gguf", "/models/c.gguf"],
        &["/models/a.gguf", "/models/b.gguf"],
        &residency,
    )
    .await;

    // WHEN the server keeps only b, the one it loaded last
    let unloaded = unload_other_server_models(&residency, sup.as_ref(), "/models/b.gguf").await;

    // THEN a goes, for that reason, and c, the application's, stays
    assert_eq!(unloaded, vec!["/models/a.gguf".to_owned()]);
    assert_eq!(
        resident(&sup).await,
        vec!["/models/b.gguf", "/models/c.gguf"]
    );
    assert_eq!(
        unloads(&mut bus),
        vec![(
            "/models/a.gguf".to_owned(),
            ModelUnloadReason::ServerReplaced
        )]
    );
}

#[tokio::test]
async fn test_keeping_the_last_spares_a_pinned_a_held_and_an_answering_model() {
    // GIVEN three other models the server loaded: one pinned, one an open
    //       conversation designates, one answering a request
    let (residency, _bus) = residency_with_bus();
    let models = [
        "/models/a.gguf",
        "/models/b.gguf",
        "/models/c.gguf",
        "/models/d.gguf",
    ];
    let sup = llama_with(&models, &models, &residency).await;
    residency.set_pinned("/models/a.gguf", true).expect("pin");
    residency.open_lease("desk", "c1", "/models/b.gguf");
    let _answering = sup.lease_for_test("/models/c.gguf").await.expect("lease");

    // WHEN the server keeps only d
    let unloaded = unload_other_server_models(&residency, sup.as_ref(), "/models/d.gguf").await;

    // THEN none of them goes
    assert!(unloaded.is_empty());
    assert_eq!(resident(&sup).await, models);
}

#[test]
fn test_keeping_the_last_is_on_by_default_and_read_from_the_file() {
    // GIVEN a residency without a file, and one whose file turns it off
    let dir = tempfile::tempdir().expect("temp dir");
    let default = Residency::new(None, None, None);
    let off = residency_reading(dir.path(), "[server]\njit_keep_last_only = false\n");

    // WHEN the switch is read
    // THEN it is on by default, and off where the file says so
    assert!(default.keep_last_server_model_only());
    assert!(!off.keep_last_server_model_only());
}
