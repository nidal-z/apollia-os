//! The accelerator profile is built from the cards of the one probe.

use std::cell::Cell;

use super::gpu_probe::{ProbeSources, VulkanSource};
use super::{accelerator_from, AcceleratorProfile, CardProbe, GpuCard, GpuProfile, GpuVendor};

/// Sources that read nothing of the machine.
pub(super) fn no_machine() -> ProbeSources {
    ProbeSources {
        nvidia_smi: None,
        sysfs_root: None,
        windows_adapters: false,
        vulkan: VulkanSource::None,
        ld_library_path: None,
        limit: super::bounded::PROBE_LIMIT,
    }
}

fn card(vendor: GpuVendor, name: &str, vram_gb: Option<f64>, cc: Option<(u8, u8)>) -> GpuCard {
    GpuCard {
        vendor,
        name: name.to_owned(),
        vram_gb,
        driver_version: None,
        driver_major: None,
        compute_capability: cc,
        gfx_target: None,
        kfd_accessible: None,
        probe: CardProbe::Complete,
    }
}

fn profile(cards: Vec<GpuCard>) -> GpuProfile {
    GpuProfile {
        cards,
        vulkan_loader: false,
        rocm_runtime: false,
    }
}

fn engine_listing() -> Option<AcceleratorProfile> {
    Some(AcceleratorProfile::Generic {
        device_name: "Vulkan0 card".to_owned(),
        vram_gb: 8.0,
    })
}

#[test]
fn the_nvidia_card_with_the_most_memory_and_a_known_capability_is_the_cuda_one() {
    // GIVEN two NVIDIA cards with their capability, a bigger one whose driver
    //       refused `compute_cap`, and an engine that would list a card
    let cards = profile(vec![
        card(GpuVendor::Nvidia, "RTX 4070", Some(12.0), Some((8, 9))),
        card(GpuVendor::Nvidia, "RTX 4090", Some(24.0), Some((8, 9))),
        card(GpuVendor::Nvidia, "Old Titan", Some(48.0), None),
    ]);
    let asked = Cell::new(false);

    // WHEN the accelerator is built
    let accelerator = accelerator_from(&cards, || {
        asked.set(true);
        engine_listing()
    });

    // THEN it is the 24 GB card, in CUDA, and the engine was not asked
    match accelerator {
        AcceleratorProfile::Cuda {
            device_name,
            vram_gb,
            compute_capability,
        } => {
            assert_eq!(device_name, "RTX 4090");
            assert_eq!(vram_gb, 24.0);
            assert_eq!(compute_capability, (8, 9));
        }
        other => panic!("expected CUDA, got {other:?}"),
    }
    assert!(!asked.get());
}

#[test]
fn without_a_capability_the_engine_listing_answers() {
    // GIVEN an NVIDIA card whose driver refused `compute_cap`, and an AMD card
    let cards = profile(vec![
        card(GpuVendor::Nvidia, "Old Titan", Some(12.0), None),
        card(GpuVendor::Amd, "Radeon", Some(16.0), None),
    ]);

    // WHEN the accelerator is built, the engine listing a card
    let accelerator = accelerator_from(&cards, engine_listing);

    // THEN the engine's card answers, before the profile's own
    assert!(matches!(
        accelerator,
        AcceleratorProfile::Generic { ref device_name, .. } if device_name == "Vulkan0 card"
    ));
}

#[test]
fn without_an_engine_the_first_card_of_the_profile_is_generic() {
    // GIVEN a 16 GB Radeon and an integrated card with no memory of its own,
    //       and no engine that answers
    let radeon = profile(vec![
        card(GpuVendor::Amd, "AMD Radeon RX 6900 XT", Some(16.0), None),
        card(GpuVendor::Intel, "Intel UHD", None, None),
    ]);
    let integrated = profile(vec![card(GpuVendor::Intel, "Intel UHD", None, None)]);

    // WHEN the accelerator is built for each
    let first = accelerator_from(&radeon, || None);
    let second = accelerator_from(&integrated, || None);

    // THEN the Radeon is sized by its own memory, and the integrated card is
    //      still named, at zero, rather than the machine read as CPU-only
    assert!(matches!(
        first,
        AcceleratorProfile::Generic { ref device_name, vram_gb } if device_name == "AMD Radeon RX 6900 XT" && vram_gb == 16.0
    ));
    assert!(matches!(
        second,
        AcceleratorProfile::Generic { ref device_name, vram_gb } if device_name == "Intel UHD" && vram_gb == 0.0
    ));
}

#[test]
fn a_machine_without_a_card_has_no_accelerator() {
    // GIVEN an empty profile and no engine that answers
    // WHEN the accelerator is built
    let accelerator = accelerator_from(&profile(Vec::new()), || None);

    // THEN there is none
    assert!(matches!(accelerator, AcceleratorProfile::None));
}

#[cfg(unix)]
#[test]
fn the_runner_and_the_accelerator_read_one_probe() {
    use super::bounded::tests::{fake_tool, ANSWER_LIMIT};
    use super::gpu_probe::ProfileCache;

    // GIVEN a `nvidia-smi` that counts its inventory queries in a file and
    //       answers one card, and an empty cache
    let dir = tempfile::tempdir().expect("tempdir");
    let count = dir.path().join("count");
    let tool = fake_tool(
        dir.path(),
        "nvidia-smi",
        &format!(
            r#"case "$1" in
  --query-gpu=name,*) echo x >> '{}'; echo 'NVIDIA GeForce RTX 4090, 580.65.06, 24564' ;;
  --query-gpu=compute_cap) echo '8.9' ;;
  *) exit 2 ;;
esac"#,
            count.display()
        ),
    );
    let sources = ProbeSources {
        nvidia_smi: Some(tool),
        limit: ANSWER_LIMIT,
        ..no_machine()
    };
    let cache = ProfileCache::new();

    // WHEN the accelerator profile is built from the cache, as `detect` does,
    //      and the profile is read again, as the runner does through
    //      `gpu_profile`
    let accelerator = accelerator_from(&cache.get_or_probe(&sources), || None);
    let runner = cache.get_or_probe(&sources);

    // THEN the inventory was asked once for both readings, and they name the
    //      same card with the same memory
    let launches = std::fs::read_to_string(&count).expect("the probe ran");
    assert_eq!(launches.lines().count(), 1, "{launches}");
    match accelerator {
        AcceleratorProfile::Cuda {
            device_name,
            vram_gb,
            ..
        } => {
            assert_eq!(device_name, runner.cards[0].name);
            assert_eq!(Some(vram_gb), runner.cards[0].vram_gb);
        }
        other => panic!("expected CUDA, got {other:?}"),
    }
}
