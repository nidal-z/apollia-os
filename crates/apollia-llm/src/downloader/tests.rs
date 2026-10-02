use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn test_client() -> reqwest::Client {
    apollia_core::net::loopback_client_builder()
        .build()
        .expect("test client")
}

fn request(url: String, dest_dir: &std::path::Path, filename: &str) -> DownloadRequest {
    DownloadRequest {
        url,
        dest_dir: dest_dir.to_path_buf(),
        filename: Some(filename.to_string()),
        hf_token: None,
        repo_id: None,
        expected_sha256: None,
    }
}

/// Records the message of every tracing event, so a test can assert that a
/// named event was published. No subscriber crate is a dependency here, and
/// this needs none.
#[derive(Default, Clone)]
struct EventNames(Arc<Mutex<Vec<String>>>);

struct MessageVisitor<'a>(&'a mut Option<String>);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = Some(format!("{value:?}"));
        }
    }
}

impl tracing::Subscriber for EventNames {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = None;
        event.record(&mut MessageVisitor(&mut message));
        if let Some(m) = message {
            self.0.lock().expect("event lock").push(m);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// Serialises the tests that reach the `llm.download.integrity_mismatch`
/// event site. Tracing caches the interest of a call site on first use, so a
/// test that hits the site on another thread with no subscriber can leave the
/// site disabled while a recording test runs, and the recording test then
/// misses the event.
static INTEGRITY_EVENT_SITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn serve(server: &MockServer, at: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

fn recorder() -> (ProgressCallback, Arc<Mutex<Vec<DownloadProgress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let cb: ProgressCallback = Arc::new(move |p: DownloadProgress| {
        sink.lock().expect("progress lock").push(p);
    });
    (cb, seen)
}

#[tokio::test]
async fn http_error_status_writes_nothing() {
    // GIVEN a server that answers 404 with a body
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.gguf"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Entry not found"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = request(
        format!("{}/model.gguf", server.uri()),
        dir.path(),
        "model.gguf",
    );
    let (cb, _) = recorder();

    // WHEN the file is downloaded
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the download fails and no file is left on disk
    assert!(result.is_err(), "a 404 must not count as a download");
    assert!(!dir.path().join("model.gguf").exists());
}

#[tokio::test]
async fn filename_with_subfolder_creates_parent() {
    // GIVEN a repository file that lives in a subfolder
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/Q4_K_M/model-00001-of-00002.gguf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"GGUF part".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = request(
        format!("{}/Q4_K_M/model-00001-of-00002.gguf", server.uri()),
        dir.path(),
        "Q4_K_M/model-00001-of-00002.gguf",
    );
    let (cb, _) = recorder();

    // WHEN the file is downloaded
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the parent folder is created and the file lands inside it
    assert!(result.is_ok(), "download failed: {result:?}");
    let written = std::fs::read(dir.path().join("Q4_K_M/model-00001-of-00002.gguf"))
        .expect("file in its subfolder");
    assert_eq!(written, b"GGUF part");
}

#[tokio::test]
async fn http_error_status_is_reported_as_failed_progress() {
    // GIVEN a server that answers 404 with a body
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.gguf"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Entry not found"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = request(
        format!("{}/model.gguf", server.uri()),
        dir.path(),
        "model.gguf",
    );
    let (cb, seen) = recorder();

    // WHEN the file is downloaded
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the error names the status and the last progress says Failed
    assert!(matches!(
        result,
        Err(DownloadError::HttpStatus { status: 404, .. })
    ));
    let seen = seen.lock().expect("progress lock");
    let last = seen.last().expect("a progress event");
    assert_eq!(last.status, DownloadStatus::Failed);
    assert!(last.error.as_deref().unwrap_or_default().contains("404"));
}

#[tokio::test]
async fn bytes_that_do_not_match_the_published_sha256_are_deleted() {
    // GIVEN a server that serves other bytes than the ones the publisher hashed
    let server = MockServer::start().await;
    serve(&server, "/model.gguf", b"substituted content").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut req = request(
        format!("{}/model.gguf", server.uri()),
        dir.path(),
        "model.gguf",
    );
    let published = sha256_hex(b"genuine content");
    req.expected_sha256 = Some(published.clone());
    let (cb, seen) = recorder();
    let _site = INTEGRITY_EVENT_SITE.lock().await;
    let events = EventNames::default();
    let _guard = tracing::subscriber::set_default(events.clone());

    // WHEN the file is downloaded
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the download fails on the fingerprint, the file is gone, the event
    // is published, and the reason reaches the progress listener
    match result {
        Err(DownloadError::IntegrityMismatch {
            file,
            expected,
            actual,
        }) => {
            assert_eq!(file, "model.gguf");
            assert_eq!(expected, published);
            assert_eq!(actual, sha256_hex(b"substituted content"));
        }
        other => panic!("expected an integrity mismatch, got {other:?}"),
    }
    assert!(!dir.path().join("model.gguf").exists());
    assert!(events
        .0
        .lock()
        .expect("event lock")
        .iter()
        .any(|m| m == "llm.download.integrity_mismatch"));
    let seen = seen.lock().expect("progress lock");
    let last = seen.last().expect("a progress event");
    assert_eq!(last.status, DownloadStatus::Failed);
    assert!(last
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("integrity check failed"));
}

#[tokio::test]
async fn matching_sha256_is_carried_by_the_result() {
    // GIVEN a server that serves exactly the bytes the publisher hashed, the
    // reference written in upper case
    let server = MockServer::start().await;
    serve(&server, "/model.gguf", b"genuine content").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut req = request(
        format!("{}/model.gguf", server.uri()),
        dir.path(),
        "model.gguf",
    );
    let published = sha256_hex(b"genuine content");
    req.expected_sha256 = Some(published.to_ascii_uppercase());
    let (cb, seen) = recorder();

    // WHEN the file is downloaded
    let outcome = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb)
        .await
        .expect("download succeeds");

    // THEN the outcome and the Completed event carry the verified fingerprint
    assert_eq!(outcome.sha256, published);
    assert!(outcome.integrity_verified);
    assert_eq!(outcome.bytes, b"genuine content".len() as u64);
    let seen = seen.lock().expect("progress lock");
    let last = seen.last().expect("a progress event");
    assert_eq!(last.status, DownloadStatus::Completed);
    assert_eq!(last.sha256.as_deref(), Some(published.as_str()));
    assert!(last.integrity_verified);
}

#[tokio::test]
async fn without_a_reference_the_hash_is_reported_unverified() {
    // GIVEN a download that carries no published fingerprint
    let server = MockServer::start().await;
    serve(&server, "/ggml-base.bin", b"whisper weights").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = request(
        format!("{}/ggml-base.bin", server.uri()),
        dir.path(),
        "ggml-base.bin",
    );
    let (cb, _) = recorder();

    // WHEN the file is downloaded
    let outcome = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb)
        .await
        .expect("download succeeds");

    // THEN the hash is known but not marked as verified
    assert_eq!(outcome.sha256, sha256_hex(b"whisper weights"));
    assert!(!outcome.integrity_verified);
}

#[tokio::test]
async fn resumed_download_is_hashed_over_the_whole_file() {
    // GIVEN half a file already on disk and a server that serves the other half
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.gguf"))
        .and(wiremock::matchers::header("range", "bytes=7-"))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(b" content".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("model.gguf"), b"genuine").expect("partial file");
    let mut req = request(
        format!("{}/model.gguf", server.uri()),
        dir.path(),
        "model.gguf",
    );
    req.expected_sha256 = Some(sha256_hex(b"genuine content"));
    let (cb, _) = recorder();

    // WHEN the download resumes
    let outcome = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb)
        .await
        .expect("download succeeds");

