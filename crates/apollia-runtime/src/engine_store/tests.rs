//! Engine store tests, against a fake server that counts requests.
//!
//! The fixtures under `tests/fixtures/engine_store/` were signed once with a
//! throwaway key pair made by `cargo tauri signer generate`; its private half
//! was not kept. `test-dist.key.pub` is its public half, `other.key.pub` a key
//! that signed none of them. To change a fixture manifest, generate a new
//! pair, re-sign every `*.json` with `cargo tauri signer sign`, rename each
//! `.sig` to `.minisig`, and replace `test-dist.key.pub`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::Router;

use super::*;

mod auto_update;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/engine_store")
            .join(name),
    )
    .unwrap()
}

fn key(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

fn store(dir: &Path, platform: &str) -> EngineStore {
    EngineStore {
        root: dir.join("engines"),
        trusted_key: key("test-dist.key.pub"),
        platform: platform.to_owned(),
    }
}

#[derive(Clone, Default)]
struct Fake {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn put(&self, path: &str, bytes: Vec<u8>) {
        self.files.lock().unwrap().insert(path.to_owned(), bytes);
    }

    /// Publish a fixture manifest as the `stable` channel's manifest.
    fn publish(&self, manifest: &str, signature: &str) {
        self.put("/engines-stable/engines.json", fixture(manifest));
        self.put("/engines-stable/engines.json.minisig", fixture(signature));
    }

    fn archives(&self) {
        for name in [
            "llama-b6000-cuda.zip",
            "llama-b7000-cuda.zip",
            "vllm-mlx-0.2.0-metal.zip",
        ] {
            self.put(&format!("/engines-stable/{name}"), fixture(name));
        }
    }

    fn archive_hits(&self) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|p| !p.ends_with("engines.json") && !p.ends_with(".minisig"))
            .count()
    }
}

async fn serve(State(fake): State<Fake>, uri: Uri) -> (StatusCode, Vec<u8>) {
    fake.hits.lock().unwrap().push(uri.path().to_owned());
    match fake.files.lock().unwrap().get(uri.path()) {
        Some(bytes) => (StatusCode::OK, bytes.clone()),
        None => (StatusCode::NOT_FOUND, Vec::new()),
    }
}

async fn start(fake: &Fake) -> EngineStoreSettings {
    let app = Router::new().fallback(serve).with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    EngineStoreSettings {
        channel: Channel::Stable,
        base_url: format!("http://{addr}"),
    }
}

// ── Criterion 1: the manifest contract ──────────────────────────────────────

#[test]
fn test_manifest_round_trips() {
    // GIVEN a fixture manifest
    let parsed = Manifest::parse(&fixture("stable-6.json")).unwrap();

    // WHEN it is written back and parsed again
    let again = Manifest::parse(&serde_json::to_vec(&parsed).unwrap()).unwrap();

    // THEN nothing is lost, and every field is read
    assert_eq!(parsed, again);
    assert_eq!(parsed.channel, Channel::Stable);
    assert_eq!(parsed.sequence, 6);
    let p = &parsed.packages[0];
    assert_eq!(
        (p.engine, p.tag.as_str(), p.variant, p.platform.as_str()),
        (Engine::LlamaServer, "b6000", Variant::Cuda, "linux-x64")
    );
    assert_eq!(p.url, "llama-b6000-cuda.zip");
    assert_eq!(p.sha256.len(), 64);
}

#[test]
fn test_manifest_with_absolute_or_climbing_url_is_refused() {
    // GIVEN manifests whose archive address is not a plain relative one
    for url in [
        "https://elsewhere.example/a.zip",
        "../a.zip",
        "/a.zip",
        "a.exe",
        "a.tar.gz",
    ] {
        let text = format!(
            r#"{{"format":1,"channel":"stable","sequence":1,"packages":[{{"engine":"llama-server","tag":"b1","variant":"cpu","platform":"linux-x64","size":1,"url":"{url}","sha256":"{}"}}]}}"#,
            "a".repeat(64)
        );

        // WHEN it is parsed
        let err = Manifest::parse(text.as_bytes()).unwrap_err();

        // THEN it is refused
        assert!(
            matches!(err, EngineStoreError::ManifestInvalid { .. }),
            "{url}"
        );
    }
}

