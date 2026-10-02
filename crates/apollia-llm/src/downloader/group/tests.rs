//! Variant downloads against a fake repository served locally.

use std::sync::Mutex;

use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::hf_registry::VariantFile;
use crate::model_index::ModelIndex;

fn test_client() -> reqwest::Client {
    apollia_core::net::loopback_client_builder()
        .build()
        .expect("test client")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The git blob identifier of `bytes`, computed here independently of the
/// code under test.
fn git_oid_hex(bytes: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// One file of the fake repository: its path in the repository, where it
/// lands, its content, and whether the repository stores it through LFS (a
/// SHA-256) or outside it (a git blob identifier).
struct Fake {
    repo_path: &'static str,
    dest: &'static str,
    body: &'static [u8],
    lfs: bool,
}

async fn serve_plan(
    format: ModelFormat,
    model_path: &str,
    files: &[Fake],
) -> (MockServer, VariantPlan) {
    let server = MockServer::start().await;
    let mut planned = Vec::new();
    for f in files {
        let at = format!("/org/repo/resolve/main/{}", f.repo_path);
        Mock::given(method("GET"))
            .and(path(at.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(f.body.to_vec()))
            .mount(&server)
            .await;
        planned.push(VariantFile {
            repo_path: f.repo_path.to_owned(),
            dest_name: f.dest.to_owned(),
            url: format!("{}{at}", server.uri()),
            size_bytes: f.body.len() as u64,
            sha256: f.lfs.then(|| sha256_hex(f.body)),
            git_oid: (!f.lfs).then(|| git_oid_hex(f.body)),
        });
    }
    let total_bytes = planned.iter().map(|f| f.size_bytes).sum();
    let plan = VariantPlan {
        repo_id: "org/repo".to_owned(),
        format,
        model_path: model_path.to_owned(),
        origin_file: files[0].repo_path.to_owned(),
        files: planned,
        total_bytes,
    };
    (server, plan)
}

fn recorder() -> (ProgressCallback, Arc<Mutex<Vec<DownloadProgress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let cb: ProgressCallback = Arc::new(move |p: DownloadProgress| {
        sink.lock().expect("progress lock").push(p);
    });
    (cb, seen)
}

async fn run(
    plan: VariantPlan,
    dir: &std::path::Path,
) -> (
    Result<DownloadProgress, DownloadError>,
    Vec<DownloadProgress>,
) {
    let request = VariantDownload {
        plan,
        dest_dir: dir.to_path_buf(),
        hf_token: None,
    };
    let (cb, seen) = recorder();
    let result = run_variant(
        test_client(),
        request,
        "v".into(),
        CancellationToken::new(),
        cb,
    )
    .await;
    let events = seen.lock().expect("progress lock").clone();
    (result, events)
}

/// The progress stream of a successful variant: every event under the model
/// path, against the variant total, never going back, and one `Completed`.
fn assert_one_aggregated_stream(events: &[DownloadProgress], model: &std::path::Path, total: u64) {
    let completed: Vec<_> = events
        .iter()
        .filter(|e| e.status == DownloadStatus::Completed)
        .collect();
    assert_eq!(completed.len(), 1, "one completion for the whole variant");
    assert_eq!(
        events.last().map(|e| &e.status),
        Some(&DownloadStatus::Completed)
    );
    assert!(
        events.iter().all(|e| e.dest_path == model),
        "one progress line"
    );
    assert!(events.iter().all(|e| e.id == "v"), "one download id");
    let in_progress: Vec<u64> = events
        .iter()
        .filter(|e| e.status == DownloadStatus::InProgress)
        .map(|e| e.downloaded_bytes)
        .collect();
    assert!(
        in_progress.windows(2).all(|w| w[0] <= w[1]),
        "progress never goes back"
    );
    assert!(events
        .iter()
        .filter(|e| e.status == DownloadStatus::InProgress)
        .all(|e| e.total_bytes == Some(total)));
    assert_eq!(completed[0].downloaded_bytes, total);
}

fn index_of(dir: &std::path::Path) -> ModelIndex {
    let mut index = ModelIndex::default();
    index.rebuild(dir).expect("rebuild");
    index
}

#[tokio::test]
async fn test_split_gguf_variant_is_one_download_and_one_indexed_model() {
    // GIVEN a fake repository with a quantization split in three parts
    let files = [
        Fake {
            repo_path: "Q8_0/m-Q8_0-00001-of-00003.gguf",
            dest: "Q8_0/m-Q8_0-00001-of-00003.gguf",
            body: b"GGUF part one",
            lfs: true,
        },
        Fake {
            repo_path: "Q8_0/m-Q8_0-00002-of-00003.gguf",
            dest: "Q8_0/m-Q8_0-00002-of-00003.gguf",
            body: b"part two",
            lfs: true,
        },
        Fake {
            repo_path: "Q8_0/m-Q8_0-00003-of-00003.gguf",
            dest: "Q8_0/m-Q8_0-00003-of-00003.gguf",
            body: b"three",
            lfs: true,
        },
    ];
    let (_server, plan) =
        serve_plan(ModelFormat::Gguf, "Q8_0/m-Q8_0-00001-of-00003.gguf", &files).await;
    let total = plan.total_bytes;
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, events) = run(plan, dir.path()).await;

    // THEN one aggregated download completes, every part checked by its digest
    let done = result.expect("variant download");
    assert!(done.integrity_verified);
    let model = dir.path().join("Q8_0/m-Q8_0-00001-of-00003.gguf");
    assert_one_aggregated_stream(&events, &model, total);
    // AND the index holds one model owning the three parts
    let index = index_of(dir.path());
    assert_eq!(index.models.len(), 1);
    assert_eq!(index.models[0].path, model);
    assert_eq!(index.models[0].parts.len(), 3);
}

#[tokio::test]
async fn test_gguf_variant_with_projector_is_one_indexed_model_with_its_annex() {
    // GIVEN a fake vision repository whose plan puts weights and projector in one folder
    let files = [
        Fake {
            repo_path: "Vision-Q4_K_M.gguf",
            dest: "Vision-Q4_K_M/Vision-Q4_K_M.gguf",
            body: b"GGUF weights",
            lfs: true,
        },
        Fake {
            repo_path: "mmproj-Vision-f16.gguf",
            dest: "Vision-Q4_K_M/mmproj-Vision-f16.gguf",
            body: b"GGUF projector",
            lfs: true,
        },
    ];
    let (_server, plan) = serve_plan(
        ModelFormat::Gguf,
        "Vision-Q4_K_M/Vision-Q4_K_M.gguf",
        &files,
    )
    .await;
    let total = plan.total_bytes;
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, events) = run(plan, dir.path()).await;

    // THEN one aggregated download completes
    assert!(result.expect("variant download").integrity_verified);
    let model = dir.path().join("Vision-Q4_K_M/Vision-Q4_K_M.gguf");
    assert_one_aggregated_stream(&events, &model, total);
    // AND the index holds one model, the projector as its annex and not a model
    let index = index_of(dir.path());
    assert_eq!(index.models.len(), 1);
    assert_eq!(index.models[0].path, model);
    assert_eq!(
        index.models[0].annexes,
        vec![dir.path().join("Vision-Q4_K_M/mmproj-Vision-f16.gguf")]
    );
}

#[tokio::test]
async fn test_mlx_variant_is_one_download_and_one_indexed_folder() {
    // GIVEN a fake MLX repository, its small files published with a git blob identifier only
    let files = [
        Fake {
            repo_path: "model.safetensors",
            dest: "Small-4bit/model.safetensors",
            body: b"weights",
            lfs: true,
        },
        Fake {
            repo_path: "config.json",
            dest: "Small-4bit/config.json",
            body: b"{\"model_type\":\"qwen2\"}",
            lfs: false,
        },
        Fake {
            repo_path: "tokenizer.json",
            dest: "Small-4bit/tokenizer.json",
            body: b"{}",
            lfs: false,
        },
    ];
    let (_server, plan) = serve_plan(ModelFormat::Mlx, "Small-4bit", &files).await;
    let total = plan.total_bytes;
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, events) = run(plan, dir.path()).await;

    // THEN one aggregated download completes under the folder, every file checked by a digest
    let done = result.expect("variant download");
    assert!(
        done.integrity_verified,
        "the small files are checked by their git blob identifier"
    );
    let model = dir.path().join("Small-4bit");
    assert_one_aggregated_stream(&events, &model, total);
    // AND the index holds the folder as one MLX model
    let index = index_of(dir.path());
    assert_eq!(index.models.len(), 1);
    assert_eq!(index.models[0].format, ModelFormat::Mlx);
    assert_eq!(index.models[0].path, model);
}

