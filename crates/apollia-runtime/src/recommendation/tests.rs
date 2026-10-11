use super::*;
use apollia_llm::hardware::AcceleratorProfile;

/// A machine large enough for the plan to keep several candidates, so the
/// Hub is actually asked.
fn roomy_machine() -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: 64.0,
        available_ram_gb: 64.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 8,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: 38.0,
        devices: Vec::new(),
    }
}

/// A Hub address nothing listens on: the reserved TCP port 1 of the
/// loopback, refused at once, with no port reserved by the test.
const UNREACHABLE_HUB: &str = "http://127.0.0.1:1";

/// A directory holding no accepted index, so the shipped one is read.
const NO_KEPT_INDEX: &str = "/nonexistent/apollia-cards";

#[tokio::test]
async fn an_unreachable_hub_still_renders_the_editorial_selection() {
    // GIVEN a roomy machine and a HuggingFace that does not answer
    let options = ResolveOptions {
        hub_base: Some(UNREACHABLE_HUB.to_owned()),
        ..ResolveOptions::default()
    };

    // WHEN the recommendation is asked for
    let outcome = recommend_for(&roomy_machine(), &options, NO_KEPT_INDEX.as_ref())
        .await
        .expect("an unreachable hub is an outcome, not an error");

    // THEN the outcome says the Hub was unreachable and still carries every
    // family of the curated table, each with a repository to offer
    let manifest = FamilyManifest::embedded().expect("the shipped table loads");
    let RecommendOutcome::Unreachable { selection, .. } = outcome else {
        panic!("expected the unreachable outcome, got {outcome:?}");
    };
    assert_eq!(selection.len(), manifest.families().len());
    assert!(selection.iter().all(|f| f
        .variants
        .iter()
        .all(|v| !v.gguf_repos.is_empty() || !v.mlx_repos.is_empty())));
}

#[test]
fn the_selection_is_serialised_with_the_unreachable_status() {
    // GIVEN an unreachable outcome carrying the selection
    let manifest = FamilyManifest::embedded().expect("the shipped table loads");
    let outcome = RecommendOutcome::Unreachable {
        detail: "offline".to_owned(),
        hardware: HardwareResponse::from(roomy_machine()),
        selection: manifest.editorial_selection(),
    };

    // WHEN it crosses the IPC boundary
    let json = serde_json::to_value(&outcome).expect("the outcome serialises");

    // THEN the interface reads the status and the selection side by side
    assert_eq!(json["status"], "unreachable");
    let families = json["selection"].as_array().expect("selection is a list");
    assert_eq!(families.len(), manifest.families().len());
    assert!(families[0]["variants"][0]["gguf_repos"].is_array());
    assert!(families[0]["variants"][0]["mlx_repos"].is_array());
}

#[tokio::test]
async fn offline_the_selection_is_cut_to_what_fits_when_asked() {
    // GIVEN a small machine and a HuggingFace that does not answer
    let small = HardwareProfile {
        total_ram_gb: 8.0,
        available_ram_gb: 4.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 4,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: 4.8,
        devices: Vec::new(),
    };
    let options = ResolveOptions {
        hub_base: Some(UNREACHABLE_HUB.to_owned()),
        ..ResolveOptions::default()
    };
    let outcome = recommend_for(&small, &options, NO_KEPT_INDEX.as_ref())
        .await
        .expect("an unreachable hub is an outcome, not an error");
    let whole = match &outcome {
        RecommendOutcome::Unreachable { selection, .. } => selection.clone(),
        other => panic!("expected the unreachable outcome, got {other:?}"),
    };

    // WHEN the selection is asked with and without the fit parameter
    let kept = fitting(outcome.clone(), true, &small);
    let untouched = fitting(outcome, false, &small);

    // THEN the fit parameter keeps a non-empty part of it, every size of
    // which fits, and without it nothing is cut
    let RecommendOutcome::Unreachable { selection, .. } = kept else {
        panic!("the outcome keeps its status");
    };
    let count = |s: &[SelectionFamily]| s.iter().map(|f| f.variants.len()).sum::<usize>();
    assert!(count(&selection) > 0);
    assert!(count(&selection) < count(&whole));
    assert!(selection
        .iter()
        .flat_map(|f| f.variants.iter())
        .all(|v| v.fits(&small)));
    let RecommendOutcome::Unreachable { selection, .. } = untouched else {
        panic!("the outcome keeps its status");
    };
    assert_eq!(count(&selection), count(&whole));
}
