//! Node.js through the engine store, against a fake server that counts
//! requests.
//!
//! `nodejs-7.json` and its `.minisig` under `tests/fixtures/engine_store/`
//! were signed once with a throwaway pair made by `cargo tauri signer
//! generate`, whose private half was not kept; `node-dist.key.pub` is its
//! public half. The archive is a stand-in Node.js release: a `bin/node`
//! script and npm's `npx-cli.js`, laid out as upstream does.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::Router;

use super::*;
use crate::engine_store::{import_from_dir, Channel, EngineStoreError, Manifest, MANIFEST_FILE};

const ARCHIVE: &str = "node-v22.11.0-linux-x64.zip";

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

fn store_with_key(dir: &Path, key_file: &str) -> EngineStore {
    EngineStore {
        root: dir.join("engines"),
        trusted_key: key(key_file),
        platform: "linux-x64".to_owned(),
    }
}

#[derive(Clone, Default)]
struct Fake {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn publish(&self, manifest: Vec<u8>, signature: Vec<u8>) {
        let mut files = self.files.lock().unwrap();
        files.insert("/engines-stable/engines.json".to_owned(), manifest);
        files.insert("/engines-stable/engines.json.minisig".to_owned(), signature);
        files.insert(format!("/engines-stable/{ARCHIVE}"), fixture(ARCHIVE));
    }

    fn archive_hits(&self) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.ends_with(".zip"))
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

/// Ask for Node.js and wait for the download to end, on its own channel.
async fn ensure_and_wait(store: &EngineStore, settings: &EngineStoreSettings) -> NodeState {
    let first = ensure(store, settings);
    assert!(matches!(first, NodeState::Downloading { .. }), "{first:?}");
    let mut rx = jobs().lock().unwrap().get(&store.root).cloned().unwrap();
    let end = rx
        .wait_for(|s| !matches!(s, NodeState::Downloading { .. }))
        .await
        .unwrap()
        .clone();
    end
}

// ── The manifest carries Node.js ────────────────────────────────────────────

#[test]
fn test_manifest_with_a_nodejs_package_round_trips() {
    // GIVEN the signed fixture manifest that publishes Node.js
    let bytes = fixture("nodejs-7.json");
    let signature = key("nodejs-7.json.minisig");

    // WHEN it is verified, written back and parsed again
    let parsed = crate::engine_store::verify_manifest(
        &bytes,
        &signature,
        &key("node-dist.key.pub"),
        Channel::Stable,
    )
    .unwrap();
    let again = Manifest::parse(&serde_json::to_vec(&parsed).unwrap()).unwrap();

    // THEN the Node.js package keeps every field
    assert_eq!(parsed, again);
    let p = &parsed.packages[0];
    assert_eq!(
        (p.engine, p.tag.as_str(), p.variant, p.platform.as_str()),
        (Engine::NodeJs, "v22.11.0", Variant::Cpu, "linux-x64")
    );
    assert_eq!(p.url, ARCHIVE);
    assert_eq!(p.size, fixture(ARCHIVE).len() as u64);
    assert_eq!(p.sha256.len(), 64);
    assert_eq!(
        serde_json::to_value(Engine::NodeJs).unwrap(),
        serde_json::json!("nodejs")
    );
}

#[test]
fn test_release_tags_are_ordered_by_their_numbers() {
    // GIVEN two release tags whose byte order differs from their version order
    // WHEN they are compared
    let order = crate::engine_store::compare_tags("v22.11.0", "v22.9.0");

    // THEN the higher version is the newer one
    assert_eq!(order, std::cmp::Ordering::Greater);
}

// ── First need: download, verification, installation ───────────────────────

#[tokio::test]
async fn test_node_is_downloaded_verified_and_installed() {
    // GIVEN a mirror publishing the signed manifest and the archive
    let fake = Fake::default();
    fake.publish(fixture("nodejs-7.json"), fixture("nodejs-7.json.minisig"));
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");
    assert_eq!(state(&store.root), NodeState::Absent);

    // WHEN Node.js is asked for
    let end = ensure_and_wait(&store, &settings).await;

    // THEN it is ready in the store, fetched once, and read as ready after
    let NodeState::Ready { tag, path } = end else {
        panic!("not ready: {end:?}");
    };
    assert_eq!(tag, "v22.11.0");
    let node = Path::new(&path).join("node-v22.11.0-linux-x64/bin/node");
    assert!(node.is_file(), "{}", node.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&node).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "node keeps its execute bits");
    }
    assert_eq!(fake.archive_hits(), 1);
    assert!(matches!(state(&store.root), NodeState::Ready { .. }));
    assert!(matches!(ensure(&store, &settings), NodeState::Ready { .. }));
    assert_eq!(
        fake.archive_hits(),
        1,
        "an installed Node.js is not fetched again"
    );
}

