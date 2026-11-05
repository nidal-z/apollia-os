#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

const X64: [Variant; 5] = [
    Variant::Cuda12,
    Variant::Cuda13,
    Variant::Rocm,
    Variant::Vulkan,
    Variant::Cpu,
];

fn card(vendor: GpuVendor) -> GpuCard {
    GpuCard {
        vendor,
        name: "card".to_owned(),
        vram_gb: Some(16.0),
        driver_version: None,
        driver_major: None,
        compute_capability: None,
        gfx_target: None,
        kfd_accessible: None,
        probe: CardProbe::Complete,
    }
}

fn nvidia(cc: (u8, u8), driver: &str) -> GpuProfile {
    let mut c = card(GpuVendor::Nvidia);
    c.compute_capability = Some(cc);
    c.driver_version = Some(driver.to_owned());
    c.driver_major = driver.split('.').next().and_then(|m| m.parse().ok());
    GpuProfile {
        cards: vec![c],
        vulkan_loader: false,
        rocm_runtime: false,
    }
}

fn amd(gfx: Option<&str>, kfd: Option<bool>, runtime: bool, loader: bool) -> GpuProfile {
    let mut c = card(GpuVendor::Amd);
    c.gfx_target = gfx.map(str::to_owned);
    c.kfd_accessible = kfd;
    GpuProfile {
        cards: vec![c],
        vulkan_loader: loader,
        rocm_runtime: runtime,
    }
}

fn never_asked(_: Variant) -> Option<bool> {
    None
}

/// The ratings of `profile` on `platform`, checked to hold one recommendation.
fn rated(profile: &GpuProfile, platform: &str, offered: &[Variant]) -> Vec<Rating> {
    let ratings = rate(profile, platform, offered, &never_asked);
    let recommended = ratings
        .iter()
        .filter(|r| r.verdict == Verdict::Recommended)
        .count();
    assert_eq!(
        recommended, 1,
        "one recommendation per profile: {ratings:?}"
    );
    ratings
}

/// A verdict and its reason, as a row of a table expects them.
type Expected = (Verdict, Option<Reason>);

fn of(ratings: &[Rating], variant: Variant) -> Expected {
    let r = ratings.iter().find(|r| r.variant == variant).unwrap();
    (r.verdict, r.reason.clone())
}

fn old(have: &str, need: &str) -> Option<Reason> {
    Some(Reason::DriverTooOld {
        have: have.to_owned(),
        need: need.to_owned(),
    })
}

use Verdict::{Compatible, Incompatible, Recommended};

#[test]
fn nvidia_cards_are_rated_by_compute_capability_and_driver() {
    // GIVEN one row per NVIDIA case of the rules: card, driver, platform, and
    //       the verdict expected on CUDA 13 then on CUDA 12
    type Row = (
        &'static str,
        (u8, u8),
        &'static str,
        &'static str,
        (Verdict, Option<Reason>),
        (Verdict, Option<Reason>),
    );
    let rows: Vec<Row> = vec![
        (
            "RTX 4090, R620",
            (8, 9),
            "620.10",
            "linux-x64",
            (Recommended, None),
            (Compatible, None),
        ),
        (
            "RTX 4090, R590",
            (8, 9),
            "590.44",
            "linux-x64",
            (Compatible, old("590.44", "615")),
            (Recommended, None),
        ),
        (
            "RTX 4090, R560",
            (8, 9),
            "560.35",
            "linux-x64",
            (Incompatible, old("560.35", "580")),
            (Recommended, old("560.35", "570.26")),
        ),
        (
            "GTX 1080",
            (6, 1),
            "580.65.06",
            "linux-x64",
            (
                Incompatible,
                Some(Reason::ArchDroppedInCuda13 {
                    cc: "6.1".to_owned(),
                }),
            ),
            (Recommended, None),
        ),
        (
            "RTX 5090, Windows, R620",
            (12, 0),
            "620.10",
            "windows-x64",
            (Recommended, None),
            (Compatible, None),
        ),
        (
            "RTX 5090, Windows, R590",
            (12, 0),
            "590.44",
            "windows-x64",
            (Recommended, old("590.44", "615")),
            (Compatible, None),
        ),
    ];

    for (label, cc, driver, platform, cuda13, cuda12) in rows {
        // WHEN the card is rated
        let ratings = rated(&nvidia(cc, driver), platform, &X64);

        // THEN each CUDA build has the verdict of its row
        assert_eq!(of(&ratings, Variant::Cuda13), cuda13, "{label}, CUDA 13");
        assert_eq!(of(&ratings, Variant::Cuda12), cuda12, "{label}, CUDA 12");
    }
}

