//! The bases a channel is read from: the registry of Apollia, then its GitHub
//! mirror, or a mirror set in the settings alone. Two fake servers stand in
//! for the registry and GitHub; nothing here reaches the network.

use super::*;
use crate::engine_store::fetch::{install_from, refresh_manifest_from};

/// An address nothing listens on: the discard port of the loopback.
const SILENT: &str = "http://127.0.0.1:9";

fn hits(fake: &Fake) -> usize {
    fake.hits.lock().unwrap().len()
}

fn signed_channel() -> Fake {
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    fake
}

// ── The settings name the bases ─────────────────────────────────────────────

#[test]
fn test_default_settings_read_the_registry_then_github() {
    // GIVEN a store with no settings file
    let dir = tempfile::tempdir().unwrap();

    // WHEN its settings are read
    let settings = EngineStoreSettings::load(&dir.path().join("engines")).unwrap();

    // THEN the registry comes first and GitHub second
    assert_eq!(
        settings.bases(),
        vec![
            "https://registry.apollia.fr".to_owned(),
            "https://github.com/Apollia-OS/apollia-os/releases/download".to_owned(),
        ]
    );
}

#[test]
fn test_a_mirror_saved_by_an_earlier_version_is_the_only_base() {
    // GIVEN a settings file an earlier version wrote with a mirror
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("settings.json"),
        r#"{"channel": "preview", "base_url": "https://mirror.example/engines"}"#,
    )
    .unwrap();

    // WHEN it is read
    let settings = EngineStoreSettings::load(&root).unwrap();

    // THEN its channel and its mirror are kept, the mirror alone
    assert_eq!(settings.channel, Channel::Preview);
    assert_eq!(
        settings.bases(),
        vec!["https://mirror.example/engines".to_owned()]
    );
}

#[test]
fn test_the_former_default_saved_by_an_earlier_version_reads_the_registry_first() {
    // GIVEN a settings file that holds the single default earlier versions
    // wrote whenever they saved, GitHub
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("settings.json"),
        r#"{"channel": "stable", "base_url": "https://github.com/Apollia-OS/apollia-os/releases/download"}"#,
    )
    .unwrap();

    // WHEN it is read, then saved again
    let settings = EngineStoreSettings::load(&root).unwrap();
    settings.save(&root).unwrap();

    // THEN it is no mirror: the default bases are read, and none is written
    assert_eq!(settings.base_url, None);
    assert_eq!(settings.bases().len(), 2);
    let written = std::fs::read_to_string(root.join("settings.json")).unwrap();
    assert!(!written.contains("base_url"), "{written}");
}

#[tokio::test]
async fn test_a_set_mirror_that_does_not_answer_asks_no_default_base() {
    // GIVEN a mirror nothing listens on, and two servers in place of the
    // default bases
    let registry = signed_channel();
    let github = signed_channel();
    let defaults = [start_base(&registry).await, start_base(&github).await];
    let settings = EngineStoreSettings {
        channel: Channel::Stable,
        base_url: Some(SILENT.to_owned()),
    };
    let dir = tempfile::tempdir().unwrap();

    // WHEN the channel is read
    let bases = settings.bases_or(&[defaults[0].as_str(), defaults[1].as_str()]);
    let err = refresh_manifest_from(&store(dir.path(), "linux-x64"), Channel::Stable, &bases)
        .await
        .unwrap_err();

    // THEN the mirror's failure is the answer, and no default base was asked
    assert!(
        matches!(err, EngineStoreError::Transport { ref url, .. } if url.starts_with(SILENT)),
        "{err}"
    );
    assert_eq!((hits(&registry), hits(&github)), (0, 0));
}

#[tokio::test]
async fn test_settings_without_a_mirror_ask_the_first_default_base() {
    // GIVEN no mirror, and two servers in place of the default bases
    let registry = signed_channel();
    let github = signed_channel();
    let defaults = [start_base(&registry).await, start_base(&github).await];
    let settings = EngineStoreSettings::default();
    let dir = tempfile::tempdir().unwrap();

    // WHEN the channel is read
    let bases = settings.bases_or(&[defaults[0].as_str(), defaults[1].as_str()]);
    let (manifest, url) =
        refresh_manifest_from(&store(dir.path(), "linux-x64"), Channel::Stable, &bases)
            .await
            .unwrap();

    // THEN the first one answered, and the second was not asked
    assert_eq!(manifest.sequence, 5);
    assert!(url.as_str().starts_with(&defaults[0]), "{url}");
    assert_eq!(hits(&github), 0);
}

// ── The next base answers when one does not ─────────────────────────────────

/// What reading the channel gave, with the registry at `registry_base` and
/// GitHub serving the channel: the sequence accepted, whether GitHub served
/// it, and whether it was kept.
async fn read_with_github_after(registry_base: String) -> (u64, bool, bool) {
    let github = signed_channel();
    let bases = [registry_base, start_base(&github).await];
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), "linux-x64");
    let (manifest, url) = refresh_manifest_from(&store, Channel::Stable, &bases)
        .await
        .unwrap();
    (
        manifest.sequence,
        url.as_str().starts_with(&bases[1]) && hits(&github) >= 2,
        cached_manifest(&store.root, Channel::Stable, &store.trusted_key).is_some(),
    )
}

