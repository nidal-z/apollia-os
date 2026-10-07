//! The followed installation, against the fake server and the signed fixture
//! manifests of the store tests.

use super::*;
use crate::engine_store::install_job::{
    dismiss, refuse, start_download, start_import, state, subscribe, InstallKind, InstallationState,
};

/// Wait until the installation of `s` is no longer running, and answer where
/// it stands.
async fn outcome(s: &EngineStore) -> InstallationState {
    let mut rx = subscribe(&s.root).expect("an installation was started");
    let done = rx.wait_for(|state| !state.is_running()).await.unwrap();
    done.clone()
}

fn start_cuda(s: &EngineStore, settings: &EngineStoreSettings) -> InstallationState {
    start_download(s, settings, Engine::LlamaServer, Variant::Cuda)
}

#[tokio::test]
async fn test_a_download_is_an_install_then_an_update_and_ends_installed() {
    // GIVEN a signed manifest and its archives on the server
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN the CUDA engine is asked for, then again once the channel moved on
    let first = start_cuda(&s, &settings);
    let installed = outcome(&s).await;
    fake.publish("stable-6.json", "stable-6.json.minisig");
    let second = start_cuda(&s, &settings);
    let updated = outcome(&s).await;

    // THEN the first starts on the signature as an install and the second as
    //      an update, and each ends installed
    let kind = InstallKind::Install;
    assert_eq!(first, InstallationState::Verifying { kind });
    assert_eq!(installed, InstallationState::Installed { kind });
    let kind = InstallKind::Update;
    assert_eq!(second, InstallationState::Verifying { kind });
    assert_eq!(updated, InstallationState::Installed { kind });
    let newest = newest_installed(&s.root, Engine::LlamaServer, Variant::Cuda).unwrap();
    assert_eq!(newest.tag, "b7000");
}

#[tokio::test]
async fn test_each_signature_refusal_has_its_code_and_requested_no_archive() {
    // GIVEN a manifest altered after signing, a store trusting another key,
    //       and a server back at an older sequence
    let altered = Fake::default();
    altered.publish("stable-5.json", "stable-5.json.minisig");
    let mut bytes = fixture("stable-5.json");
    let at = bytes.iter().position(|b| *b == b'5').unwrap();
    bytes[at] = b'9';
    altered.put("/engines-stable/engines.json", bytes);
    altered.archives();
    let altered_dir = tempfile::tempdir().unwrap();
    let altered_store = store(altered_dir.path(), "linux-x64");

    let unknown = Fake::default();
    unknown.publish("stable-5.json", "stable-5.json.minisig");
    unknown.archives();
    let unknown_dir = tempfile::tempdir().unwrap();
    let mut unknown_store = store(unknown_dir.path(), "linux-x64");
    unknown_store.trusted_key = key("other.key.pub");

    let older = Fake::default();
    older.publish("stable-5.json", "stable-5.json.minisig");
    older.archives();
    let older_settings = start(&older).await;
    let older_dir = tempfile::tempdir().unwrap();
    let older_store = store(older_dir.path(), "linux-x64");
    refresh_manifest(&older_store, &older_settings)
        .await
        .unwrap();
    older.publish("stable-3.json", "stable-3.json.minisig");

    // WHEN the CUDA engine is asked for on each
    let _ = start_cuda(&altered_store, &start(&altered).await);
    let _ = start_cuda(&unknown_store, &start(&unknown).await);
    let _ = start_cuda(&older_store, &older_settings);
    let outcomes = [
        outcome(&altered_store).await,
        outcome(&unknown_store).await,
        outcome(&older_store).await,
    ];

    // THEN each is refused under its own code, before any archive
    let codes = [
        RefusalCode::SignatureInvalid,
        RefusalCode::SignatureUnknownKey,
        RefusalCode::Rollback,
    ];
    for (outcome, expected) in outcomes.iter().zip(codes) {
        match outcome {
            InstallationState::Failed {
                kind,
                code,
                archive_requested,
                ..
            } => {
                assert_eq!(*kind, InstallKind::Install);
                assert_eq!(*code, expected);
                assert!(!archive_requested, "{expected:?}");
            }
            other => panic!("expected a refusal with {expected:?}, got {other:?}"),
        }
    }
    for fake in [&altered, &unknown, &older] {
        assert_eq!(fake.archive_hits(), 0);
    }
}

#[tokio::test]
async fn test_a_wrong_digest_is_refused_after_the_archive_was_requested() {
    // GIVEN a signed manifest, and an archive altered on the server
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let mut tampered = fixture("llama-b6000-cuda.zip");
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    fake.put("/engines-stable/llama-b6000-cuda.zip", tampered);
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN the CUDA engine is asked for
    let _ = start_cuda(&s, &settings);
    let refused = outcome(&s).await;

    // THEN the refusal names the digest and says an archive was requested
    assert!(
        matches!(
            refused,
            InstallationState::Failed {
                code: RefusalCode::DigestMismatch,
                archive_requested: true,
                ..
            }
        ),
        "{refused:?}"
    );
    assert!(installed_packages(&s.root).is_empty());
}