    // THEN the fingerprint covers the bytes from before and after the resume
    assert!(outcome.integrity_verified);
    assert_eq!(
        std::fs::read(dir.path().join("model.gguf")).expect("file"),
        b"genuine content"
    );
}

#[tokio::test]
async fn filename_climbing_out_of_the_destination_is_refused() {
    // GIVEN a destination name that walks up out of the models folder
    let server = MockServer::start().await;
    serve(&server, "/x.gguf", b"payload").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("models");
    let req = request(format!("{}/x.gguf", server.uri()), &dest, "../escaped.gguf");
    let (cb, _) = recorder();

    // WHEN the file is downloaded
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the name is refused and nothing is written outside the folder
    assert!(matches!(result, Err(DownloadError::InvalidFilename(_))));
    assert!(!dir.path().join("escaped.gguf").exists());
}

#[tokio::test]
async fn an_embeddings_entry_downloads_through_the_same_verified_path() {
    // GIVEN the ordinary request for the repository and file of the
    // multilingual-e5-small entry, checked against its pinned digest, served
    // by a host that substitutes other bytes for that file
    let catalogue = crate::recommend::EmbeddingCatalogue::embedded().expect("entries load");
    let e5 = catalogue
        .get("multilingual-e5-small")
        .expect("e5 is in the category");
    let server = MockServer::start().await;
    serve(&server, "/e5.gguf", b"not the pinned weights").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = DownloadRequest {
        url: format!("{}/e5.gguf", server.uri()),
        dest_dir: dir.path().to_path_buf(),
        filename: Some(e5.file.clone()),
        hf_token: None,
        repo_id: Some(e5.repo.clone()),
        expected_sha256: Some(e5.sha256.clone()),
    };
    let (cb, _) = recorder();
    let _site = INTEGRITY_EVENT_SITE.lock().await;
    let events = EventNames::default();
    let _guard = tracing::subscriber::set_default(events.clone());

    // WHEN the downloader every model goes through runs it
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the pinned digest refuses the bytes, nothing stays on disk, and
    // the same integrity event is published
    match result {
        Err(DownloadError::IntegrityMismatch { expected, .. }) => {
            assert_eq!(expected, e5.sha256);
        }
        other => panic!("expected an integrity refusal, got {other:?}"),
    }
    assert!(!dir.path().join(&e5.file).exists());
    assert!(events
        .0
        .lock()
        .expect("event lock")
        .iter()
        .any(|m| m == "llm.download.integrity_mismatch"));
}

