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
