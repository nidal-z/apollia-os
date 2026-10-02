//! The dictation category of the registry: its entries, the one verdict a
//! dictation file gets, and the recommendation by machine.

use super::*;
use crate::hardware::AcceleratorProfile;

/// The size of an 8B language model at Q4_K_M, the one the table runs beside.
const LANGUAGE_MODEL_BYTES: u64 = 4_920_000_000;

fn catalogue() -> DictationCatalogue {
    DictationCatalogue::embedded().expect("the shipped dictation entries must load")
}

/// A machine with no accelerator and `ram_gb` of memory, sixty percent of it
/// budgeted for models, as the hardware probe budgets it.
fn host_only(ram_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb / 2.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 8,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: ram_gb * 0.60,
    }
}

#[test]
fn the_footprint_is_the_file_plus_one_gibibyte() {
    // GIVEN the margin written here, and the size of the Large-v3 Q5_0 file
    let margin_gb = 1.0;
    let size_bytes = 1_081_140_203u64;

    // WHEN the footprint is computed
    let footprint = dictation_footprint_gb(size_bytes);

    // THEN it is the file in gibibytes plus that margin, and nothing else
    assert!((DICTATION_MARGIN_GB - margin_gb).abs() < f64::EPSILON);
    let expected = size_bytes as f64 / (1024.0 * 1024.0 * 1024.0) + margin_gb;
    assert!(
        (footprint - expected).abs() < 1e-12,
        "{footprint} vs {expected}"
    );
}

#[test]
fn the_category_holds_every_size_and_its_published_quantisations() {
    // GIVEN the shipped table
    let catalogue = catalogue();

    // WHEN its files are listed
    let files: Vec<&str> = catalogue.models().iter().map(|m| m.file.as_str()).collect();

    // THEN base, small, medium and large-v3 are there with their quantised
    // files, and the French fine-tune the command line downloads by default
    for file in [
        "ggml-base.bin",
        "ggml-base-q5_1.bin",
        "ggml-base-q8_0.bin",
        "ggml-small.bin",
        "ggml-small-q5_1.bin",
        "ggml-small-q8_0.bin",
        "ggml-medium.bin",
        "ggml-medium-q5_0.bin",
        "ggml-medium-q8_0.bin",
        "ggml-large-v3.bin",
        "ggml-large-v3-q5_0.bin",
        "ggml-model-q5_0.bin",
    ] {
        assert!(files.contains(&file), "{file} is missing from {files:?}");
    }
    let french = catalogue
        .get("whisper-large-v3-french-q5_0")
        .expect("the French default is an entry");
    assert_eq!(french.repo, "bofenghuang/whisper-large-v3-french");
    assert_eq!(
        catalogue
            .by_url(&french.download_url())
            .map(|m| m.id.as_str()),
        Some("whisper-large-v3-french-q5_0")
    );
}

#[test]
fn an_entry_without_a_full_digest_or_a_bin_file_is_refused() {
    // GIVEN two tables, one entry each, one with a short digest and one whose
    // file is a GGUF
    let entry = |sha: &str, file: &str| {
        format!(
            "[[dictation]]\nid = \"x\"\nlabel = \"X\"\npublisher = \"p\"\nreleased = \"2024-01-01\"\n\
             license = \"mit\"\nrepo = \"o/r\"\nfile = \"{file}\"\nquantization = \"F16\"\n\
             size_bytes = 10\nsha256 = \"{sha}\"\ncard = \"c\"\nsource = \"s\"\n"
        )
    };
    let good = "a".repeat(64);

    // WHEN each is loaded
    let short = DictationCatalogue::from_toml(&entry("abc", "ggml-x.bin"));
    let gguf = DictationCatalogue::from_toml(&entry(&good, "x.gguf"));
    let fine = DictationCatalogue::from_toml(&entry(&good, "ggml-x.bin"));

    // THEN the two faults are refused and the control loads
    assert!(matches!(short, Err(ManifestError::InvalidDictation { .. })));
    assert!(matches!(gguf, Err(ManifestError::InvalidDictation { .. })));
    assert_eq!(fine.expect("control loads").models().len(), 1);
}

#[test]
fn the_recommendation_follows_the_machine_in_a_table() {
    // GIVEN three machines of 8, 16 and 32 GB with no accelerator, each
    // running the same 8B language model
    let catalogue = catalogue();
    let table = [
        (8.0, "whisper-tiny-q5_1", "no_room_beside"),
        (16.0, "whisper-large-v3-turbo", "fits_beside"),
        (32.0, "whisper-large-v3-french", "fits_beside"),
    ];

    for (ram_gb, expected, reason) in table {
        // WHEN each asks for its dictation model
        let recommendation = catalogue
            .recommend(&host_only(ram_gb), Some(LANGUAGE_MODEL_BYTES))
            .expect("the category is not empty");

        // THEN the 8 GB machine has no room beside the language model and is
        // offered the smallest file, the 16 GB one the largest file that
        // still fits, and the 32 GB one the largest of all, the French
        // fine-tune first on a tie in size
        assert_eq!(recommendation.model, expected, "{ram_gb} GB");
        let kind = serde_json::to_value(&recommendation.reason).expect("serialise")["kind"]
            .as_str()
            .map(str::to_owned);
        assert_eq!(kind.as_deref(), Some(reason), "{ram_gb} GB");
        let chosen = catalogue.get(expected).expect("entry");
        let fits = dictation_fits(chosen.size_bytes, &host_only(ram_gb));
        assert!(fits, "the recommended file fits {ram_gb} GB on its own");
    }
}

#[test]
fn a_file_too_large_for_the_machine_does_not_fit() {
    // GIVEN a machine of 4 GB, 2.4 GB budgeted, and the two largest files
    let small_machine = host_only(4.0);
    let catalogue = catalogue();
    let large = catalogue.get("whisper-large-v3").expect("entry");
    let turbo_q5 = catalogue.get("whisper-large-v3-turbo-q5_0").expect("entry");

    // WHEN each is judged
    // THEN the full Large-v3 (3.9 GiB with its margin) is refused and the
    // quantised Turbo (1.5 GiB) fits
    assert!(!dictation_fits(large.size_bytes, &small_machine));
    assert!(dictation_fits(turbo_q5.size_bytes, &small_machine));
}
