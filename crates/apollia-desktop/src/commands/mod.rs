//! Tauri IPC commands: translation layer between the Svelte frontend and the
//! Apollia runtime handles.
//!
//! No business logic: each command delegates entirely to the existing handles
//! (`AgentRegistryHandle`, `TaskRouterHandle`, `PendingApprovals`) or to the
//! internal REST API for complex operations (timeline, start agent, resume
//! task).

pub mod agent_adjust;
pub mod agent_create;
pub mod agent_export;
pub mod agent_packages;
pub mod agent_sheet;
pub mod agents;
pub mod ai_model_detail;
pub mod ai_models;
pub mod artifacts;
pub mod authorizations;
#[cfg(debug_assertions)]
pub mod automation;
pub mod background_service;
pub mod batch_view;
pub mod batches;
pub mod chat;
pub mod cli;
pub mod config;
pub mod connections;
pub mod documentation;
pub mod embeddings;
pub mod engine_installation;
pub mod example_dataset;
pub mod hardware_window;
pub mod hf_token;
pub mod hitl;
pub mod integrations;
pub mod journal;
pub mod link_preview;
pub mod llm;
pub mod llm_load;
pub mod llm_loaded;
pub mod llm_residency;
pub mod llm_servers;
pub mod local_server;
pub mod logs_window;
pub mod mcp;
pub mod mcp_bridge;
pub mod mcp_coaching;
pub mod memory;
pub mod meta_automation;
pub mod meta_next_steps;
pub mod meta_rewrite;
pub mod model_cards;
pub mod model_hub;
pub mod model_import;
pub(crate) mod model_index;
pub mod notifications;
pub mod observability;
pub mod onboarding;
pub mod org_policy;
pub mod package_examples;
pub mod package_files;
pub mod plan_mode;
pub mod project_migration;
pub mod recommend;
pub mod server_logs;
pub mod server_tokens;
pub mod sources;
pub mod stt;
pub mod tasks;
pub mod tool_governance;
pub mod tools;
pub mod trace;
pub mod triggers;
pub mod updates;
pub mod user_memory;
pub mod workflows;

/// Process-wide lock for tests that read or write the home-directory env vars.
///
/// `HOME` and `USERPROFILE` are process globals, and the test harness runs the
/// whole binary's tests on a shared thread pool. Several modules touch them:
/// `chat` fakes a home to expand `~`, `model_hub` fakes one too, and `config`
/// reads the resolved data directory back out. Without a common lock one sees
/// whichever value another happened to leave behind, and the failure depends on scheduling order, which the repository
/// rules forbid outright.
///
/// Every site that sets or reads those variables takes this guard for the whole
/// duration of its assertions, and restores the previous value before dropping
/// it. Poisoning is ignored on purpose: a panicking test has already failed,
/// and letting the poison cascade would convert one failure into many.
#[cfg(test)]
pub(crate) fn home_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

/// Bearer token the desktop attaches to its REST bridge calls to the embedded
/// runtime over TCP. Set once at startup from the same token the embedded
/// APIServer authenticates against.
static API_TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Register the bearer token used for the desktop REST bridge. Called once in
/// `main()` after the token is loaded, before any Tauri command runs.
pub(crate) fn set_api_token(token: String) {
    let _ = API_TOKEN.set(token);
}

/// The `Authorization` header value for bridge requests, when a token is set.
fn bridge_auth_header() -> Option<String> {
    API_TOKEN.get().map(|t| format!("Bearer {t}"))
}

/// Upper bound on the body of a refused stream: an error message.
const STREAM_REFUSAL_BYTES: u64 = 64 * 1024;

