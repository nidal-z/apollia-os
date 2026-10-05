//! The journal of the compatible server, against a signed journal, a fake
//! engine, and the real batch engine for the call to an agent: one entry per
//! request whatever its fate, who called and from where, no content, a chain
//! that verifies, and the run an agent call started.

use std::net::SocketAddr;
use std::sync::Arc;

use apollia_tools::TaskRepository;
use axum::body::{Body, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use futures::StreamExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use super::agents_tests::{fixture, CLERK};
use super::journal::{with_call_journal, CallJournal, ServerCallRecord, SERVER_CALLS_RUN_ID};
use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::origin::CallOrigin;
use super::relay::RelayState;
use super::relay_tests::{registry, serve_engine};
use super::router::{compat_router, serve};
use super::tokens::ServerTokensHandle;
use crate::audit_journal::verify::verify_entries;
use crate::audit_journal::{AuditJournalHandle, HmacSigner, JournalEntry, JournalEntryKind};
use crate::llama_server::LlamaServerSupervisor;

const KEY: &[u8] = b"server-call-journal-key-for-tests";

/// What the caller says and what the engine answers: neither may reach the
/// journal.
const QUESTION: &str = "SECRET-QUESTION-7c1";
const ANSWER: &str = "SECRET-ANSWER-9d4";

const LOOPBACK: &str = "127.0.0.1:50100";
const LAN: &str = "192.168.1.20:50200";

/// Every field a `server_call` payload may hold.
const FIELDS: [&str; 14] = [
    "event",
    "method",
    "endpoint",
    "target",
    "target_kind",
    "token_name",
    "caller_address",
    "origin",
    "status",
    "prompt_tokens",
    "completion_tokens",
    "duration_ms",
    "task_id",
    "run_id",
];

/// The engine: a stream whose last chunk states the usage when the body asks
/// for one, else a chat completion with its usage.
async fn engine_answer(body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if body.get("stream") == Some(&Value::Bool(true)) {
        let chunks = [
            format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{ANSWER}\"}}}}]}}\n\n"),
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":31,\"completion_tokens\":6}}\n\n"
                .to_owned(),
            "data: [DONE]\n\n".to_owned(),
        ];
        let chunks =
            futures::stream::iter(chunks).map(|chunk| Ok::<_, std::io::Error>(Bytes::from(chunk)));
        return (
            [("content-type", "text/event-stream")],
            Body::from_stream(chunks),
        )
            .into_response();
    }
    axum::Json(json!({
        "choices": [{"message": {"role": "assistant", "content": ANSWER}}],
        "usage": {"prompt_tokens": 21, "completion_tokens": 8, "total_tokens": 29}
    }))
    .into_response()
}

/// A compatible server over one chat model, its requests journaled in a
/// signed journal.
struct Rig {
    journal: AuditJournalHandle,
    tokens: ServerTokensHandle,
    router: Router,
    _supervisor: Arc<LlamaServerSupervisor>,
    _registry: TempDir,
    _dir: TempDir,
}

async fn signed_journal(dir: &std::path::Path) -> AuditJournalHandle {
    AuditJournalHandle::open_with_key_bytes(&dir.join("audit_journal.db"), KEY.to_vec())
        .await
        .unwrap()
}

/// The server over the model `acme/chat`, served by an engine on
/// `engine_port`; behind the token check when `require_token` says so.
async fn rig_on(engine_port: u16, require_token: bool) -> Rig {
    let dir = TempDir::new().unwrap();
    let journal = signed_journal(dir.path()).await;
    let tokens = ServerTokensHandle::spawn(dir.path().join("system.db"));
    let supervisor =
        LlamaServerSupervisor::with_resident_for_test(&[("/models/chat.gguf", engine_port)], None)
            .await;
    let registry = registry();
    let models = LocalModels::new(
        Some(supervisor.clone()),
        None,
        Some(registry.path().to_path_buf()),
    );
    let router = compat_router(
        require_token.then(|| tokens.clone()),
        RelayState::new(models).unwrap(),
        ServerLogsHandle::spawn(&crate::eventbus::EventBus::new().0),
        false,
    );
    Rig {
        router: with_call_journal(
            router,
            Some(CallJournal::new(journal.clone(), tokens.clone())),
        ),
        journal,
        tokens,
        _supervisor: supervisor,
        _registry: registry,
        _dir: dir,
    }
}

/// An engine that accepts a connection and closes it without answering, on
/// a port held for the whole test.
async fn silent_engine() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });
    port
}

