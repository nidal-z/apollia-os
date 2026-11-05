#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use apollia_llm::hardware::{CardProbe, GpuCard, GpuVendor};

use super::*;
use crate::engine_store::compat::{machine_ratings_with, rate};
use crate::engine_store::{
    current_platform, package_dir, Engine, InstalledPackage, Source, RECORD_FILE,
};

/// How many builds the fake listing below was asked about.
static ASKED: AtomicUsize = AtomicUsize::new(0);

/// An Ada Lovelace card (8.9) under driver 620, a Vulkan loader beside it.
fn ada() -> GpuProfile {
    GpuProfile {
        cards: vec![GpuCard {
            vendor: GpuVendor::Nvidia,
            name: "card".to_owned(),
            vram_gb: Some(16.0),
            driver_version: Some("620.10".to_owned()),
            driver_major: Some(620),
            compute_capability: Some((8, 9)),
            gfx_target: None,
            kfd_accessible: None,
            probe: CardProbe::Complete,
        }],
        vulkan_loader: true,
        rocm_runtime: false,
    }
}

/// A listing that sees a device at once, except for the CUDA 13 and ROCm
/// builds, which hang for thirty seconds as a build on a driver in a bad
/// state does.
fn two_builds_hang(binary: &Path) -> Option<bool> {
    ASKED.fetch_add(1, Ordering::SeqCst);
    let path = binary.to_string_lossy();
    if path.contains("cuda-13") || path.contains("rocm") {
        std::thread::sleep(Duration::from_secs(30));
    }
    Some(true)
}

/// A store under `root` holding a verified build of each of `variants`.
fn holding(root: &Path, variants: &[Variant]) {
    for variant in variants {
        let dir = package_dir(root, Engine::LlamaServer, "b7000", *variant);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("llama-server"), b"binary").unwrap();
        let record = InstalledPackage {
            engine: Engine::LlamaServer,
            tag: "b7000".to_owned(),
            variant: *variant,
            platform: "linux-x64".to_owned(),
            sha256: "0".repeat(64),
            source: Source::Imported,
            released: None,
            path: dir.clone(),
        };
        std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&record).unwrap()).unwrap();
    }
}

#[test]
fn test_two_hung_builds_cost_one_deadline_once_and_rate_as_not_asked() {
    // GIVEN a store holding four GPU builds, two of which hang for thirty
    //       seconds when asked for their devices, on an Ada card
    let dir = tempfile::tempdir().unwrap();
    holding(
        dir.path(),
        &[
            Variant::Cuda12,
            Variant::Cuda13,
            Variant::Rocm,
            Variant::Vulkan,
        ],
    );
    let probes = Probes {
        profile: ada,
        listing: two_builds_hang,
    };
    let cache = Listings::new();

    // WHEN the machine is rated, then rated again
    let started = Instant::now();
    let first = machine_ratings_with(dir.path(), probes, &cache, RATING_DEADLINE);
    let first_took = started.elapsed();
    let asked_once = ASKED.load(Ordering::SeqCst);
    let again = Instant::now();
    let second = machine_ratings_with(dir.path(), probes, &cache, RATING_DEADLINE);
    let second_took = again.elapsed();

    // THEN the first rating comes back within the deadline and a second,
    //      having asked the four builds at once; the hung builds are rated as
    //      builds never asked, from the card alone, never as seeing nothing;
    //      the second rating asks no build again, and gives the same ratings
    assert!(
        first_took < RATING_DEADLINE + Duration::from_secs(1),
        "{first_took:?}"
    );
    assert_eq!(asked_once, 4);
    let offered = [
        Variant::Cuda12,
        Variant::Cuda13,
        Variant::Rocm,
        Variant::Vulkan,
    ];
    let answered = |v: Variant| (v == Variant::Cuda12 || v == Variant::Vulkan).then_some(true);
    let expected = rate(&ada(), &current_platform(), &offered, &answered);
    assert_eq!(first, expected);
    assert_eq!(ASKED.load(Ordering::SeqCst), 4);
    assert!(second_took < Duration::from_secs(1), "{second_took:?}");
    assert_eq!(second, expected);

    // AND forgetting the answers, as the channel check does, leaves none kept
    cache.forget();
    let kept: Vec<_> = [
        Variant::Cuda12,
        Variant::Cuda13,
        Variant::Rocm,
        Variant::Vulkan,
    ]
    .into_iter()
    .filter_map(|v| {
        let binary = package_dir(dir.path(), Engine::LlamaServer, "b7000", v).join("llama-server");
        cache.kept(&binary)
    })
    .collect();
    assert!(kept.is_empty());
}

#[test]
fn test_a_hung_build_is_kept_as_not_asked_and_an_answer_as_given() {
    // GIVEN two builds, one that answers and one that hangs
    let dir = tempfile::tempdir().unwrap();
    let (answers, hangs) = (dir.path().join("cuda-12"), dir.path().join("cuda-13"));
    let probes = Probes {
        profile: ada,
        listing: |binary| {
            if binary.ends_with("cuda-13") {
                std::thread::sleep(Duration::from_secs(30));
            }
            Some(false)
        },
    };
    let cache = Listings::new();

    // WHEN they are asked under a short deadline
    let facts = facts_within(
        vec![
            (Variant::Cuda12, answers.clone()),
            (Variant::Cuda13, hangs.clone()),
        ],
        probes,
        &cache,
        Duration::from_millis(500),
    );

    // THEN the profile and the answer come back, the hung build is absent,
    //      and both are kept: the answer as given, the hung build as not run
    assert!(facts.profile.is_some());
    assert_eq!(facts.listings.get(&Variant::Cuda12), Some(&false));
    assert_eq!(facts.listings.get(&Variant::Cuda13), None);
    assert_eq!(cache.kept(&answers), Some(Some(false)));
    assert_eq!(cache.kept(&hangs), Some(None));
}