#[tokio::test]
async fn test_an_unreachable_registry_hands_over_to_github() {
    // GIVEN a registry nothing listens on, and GitHub serving the channel
    let registry = SILENT.to_owned();

    // WHEN the channel is read
    let outcome = read_with_github_after(registry).await;

    // THEN GitHub's manifest is verified and kept
    assert_eq!(outcome, (5, true, true));
}

#[tokio::test]
async fn test_a_registry_answering_an_error_status_hands_over_to_github() {
    for status in [
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        // GIVEN a registry that answers every request with an error status,
        // and GitHub serving the channel
        let registry = signed_channel();
        *registry.refuse_with.lock().unwrap() = Some(status);
        let base = start_base(&registry).await;

        // WHEN the channel is read
        let outcome = read_with_github_after(base).await;

        // THEN the registry was asked first, and GitHub's manifest is
        // verified and kept
        assert_eq!(hits(&registry), 1, "{status}");
        assert_eq!(outcome, (5, true, true), "{status}");
    }
}

#[tokio::test]
async fn test_a_registry_without_the_signature_hands_over_to_github() {
    // GIVEN a registry that serves the manifest but not its signature
    let registry = Fake::default();
    registry.put("/engines-stable/engines.json", fixture("stable-5.json"));
    let base = start_base(&registry).await;

    // WHEN the channel is read
    let outcome = read_with_github_after(base).await;

    // THEN GitHub's manifest is verified and kept
    assert_eq!(hits(&registry), 2);
    assert_eq!(outcome, (5, true, true));
}

#[tokio::test]
async fn test_a_signature_refused_on_the_registry_is_not_retried_on_github() {
    // GIVEN a registry serving a manifest with the signature of another one,
    // and GitHub serving a valid channel
    let registry = Fake::default();
    registry.publish("stable-5.json", "stable-6.json.minisig");
    let github = signed_channel();
    let bases = [start_base(&registry).await, start_base(&github).await];
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), "linux-x64");

    // WHEN the channel is read
    let err = refresh_manifest_from(&store, Channel::Stable, &bases)
        .await
        .unwrap_err();

    // THEN the refusal is the answer, GitHub was never asked, and nothing
    // was kept
    assert!(matches!(err, EngineStoreError::Signature { .. }), "{err}");
    assert_eq!(hits(&github), 0);
    assert!(cached_manifest(&store.root, Channel::Stable, &store.trusted_key).is_none());
}

#[tokio::test]
async fn test_a_rollback_on_the_registry_is_not_retried_on_github() {
    // GIVEN a store that accepted sequence 6, a registry serving sequence 5
    // and GitHub serving sequence 6
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), "linux-x64");
    let newer = Fake::default();
    newer.publish("stable-6.json", "stable-6.json.minisig");
    let newer_base = start_base(&newer).await;
    refresh_manifest_from(&store, Channel::Stable, std::slice::from_ref(&newer_base))
        .await
        .unwrap();
    let registry = signed_channel();
    let bases = [start_base(&registry).await, newer_base];
    let before = hits(&newer);

    // WHEN the channel is read again
    let err = refresh_manifest_from(&store, Channel::Stable, &bases)
        .await
        .unwrap_err();

    // THEN the rollback is refused and the second base is not asked
    assert!(matches!(err, EngineStoreError::Rollback { .. }), "{err}");
    assert_eq!(hits(&newer), before);
}

// ── Archives come from the base that answered ───────────────────────────────

#[tokio::test]
async fn test_the_registry_serves_the_archive_when_it_served_the_manifest() {
    // GIVEN the registry serving the manifest and its archives, GitHub
    // serving nothing
    let registry = signed_channel();
    let github = Fake::default();
    let bases = [start_base(&registry).await, start_base(&github).await];
    let dir = tempfile::tempdir().unwrap();

    // WHEN the CUDA engine is installed
    let installed = install_from(
        &store(dir.path(), "linux-x64"),
        Channel::Stable,
        &bases,
        (Engine::LlamaServer, Variant::Cuda12),
        |_, _| {},
    )
    .await
    .unwrap();

    // THEN it came from the registry, and GitHub was never asked
    assert_eq!(installed.tag, "b6000");
    assert_eq!(registry.archive_hits(), 1);
    assert_eq!(hits(&github), 0);
}

#[tokio::test]
async fn test_github_serves_the_archive_when_it_served_the_manifest() {
    // GIVEN a registry with the archives but no manifest, and GitHub serving
    // the whole channel
    let registry = Fake::default();
    registry.archives();
    let github = signed_channel();
    let bases = [start_base(&registry).await, start_base(&github).await];
    let dir = tempfile::tempdir().unwrap();

    // WHEN the CUDA engine is installed
    let installed = install_from(
        &store(dir.path(), "linux-x64"),
        Channel::Stable,
        &bases,
        (Engine::LlamaServer, Variant::Cuda12),
        |_, _| {},
    )
    .await
    .unwrap();

    // THEN the archive was read beside the manifest that was accepted
    assert_eq!(installed.tag, "b6000");
    assert_eq!(github.archive_hits(), 1);
    assert_eq!(registry.archive_hits(), 0);
}
