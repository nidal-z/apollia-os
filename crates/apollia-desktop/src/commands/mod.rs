//! Tauri IPC commands: translation layer between the Svelte frontend and the
//! Apollia runtime handles.
//!
//! No business logic: each command delegates entirely to the existing handles
//! (`AgentRegistryHandle`, `TaskRouterHandle`, `PendingApprovals`) or to the
//! internal REST API for complex operations (timeline, start agent, resume
//! task).

pub mod agent_adjust;
pub mod agent_export;
pub mod agent_packages;
pub mod agent_sheet;
pub mod agents;
pub mod artifacts;
pub mod authorizations;
#[cfg(debug_assertions)]
pub mod automation;
pub mod background_service;
pub mod chat;
pub mod cli;
pub mod config;
pub mod connections;
pub mod hf_token;
pub mod hitl;
pub mod integrations;
pub mod link_preview;
pub mod llm;
pub mod llm_loaded;
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

/// Sends a GET request to the internal REST API on `localhost:{port}` and
/// returns the parsed JSON body.
///
/// Used for `get_task_timeline` and `list_pending_approvals` (enrichment via
/// the timeline API).
pub(crate) async fn http_get_json(port: u16, path: &str) -> Result<serde_json::Value, String> {
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
/// Used to register an installed agent, for `resume_task`, and for the
/// graceful shutdown via the tray, which need complex operations handled by
/// the REST handlers.
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
/// Used for partial updates (e.g. an MCP server's `requires_approval` without
/// restarting it).
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

    let body_bytes =
        serde_json::to_vec(body).map_err(|e| format!("failed to serialize body: {e}"))?;
    let mut builder = hyper::Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "localhost")
        .header("content-type", "application/json");
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

    let auth = bridge_auth_header();
    let req = if let Some(json_body) = body {
        let body_bytes =
            serde_json::to_vec(json_body).map_err(|e| format!("failed to serialize body: {e}"))?;
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
            .map_err(|e| format!("failed to build request: {e}"))?
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
            .map_err(|e| format!("failed to build request: {e}"))?
    };

    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let resp_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?
        .to_bytes();
    let body_str = String::from_utf8_lossy(&resp_bytes);

    if !status.is_success() {
        let error_msg = serde_json::from_str::<serde_json::Value>(&body_str)
            .ok()
            .and_then(|j| j.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or_else(|| format!("API error ({}): {}", status.as_u16(), body_str));
        return Err(error_msg);
    }

    // A route that answers `204 No Content` has nothing to parse.
    if resp_bytes.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&body_str).map_err(|e| format!("invalid JSON response: {e}"))
}
