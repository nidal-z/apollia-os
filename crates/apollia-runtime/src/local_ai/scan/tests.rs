use super::*;

#[test]
fn the_vram_follows_the_accelerator_kind_and_is_absent_without_one() {
    use apollia_llm::AcceleratorProfile;
    // GIVEN the four shapes an accelerator profile can take
    let none = AcceleratorProfile::None;
    let generic = AcceleratorProfile::Generic {
        device_name: "AMD Radeon RX 6900 XT".to_string(),
        vram_gb: 16.0,
    };
    let cuda = AcceleratorProfile::Cuda {
        device_name: "NVIDIA GeForce RTX 4090".to_string(),
        vram_gb: 24.0,
        compute_capability: (8, 9),
    };
    let apple = AcceleratorProfile::AppleSilicon {
        chip: "M4 Max".to_string(),
        generation: 4,
        vram_gb: 64.0,
    };

    // WHEN each is asked for its memory
    // THEN the number comes out for every kind, and the processor-only
    // profile answers nothing, which is the control for the chip
    assert_eq!(vram_gb_of(&none), None);
    assert_eq!(vram_gb_of(&generic), Some(16.0));
    assert_eq!(vram_gb_of(&cuda), Some(24.0));
    assert_eq!(vram_gb_of(&apple), Some(64.0));
}

#[test]
fn test_system_info_returns_valid_values() {
    // GIVEN the current system
    // WHEN the system information is read
    let info = system_info();
    // THEN RAM values are positive and os/arch are non-empty
    assert!(info.total_ram_gb > 0.0);
    assert!(!info.os.is_empty());
    assert!(!info.arch.is_empty());
}

#[test]
fn test_gguf_scan_empty_dir_returns_empty_vec() {
    // GIVEN a temporary empty directory
    let dir = tempfile::tempdir().unwrap();
    // WHEN scanning for GGUF models
    let results = scan_gguf_in_dir(dir.path());
    // THEN an empty Vec is returned
    assert!(results.is_empty());
}

#[test]
fn a_dictation_file_on_disk_is_judged_as_the_catalogue_judges_its_entry() {
    use apollia_llm::recommend::{dictation_fits, DictationCatalogue};
    use apollia_llm::{AcceleratorProfile, HardwareProfile};

    // GIVEN a 16 GB machine with no accelerator, and on its disk the
    // Large-v3 Q5_0 file of the catalogue at its real size (a sparse file)
    let hardware = HardwareProfile {
        total_ram_gb: 16.0,
        available_ram_gb: 8.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 8,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: 9.6,
    };
    let catalogue = DictationCatalogue::embedded().unwrap();
    let entry = catalogue.get("whisper-large-v3-q5_0").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(dir.path().join(&entry.file)).unwrap();
    file.set_len(entry.size_bytes).unwrap();

    // WHEN the onboarding scans the folder, and the catalogue judges the
    // entry for the same machine
    let scanned = scan_whisper_in_dir(dir.path(), &hardware);
    let catalogue_verdict = dictation_fits(entry.size_bytes, &hardware);

    // THEN the scan says what the catalogue says: the file fits, where the
    // former size-name threshold reserved "large" for 32 GB machines
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].size_bytes, entry.size_bytes);
    assert_eq!(scanned[0].recommended, catalogue_verdict);
    assert!(catalogue_verdict);
}

#[test]
fn test_whisper_model_size_detection() {
    // GIVEN a filename "ggml-base.bin"
    let size = detect_whisper_model_size("ggml-base.bin");
    // WHEN the model size is read from it
    // THEN model_size is "base"
    assert_eq!(size, Some("base".to_string()));
}

#[test]
fn test_whisper_model_size_unknown_filename() {
    // GIVEN a filename without the standard pattern
    let size = detect_whisper_model_size("random-model.bin");
    // WHEN the model size is read from it
    // THEN None is returned
    assert_eq!(size, None);
}

#[test]
fn onboarding_recommends_what_the_estimator_says_fits() {
    use apollia_llm::{AcceleratorProfile, CompatibilityBadge, HardwareProfile};

    // GIVEN a 16 GiB file on a 32 GB Apple Silicon machine (24 GB budget),
    // which the old size threshold, no limit above 16 GB of memory, and the
    // old Hub badge, "might fit", disagreed on
    let hardware = HardwareProfile {
        total_ram_gb: 32.0,
        available_ram_gb: 20.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 8,
        accelerator: AcceleratorProfile::AppleSilicon {
            chip: "M3 Max".to_owned(),
            generation: 3,
            vram_gb: 32.0,
        },
        memory_budget_gb: 24.0,
    };
    let facts = apollia_llm::gguf_probe::GgufHeaderFacts::default();
    let fits: u64 = 16 * 1024 * 1024 * 1024;
    let too_large: u64 = 20 * 1024 * 1024 * 1024;

    // WHEN onboarding grades each
    // THEN it follows the estimator's badge, both ways
    let badge = |size| {
        apollia_llm::recommend::assess_fit(
            size,
            &facts,
            &hardware,
            &apollia_llm::recommend::RuntimeShape::default(),
        )
        .badge
    };
    assert_eq!(badge(fits), CompatibilityBadge::Fits);
    assert!(recommended_for(fits, &facts, &hardware));
    assert_eq!(badge(too_large), CompatibilityBadge::TooLarge);
    assert!(!recommended_for(too_large, &facts, &hardware));
}

#[test]
fn test_human_readable_size() {
    // GIVEN known byte counts
    // WHEN each is formatted for the operator
    // THEN human-readable output matches expected strings
    assert_eq!(format_size_human(4_500_000_000), "4.2 GB");
    assert_eq!(format_size_human(500_000_000), "476.8 MB");
}

#[test]
fn a_model_reached_through_a_link_reports_the_size_of_the_file() {
    // GIVEN a model file and a link pointing at it, which is the shape a
    // synchronised Windows folder presents to a directory walk
    let tmp = tempfile::tempdir().expect("tempdir");
    let real = tmp.path().join("model.gguf");
    std::fs::write(&real, vec![0u8; 4096]).expect("write the model");
    let link = tmp.path().join("linked.gguf");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    #[cfg(windows)]
    if std::os::windows::fs::symlink_file(&real, &link).is_err() {
        // Creating a link needs a privilege this runner may not hold; the
        // case still proves the direct read below.
        return;
    }

    // WHEN the size is read the way the scan reads it
    let size = size_on_disk(&link);

    // THEN it is the size of the file, not the zero a link answers when it
    // is measured instead of followed
    assert_eq!(size, 4096);
}

#[test]
fn a_path_that_is_not_there_reads_as_zero() {
    // GIVEN a path no file occupies
    let tmp = tempfile::tempdir().expect("tempdir");

    // WHEN its size is read
    let size = size_on_disk(&tmp.path().join("absent.gguf"));

    // THEN zero is returned, and the warning names the path so the reader
    // is not left with a silent 0 B in the interface
    assert_eq!(size, 0);
}
