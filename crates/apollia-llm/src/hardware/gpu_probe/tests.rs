use super::*;

/// Sources that read nothing of the machine: no NVIDIA tool, an empty root.
fn fake_sources(root: &std::path::Path) -> ProbeSources {
    ProbeSources {
        nvidia_smi: None,
        sysfs_root: Some(root.to_path_buf()),
        windows_adapters: false,
        vulkan: VulkanSource::Linux(root.to_path_buf()),
        limit: PROBE_LIMIT,
    }
}

fn amd_card(root: &std::path::Path, index: u32) {
    let device = root.join(format!("sys/class/drm/card{index}/device"));
    std::fs::create_dir_all(&device).expect("drm dir");
    std::fs::write(device.join("vendor"), "0x1002\n").expect("vendor");
}

#[test]
fn the_profile_is_kept_until_it_is_refreshed() {
    // GIVEN a fake machine with one AMD card, and an empty cache
    let root = tempfile::tempdir().expect("tempdir");
    amd_card(root.path(), 0);
    let sources = fake_sources(root.path());
    let cache = ProfileCache::new();
    let first = cache.get_or_probe(&sources);

    // WHEN a second card appears, then the cache is read, then refreshed
    amd_card(root.path(), 1);
    let kept = cache.get_or_probe(&sources);
    let refreshed = cache.refresh(&sources);
    let after = cache.get_or_probe(&sources);

    // THEN the read before the refresh is the kept profile, and the refresh
    //      sees the new card and keeps it in turn
    assert_eq!(first.cards.len(), 1);
    assert_eq!(kept, first);
    assert_eq!(refreshed.cards.len(), 2);
    assert_eq!(after, refreshed);
}

#[test]
fn the_profile_reports_the_vulkan_loader() {
    // GIVEN a fake root with the library and a manifest
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(root.path().join("usr/lib")).expect("lib dir");
    std::fs::write(root.path().join("usr/lib/libvulkan.so.1"), "").expect("lib");
    std::fs::create_dir_all(root.path().join("etc/vulkan/icd.d")).expect("icd dir");
    std::fs::write(root.path().join("etc/vulkan/icd.d/intel_icd.json"), "{}").expect("icd");

    // WHEN the profile is probed
    let profile = probe_with(&fake_sources(root.path()));

    // THEN it says a loader is there
    assert!(profile.vulkan_loader);
    assert!(profile.cards.is_empty());
}

#[test]
fn vendors_read_from_their_pci_identifiers() {
    // GIVEN the three vendors of the engine's backends and another
    // WHEN their identifiers are read
    // THEN each is named, and the unknown one is `Other`
    assert_eq!(GpuVendor::from_pci_id(0x10de), GpuVendor::Nvidia);
    assert_eq!(GpuVendor::from_pci_id(0x1002), GpuVendor::Amd);
    assert_eq!(GpuVendor::from_pci_id(0x8086), GpuVendor::Intel);
    assert_eq!(GpuVendor::from_pci_id(0x1414), GpuVendor::Other);
}

#[cfg(unix)]
#[test]
fn a_hung_nvidia_smi_gives_the_profile_within_six_seconds() {
    use crate::hardware::bounded::tests::fake_tool;
    use std::time::Instant;

    // GIVEN a `nvidia-smi` that sleeps thirty seconds, beside an AMD card the
    //       operating system lists, under the real limit of five seconds
    let tools = tempfile::tempdir().expect("tempdir");
    let tool = fake_tool(tools.path(), "nvidia-smi", "exec sleep 30");
    let root = tempfile::tempdir().expect("tempdir");
    amd_card(root.path(), 0);
    let sources = ProbeSources {
        nvidia_smi: Some(tool),
        ..fake_sources(root.path())
    };
    let started = Instant::now();

    // WHEN the profile is probed
    let profile = probe_with(&sources);

    // THEN it comes back in under six seconds, the NVIDIA card marked as a
    //      probe that hung, the AMD card still listed
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(6), "took {elapsed:?}");
    assert_eq!(profile.cards.len(), 2);
    assert_eq!(profile.cards[0].vendor, GpuVendor::Nvidia);
    assert_eq!(profile.cards[0].probe, CardProbe::TimedOut);
    assert_eq!(profile.cards[1].vendor, GpuVendor::Amd);
    assert_eq!(profile.cards[1].probe, CardProbe::Complete);
}
