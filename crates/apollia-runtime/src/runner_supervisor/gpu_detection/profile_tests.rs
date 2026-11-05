//! The runner's card and backend follow the shared hardware profile.
//!
//! Every profile here is made by hand: nothing probes the machine the tests
//! run on.

use apollia_llm::hardware::{CardProbe, GpuCard, GpuProfile, GpuVendor};

use super::{info_from_profile, GpuInfo, Platform, RunnerBackend, Vendor};

fn card(vendor: GpuVendor, name: &str) -> GpuCard {
    GpuCard {
        vendor,
        name: name.to_owned(),
        vram_gb: None,
        driver_version: None,
        driver_major: None,
        compute_capability: None,
        gfx_target: None,
        kfd_accessible: None,
        probe: CardProbe::Complete,
    }
}

fn nvidia(driver: &str, major: u32) -> GpuCard {
    GpuCard {
        vram_gb: Some(12.0),
        driver_version: Some(driver.to_owned()),
        driver_major: Some(major),
        compute_capability: Some((8, 9)),
        ..card(GpuVendor::Nvidia, "NVIDIA GeForce RTX 4070")
    }
}

fn amd_linux(gfx: Option<&str>, kfd: bool) -> GpuCard {
    GpuCard {
        vram_gb: Some(24.0),
        gfx_target: gfx.map(str::to_owned),
        kfd_accessible: Some(kfd),
        ..card(GpuVendor::Amd, "AMD Radeon RX 7900 XTX")
    }
}

fn amd_windows(name: &str) -> GpuCard {
    GpuCard {
        vram_gb: Some(16.0),
        ..card(GpuVendor::Amd, name)
    }
}

fn profile(cards: Vec<GpuCard>, vulkan_loader: bool) -> GpuProfile {
    GpuProfile {
        cards,
        vulkan_loader,
        rocm_runtime: false,
    }
}

fn backend(cards: Vec<GpuCard>, vulkan_loader: bool, platform: Platform) -> RunnerBackend {
    info_from_profile(&profile(cards, vulkan_loader), platform).recommended_backend
}

#[test]
fn the_recommended_backend_follows_the_profile() {
    // GIVEN profiles made by hand, one per case of the decision, with the
    //       platform each was read on and the backend expected
    let cases: Vec<(&str, Vec<GpuCard>, bool, Platform, RunnerBackend)> = vec![
        (
            "nvidia 550",
            vec![nvidia("550.78", 550)],
            false,
            Platform::Linux,
            RunnerBackend::Cuda,
        ),
        (
            "nvidia 620",
            vec![nvidia("620.10", 620)],
            false,
            Platform::Windows,
            RunnerBackend::Cuda,
        ),
        (
            "nvidia 525",
            vec![nvidia("525.60.11", 525)],
            false,
            Platform::Linux,
            RunnerBackend::Cuda,
        ),
        (
            "nvidia 470 with vulkan",
            vec![nvidia("470.256.02", 470)],
            true,
            Platform::Linux,
            RunnerBackend::Vulkan,
        ),
        (
            "nvidia 470 without vulkan",
            vec![nvidia("470.256.02", 470)],
            false,
            Platform::Linux,
            RunnerBackend::Cpu,
        ),
        (
            "amd linux gfx1100 kfd",
            vec![amd_linux(Some("gfx1100"), true)],
            false,
            Platform::Linux,
            RunnerBackend::Rocm,
        ),
        (
            "amd linux no kfd, vulkan",
            vec![amd_linux(Some("gfx1100"), false)],
            true,
            Platform::Linux,
            RunnerBackend::Vulkan,
        ),
        (
            "amd linux no kfd, no vulkan",
            vec![amd_linux(Some("gfx1100"), false)],
            false,
            Platform::Linux,
            RunnerBackend::Cpu,
        ),
        (
            "amd linux no gfx target",
            vec![amd_linux(None, true)],
            true,
            Platform::Linux,
            RunnerBackend::Vulkan,
        ),
        (
            "amd windows consumer",
            vec![amd_windows("AMD Radeon RX 6900 XT")],
            false,
            Platform::Windows,
            RunnerBackend::Rocm,
        ),
        (
            "amd windows pro",
            vec![amd_windows("AMD Radeon Pro W7900")],
            true,
            Platform::Windows,
            RunnerBackend::Rocm,
        ),
        (
            "intel with vulkan",
            vec![card(GpuVendor::Intel, "Intel(R) UHD Graphics 770")],
            true,
            Platform::Windows,
            RunnerBackend::Vulkan,
        ),
        (
            "intel without vulkan",
            vec![card(GpuVendor::Intel, "Intel Arc A770")],
            false,
            Platform::Linux,
            RunnerBackend::Cpu,
        ),
        (
            "other with vulkan",
            vec![card(GpuVendor::Other, "Moore Threads MTT S80")],
            true,
            Platform::Linux,
            RunnerBackend::Vulkan,
        ),
    ];

    // WHEN the runner's backend is read from each
    // THEN each gets the backend of its case
    for (label, cards, vulkan, platform, expected) in cases {
        assert_eq!(backend(cards, vulkan, platform), expected, "{label}");
    }
}