#[test]
fn cuda_12_thresholds_follow_the_toolkit_then_the_family_floor() {
    // GIVEN an RTX 4090 (native code in both CUDA 12 builds) and an RTX 2080
    //       (cc 7.5, served by PTX), each with drivers around the thresholds,
    //       CUDA 13 left out so CUDA 12 is rated on its own
    let only_12 = [Variant::Cuda12, Variant::Cpu];
    type Row = (&'static str, (u8, u8), &'static str, &'static str, Expected);
    let rows: Vec<Row> = vec![
        (
            "Linux toolkit",
            (8, 9),
            "570.26",
            "linux-x64",
            (Recommended, None),
        ),
        (
            "Linux below toolkit, native",
            (8, 9),
            "570.25",
            "linux-x64",
            (Recommended, old("570.25", "570.26")),
        ),
        (
            "Linux floor, native",
            (8, 9),
            "525.60",
            "linux-x64",
            (Recommended, old("525.60", "570.26")),
        ),
        (
            "Windows floor, native",
            (8, 9),
            "527.41",
            "windows-x64",
            (Recommended, old("527.41", "551.61")),
        ),
        (
            "Linux below floor",
            (8, 9),
            "520.61",
            "linux-x64",
            (Incompatible, old("520.61", "525")),
        ),
        (
            "Linux below toolkit, PTX",
            (7, 5),
            "560.35",
            "linux-x64",
            (Incompatible, old("560.35", "570.26")),
        ),
        (
            "Windows toolkit",
            (8, 9),
            "551.61",
            "windows-x64",
            (Recommended, None),
        ),
        (
            "Windows below floor",
            (8, 9),
            "527.40",
            "windows-x64",
            (Incompatible, old("527.40", "527.41")),
        ),
        (
            "Windows below toolkit, PTX",
            (7, 5),
            "537.13",
            "windows-x64",
            (Incompatible, old("537.13", "551.61")),
        ),
    ];

    for (label, cc, driver, platform, expected) in rows {
        // WHEN the card is rated
        let ratings = rated(&nvidia(cc, driver), platform, &only_12);

        // THEN CUDA 12 has the verdict of its row
        assert_eq!(of(&ratings, Variant::Cuda12), expected, "{label}");
    }
}

#[test]
fn amd_intel_and_cardless_machines_are_rated() {
    // GIVEN one row per non-NVIDIA case: profile, platform, the variant
    //       expected recommended, and one other expected verdict
    let mut intel = card(GpuVendor::Intel);
    intel.vram_gb = None;
    let intel_with = GpuProfile {
        cards: vec![intel.clone()],
        vulkan_loader: true,
        rocm_runtime: false,
    };
    let intel_without = GpuProfile {
        cards: vec![intel],
        vulkan_loader: false,
        rocm_runtime: false,
    };
    let rows: Vec<(&str, GpuProfile, &str, Variant, Variant, Expected)> = vec![
        (
            "AMD gfx1100 with /dev/kfd",
            amd(Some("gfx1100"), Some(true), true, true),
            "linux-x64",
            Variant::Rocm,
            Variant::Vulkan,
            (Compatible, None),
        ),
        (
            "AMD gfx906",
            amd(Some("gfx906"), Some(true), true, true),
            "linux-x64",
            Variant::Vulkan,
            Variant::Rocm,
            (
                Incompatible,
                Some(Reason::GfxNotInBuild {
                    gfx: "gfx906".to_owned(),
                }),
            ),
        ),
        (
            "AMD under Windows",
            amd(None, None, false, true),
            "windows-x64",
            Variant::Vulkan,
            Variant::Rocm,
            (Compatible, None),
        ),
        (
            "Intel with a loader",
            intel_with,
            "linux-x64",
            Variant::Vulkan,
            Variant::Cuda12,
            (Incompatible, Some(Reason::NoGpu)),
        ),
        (
            "Intel without a loader",
            intel_without,
            "linux-x64",
            Variant::Cpu,
            Variant::Vulkan,
            (Incompatible, Some(Reason::NoVulkanLoader)),
        ),
        (
            "no card",
            GpuProfile::default(),
            "linux-x64",
            Variant::Cpu,
            Variant::Vulkan,
            (Incompatible, Some(Reason::NoGpu)),
        ),
    ];

    for (label, profile, platform, recommended, other, expected) in rows {
        // WHEN the machine is rated
        let ratings = rated(&profile, platform, &X64);

        // THEN the expected variant is the recommendation, and the other one
        //      has its verdict
        assert_eq!(of(&ratings, recommended), (Recommended, None), "{label}");
        assert_eq!(of(&ratings, other), expected, "{label}");
    }
}

#[test]
fn a_machine_without_a_card_rates_every_gpu_build_no_gpu() {
    // GIVEN no card at all
    // WHEN the machine is rated
    let ratings = rated(&GpuProfile::default(), "linux-x64", &X64);

    // THEN the processor build is recommended and every GPU build is no_gpu
    assert_eq!(of(&ratings, Variant::Cpu), (Recommended, None));
    for gpu in [
        Variant::Cuda12,
        Variant::Cuda13,
        Variant::Rocm,
        Variant::Vulkan,
    ] {
        assert_eq!(
            of(&ratings, gpu),
            (Incompatible, Some(Reason::NoGpu)),
            "{gpu:?}"
        );
    }
}

#[test]
fn rocm_under_linux_is_recommended_only_with_the_host_runtime() {
    // GIVEN a gfx1100 card whose /dev/kfd opens and a Vulkan loader, with then
    //       without `libamdhip64` on the host
    let with = amd(Some("gfx1100"), Some(true), true, true);
    let without = amd(Some("gfx1100"), Some(true), false, true);

    // WHEN each machine is rated
    let with = rated(&with, "linux-x64", &X64);
    let without = rated(&without, "linux-x64", &X64);

    // THEN ROCm is recommended with the runtime; without it, it is
    //      incompatible `no_rocm_runtime` and Vulkan comes first
    assert_eq!(of(&with, Variant::Rocm), (Recommended, None));
    assert_eq!(
        of(&without, Variant::Rocm),
        (Incompatible, Some(Reason::NoRocmRuntime))
    );
    assert_eq!(of(&without, Variant::Vulkan), (Recommended, None));
}

#[test]
fn rocm_under_windows_ignores_the_linux_runtime_probe() {
    // GIVEN an AMD card under Windows whose gfx target is known and in the
    //       Windows build, on a machine where no `libamdhip64.so` is looked for
    let profile = amd(Some("gfx1100"), None, false, true);

    // WHEN it is rated
    let ratings = rated(&profile, "windows-x64", &X64);

    // THEN ROCm stays compatible, to be confirmed after installation
    assert_eq!(of(&ratings, Variant::Rocm), (Compatible, None));
}

#[test]
fn builds_the_channel_lacks_for_the_platform_are_not_built_for_it() {
    // GIVEN Linux arm64, where only CUDA 13, Vulkan and the processor build
    //       are published, and an NVIDIA card CUDA 12 would otherwise suit
    let offered = [Variant::Cuda13, Variant::Vulkan, Variant::Cpu];
    let mut profile = nvidia((8, 7), "620.10");
    profile.cards.push(card(GpuVendor::Amd));

    // WHEN the machine is rated
    let ratings = rated(&profile, "linux-arm64", &offered);

    // THEN CUDA 12, ROCm and Metal are not built for the platform
    for variant in [Variant::Cuda12, Variant::Rocm, Variant::Metal] {
        assert_eq!(
            of(&ratings, variant),
            (Incompatible, Some(Reason::NotBuiltForPlatform)),
            "{variant:?}"
        );
    }
    assert_eq!(of(&ratings, Variant::Cuda13), (Recommended, None));
}

#[test]
fn an_nvidia_card_without_a_driver_is_no_nvidia_driver() {
    // GIVEN an NVIDIA card the operating system lists and no driver answers for
    let profile = GpuProfile {
        cards: vec![card(GpuVendor::Nvidia)],
        vulkan_loader: false,
        rocm_runtime: false,
    };

    // WHEN it is rated
    let ratings = rated(&profile, "linux-x64", &X64);

    // THEN both CUDA builds say so, and the processor build is recommended
    for cuda in [Variant::Cuda12, Variant::Cuda13] {
        assert_eq!(
            of(&ratings, cuda),
            (Incompatible, Some(Reason::NoNvidiaDriver))
        );
    }
    assert_eq!(of(&ratings, Variant::Cpu), (Recommended, None));
}

#[test]
fn an_installed_build_that_lists_no_device_is_incompatible() {
    // GIVEN an RTX 4090 that CUDA 13 suits, whose installed CUDA 13 build
    //       listed no device
    let listed = |v: Variant| (v == Variant::Cuda13).then_some(false);

    // WHEN the machine is rated
    let ratings = rate(&nvidia((8, 9), "620.10"), "linux-x64", &X64, &listed);

    // THEN CUDA 13 is incompatible `engine_sees_no_device`, and CUDA 12 takes
    //      the recommendation
    assert_eq!(
        of(&ratings, Variant::Cuda13),
        (Incompatible, Some(Reason::EngineSeesNoDevice))
    );
    assert_eq!(of(&ratings, Variant::Cuda12), (Recommended, None));
}

#[cfg(unix)]
#[test]
fn a_fake_engine_that_lists_nothing_sees_no_device() {
    use std::os::unix::fs::PermissionsExt;

    // GIVEN two fake engines, one listing a CUDA device, one listing none
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let sees = write(
        "sees",
        "echo 'Available devices:'; echo '  CUDA0: NVIDIA GeForce RTX 4090 (24564 MiB, 23000 MiB free)'",
    );
    let blind = write("blind", "echo 'Available devices:'");

    // WHEN each is asked
    // THEN the first sees a device, the second none, and the empty answer is
    //      what rates its variant `engine_sees_no_device`
    assert_eq!(sees_a_device(&sees), Some(true));
    assert_eq!(sees_a_device(&blind), Some(false));
    let listed = |v: Variant| {
        (v == Variant::Cuda13)
            .then(|| sees_a_device(&blind))
            .flatten()
    };
    let ratings = rate(&nvidia((8, 9), "620.10"), "linux-x64", &X64, &listed);
    assert_eq!(
        of(&ratings, Variant::Cuda13),
        (Incompatible, Some(Reason::EngineSeesNoDevice))
    );
}

#[test]
fn driver_versions_compare_field_by_field() {
    // GIVEN driver pairs around each threshold
    // WHEN they are compared
    // THEN a field only matters when the ones before it are equal
    assert!(driver_below("570.25", "570.26"));
    assert!(!driver_below("570.26", "570.26"));
    assert!(!driver_below("570.172.08", "570.26"));
    assert!(driver_below("527.40", "527.41"));
    assert!(!driver_below("580.65.06", "580"));
    assert!(driver_below("579.99", "580"));
}
