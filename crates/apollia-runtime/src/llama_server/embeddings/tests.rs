use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::llama_server::test_support::{live_instance, supervisor_with_ceiling};
use crate::llama_server::ModelKind;

/// A stand-in for `llama-server --embeddings`: records its arguments, answers
/// `/health`, and answers `/v1/embeddings` with one vector per input, the
/// request body recorded beside the arguments.
const FAKE_ENGINE: &str = r#"#!/usr/bin/env python3
import http.server, json, os, sys
here = os.path.dirname(os.path.abspath(sys.argv[0]))
args = sys.argv[1:]
with open(os.path.join(here, "args.txt"), "a") as f:
    f.write(" ".join(args) + "\n")
port = int(args[args.index("--port") + 1])
class Engine(http.server.BaseHTTPRequestHandler):
    def reply(self, body):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def do_GET(self):
        self.reply(b'{"status":"ok"}')
    def do_POST(self):
        sent = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with open(os.path.join(here, "bodies.txt"), "a") as f:
            f.write(json.dumps(sent) + "\n")
        data = [{"index": i, "embedding": [float(i + 1), 1.0]} for i in range(len(sent["input"]))]
        self.reply(json.dumps({"object": "list", "data": data}).encode())
    def log_message(self, *a):
        pass
http.server.HTTPServer(("127.0.0.1", port), Engine).serve_forever()
"#;

