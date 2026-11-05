#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use apollia_llm::hardware::GpuProfile;
use serde_json::{json, Value};

use super::hardware::{HardwareSource, HostFacts, ROUTE_DEADLINE};
use super::tests::{fixture, nvidia_facts};
use super::*;
use crate::api::routes_llm::{test_app_state_no_llm, MockBackend};
use crate::engine_store::{import_from_dir, MANIFEST_FILE, SIGNATURE_FILE};

/// A store holding the CUDA 12 build of the fixture manifest, and its key.
fn store_with_cuda(data_dir: &Path) -> String {
    let usb = tempfile::tempdir().unwrap();
    for (to, from) in [
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ] {
        std::fs::write(usb.path().join(to), fixture(from)).unwrap();
    }
    let key = String::from_utf8(fixture("test-dist.key.pub")).unwrap();
    let store = EngineStore {
        root: engine_store::store_root(data_dir),
        trusted_key: key.clone(),
        platform: "linux-x64".to_owned(),
    };
    import_from_dir(&store, usb.path()).unwrap();
    // Kept where a check keeps the verified manifest, which the list reads.
    let kept = store.root.join("manifests");
    std::fs::create_dir_all(&kept).unwrap();
    std::fs::write(kept.join("stable.json"), fixture("stable-5.json")).unwrap();
    std::fs::write(
        kept.join("stable.json.minisig"),
        fixture("stable-5.json.minisig"),
    )
    .unwrap();
    key
}

