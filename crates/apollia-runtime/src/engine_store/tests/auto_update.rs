//! The automatic update of the chosen engines, against the fake server and
//! the signed fixture manifests of the store tests.

use super::*;
use crate::engine_store::auto_update::check_and_update;
use crate::engine_store::selection::{chosen_variants, EngineChoices};
use apollia_core::config::EngineChoice;

/// A store holding the b6000 CUDA build, on a channel that now announces
/// b7000, with the count of archives requested so far.
async fn store_behind_its_channel() -> (Fake, EngineStoreSettings, tempfile::TempDir, EngineStore) {
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    install_package(&s, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap();
    fake.publish("stable-6.json", "stable-6.json.minisig");
    (fake, settings, dir, s)
}

fn newest_cuda_tag(s: &EngineStore) -> String {
    newest_installed(&s.root, Engine::LlamaServer, Variant::Cuda)
        .unwrap()
        .tag
}

const CUDA: (Engine, Variant) = (Engine::LlamaServer, Variant::Cuda);

#[tokio::test]
async fn test_an_announced_update_of_a_chosen_engine_is_downloaded_and_verified() {
    // GIVEN b6000 in service, a channel announcing b7000, automatic update on
    let (fake, settings, _dir, s) = store_behind_its_channel().await;
    let before = fake.archive_hits();

    // WHEN the channel is checked
    let report = check_and_update(&s, &settings, true, &[CUDA])
        .await
        .unwrap();

    // THEN b7000 was downloaded once, verified and installed, and serves
    assert_eq!(fake.archive_hits() - before, 1);
    assert_eq!(report.installed.len(), 1);
    assert_eq!(report.installed[0].tag, "b7000");
    assert_eq!(report.installed[0].source, Source::Downloaded);
    assert!(report.refused.is_empty());
    assert_eq!(newest_cuda_tag(&s), "b7000");
}

#[tokio::test]
async fn test_with_automatic_update_off_the_update_is_only_announced() {
    // GIVEN the same store and channel, automatic update off
    let (fake, settings, _dir, s) = store_behind_its_channel().await;
    let before = fake.archive_hits();

    // WHEN the channel is checked
    let report = check_and_update(&s, &settings, false, &[CUDA])
        .await
        .unwrap();

    // THEN the update is announced, no archive is requested, b6000 stays
    let cuda = report
        .updates
        .iter()
        .find(|u| u.variant == Variant::Cuda)
        .unwrap();
    assert!(cuda.update_available);
    assert_eq!(cuda.latest_tag, "b7000");
    assert!(report.installed.is_empty());
    assert_eq!(fake.archive_hits(), before);
    assert_eq!(newest_cuda_tag(&s), "b6000");
}

#[tokio::test]
async fn test_an_update_of_an_engine_that_is_not_chosen_is_only_announced() {
    // GIVEN the same store and channel, automatic update on, and settings
    // that keep the embedded engine in service
    let (fake, settings, _dir, s) = store_behind_its_channel().await;
    let before = fake.archive_hits();
    let choices = EngineChoices {
        gguf: EngineChoice::Embedded,
        ..EngineChoices::default()
    };
    let chosen = chosen_variants(&s.root, choices, || true);

    // WHEN the channel is checked
    let report = check_and_update(&s, &settings, true, &chosen)
        .await
        .unwrap();

    // THEN nothing is chosen in the store, so nothing is downloaded
    assert!(chosen.is_empty());
    assert!(report.installed.is_empty());
    assert_eq!(fake.archive_hits(), before);
    assert_eq!(newest_cuda_tag(&s), "b6000");
}

#[tokio::test]
async fn test_an_update_is_never_installed_without_verification() {
    // GIVEN automatic update on, and a channel whose manifest does not carry
    // its own signature
    let (fake, settings, _dir, s) = store_behind_its_channel().await;
    fake.publish("stable-6.json", "stable-5.json.minisig");
    let before = fake.archive_hits();

    // WHEN the channel is checked
    let unsigned = check_and_update(&s, &settings, true, &[CUDA]).await;

    // THEN the check is refused before any archive is requested
    assert!(matches!(unsigned, Err(EngineStoreError::Signature { .. })));
    assert_eq!(fake.archive_hits(), before);

    // GIVEN the signed manifest again, and an archive altered on the mirror
    fake.publish("stable-6.json", "stable-6.json.minisig");
    let mut altered = fixture("llama-b7000-cuda.zip");
    let last = altered.len() - 1;
    altered[last] ^= 0xff;
    fake.put("/engines-stable/llama-b7000-cuda.zip", altered);

    // WHEN the channel is checked
    let report = check_and_update(&s, &settings, true, &[CUDA])
        .await
        .unwrap();

    // THEN the update is refused on its digest and b6000 stays in service
    assert!(report.installed.is_empty());
    assert!(matches!(
        report.refused.as_slice(),
        [(
            Engine::LlamaServer,
            Variant::Cuda,
            EngineStoreError::DigestMismatch { .. }
        )]
    ));
    assert_eq!(newest_cuda_tag(&s), "b6000");
}

#[test]
fn test_the_chosen_engines_are_the_store_builds_the_settings_put_in_service() {
    // GIVEN a store holding a CUDA build
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    install_fixture(
        &s,
        "stable-5.json",
        "stable-5.json.minisig",
        "llama-b6000-cuda.zip",
    );
    let with = |gguf| EngineChoices {
        gguf,
        ..EngineChoices::default()
    };

    // WHEN the chosen builds are listed under each setting
    let automatic_with_card = chosen_variants(&s.root, with(EngineChoice::Auto), || true);
    let automatic_without = chosen_variants(&s.root, with(EngineChoice::Auto), || false);
    let named = chosen_variants(&s.root, with(EngineChoice::Cuda), || false);
    let removed = chosen_variants(&s.root, with(EngineChoice::Vulkan), || true);

    // THEN the automatic choice follows the card, a named variant is chosen
    // whatever the card, and a variant since removed is the automatic choice
    assert_eq!(automatic_with_card, [CUDA]);
    assert!(automatic_without.is_empty());
    assert_eq!(named, [CUDA]);
    assert_eq!(removed, [CUDA]);
}