/// Write the stand-in engine as an executable in `dir`, and return its path.
pub(crate) fn fake_engine(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("llama-server");
    std::fs::write(&bin, FAKE_ENGINE).expect("write the stand-in engine");
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

/// The argument lines the stand-in engine recorded in `dir`.
pub(crate) fn launches(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("args.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn test_the_instance_is_launched_for_embeddings_with_the_entry_pooling() {
    // GIVEN an embeddings model whose entry pools by the mean
    // WHEN its argument vector is built
    let args = build_embedding_args("/models/e5.gguf", Pooling::Mean, 512, 8123).join(" ");

    // THEN it serves embeddings with that pooling, on the loopback, with a
    // micro-batch as long as the window, and without the chat flags
    assert!(
        args.starts_with("-m /models/e5.gguf --embeddings --pooling mean"),
        "{args}"
    );
    assert!(args.contains("-c 512 -b 512 -ub 512"), "{args}");
    assert!(args.ends_with("--host 127.0.0.1 --port 8123"), "{args}");
    assert!(!args.contains("--jinja"), "{args}");
    let last = build_embedding_args("/m.gguf", Pooling::Last, 512, 1).join(" ");
    assert!(last.contains("--pooling last"), "{last}");
}

#[test]
fn test_the_window_is_the_entry_context_capped() {
    // GIVEN entries reading 512 and 32 768 tokens
    // WHEN their windows are computed
    // THEN the short one keeps its own, the long one is capped
    assert_eq!(embedding_window(512), 512);
    assert_eq!(embedding_window(32_768), EMBEDDING_MAX_CTX);
}

#[tokio::test]
async fn test_embeddings_never_evict_the_chat_model_under_a_ceiling_of_one() {
    // GIVEN a chat model loaded and the residency ceiling at one
    let tmp = tempfile::tempdir().expect("tempdir");
    let sup = LlamaServerSupervisor::with_binary_for_test(
        fake_engine(tmp.path()),
        1,
        &["/models/chat.gguf"],
    )
    .await;

    // WHEN embeddings are asked for, twice
    let first = sup
        .ensure_embedding_model("/models/e5.gguf", Pooling::Mean, 512)
        .await
        .expect("the stand-in becomes healthy");
    let second = sup
        .ensure_embedding_model("/models/e5.gguf", Pooling::Mean, 512)
        .await
        .expect("still served");

    // THEN the chat model is still loaded, the embeddings instance is served on
    // the loopback and launched once, with --embeddings and the pooling
    let chat: Vec<String> = sup
        .instances
        .lock()
        .await
        .iter()
        .map(|i| i.model_path.clone())
        .collect();
    assert_eq!(chat, vec!["/models/chat.gguf".to_owned()]);
    assert!(first.base_url().starts_with("http://127.0.0.1:"));
    assert_eq!(second.base_url(), first.base_url());
    let launched = launches(tmp.path());
    assert_eq!(launched.len(), 1, "{launched:?}");
    assert!(
        launched[0].contains("--embeddings --pooling mean"),
        "{launched:?}"
    );
    sup.shutdown_in_place().await;
}

#[tokio::test]
async fn test_the_loaded_list_shows_the_embeddings_instance_with_its_kind() {
    // GIVEN a chat model loaded and an embeddings instance started beside it
    let tmp = tempfile::tempdir().expect("tempdir");
    let sup = LlamaServerSupervisor::with_binary_for_test(
        fake_engine(tmp.path()),
        1,
        &["/models/chat.gguf"],
    )
    .await;
    drop(
        sup.ensure_embedding_model("/models/e5.gguf", Pooling::Mean, 512)
            .await
            .expect("served"),
    );

    // WHEN the loaded models are listed
    let listed = sup.loaded_models().await;

    // THEN both appear, the embeddings one with its kind, most recent first,
    // and the ceiling still counts the chat model alone
    let kinds: Vec<(String, ModelKind)> =
        listed.iter().map(|m| (m.model.clone(), m.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            ("e5".to_owned(), ModelKind::Embeddings),
            ("chat".to_owned(), ModelKind::Chat),
        ]
    );
    assert_eq!(sup.max_loaded(), 1);

    // AND it unloads like any model
    assert_eq!(sup.unload_model("/models/e5.gguf").await, Ok(1));
    assert_eq!(sup.loaded_models().await.len(), 1);
    sup.shutdown_in_place().await;
}

/// Put a stand-in embeddings instance in the slot, last called at `at`.
async fn resident_since(sup: &LlamaServerSupervisor, at: Instant) {
    let instance = live_instance(sup, "/models/e5.gguf", 9100).await;
    *sup.embedding.lock().await = Some(EmbeddingInstance {
        instance,
        pooling: Pooling::Mean,
        last_call: at,
    });
}

#[tokio::test]
async fn test_the_instance_stops_after_ten_minutes_without_a_call() {
    // GIVEN an embeddings instance last called at t0
    let sup = supervisor_with_ceiling(1);
    let t0 = Instant::now();
    resident_since(&sup, t0).await;

    // WHEN the clock reads one second short of ten minutes, then ten minutes
    let early = sup
        .stop_idle_embedding(t0 + EMBEDDING_IDLE - Duration::from_secs(1))
        .await;
    let still_there = sup.embedding.lock().await.is_some();
    let due = sup.stop_idle_embedding(t0 + EMBEDDING_IDLE).await;

    // THEN it is kept at the first reading and stopped at the second
    assert_eq!(EMBEDDING_IDLE, Duration::from_secs(600));
    assert!(!early);
    assert!(still_there);
    assert!(due);
    assert!(sup.embedding.lock().await.is_none());
}

#[tokio::test]
async fn test_an_instance_answering_a_request_is_not_stopped_for_idleness() {
    // GIVEN an embeddings instance idle for an hour, a request in progress
    let sup = supervisor_with_ceiling(1);
    let t0 = Instant::now();
    resident_since(&sup, t0).await;
    let lease = {
        let slot = sup.embedding.lock().await;
        let resident = slot.as_ref().expect("resident");
        ModelLease::new(
            resident.instance.base_url(),
            resident.instance.n_ctx,
            Arc::clone(&resident.instance.in_flight),
        )
    };

    // WHEN the idle check runs an hour later
    let stopped = sup
        .stop_idle_embedding(t0 + Duration::from_secs(3600))
        .await;

    // THEN it is kept until the request ends
    assert!(!stopped);
    drop(lease);
    assert!(
        sup.stop_idle_embedding(t0 + Duration::from_secs(3600))
            .await
    );
}