#[tokio::test]
async fn test_variant_file_with_a_wrong_digest_fails_the_whole_variant_once() {
    // GIVEN a split variant whose second part does not match its published digest
    let files = [
        Fake {
            repo_path: "m-00001-of-00002.gguf",
            dest: "m-00001-of-00002.gguf",
            body: b"one",
            lfs: true,
        },
        Fake {
            repo_path: "m-00002-of-00002.gguf",
            dest: "m-00002-of-00002.gguf",
            body: b"two",
            lfs: true,
        },
    ];
    let (_server, mut plan) = serve_plan(ModelFormat::Gguf, "m-00001-of-00002.gguf", &files).await;
    plan.files[1].sha256 = Some(sha256_hex(b"something else"));
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, events) = run(plan, dir.path()).await;

    // THEN it fails with the integrity error, reported once and never as completed
    assert!(matches!(
        result,
        Err(DownloadError::IntegrityMismatch { .. })
    ));
    let failed = events
        .iter()
        .filter(|e| e.status == DownloadStatus::Failed)
        .count();
    assert_eq!(failed, 1);
    assert!(events.iter().all(|e| e.status != DownloadStatus::Completed));
    assert!(!dir.path().join("m-00002-of-00002.gguf").exists());
}

#[tokio::test]
async fn test_variant_file_without_digest_and_wrong_size_is_refused() {
    // GIVEN an MLX file committed outside LFS whose size differs from the declared one
    let files = [
        Fake {
            repo_path: "model.safetensors",
            dest: "S/model.safetensors",
            body: b"weights",
            lfs: true,
        },
        Fake {
            repo_path: "config.json",
            dest: "S/config.json",
            body: b"{}",
            lfs: false,
        },
    ];
    let (_server, mut plan) = serve_plan(ModelFormat::Mlx, "S", &files).await;
    plan.files[1].size_bytes = 99;
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, _) = run(plan, dir.path()).await;

    // THEN the size check refuses it and removes the file
    assert!(matches!(
        result,
        Err(DownloadError::SizeMismatch {
            expected: 99,
            actual: 2,
            ..
        })
    ));
    assert!(!dir.path().join("S/config.json").exists());
}