#[test]
fn test_manifest_of_unknown_format_or_field_is_refused() {
    // GIVEN a format 2 manifest, and one with an unknown field
    for text in [
        r#"{"format":2,"channel":"stable","sequence":1,"packages":[]}"#,
        r#"{"format":1,"channel":"stable","sequence":1,"packages":[],"key":"x"}"#,
    ] {
        // WHEN it is parsed
        let err = Manifest::parse(text.as_bytes()).unwrap_err();

        // THEN it is refused
        assert!(
            matches!(err, EngineStoreError::ManifestInvalid { .. }),
            "{text}"
        );
    }
}

#[test]
fn test_numbered_tags_order_by_number() {
    // GIVEN tags whose text order differs from their number order
    // WHEN they are compared
    // THEN the number decides
    assert_eq!(compare_tags("b999", "b1000"), std::cmp::Ordering::Less);
    assert_eq!(compare_tags("b7000", "b6000"), std::cmp::Ordering::Greater);
}

// ── Criterion 2: verify before any download ─────────────────────────────────

#[tokio::test]
async fn test_valid_signature_downloads_the_archive() {
    // GIVEN a signed manifest and its archives on the server
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();

    // WHEN the CUDA engine is installed
    let installed = install_package(
        &store(dir.path(), "linux-x64"),
        &settings,
        Engine::LlamaServer,
        Variant::Cuda,
    )
    .await
    .unwrap();

    // THEN exactly one archive was requested and the engine is installed
    assert_eq!(fake.archive_hits(), 1);
    assert_eq!(installed.tag, "b6000");
    assert_eq!(installed.source, Source::Downloaded);
}

