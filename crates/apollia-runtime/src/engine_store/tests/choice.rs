//! Which installed `llama-server` build each choice takes, by the ratings of
//! the machine.

use super::variants::Recorder;
use super::*;
use crate::engine_store::compat::machines::{amd_with_vulkan, nvidia_on};
use crate::engine_store::compat::{Rating, Verdict};
use crate::engine_store::selection::{
    chosen_variants, resolve, EngineChoices, Resolution, Resolved,
};
use apollia_core::config::EngineChoice;

/// File under `root` a verified build of `variant` with a binary, as the
/// store does once an archive is extracted.
fn file(root: &Path, variant: Variant) {
    let dir = package_dir(root, Engine::LlamaServer, "b7000", variant);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("llama-server"), b"binary").unwrap();
    let record = InstalledPackage {
        engine: Engine::LlamaServer,
        tag: "b7000".to_owned(),
        variant,
        platform: "linux-x64".to_owned(),
        sha256: "0".repeat(64),
        source: Source::Imported,
        released: None,
        path: dir.clone(),
    };
    std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&record).unwrap()).unwrap();
}

/// A store holding a build of each of `variants`.
fn holding(variants: &[Variant]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for variant in variants {
        file(dir.path(), *variant);
    }
    dir
}

/// The variant the automatic choice launches from the store under `root`.
fn launched(root: &Path, ratings: Vec<Rating>) -> Option<Variant> {
    let binary = preferred_llama_server(root, "llama-server", || ratings)?;
    let variant = binary.parent()?.file_name()?.to_str()?.to_owned();
    Variant::from_wire(&variant)
}

/// An Ada Lovelace card (8.9) under driver 620.
fn ada() -> Vec<Rating> {
    nvidia_on((8, 9), "620.10")
}

/// A Pascal card (6.1) under driver 620, which CUDA 13 no longer builds for.
fn pascal() -> Vec<Rating> {
    nvidia_on((6, 1), "620.10")
}

fn stored(resolution: &Resolution) -> Option<Variant> {
    match &resolution.resolved {
        Resolved::Stored(package) => Some(package.variant),
        _ => None,
    }
}

#[test]
fn test_the_automatic_choice_launches_the_recommended_build_when_it_is_installed() {
    // GIVEN an Ada card under driver 620, and a store holding CUDA 12 and CUDA
    // 13, then one holding CUDA 12 only
    let both = holding(&[Variant::Cuda12, Variant::Cuda13]);
    let twelve = holding(&[Variant::Cuda12]);

    // WHEN the automatic choice picks a build in each
    let from_both = launched(both.path(), ada());
    let from_twelve = launched(twelve.path(), ada());
    let chosen = chosen_variants(both.path(), EngineChoices::default(), ada);

    // THEN CUDA 13, the recommended one, is launched where it is installed,
    // CUDA 12 otherwise, and the automatic update follows the same build
    assert_eq!(from_both, Some(Variant::Cuda13));
    assert_eq!(from_twelve, Some(Variant::Cuda12));
    assert_eq!(chosen, [(Engine::LlamaServer, Variant::Cuda13)]);
}

#[test]
fn test_an_amd_card_launches_the_installed_vulkan_build() {
    // GIVEN an AMD card the ROCm build does not target, and a store holding
    // the Vulkan and the processor builds
    let dir = holding(&[Variant::Vulkan, Variant::Cpu]);

    // WHEN the automatic choice picks a build
    let taken = launched(dir.path(), amd_with_vulkan());

    // THEN the Vulkan build is launched
    assert_eq!(taken, Some(Variant::Vulkan));
}

#[test]
fn test_without_the_recommended_build_the_best_compatible_one_is_taken_in_order() {
    // GIVEN an Ada card, whose recommended CUDA 13 is not installed, and a
    // store holding the processor, Vulkan and CUDA 12 builds, all compatible
    let dir = holding(&[Variant::Cpu, Variant::Vulkan, Variant::Cuda12]);
    let ratings = ada();

    // WHEN the automatic choice picks a build, then picks among each smaller
    // set of compatible builds
    let all = wanted_llama_variant(&ratings, &[Variant::Cpu, Variant::Vulkan, Variant::Cuda12]);
    let no_cuda = wanted_llama_variant(&ratings, &[Variant::Cpu, Variant::Vulkan]);
    let cpu = wanted_llama_variant(&ratings, &[Variant::Cpu]);

    // THEN CUDA 12 comes before Vulkan, Vulkan before the processor
    assert_eq!(launched(dir.path(), ada()), Some(Variant::Cuda12));
    assert_eq!(all, Some(Variant::Cuda12));
    assert_eq!(no_cuda, Some(Variant::Vulkan));
    assert_eq!(cpu, Some(Variant::Cpu));
}