#[tokio::test]
async fn test_variant_file_outside_lfs_with_a_wrong_git_oid_is_refused() {
    // GIVEN an MLX file committed outside LFS, of the declared size, whose content
    // does not match the git blob identifier its repository publishes
    let files = [
        Fake {
            repo_path: "model.safetensors",
            dest: "S/model.safetensors",
            body: b"weights",
            lfs: true,
        },
        Fake {
            repo_path: "config.json",
            dest: "S/config.json",
            body: b"{}",
            lfs: false,
        },
    ];
    let (_server, mut plan) = serve_plan(ModelFormat::Mlx, "S", &files).await;
    let published = git_oid_hex(b"[]");
    plan.files[1].git_oid = Some(published.clone());
    let dir = tempfile::tempdir().expect("tempdir");

    // WHEN the variant is downloaded
    let (result, events) = run(plan, dir.path()).await;

    // THEN the git blob check refuses it, removes the file, and the variant fails once
    match result {
        Err(DownloadError::GitOidMismatch {
            expected, actual, ..
        }) => {
            assert_eq!(expected, published);
            assert_eq!(actual, git_oid_hex(b"{}"));
        }
        other => panic!("expected a git blob mismatch, got {other:?}"),
    }
    assert!(!dir.path().join("S/config.json").exists());
    let failed = events
        .iter()
        .filter(|e| e.status == DownloadStatus::Failed)
        .count();
    assert_eq!(failed, 1);
    assert!(events.iter().all(|e| e.status != DownloadStatus::Completed));
}

#[test]
fn test_git_oid_matches_the_identifier_git_publishes() {
    // GIVEN the content "hello\n", whose blob git names ce013625030ba8dba906f756967f9e9ca394464a
    let path = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(path.path(), b"hello\n").expect("write");

    // WHEN its git blob identifier is computed
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    let oid = rt.block_on(git_blob_oid(path.path())).expect("oid");

    // THEN it is the identifier `git hash-object` prints
    assert_eq!(oid, "ce013625030ba8dba906f756967f9e9ca394464a");
}