async fn refused_before_any_archive(fake: &Fake, store: EngineStore) -> EngineStoreError {
    fake.archives();
    let settings = start(fake).await;
    let err = install_package(&store, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap_err();
    assert_eq!(fake.archive_hits(), 0, "an archive was requested: {err}");
    assert!(installed_packages(&store.root).is_empty());
    err
}

#[tokio::test]
async fn test_signature_of_another_manifest_is_refused_before_any_archive() {
    // GIVEN the manifest of sequence 5 served with the signature of sequence 6
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-6.json.minisig");
    let dir = tempfile::tempdir().unwrap();

    // WHEN an install is asked for
    let err = refused_before_any_archive(&fake, store(dir.path(), "linux-x64")).await;

    // THEN it is a signature refusal with a readable message
    assert!(matches!(err, EngineStoreError::Signature { .. }));
    assert!(err.to_string().contains("signature is not valid"));
}

#[tokio::test]
async fn test_manifest_altered_by_one_byte_is_refused_before_any_archive() {
    // GIVEN a signed manifest whose sequence digit changed after signing
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    let mut altered = fixture("stable-5.json");
    let at = altered.iter().position(|b| *b == b'5').unwrap();
    altered[at] = b'9';
    fake.put("/engines-stable/engines.json", altered);
    let dir = tempfile::tempdir().unwrap();

    // WHEN an install is asked for
    let err = refused_before_any_archive(&fake, store(dir.path(), "linux-x64")).await;

    // THEN the signature does not hold
    assert!(matches!(err, EngineStoreError::Signature { .. }));
}

#[tokio::test]
async fn test_manifest_from_an_unknown_key_is_refused_before_any_archive() {
    // GIVEN a store that trusts another key than the one that signed
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    let dir = tempfile::tempdir().unwrap();
    let mut other = store(dir.path(), "linux-x64");
    other.trusted_key = key("other.key.pub");

    // WHEN an install is asked for
    let err = refused_before_any_archive(&fake, other).await;

    // THEN it is refused
    assert!(matches!(err, EngineStoreError::Signature { .. }));
}

#[tokio::test]
async fn test_older_sequence_is_refused_as_a_rollback() {
    // GIVEN a store that accepted sequence 5, then a server back at sequence 3
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    refresh_manifest(&s, &settings).await.unwrap();
    fake.publish("stable-3.json", "stable-3.json.minisig");
    fake.archives();

    // WHEN an install is asked for
    let err = install_package(&s, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap_err();

    // THEN it is refused as a rollback, with no archive requested
    assert!(
        matches!(
            err,
            EngineStoreError::Rollback {
                found: 3,
                accepted: 5
            }
        ),
        "{err}"
    );
    assert_eq!(fake.archive_hits(), 0);
}

#[tokio::test]
async fn test_manifest_of_another_channel_is_refused() {
    // GIVEN the signed preview manifest served where the stable one belongs
    let fake = Fake::default();
    fake.publish("preview-1.json", "preview-1.json.minisig");
    let dir = tempfile::tempdir().unwrap();

    // WHEN an install is asked for
    let err = refused_before_any_archive(&fake, store(dir.path(), "linux-x64")).await;

    // THEN it is refused
    assert!(
        matches!(err, EngineStoreError::ChannelMismatch { .. }),
        "{err}"
    );
}

#[tokio::test]
async fn test_missing_manifest_reports_the_http_status() {
    // GIVEN a server with nothing published
    let fake = Fake::default();
    let dir = tempfile::tempdir().unwrap();

    // WHEN an install is asked for
    let err = refused_before_any_archive(&fake, store(dir.path(), "linux-x64")).await;

    // THEN the status is reported
    assert!(
        matches!(err, EngineStoreError::HttpStatus { status: 404, .. }),
        "{err}"
    );
}

#[test]
fn test_store_without_compiled_key_refuses_to_open_or_opens_on_it() {
    // GIVEN this build's compiled distribution key
    let dir = tempfile::tempdir().unwrap();

    // WHEN the store is opened
    let opened = EngineStore::open(dir.path());

    // THEN it trusts exactly the compiled key, or refuses when there is none
    match apollia_signing::distribution::distribution_public_key() {
        Some(k) => assert_eq!(opened.unwrap().trusted_key, k),
        None => assert!(matches!(opened, Err(EngineStoreError::NoTrustedKey))),
    }
}

#[test]
fn test_settings_default_to_stable_and_round_trip() {
    // GIVEN an empty store
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");

    // WHEN the settings are read, changed and read again
    let first = EngineStoreSettings::load(&root).unwrap();
    let changed = EngineStoreSettings {
        channel: Channel::Preview,
        base_url: "https://mirror.example.org/apollia".to_owned(),
    };
    changed.save(&root).unwrap();

    // THEN the default is stable, and the change persists
    assert_eq!(first.channel, Channel::Stable);
    assert_eq!(first.base_url, DEFAULT_BASE_URL);
    assert_eq!(EngineStoreSettings::load(&root).unwrap(), changed);
}

#[test]
fn test_settings_refuse_a_non_http_mirror() {
    // GIVEN a mirror address with another scheme
    let dir = tempfile::tempdir().unwrap();
    let bad = EngineStoreSettings {
        channel: Channel::Stable,
        base_url: "file:///etc".to_owned(),
    };

    // WHEN it is saved
    let err = bad.save(&dir.path().join("engines")).unwrap_err();

    // THEN it is refused
    assert!(matches!(err, EngineStoreError::InvalidUrl { .. }));
}

// ── Criterion 3: download and file ──────────────────────────────────────────

#[tokio::test]
async fn test_archive_is_filed_by_tag_and_variant_with_its_record() {
    // GIVEN a signed manifest and its archives
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN the CUDA engine is installed
    install_package(&s, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap();

    // THEN it sits under llama-server/b6000/cuda with its record, and no
    //      downloaded archive is left behind
    let dir_of = s.root.join("llama-server/b6000/cuda");
    assert!(dir_of.join(RECORD_FILE).is_file());
    assert!(find_binary(&dir_of, "llama-server").is_some());
    assert_eq!(
        std::fs::read_dir(s.root.join(".downloads"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn test_archive_with_wrong_digest_is_deleted_and_not_installed() {
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

    // WHEN the CUDA engine is installed
    let err = install_package(&s, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap_err();

    // THEN the digest refusal is readable, the archive is gone, nothing installed
    assert!(
        matches!(err, EngineStoreError::DigestMismatch { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("does not match the manifest"));
    assert_eq!(
        std::fs::read_dir(s.root.join(".downloads"))
            .unwrap()
            .count(),
        0
    );
    assert!(installed_packages(&s.root).is_empty());
}

// ── Criterion 4: an engine channel of its own ───────────────────────────────

#[tokio::test]
async fn test_newer_tag_on_the_channel_is_offered_as_an_update() {
    // GIVEN b6000 installed from sequence 5, then sequence 6 announcing b7000
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

    // WHEN updates are checked
    let (manifest, _) = refresh_manifest(&s, &settings).await.unwrap();
    let found = updates(&s.root, &manifest, &s.platform);

    // THEN the CUDA update to b7000 is offered
    let cuda = found.iter().find(|u| u.variant == Variant::Cuda).unwrap();
    assert_eq!(cuda.installed_tag.as_deref(), Some("b6000"));
    assert_eq!(cuda.latest_tag, "b7000");
    assert!(cuda.update_available);
}

#[tokio::test]
async fn test_same_tag_on_the_channel_offers_nothing() {
    // GIVEN b6000 installed from sequence 5, still the channel's manifest
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    install_package(&s, &settings, Engine::LlamaServer, Variant::Cuda)
        .await
        .unwrap();

    // WHEN updates are checked
    let (manifest, _) = refresh_manifest(&s, &settings).await.unwrap();
    let found = updates(&s.root, &manifest, &s.platform);

    // THEN nothing is offered
    assert!(found.iter().all(|u| !u.update_available));
}

#[tokio::test]
async fn test_cached_manifest_is_verified_again_when_read() {
    // GIVEN a verified manifest kept on disk, then altered there
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    refresh_manifest(&s, &settings).await.unwrap();
    let kept = cached_manifest(&s.root, Channel::Stable, &s.trusted_key);
    let path = s.root.join("manifests/stable.json");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b' ');
    std::fs::write(&path, bytes).unwrap();

    // WHEN it is read again
    let reread = cached_manifest(&s.root, Channel::Stable, &s.trusted_key);

    // THEN it was readable before, and is refused once altered
    assert_eq!(kept.unwrap().sequence, 5);
    assert!(reread.is_none());
}

// ── Criterion 5: offline import ─────────────────────────────────────────────

fn disk(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (to, from) in files {
        std::fs::write(dir.path().join(to), fixture(from)).unwrap();
    }
    dir
}

#[test]
fn test_import_from_a_disk_verifies_and_files_the_engine() {
    // GIVEN a disk with the manifest, its signature and the archive
    let usb = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN it is imported
    let done = import_from_dir(&s, usb.path()).unwrap();

    // THEN the engine is filed as imported, at the same place as a download
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].source, Source::Imported);
    assert!(s
        .root
        .join("llama-server/b6000/cuda")
        .join(RECORD_FILE)
        .is_file());
}

#[test]
fn test_import_without_signature_is_refused() {
    // GIVEN a disk with the manifest and the archive but no signature
    let usb = disk(&[
        (MANIFEST_FILE, "stable-5.json"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN it is imported
    let err = import_from_dir(&s, usb.path()).unwrap_err();

    // THEN it is refused and nothing is installed
    assert!(
        matches!(err, EngineStoreError::MissingSignature { .. }),
        "{err}"
    );
    assert!(installed_packages(&s.root).is_empty());
}

#[test]
fn test_import_with_a_wrong_signature_or_archive_is_refused() {
    // GIVEN a disk whose signature is another manifest's, then one whose
    //       archive is another file
    let wrong_sig = disk(&[
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
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");

    // WHEN each is imported
    let sig = import_from_dir(&s, wrong_sig.path()).unwrap_err();
    let archive = import_from_dir(&s, wrong_archive.path()).unwrap_err();

    // THEN the first fails on the signature, the second on the digest
    assert!(matches!(sig, EngineStoreError::Signature { .. }), "{sig}");
    assert!(
        matches!(archive, EngineStoreError::DigestMismatch { .. }),
        "{archive}"
    );
    assert!(installed_packages(&s.root).is_empty());
}

// ── The MLX engine follows the same mechanism ───────────────────────────────

#[tokio::test]
async fn test_mlx_environment_is_installed_from_the_same_manifest() {
    // GIVEN a signed manifest that carries the MLX environment
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "macos-arm64");

    // WHEN the MLX environment is installed
    let done = install_package(&s, &settings, Engine::VllmMlx, Variant::Metal)
        .await
        .unwrap();

    // THEN it is filed like any engine
    assert_eq!((done.engine, done.tag.as_str()), (Engine::VllmMlx, "0.2.0"));
    assert!(s
        .root
        .join("vllm-mlx/0.2.0/metal/site-packages/vllm_mlx/__init__.py")
        .is_file());
}

#[tokio::test]
async fn test_mlx_environment_update_with_wrong_digest_is_refused() {
    // GIVEN a signed manifest, and an MLX environment altered on the server
    let fake = Fake::default();
    fake.publish("stable-5.json", "stable-5.json.minisig");
    fake.archives();
    let mut tampered = fixture("vllm-mlx-0.2.0-metal.zip");
    tampered[20] ^= 0x01;
    fake.put("/engines-stable/vllm-mlx-0.2.0-metal.zip", tampered);
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "macos-arm64");

    // WHEN the MLX environment is installed
    let err = install_package(&s, &settings, Engine::VllmMlx, Variant::Metal)
        .await
        .unwrap_err();

    // THEN it is refused and nothing is installed
    assert!(
        matches!(err, EngineStoreError::DigestMismatch { .. }),
        "{err}"
    );
    assert!(installed_packages(&s.root).is_empty());
}

// ── Criterion 6: which installed engine the launcher takes ─────────────────

fn install_fixture(s: &EngineStore, manifest: &str, sig: &str, archive: &str) {
    let usb = disk(&[
        (MANIFEST_FILE, manifest),
        (SIGNATURE_FILE, sig),
        (archive, archive),
    ]);
    import_from_dir(s, usb.path()).unwrap();
}

#[test]
fn test_newest_verified_cuda_build_is_preferred_on_an_nvidia_machine() {
    // GIVEN b6000 and b7000 CUDA builds installed
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    install_fixture(
        &s,
        "stable-5.json",
        "stable-5.json.minisig",
        "llama-b6000-cuda.zip",
    );
    install_fixture(
        &s,
        "stable-6.json",
        "stable-6.json.minisig",
        "llama-b7000-cuda.zip",
    );

    // WHEN the launcher asks on a machine with an NVIDIA card
    let chosen = preferred_llama_server(&s.root, "llama-server", || true).unwrap();

    // THEN it takes the b7000 build
    assert!(
        chosen.starts_with(s.root.join("llama-server/b7000/cuda")),
        "{}",
        chosen.display()
    );
}

#[test]
fn test_cuda_build_is_ignored_without_an_nvidia_card() {
    // GIVEN a CUDA build installed
    let dir = tempfile::tempdir().unwrap();
    let s = store(dir.path(), "linux-x64");
    install_fixture(
        &s,
        "stable-5.json",
        "stable-5.json.minisig",
        "llama-b6000-cuda.zip",
    );

    // WHEN the launcher asks on a machine without an NVIDIA card
    let chosen = preferred_llama_server(&s.root, "llama-server", || false);

    // THEN the embedded engine keeps the place
    assert!(chosen.is_none());
}

#[test]
fn test_unverified_directory_is_never_taken() {
    // GIVEN a CUDA directory with a binary but no record
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    let fake_dir = root.join("llama-server/b9000/cuda");
    std::fs::create_dir_all(&fake_dir).unwrap();
    std::fs::write(fake_dir.join("llama-server"), b"binary").unwrap();
    let mut asked = false;

    // WHEN the launcher asks
    let chosen = preferred_llama_server(&root, "llama-server", || {
        asked = true;
        true
    });

    // THEN nothing is taken, and the card was not even probed
    assert!(chosen.is_none());
    assert!(!asked);
}