/// The server over a live fake engine.
async fn rig(require_token: bool) -> Rig {
    let port = serve_engine(Router::new().fallback(engine_answer)).await;
    rig_on(port, require_token).await
}

/// Sends a request as the accept loop hands it over: with the address of its
/// peer and the origin read from it. Returns the status and the whole body.
async fn send(
    router: &Router,
    peer: &str,
    path: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, String) {
    let peer: SocketAddr = peer.parse().unwrap();
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut request = request.body(Body::from(body)).unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request.extensions_mut().insert(CallOrigin::of_peer(peer));
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn ask(stream: bool) -> String {
    json!({
        "model": "acme/chat",
        "stream": stream,
        "messages": [{"role": "user", "content": QUESTION}]
    })
    .to_string()
}

async fn calls(journal: &AuditJournalHandle) -> Vec<JournalEntry> {
    journal.query_run(SERVER_CALLS_RUN_ID).await
}

/// The one call the journal holds.
async fn only_call(journal: &AuditJournalHandle) -> Value {
    let mut entries = calls(journal).await;
    assert_eq!(entries.len(), 1, "{entries:?}");
    let entry = entries.remove(0);
    assert_eq!(entry.kind, JournalEntryKind::ServerCall);
    entry.payload
}

/// `payload` without its duration, which no test can foretell.
fn timeless(mut payload: Value) -> Value {
    let duration = payload
        .as_object_mut()
        .and_then(|fields| fields.remove("duration_ms"));
    assert!(duration.is_some_and(|d| d.is_u64()), "{payload}");
    payload
}

#[tokio::test]
async fn test_an_answered_call_names_endpoint_model_caller_status_and_tokens() {
    // GIVEN the server over a model whose engine states its usage
    let rig = rig(false).await;

    // WHEN the machine itself asks the model a question
    let (status, body) = send(
        &rig.router,
        LOOPBACK,
        "/v1/chat/completions?trace=SECRET-QUERY",
        ask(false),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(ANSWER), "{body}");

    // THEN one entry says the endpoint, the model, the caller and its origin,
    // the status and the tokens the engine stated
    let payload = only_call(&rig.journal).await;
    assert_eq!(
        timeless(payload),
        json!({
            "event": "server_call",
            "method": "POST",
            "endpoint": "/v1/chat/completions",
            "target": "acme/chat",
            "target_kind": "model",
            "caller_address": "127.0.0.1",
            "origin": "loopback",
            "status": 200,
            "prompt_tokens": 21,
            "completion_tokens": 8,
        })
    );
}

#[tokio::test]
async fn test_a_call_without_a_token_is_journaled_as_refused_and_names_no_token() {
    // GIVEN the server behind the token check
    let rig = rig(true).await;

    // WHEN a device of the local network calls with a value that is no token
    let (status, _) = send(
        &rig.router,
        LAN,
        "/v1/chat/completions",
        ask(false),
        &[("authorization", "Bearer apo_not-a-token-of-this-server")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // THEN the refusal has its entry: the 401, the address and the network it
    // came from, no token name, no model since the body was never read
    let payload = only_call(&rig.journal).await;
    assert_eq!(
        timeless(payload.clone()),
        json!({
            "event": "server_call",
            "method": "POST",
            "endpoint": "/v1/chat/completions",
            "caller_address": "192.168.1.20",
            "origin": "local_network",
            "status": 401,
        })
    );
    // AND the value presented is not in it
    assert!(!payload.to_string().contains("not-a-token"), "{payload}");
}

#[tokio::test]
async fn test_a_call_with_a_token_names_the_token_and_never_its_value() {
    // GIVEN the server behind the token check, and a token named for its user
    let rig = rig(true).await;
    let created = rig.tokens.create("billing-app".to_owned()).await.unwrap();

    // WHEN a device of the local network calls with it, in either header
    let bearer = format!("Bearer {}", created.value);
    for header in [
        ("authorization", bearer.as_str()),
        ("x-api-key", created.value.as_str()),
    ] {
        let (status, body) = send(
            &rig.router,
            LAN,
            "/v1/chat/completions",
            ask(false),
            &[header],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    // THEN both entries name the token, and nothing of its value is kept
    let entries = calls(&rig.journal).await;
    assert_eq!(entries.len(), 2);
    for entry in &entries {
        assert_eq!(entry.payload["token_name"], "billing-app");
        assert_eq!(entry.payload["origin"], "local_network");
        let written = entry.payload.to_string();
        assert!(!written.contains(&created.value), "{written}");
    }
}

#[tokio::test]
async fn test_a_malformed_call_is_journaled_with_its_400() {
    // GIVEN the server
    let rig = rig(false).await;

    // WHEN a body that is not a JSON object is sent, then one naming no model
    let (first, _) = send(
        &rig.router,
        LOOPBACK,
        "/v1/chat/completions",
        format!("not json {QUESTION}"),
        &[],
    )
    .await;
    let (second, _) = send(
        &rig.router,
        LOOPBACK,
        "/v1/messages",
        json!({"messages": [{"role": "user", "content": QUESTION}]}).to_string(),
        &[],
    )
    .await;
    assert_eq!(
        (first, second),
        (StatusCode::BAD_REQUEST, StatusCode::BAD_REQUEST)
    );

    // THEN each refusal has its entry, with its endpoint and its 400, and
    // nothing of the body
    let entries = calls(&rig.journal).await;
    let seen: Vec<(Value, Value)> = entries
        .iter()
        .map(|e| (e.payload["endpoint"].clone(), e.payload["status"].clone()))
        .collect();
    assert_eq!(
        seen,
        [
            (json!("/v1/chat/completions"), json!(400)),
            (json!("/v1/messages"), json!(400)),
        ]
    );
    for entry in &entries {
        assert!(entry.payload.get("target").is_none(), "{}", entry.payload);
        assert!(!entry.payload.to_string().contains("SECRET"));
    }
}

#[tokio::test]
async fn test_a_failed_call_is_journaled_with_its_failure() {
    // GIVEN a model whose engine hangs up without answering, and a path
    // nothing serves
    let rig = rig_on(silent_engine().await, false).await;

    // WHEN the model is called, then the unknown path
    let (failed, body) = send(
        &rig.router,
        LOOPBACK,
        "/v1/chat/completions",
        ask(false),
        &[],
    )
    .await;
    assert_eq!(failed, StatusCode::BAD_GATEWAY, "{body}");
    let (unknown, _) = send(&rig.router, LOOPBACK, "/v1/nothing", ask(false), &[]).await;
    assert_eq!(unknown, StatusCode::NOT_FOUND);

    // THEN the failure has its entry, naming the model and the 502 and no
    // token count, and so has the unknown path with its 404
    let entries = calls(&rig.journal).await;
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_eq!(
        timeless(entries[0].payload.clone()),
        json!({
            "event": "server_call",
            "method": "POST",
            "endpoint": "/v1/chat/completions",
            "target": "acme/chat",
            "target_kind": "model",
            "caller_address": "127.0.0.1",
            "origin": "loopback",
            "status": 502,
        })
    );
    assert_eq!(entries[1].payload["endpoint"], "/v1/nothing");
    assert_eq!(entries[1].payload["status"], 404);
}

#[tokio::test]
async fn test_a_streamed_call_is_relayed_untouched_and_journaled_when_it_closes() {
    // GIVEN the server over an engine that streams, its usage in the last
    // chunk
    let rig = rig(false).await;

    // WHEN a stream is asked for and read to its end
    let (status, body) = send(
        &rig.router,
        LOOPBACK,
        "/v1/chat/completions",
        ask(true),
        &[],
    )
    .await;

    // THEN the client received every chunk as the engine sent it
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(ANSWER), "{body}");
    assert!(body.ends_with("data: [DONE]\n\n"), "{body}");
    assert_eq!(body.matches("data: ").count(), 3, "{body}");
    // AND the entry carries the tokens read in passing
    let payload = only_call(&rig.journal).await;
    assert_eq!(payload["status"], 200);
    assert_eq!(payload["prompt_tokens"], 31);
    assert_eq!(payload["completion_tokens"], 6);
}

#[tokio::test]
async fn test_no_entry_carries_what_was_asked_or_answered() {
    // GIVEN the server behind the token check, and a token
    let rig = rig(true).await;
    let created = rig.tokens.create("notes-app".to_owned()).await.unwrap();
    let key = [("x-api-key", created.value.as_str())];

    // WHEN a question is answered whole, answered as a stream, refused for
    // want of a token, and refused as malformed
    for (body, headers) in [
        (ask(false), &key[..]),
        (ask(true), &key[..]),
        (ask(false), &[][..]),
        (format!("{{\"messages\": \"{QUESTION}\"}}"), &key[..]),
    ] {
        send(&rig.router, LAN, "/v1/chat/completions", body, headers).await;
    }

    // THEN the four entries hold only the fields of a call, each a number, a
    // short name or an address, and nothing of the question, the answer or
    // the token
    let entries = calls(&rig.journal).await;
    assert_eq!(entries.len(), 4);
    for entry in &entries {
        let fields = entry.payload.as_object().unwrap();
        for (name, value) in fields {
            assert!(FIELDS.contains(&name.as_str()), "unexpected field {name}");
            assert!(value.is_string() || value.is_u64(), "{name}: {value}");
        }
        let written = entry.payload.to_string();
        for secret in [QUESTION, ANSWER, created.value.as_str(), "messages"] {
            assert!(!written.contains(secret), "{secret} in {written}");
        }
        let _: ServerCallRecord = serde_json::from_value(entry.payload.clone()).unwrap();
    }
}

#[tokio::test]
async fn test_the_entries_of_the_calls_are_chained_and_signed() {
    // GIVEN the server over a signed journal
    let rig = rig(false).await;

    // WHEN three calls are answered
    for peer in [LOOPBACK, LAN, LOOPBACK] {
        send(&rig.router, peer, "/v1/chat/completions", ask(false), &[]).await;
    }

    // THEN the run of the calls holds the three, each signed, and verifies
    // with signatures required
    let run = calls(&rig.journal).await;
    assert_eq!(run.len(), 3);
    assert!(run.iter().all(|e| e.kind == JournalEntryKind::ServerCall));
    assert!(run.iter().all(|e| e.signature.is_some()));
    let signer = HmacSigner::from_key_bytes(KEY.to_vec()).unwrap();
    let report = verify_entries(SERVER_CALLS_RUN_ID, &run, Some(&signer), true);
    assert!(report.ok, "{report:?}");
    // AND so does the whole journal
    let whole = rig.journal.verify_journal().await.unwrap();
    assert!(whole.ok, "{whole:?}");
    assert_eq!(whole.entries_checked, 3);
}

#[tokio::test]
async fn test_the_address_of_the_caller_is_the_one_of_the_accepted_connection() {
    // GIVEN the server listening for real, its accept loop serving the router
    let rig = rig(false).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cancel = CancellationToken::new();
    let serving = tokio::spawn(serve(listener, rig.router.clone(), cancel.clone()));

    // WHEN a client connects and calls, sending no header that names it
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let body = ask(false);
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\n\
         X-Forwarded-For: 203.0.113.9\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");

    // THEN the entry names the address the connection was accepted from, not
    // the one a header claims
    let payload = only_call(&rig.journal).await;
    assert_eq!(payload["caller_address"], "127.0.0.1");
    assert_eq!(payload["origin"], "loopback");
    cancel.cancel();
    serving.await.unwrap();
}

#[tokio::test]
async fn test_a_call_to_an_agent_names_the_task_and_the_run_it_started() {
    // GIVEN an exposed agent, and the server journaling its requests
    let fx = fixture().await;
    let journal = signed_journal(fx._dir.path()).await;
    let router = with_call_journal(
        fx.router(None),
        Some(CallJournal::new(journal.clone(), fx.tokens.clone())),
    );

    // WHEN the machine calls the agent
    let model = format!("apollia/{CLERK}");
    let (status, body) = send(
        &router,
        LOOPBACK,
        "/v1/chat/completions",
        json!({"model": model, "messages": [{"role": "user", "content": QUESTION}]}).to_string(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answer: Value = serde_json::from_str(&body).unwrap();
    let task_id = answer["id"]
        .as_str()
        .and_then(|id| id.strip_prefix("chatcmpl-"))
        .unwrap()
        .to_owned();

    // THEN the entry of the call names the agent, the task it started and the
    // run that task's own entries are journaled under, with the tokens of
    // every turn
    let tasks = TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(fx._dir.path()))
        .await
        .unwrap();
    let run_id = tasks.get_run_id(&task_id).await.unwrap();
    assert!(run_id.is_some());
    let payload = only_call(&journal).await;
    assert_eq!(
        timeless(payload.clone()),
        json!({
            "event": "server_call",
            "method": "POST",
            "endpoint": "/v1/chat/completions",
            "target": model,
            "target_kind": "agent",
            "caller_address": "127.0.0.1",
            "origin": "loopback",
            "status": 200,
            "prompt_tokens": 250,
            "completion_tokens": 16,
            "task_id": task_id,
            "run_id": run_id,
        })
    );
    // AND nothing of the request reached it
    assert!(!payload.to_string().contains(QUESTION));
}