/// Opens an open-ended SSE stream of the internal REST API at `path`, query
/// included. The answer is the body, read frame by frame by the caller; a
/// refusal is the runtime's own message.
pub(crate) async fn http_open_stream(port: u16, path: &str) -> Result<reqwest::Response, String> {
    let client = apollia_core::net::loopback_client_builder()
        .build()
        .map_err(|e| format!("failed to build the stream client: {e}"))?;
    let mut request = client
        .get(format!("http://127.0.0.1:{port}{path}"))
        .header(reqwest::header::ACCEPT, "text/event-stream");
    if let Some(auth) = bridge_auth_header() {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("failed to connect to runtime API: {e}"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message =
        apollia_core::net::read_capped_json::<serde_json::Value>(response, STREAM_REFUSAL_BYTES)
            .await
            .ok()
            .and_then(|body| {
                body.get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("runtime API answered {status}"));
    Err(message)
}

/// How long a read of the internal REST API may take, answer included: the
/// deadline the attached arm sets on its requests. A runtime that stops
/// answering a read then turns into an error the interface shows, instead of
/// a screen that waits forever. The long writes (an addition, an
/// installation) go through [`crate::attached::RuntimeLink::post_json_within`],
/// which sets no such deadline here.
pub(crate) const EMBEDDED_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Sends a GET request to the internal REST API on `localhost:{port}` and
/// returns the parsed JSON body, within [`EMBEDDED_READ_TIMEOUT`].
///
/// Used for `get_task_timeline` and `list_pending_approvals` (enrichment via
/// the timeline API).
pub(crate) async fn http_get_json(port: u16, path: &str) -> Result<serde_json::Value, String> {
    http_get_json_within(port, path, EMBEDDED_READ_TIMEOUT).await
}

/// [`http_get_json`] under `deadline`: past it, the read is abandoned and the
/// error says the runtime did not answer.
async fn http_get_json_within(
    port: u16,
    path: &str,
    deadline: std::time::Duration,
) -> Result<serde_json::Value, String> {
    tokio::time::timeout(deadline, http_get_json_unbounded(port, path))
        .await
        .map_err(|_| format!("the runtime did not answer within {deadline:?}"))?
}

/// The GET itself, with no deadline of its own.
async fn http_get_json_unbounded(port: u16, path: &str) -> Result<serde_json::Value, String> {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| format!("failed to connect to runtime API: {e}"))?;

    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake(io)
        .await
        .map_err(|e| format!("HTTP handshake failed: {e}"))?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut builder = hyper::Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "localhost");
    if let Some(auth) = bridge_auth_header() {
        builder = builder.header("authorization", auth);
    }
    let req = builder
        .body(Full::new(Bytes::new()))
        .map_err(|e| format!("failed to build request: {e}"))?;

    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?
        .to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);

    if !status.is_success() {
        return Err(format!("API error ({}): {}", status.as_u16(), body_str));
    }

    serde_json::from_str(&body_str).map_err(|e| format!("invalid JSON response: {e}"))
}

/// Sends a POST request with a JSON body to the internal REST API.
///
/// Used to register an installed agent and for the graceful shutdown via the
/// tray, which need complex operations handled by the REST handlers.
pub(crate) async fn http_post_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    http_request_json(port, "POST", path, Some(body)).await
}

/// Sends a PUT request with a JSON body to the internal REST API.
///
/// Used for CRUD update operations (triggers, pipelines, notifications).
pub(crate) async fn http_put_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    http_request_json(port, "PUT", path, Some(body)).await
}

/// Sends a PATCH request with a JSON body to the internal REST API.
///
/// Used for partial updates.
pub(crate) async fn http_patch_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    http_request_json(port, "PATCH", path, Some(body)).await
}

/// Sends a DELETE request to the internal REST API and returns the parsed JSON
/// body.
///
/// Used for CRUD delete operations (triggers, pipelines, notifications).
pub(crate) async fn http_delete_json(port: u16, path: &str) -> Result<serde_json::Value, String> {
    http_request_json(port, "DELETE", path, None).await
}

/// Sends a POST request with a JSON body to the internal REST API, for an
/// answer that is a file: the value of its `report_header`, and its bytes.
pub(crate) async fn http_post_for_file(
    port: u16,
    path: &str,
    body: &serde_json::Value,
    report_header: &str,
) -> Result<(Option<String>, Vec<u8>), String> {
    http_file_request(port, "POST", path, Some(body), report_header).await
}

/// Sends a GET request to the internal REST API, for an answer that is a
/// file: its bytes.
pub(crate) async fn http_get_for_file(port: u16, path: &str) -> Result<Vec<u8>, String> {
    let (_, bytes) = http_file_request(port, "GET", path, None, "").await?;
    Ok(bytes)
}

/// Shared by [`http_post_for_file`] and [`http_get_for_file`]: the request,
/// then the value of the `report_header` of the answer and its bytes.
async fn http_file_request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    report_header: &str,
) -> Result<(Option<String>, Vec<u8>), String> {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| format!("failed to connect to runtime API: {e}"))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake(io)
        .await
        .map_err(|e| format!("HTTP handshake failed: {e}"))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut builder = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost");
    let body_bytes = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            serde_json::to_vec(body).map_err(|e| format!("failed to serialize body: {e}"))?
        }
        None => Vec::new(),
    };
    if let Some(auth) = bridge_auth_header() {
        builder = builder.header("authorization", auth);
    }
    let req = builder
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| format!("failed to build request: {e}"))?;
    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let report = resp
        .headers()
        .get(report_header)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let resp_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?
        .to_bytes();
    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&resp_bytes);
        return Err(serde_json::from_str::<serde_json::Value>(&body_str)
            .ok()
            .and_then(|j| j.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or_else(|| format!("API error ({}): {}", status.as_u16(), body_str)));
    }
    Ok((report, resp_bytes.to_vec()))
}

/// Internal function shared by `http_post_json`, `http_put_json` and
/// `http_delete_json`.
///
/// Opens an HTTP/1.1 connection to `localhost:{port}`, sends the request with
/// the method and optional body, and returns the response JSON or a descriptive
/// error.
async fn http_request_json(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    http_request_refusable(port, method, path, body)
        .await
        .map_err(|failure| failure.message)
}

/// Why a request to the internal REST API did not return its JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpFailure {
    /// The HTTP status of a refusal; `None` when no answer was read.
    pub(crate) status: Option<u16>,
    /// The text the interface shows.
    pub(crate) message: String,
}