#[tokio::test]
async fn a_dictation_entry_served_with_other_bytes_is_refused_and_deleted() {
    // GIVEN the request for the French dictation file of the catalogue,
    // checked against its pinned digest, served by a host that substitutes
    // other bytes for that file
    let catalogue = crate::recommend::DictationCatalogue::embedded().expect("entries load");
    let french = catalogue
        .get("whisper-large-v3-french-q5_0")
        .expect("the French file is in the category");
    let server = MockServer::start().await;
    serve(&server, "/fr.bin", b"lmgg not the pinned weights").await;
    let dir = tempfile::tempdir().expect("tempdir");
    let req = DownloadRequest {
        url: format!("{}/fr.bin", server.uri()),
        dest_dir: dir.path().to_path_buf(),
        filename: Some(french.file.clone()),
        hf_token: None,
        repo_id: None,
        expected_sha256: Some(french.sha256.clone()),
    };
    let (cb, _) = recorder();
    let _site = INTEGRITY_EVENT_SITE.lock().await;

    // WHEN the downloader every model goes through runs it
    let result = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb).await;

    // THEN the pinned digest refuses the bytes and nothing stays on disk
    match result {
        Err(DownloadError::IntegrityMismatch { expected, .. }) => {
            assert_eq!(expected, french.sha256);
        }
        other => panic!("expected an integrity refusal, got {other:?}"),
    }
    assert!(!dir.path().join(&french.file).exists());
}

/// A GGUF header (version 3, no tensor) naming the whisper architecture.
fn whisper_gguf() -> Vec<u8> {
    let (key, value) = (b"general.architecture", b"whisper");
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes());
    out.extend_from_slice(&(key.len() as u64).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value);
    out
}

#[tokio::test]
async fn a_downloaded_dictation_model_enters_the_index_as_dictation_in_both_formats() {
    // GIVEN a whisper.cpp file (GGML header, .bin) and a whisper GGUF, each
    // served with the digest it is checked against
    let mut ggml = b"lmgg".to_vec();
    ggml.extend_from_slice(b"hparams");
    let gguf = whisper_gguf();
    let server = MockServer::start().await;
    serve(&server, "/ggml-small.bin", &ggml).await;
    serve(&server, "/whisper-small.gguf", &gguf).await;
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN both are downloaded through the verified path and the index of the
    // folder is rebuilt
    for (name, bytes) in [("ggml-small.bin", &ggml), ("whisper-small.gguf", &gguf)] {
        let mut req = request(format!("{}/{name}", server.uri()), dir.path(), name);
        req.expected_sha256 = Some(sha256_hex(bytes));
        let (cb, _) = recorder();
        let outcome = run_download(test_client(), req, "t".into(), CancellationToken::new(), cb)
            .await
            .expect("download succeeds");
        assert!(outcome.integrity_verified, "{name} is verified");
    }
    let mut index = crate::ModelIndex::default();
    index.rebuild(dir.path()).expect("rebuild");

    // THEN each is one model of the dictation category, whatever its format
    for id in ["ggml-small", "whisper-small"] {
        let model = index.get(id).unwrap_or_else(|| panic!("{id} is indexed"));
        assert_eq!(
            model.effective_category(),
            crate::ModelCategory::Dictation,
            "{id}"
        );
    }
}