#[test]
fn rocm_under_windows_asks_for_no_sdk_and_no_professional_card() {
    // GIVEN a consumer Radeon under Windows, whose gfx target is unknown, on
    //       a machine where nothing says a HIP SDK is installed
    let cards = vec![amd_windows("AMD Radeon RX 7800 XT")];

    // WHEN the runner's backend is read
    let chosen = backend(cards, false, Platform::Windows);

    // THEN it is ROCm, where the old allowlist kept it to Radeon Pro and
    //      Instinct with the HIP SDK installed
    assert_eq!(chosen, RunnerBackend::Rocm);
}

#[test]
fn an_amd_card_without_kfd_takes_vulkan_from_the_loader_of_the_profile() {
    // GIVEN the profile of a machine whose Vulkan loader the probe found by
    //       its files (no `vulkaninfo` is run, so none has to be installed),
    //       with an AMD card whose `/dev/kfd` does not open
    let shared = profile(vec![amd_linux(Some("gfx1030"), false)], true);

    // WHEN the runner's card is read
    let info = info_from_profile(&shared, Platform::Linux);

    // THEN it is that card, on Vulkan
    assert_eq!(info.vendor, Vendor::Amd);
    assert_eq!(info.recommended_backend, RunnerBackend::Vulkan);
}

#[test]
fn the_runner_names_the_card_and_the_memory_of_the_profile() {
    // GIVEN an Intel integrated card listed before a 16 GB Radeon and a 24 GB
    //       NVIDIA card
    let shared = profile(
        vec![
            card(GpuVendor::Intel, "Intel(R) UHD Graphics 770"),
            amd_windows("AMD Radeon RX 6900 XT"),
            GpuCard {
                vram_gb: Some(24.0),
                ..nvidia("580.65.06", 580)
            },
        ],
        true,
    );

    // WHEN the runner's card is read
    let info = info_from_profile(&shared, Platform::Windows);

    // THEN it is the NVIDIA card with the memory the profile gives it, in MiB
    assert_eq!(info.vendor, Vendor::Nvidia);
    assert_eq!(info.model, "NVIDIA GeForce RTX 4070");
    assert_eq!(info.memory_total_mb, 24 * 1024);
    assert_eq!(info.recommended_backend, RunnerBackend::Cuda);
}

#[test]
fn the_card_with_the_most_memory_wins_within_a_vendor() {
    // GIVEN two AMD cards under Windows, the second with more memory
    let shared = profile(
        vec![
            amd_windows("AMD Radeon(TM) Graphics"),
            GpuCard {
                vram_gb: Some(20.0),
                ..amd_windows("AMD Radeon RX 7900 XT")
            },
        ],
        false,
    );

    // WHEN the runner's card is read
    let info = info_from_profile(&shared, Platform::Windows);

    // THEN it is the bigger one
    assert_eq!(info.model, "AMD Radeon RX 7900 XT");
    assert_eq!(info.memory_total_mb, 20 * 1024);
}

#[test]
fn a_machine_without_a_card_falls_back_to_the_processor() {
    // GIVEN an empty profile, with or without a Vulkan loader
    for vulkan in [false, true] {
        // WHEN the runner's card is read
        let info = info_from_profile(&profile(Vec::new(), vulkan), Platform::Linux);

        // THEN it is the processor fallback
        let fallback = GpuInfo::cpu_fallback();
        assert_eq!(info.vendor, fallback.vendor);
        assert_eq!(info.model, fallback.model);
        assert_eq!(info.memory_total_mb, 0);
        assert_eq!(info.recommended_backend, RunnerBackend::Cpu);
    }
}