impl HttpFailure {
    fn unanswered(message: String) -> Self {
        Self {
            status: None,
            message,
        }
    }
}

/// `POST path` with a JSON body, for a caller that tells one refusal from
/// another by its HTTP status.
pub(crate) async fn http_post_json_refusable(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, HttpFailure> {
    http_request_refusable(port, "POST", path, Some(body)).await
}

async fn http_request_refusable(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, HttpFailure> {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| HttpFailure::unanswered(format!("failed to connect to runtime API: {e}")))?;

    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake(io)
        .await
        .map_err(|e| HttpFailure::unanswered(format!("HTTP handshake failed: {e}")))?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    let auth = bridge_auth_header();
    let req = if let Some(json_body) = body {
        let body_bytes = serde_json::to_vec(json_body)
            .map_err(|e| HttpFailure::unanswered(format!("failed to serialize body: {e}")))?;
        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost")
            .header("content-type", "application/json");
        if let Some(auth) = &auth {
            builder = builder.header("authorization", auth);
        }
        builder
            .body(Full::new(Bytes::from(body_bytes)))
            .map_err(|e| HttpFailure::unanswered(format!("failed to build request: {e}")))?
    } else {
        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost");
        if let Some(auth) = &auth {
            builder = builder.header("authorization", auth);
        }
        builder
            .body(Full::new(Bytes::new()))
            .map_err(|e| HttpFailure::unanswered(format!("failed to build request: {e}")))?
    };

    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| HttpFailure::unanswered(format!("request failed: {e}")))?;

    let status = resp.status();
    let resp_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| HttpFailure::unanswered(format!("failed to read response body: {e}")))?
        .to_bytes();
    let body_str = String::from_utf8_lossy(&resp_bytes);

    if !status.is_success() {
        let error_msg = serde_json::from_str::<serde_json::Value>(&body_str)
            .ok()
            .and_then(|j| j.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or_else(|| format!("API error ({}): {}", status.as_u16(), body_str));
        return Err(HttpFailure {
            status: Some(status.as_u16()),
            message: error_msg,
        });
    }

    // A route that answers `204 No Content` has nothing to parse.
    if resp_bytes.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&body_str)
        .map_err(|e| HttpFailure::unanswered(format!("invalid JSON response: {e}")))
}

#[cfg(test)]
mod embedded_deadline_tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::watch;

    /// A runtime that reads each request and answers `{}` only once `release`
    /// turns true; dropped unreleased, it never answers.
    async fn answering_on(release: watch::Receiver<bool>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut release = release.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    if release.wait_for(|go| *go).await.is_err() {
                        return;
                    }
                    let _ = socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}",
                        )
                        .await;
                });
            }
        });
        port
    }

    #[test]
    fn test_the_embedded_read_deadline_is_fifteen_seconds() {
        // GIVEN the deadline of an embedded read
        // WHEN it is compared with the attached arm's
        // THEN it is the same fifteen seconds
        assert_eq!(EMBEDDED_READ_TIMEOUT, Duration::from_secs(15));
    }

    #[tokio::test]
    async fn test_a_read_past_its_deadline_says_the_runtime_did_not_answer() {
        // GIVEN a runtime that never answers
        let (_held, release) = watch::channel(false);
        let port = answering_on(release).await;

        // WHEN a read with a deadline of 200 ms is sent
        let started = Instant::now();
        let read =
            http_get_json_within(port, "/api/v1/mcp/servers", Duration::from_millis(200)).await;

        // THEN it fails at the deadline, saying the runtime did not answer
        let error = read.unwrap_err();
        assert!(error.contains("did not answer"), "got {error}");
        assert!(started.elapsed() >= Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_a_read_within_its_deadline_returns_the_answer() {
        // GIVEN a runtime that answers at once
        let (_released, release) = watch::channel(true);
        let port = answering_on(release).await;

        // WHEN a read is sent under the embedded deadline
        let read = http_get_json(port, "/api/v1/mcp/servers").await;

        // THEN the answer comes back
        assert_eq!(read.unwrap(), serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_the_write_variant_waits_past_the_read_deadline() {
        // GIVEN a runtime that holds its answers, and an addition sent to it
        // through the write variant, which sets no deadline
        let (go, release) = watch::channel(false);
        let port = answering_on(release).await;
        let started = Instant::now();
        let addition = tokio::spawn(async move {
            http_post_json(port, "/api/v1/mcp/servers", &serde_json::json!({})).await
        });

        // WHEN a read with a deadline of 200 ms gives up on the same runtime,
        // and only then the runtime answers
        let read =
            http_get_json_within(port, "/api/v1/mcp/servers", Duration::from_millis(200)).await;
        assert!(read.is_err());
        go.send_replace(true);

        // THEN the addition, in flight longer than the read deadline, still
        // returns the answer
        assert_eq!(addition.await.unwrap().unwrap(), serde_json::json!({}));
        assert!(started.elapsed() >= Duration::from_millis(200));
    }
}