#[test]
fn test_the_rocm_build_comes_between_cuda_12_and_vulkan_in_the_automatic_order() {
    // GIVEN ratings that call CUDA 12, ROCm, Vulkan and the processor build
    // compatible and recommend none of them, and a store holding all four
    let ratings: Vec<Rating> = [
        Variant::Cuda12,
        Variant::Rocm,
        Variant::Vulkan,
        Variant::Cpu,
    ]
    .into_iter()
    .map(|variant| Rating {
        variant,
        verdict: Verdict::Compatible,
        reason: None,
    })
    .collect();
    let dir = holding(&[
        Variant::Cpu,
        Variant::Vulkan,
        Variant::Rocm,
        Variant::Cuda12,
    ]);

    // WHEN the automatic choice picks a build, then picks without CUDA 12,
    // then without CUDA 12 and ROCm
    let all = launched(dir.path(), ratings.clone());
    let no_cuda = wanted_llama_variant(&ratings, &[Variant::Cpu, Variant::Vulkan, Variant::Rocm]);
    let no_rocm = wanted_llama_variant(&ratings, &[Variant::Cpu, Variant::Vulkan]);

    // THEN CUDA 12 is taken first, ROCm next, Vulkan after it
    assert_eq!(all, Some(Variant::Cuda12));
    assert_eq!(no_cuda, Some(Variant::Rocm));
    assert_eq!(no_rocm, Some(Variant::Vulkan));
}

#[test]
fn test_an_incompatible_build_is_never_taken_automatically_and_nothing_keeps_the_embedded_engine() {
    // GIVEN a Pascal card, a store holding only CUDA 13, which dropped its
    // architecture, and an empty store
    let thirteen = holding(&[Variant::Cuda13]);
    let empty = tempfile::tempdir().unwrap();
    let mut asked = false;

    // WHEN the automatic choice picks a build in each
    let from_thirteen = launched(thirteen.path(), pascal());
    let from_empty = preferred_llama_server(empty.path(), "llama-server", || {
        asked = true;
        pascal()
    });
    let chosen = chosen_variants(thirteen.path(), EngineChoices::default(), pascal);

    // THEN no build is taken, which keeps the embedded engine, and the empty
    // store does not even probe the machine
    assert_eq!(from_thirteen, None);
    assert_eq!(from_empty, None);
    assert!(!asked);
    assert!(chosen.is_empty());
}

#[test]
fn test_the_cuda_choice_takes_the_best_installed_cuda_build_by_the_ratings() {
    // GIVEN a store holding CUDA 12 and CUDA 13, on an Ada card and on a
    // Pascal card
    let dir = holding(&[Variant::Cuda12, Variant::Cuda13]);
    let resolve_on = |ratings: Vec<Rating>| {
        resolve(
            Some(dir.path()),
            Engine::LlamaServer,
            EngineChoice::Cuda,
            &ratings,
        )
    };

    // WHEN `engines.gguf = "cuda"` is resolved on each
    let on_ada = resolve_on(ada());
    let on_pascal = resolve_on(pascal());

    // THEN the Ada card takes CUDA 13, the Pascal card CUDA 12, the one CUDA
    // build it can run
    assert_eq!(stored(&on_ada), Some(Variant::Cuda13));
    assert_eq!(stored(&on_pascal), Some(Variant::Cuda12));
    assert!(!on_ada.fell_back && !on_pascal.fell_back);
}

#[test]
fn test_an_incompatible_variant_chosen_explicitly_is_launched_and_reported() {
    // GIVEN a Pascal card, and a store holding CUDA 13 only
    let dir = holding(&[Variant::Cuda13]);
    let recorder = Recorder::default();

    // WHEN the settings name CUDA 13
    let resolution = {
        let _guard = crate::test_support::recording(recorder.clone());
        resolve(
            Some(dir.path()),
            Engine::LlamaServer,
            EngineChoice::Cuda13,
            &pascal(),
        )
    };
    let chosen = chosen_variants(
        dir.path(),
        EngineChoices {
            gguf: EngineChoice::Cuda13,
            ..EngineChoices::default()
        },
        pascal,
    );

    // THEN the CUDA 13 build is taken as chosen, the resolution says it is
    // incompatible, and a structured event names it so
    assert_eq!(stored(&resolution), Some(Variant::Cuda13));
    assert!(resolution.incompatible);
    assert!(!resolution.fell_back);
    assert_eq!(chosen, [(Engine::LlamaServer, Variant::Cuda13)]);
    let events = recorder.events.lock().unwrap();
    let reported: Vec<&str> = events
        .iter()
        .filter(|e| {
            e.get("message").map(String::as_str)
                == Some("engine_store.selection.incompatible_choice")
        })
        .map(|e| e["variant"].as_str())
        .collect();
    assert_eq!(reported, ["cuda-13"]);
}