#[test]
fn test_the_list_carries_the_hardware_the_offered_packages_and_their_ratings() {
    // GIVEN a store whose verified manifest offers a CUDA 12 build for
    //       linux-x64, that build installed, and an RTX 4090 on driver 620
    let dir = tempfile::tempdir().unwrap();
    let key = store_with_cuda(dir.path());
    let facts = nvidia_facts("NVIDIA GeForce RTX 4090");

    // WHEN the view is built and serialised as the route answers it
    let view = engines_view_with(dir.path(), Vec::new(), &facts, Some(&key), "linux-x64").unwrap();
    let body = serde_json::to_value(&view).unwrap();

    // THEN the card is described, the one llama-server package of the
    //      platform is offered and marked installed (the macOS one is left
    //      out), CUDA 12 is recommended, and a build the channel lacks for the
    //      platform says so with its code
    assert_eq!(body["hardware"]["cards"][0]["vendor"], "nvidia");
    assert_eq!(body["hardware"]["cards"][0]["driver_version"], "620.10");
    assert_eq!(body["hardware"]["cards"][0]["compute_capability"], "8.9");
    assert_eq!(
        body["available"],
        json!([{
            "engine": "llama-server", "tag": "b6000", "variant": "cuda-12",
            "size": 725, "released": null, "installed": true
        }])
    );
    let rating = |variant: &str| -> Value {
        body["ratings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["variant"] == variant)
            .unwrap()
            .clone()
    };
    assert_eq!(
        rating("cuda-12"),
        json!({"variant": "cuda-12", "verdict": "recommended", "reason": null})
    );
    assert_eq!(
        rating("cuda-13"),
        json!({"variant": "cuda-13", "verdict": "incompatible",
               "reason": {"code": "not_built_for_platform"}})
    );
    // AND the two fields kept for older readers derive from the profile
    assert_eq!(body["nvidia_card"], "NVIDIA GeForce RTX 4090");
    assert_eq!(body["cuda_proposable"], false);
}

#[test]
fn test_an_installed_build_that_sees_no_device_is_rated_so_by_the_view() {
    // GIVEN the same store, the installed CUDA 12 build having listed no device
    let dir = tempfile::tempdir().unwrap();
    let key = store_with_cuda(dir.path());
    let mut facts = nvidia_facts("NVIDIA GeForce RTX 4090");
    facts.listings.insert(Variant::Cuda12, false);

    // WHEN the view is built
    let view = engines_view_with(dir.path(), Vec::new(), &facts, Some(&key), "linux-x64").unwrap();

    // THEN CUDA 12 is incompatible `engine_sees_no_device`
    let cuda12 = view
        .ratings
        .iter()
        .find(|r| r.variant == Variant::Cuda12)
        .unwrap();
    assert_eq!(
        serde_json::to_value(cuda12).unwrap()["reason"],
        json!({"code": "engine_sees_no_device"})
    );
}

#[test]
fn test_without_a_profile_the_list_gives_no_rating() {
    // GIVEN the same store and a probe that did not answer in time
    let dir = tempfile::tempdir().unwrap();
    let key = store_with_cuda(dir.path());

    // WHEN the view is built
    let view = engines_view_with(
        dir.path(),
        Vec::new(),
        &HostFacts::default(),
        Some(&key),
        "linux-x64",
    )
    .unwrap();

    // THEN the packages are still offered, and nothing is rated on hardware
    //      nobody knows
    assert!(view.hardware.is_none());
    assert_eq!(view.available.len(), 1);
    assert!(view.ratings.is_empty());
}

/// Held by the hanging probe until the test that started it lets it go.
static HUNG: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Set when the hanging probe has returned.
static PROBE_RETURNED: AtomicBool = AtomicBool::new(false);

/// A probe that hangs until [`release_the_probe`], and at most a minute so a
/// failing test never leaves it behind for good.
fn hung_profile() -> GpuProfile {
    let (released, wake) = &HUNG;
    let guard = released.lock().unwrap();
    let _released = wake
        .wait_timeout_while(guard, Duration::from_secs(60), |released| !*released)
        .unwrap();
    PROBE_RETURNED.store(true, Ordering::SeqCst);
    GpuProfile::default()
}

fn release_the_probe() {
    let (released, wake) = &HUNG;
    *released.lock().unwrap() = true;
    wake.notify_all();
}

const HUNG_SOURCE: HardwareSource = HardwareSource {
    profile: hung_profile,
    refresh: hung_profile,
    sees_a_device: no_device,
};

/// Serialises the two tests that share the hanging probe.
static HUNG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn no_device(_: &Path) -> Option<bool> {
    None
}

#[tokio::test]
async fn test_the_hardware_wait_ends_at_the_deadline_when_the_probe_hangs() {
    // GIVEN a hardware probe that does not answer
    let _hung = HUNG_LOCK.lock().await;
    *HUNG.0.lock().unwrap() = false;
    let started = Instant::now();

    // WHEN the facts of the machine are gathered
    let facts = hardware::host_facts(HUNG_SOURCE, false, &[]).await;

    // THEN the wait ends by the deadline, without the profile; nothing but
    //      that wait is timed, so the rest of the route, slower under load,
    //      plays no part in the measure
    let elapsed = started.elapsed();
    release_the_probe();
    assert!(
        elapsed < ROUTE_DEADLINE + Duration::from_secs(1),
        "took {elapsed:?}"
    );
    assert!(facts.profile.is_none());
}

#[tokio::test]
async fn test_the_list_route_answers_without_waiting_for_a_hung_probe() {
    // GIVEN a hardware probe that does not answer until the test lets it go
    let _hung = HUNG_LOCK.lock().await;
    *HUNG.0.lock().unwrap() = false;
    PROBE_RETURNED.store(false, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();

    // WHEN the engines are listed
    let Json(view) = list_engines_with::<MockBackend>(state, HUNG_SOURCE)
        .await
        .unwrap();

    // THEN the route has answered while the probe still hangs, however long
    //      the machine took, without the hardware
    let answered_first = !PROBE_RETURNED.load(Ordering::SeqCst);
    release_the_probe();
    assert!(answered_first, "the route waited for the probe");
    assert!(view.hardware.is_none());
    assert!(view.ratings.is_empty());
}

static REFRESHED: AtomicUsize = AtomicUsize::new(0);

fn counted_refresh() -> GpuProfile {
    REFRESHED.fetch_add(1, Ordering::SeqCst);
    GpuProfile::default()
}

fn kept_profile() -> GpuProfile {
    GpuProfile::default()
}

#[tokio::test]
async fn test_the_check_route_probes_the_hardware_again() {
    // GIVEN a source that counts its refreshes, and a build without a key or
    //       a reachable mirror, so the check itself is refused
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    let counting = HardwareSource {
        profile: kept_profile,
        refresh: counted_refresh,
        sees_a_device: no_device,
    };
    EngineStoreSettings {
        channel: Channel::Stable,
        base_url: Some("http://127.0.0.1:9".to_owned()),
    }
    .save(&engine_store::store_root(dir.path()))
    .unwrap();
    let before = REFRESHED.load(Ordering::SeqCst);

    // WHEN the channel is checked
    let _ = check_engines_with::<MockBackend>(state, counting).await;

    // THEN the hardware was probed again once, before the channel was reached
    assert_eq!(REFRESHED.load(Ordering::SeqCst), before + 1);
}