#[tokio::test]
async fn test_a_download_is_cancelled_and_its_archive_deleted() {
    // GIVEN a verified manifest whose archive never finishes arriving
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    *fake.stalled.lock().unwrap() = Some("/engines-stable/llama-b6000-cuda.zip".to_owned());
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    let _ = start_cuda(&s, &settings);
    let mut rx = subscribe(&s.root).unwrap();
    let downloading = rx
        .wait_for(|state| matches!(state, InstallationState::Downloading { .. }))
        .await
        .unwrap()
        .clone();

    // WHEN the download is dismissed, then a second start is asked
    let while_running = start_cuda(&s, &settings);
    let after = dismiss(&s.root).await;

    // THEN the signature had held and the size was known, a second start
    //      joined the running one, and the cancel left nothing behind
    let kind = InstallKind::Install;
    assert_eq!(
        downloading,
        InstallationState::Downloading {
            kind,
            received: 0,
            total: 725
        }
    );
    assert_eq!(while_running, downloading);
    assert_eq!(after, InstallationState::Idle);
    assert_eq!(state(&s.root), InstallationState::Idle);
    let left = std::fs::read_dir(s.root.join(".downloads"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(left, 0);
    assert!(installed_packages(&s.root).is_empty());
}

#[tokio::test]
async fn test_each_import_refusal_has_its_code_and_a_good_disk_is_installed() {
    // GIVEN a disk without signature, one with the signature of another
    //       manifest, one with an altered archive, and a good one
    let no_signature = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ]);
    let wrong_signature = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-6.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ]);
    let wrong_archive = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
    ]);
    let mut altered = fixture("llama-b6000-cuda.zip");
    altered[30] ^= 0x01;
    std::fs::write(wrong_archive.path().join("llama-b6000-cuda.zip"), altered).unwrap();
    let good = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN each is imported in turn
    let mut outcomes = Vec::new();
    for usb in [&no_signature, &wrong_signature, &wrong_archive] {
        let started = start_import(&s, usb.path());
        assert_eq!(
            started,
            InstallationState::Verifying {
                kind: InstallKind::Import
            }
        );
        outcomes.push(outcome(&s).await);
        assert!(installed_packages(&s.root).is_empty());
    }
    let _ = start_import(&s, good.path());
    let installed = outcome(&s).await;

    // THEN the three refusals are told apart, and the good disk is installed
    let codes: Vec<RefusalCode> = outcomes
        .iter()
        .map(|outcome| match outcome {
            InstallationState::Failed { code, .. } => *code,
            other => panic!("expected a refusal, got {other:?}"),
        })
        .collect();
    assert_eq!(
        codes,
        [
            RefusalCode::SignatureMissing,
            RefusalCode::SignatureInvalid,
            RefusalCode::DigestMismatch
        ]
    );
    assert_eq!(
        installed,
        InstallationState::Installed {
            kind: InstallKind::Import
        }
    );
    assert_eq!(installed_packages(&s.root).len(), 1);
}

#[tokio::test]
async fn test_an_outcome_stays_until_it_is_dismissed() {
    // GIVEN a store whose installation was refused before it could start
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    let refused = refuse(&root, InstallKind::Install, &EngineStoreError::NoTrustedKey);

    // WHEN it is read, dismissed, then read again
    let read = state(&root);
    let dismissed = dismiss(&root).await;
    let again = state(&root);

    // THEN the refusal is kept with its code until the dismissal
    assert!(
        matches!(
            refused,
            InstallationState::Failed {
                code: RefusalCode::NoTrustedKey,
                archive_requested: false,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(read, refused);
    assert_eq!(
        (dismissed, again),
        (InstallationState::Idle, InstallationState::Idle)
    );
}

#[test]
fn test_updates_carry_the_size_and_the_release_days() {
    // GIVEN a manifest that states the day of its b7000 build, over a store
    //       that holds b6000 with its own day
    let text = format!(
        r#"{{"format":1,"channel":"stable","sequence":9,"packages":[{{"engine":"llama-server","tag":"b7000","variant":"cuda","platform":"linux-x64","size":4321,"url":"b7000.zip","sha256":"{}","released":"2026-09-28"}}]}}"#,
        "a".repeat(64)
    );
    let manifest = Manifest::parse(text.as_bytes()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let held = package_dir(dir.path(), Engine::LlamaServer, "b6000", Variant::Cuda);
    std::fs::create_dir_all(&held).unwrap();
    let record = InstalledPackage {
        engine: Engine::LlamaServer,
        tag: "b6000".to_owned(),
        variant: Variant::Cuda,
        platform: "linux-x64".to_owned(),
        sha256: "0".repeat(64),
        source: Source::Downloaded,
        released: Some("2026-09-12".to_owned()),
        path: PathBuf::new(),
    };
    std::fs::write(held.join(RECORD_FILE), serde_json::to_vec(&record).unwrap()).unwrap();

    // WHEN the store is compared to the manifest
    let found = updates(dir.path(), &manifest, "linux-x64");

    // THEN the update carries what it downloads and both days
    assert_eq!(found.len(), 1);
    let update = &found[0];
    assert!(update.update_available);
    assert_eq!(update.size, 4321);
    assert_eq!(update.latest_released.as_deref(), Some("2026-09-28"));
    assert_eq!(update.installed_released.as_deref(), Some("2026-09-12"));
}

#[test]
fn test_a_release_day_that_is_not_a_day_is_refused() {
    // GIVEN manifests whose release day is not a calendar day
    for day in ["28/09/2026", "2026-13-01", "2026-09-32", "2026-9-28", ""] {
        let text = format!(
            r#"{{"format":1,"channel":"stable","sequence":1,"packages":[{{"engine":"llama-server","tag":"b1","variant":"cpu","platform":"linux-x64","size":1,"url":"a.zip","sha256":"{}","released":"{day}"}}]}}"#,
            "a".repeat(64)
        );

        // WHEN it is parsed
        let err = Manifest::parse(text.as_bytes()).unwrap_err();

        // THEN it is refused
        assert!(
            matches!(err, EngineStoreError::ManifestInvalid { .. }),
            "{day}"
        );
    }
}