#[tokio::test]
async fn test_altered_manifest_fails_without_any_archive_request() {
    // GIVEN a mirror whose manifest was changed after it was signed
    let mut altered = fixture("nodejs-7.json");
    let at = altered.iter().position(|&b| b == b'7').unwrap();
    altered[at] = b'8';
    let fake = Fake::default();
    fake.publish(altered, fixture("nodejs-7.json.minisig"));
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");

    // WHEN Node.js is asked for
    let end = ensure_and_wait(&store, &settings).await;

    // THEN it fails readably, and no archive was requested
    let NodeState::Failed { error } = end else {
        panic!("not failed: {end:?}");
    };
    assert!(error.contains("signature"), "{error}");
    assert_eq!(fake.archive_hits(), 0);
    assert!(installed_node(&store.root).is_none());
}

#[tokio::test]
async fn test_invalid_signature_fails_without_any_archive_request() {
    // GIVEN a mirror serving the manifest beside the signature of another file
    let fake = Fake::default();
    fake.publish(fixture("nodejs-7.json"), fixture("stable-6.json.minisig"));
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");

    // WHEN Node.js is asked for
    let end = ensure_and_wait(&store, &settings).await;

    // THEN it fails readably, and no archive was requested
    assert!(
        matches!(&end, NodeState::Failed { error } if error.contains("signature")),
        "{end:?}"
    );
    assert_eq!(fake.archive_hits(), 0);
}

#[tokio::test]
async fn test_unknown_key_fails_without_any_archive_request() {
    // GIVEN a genuine manifest, and a build that trusts another key
    let fake = Fake::default();
    fake.publish(fixture("nodejs-7.json"), fixture("nodejs-7.json.minisig"));
    let settings = start(&fake).await;
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "other.key.pub");

    // WHEN Node.js is asked for
    let end = ensure_and_wait(&store, &settings).await;

    // THEN it fails readably, and no archive was requested
    assert!(
        matches!(&end, NodeState::Failed { error } if error.contains("signature")),
        "{end:?}"
    );
    assert_eq!(fake.archive_hits(), 0);
    assert!(matches!(state(&store.root), NodeState::Failed { .. }));
}

// ── Offline import ──────────────────────────────────────────────────────────

fn offline_dir(with_signature: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MANIFEST_FILE), fixture("nodejs-7.json")).unwrap();
    if with_signature {
        std::fs::write(
            dir.path().join(crate::engine_store::SIGNATURE_FILE),
            fixture("nodejs-7.json.minisig"),
        )
        .unwrap();
    }
    std::fs::write(dir.path().join(ARCHIVE), fixture(ARCHIVE)).unwrap();
    dir
}

#[test]
fn test_node_is_imported_from_a_disk_through_the_same_checks() {
    // GIVEN a disk holding the manifest, its signature and the archive
    let disk = offline_dir(true);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");

    // WHEN it is imported
    let installed = import_from_dir(&store, disk.path()).unwrap();

    // THEN Node.js is installed where a download puts it
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].engine, Engine::NodeJs);
    assert!(matches!(state(&store.root), NodeState::Ready { ref tag, .. } if tag == "v22.11.0"));
}

#[test]
fn test_import_without_signature_is_refused() {
    // GIVEN a disk holding the manifest and the archive, but no signature
    let disk = offline_dir(false);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");

    // WHEN it is imported
    let err = import_from_dir(&store, disk.path()).unwrap_err();

    // THEN it is refused, and nothing is installed
    assert!(
        matches!(err, EngineStoreError::MissingSignature { .. }),
        "{err}"
    );
    assert!(installed_node(&store.root).is_none());
}

#[test]
fn test_import_with_a_signature_of_another_key_is_refused() {
    // GIVEN a disk holding a genuine set, and a build that trusts another key
    let disk = offline_dir(true);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "other.key.pub");

    // WHEN it is imported
    let err = import_from_dir(&store, disk.path()).unwrap_err();

    // THEN it is refused on the signature
    assert!(matches!(err, EngineStoreError::Signature { .. }), "{err}");
}

// ── The stdio transport finds what the store installed ──────────────────────

#[cfg(unix)]
#[test]
fn test_the_mcp_transport_launches_the_node_the_store_installed() {
    // GIVEN Node.js imported into a store
    let disk = offline_dir(true);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_key(dir.path(), "node-dist.key.pub");
    let installed = import_from_dir(&store, disk.path()).unwrap();

    // WHEN the stdio transport resolves `npx` against that store
    let launch = apollia_mcp::transport::node::resolve("npx", &[], Some(&store.root))
        .unwrap()
        .unwrap();

    // THEN it runs the `node` of that install, by absolute path
    assert!(launch.program.starts_with(&installed[0].path));
    assert!(launch.program.ends_with("bin/node"));
    assert!(launch.args[0].ends_with("npx-cli.js"));
}
