//! Tauri IPC commands — couche de traduction entre le frontend Svelte et les
//! handles du runtime Apollia.
//!
//! Zéro logique métier : chaque commande délègue intégralement aux handles
//! existants (`AgentRegistryHandle`, `TaskRouterHandle`, `PendingApprovals`)
//! ou à l'API REST interne pour les opérations complexes (timeline, start agent,
//! resume task).

pub mod agents;
pub mod chat;
pub mod config;
pub mod hitl;
pub mod llm;
pub mod memory;
pub mod notifications;
pub mod observability;
pub mod pipelines;
pub mod tasks;
pub mod triggers;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

/// Envoie une requête GET à l'API REST interne sur `localhost:{port}` et retourne
/// le corps JSON parsé.
///
/// Utilisé pour `get_task_timeline` et `list_pending_approvals` (enrichissement
/// via la timeline API Sprint 13).
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

    let req = hyper::Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "localhost")
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

/// Envoie une requête POST avec un corps JSON à l'API REST interne.
///
/// Utilisé pour `start_agent`, `resume_task`, et le graceful shutdown via tray
/// qui nécessitent des opérations complexes gérées par les handlers REST.
pub(crate) async fn http_post_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    http_request_json(port, "POST", path, Some(body)).await
}

/// Envoie une requête PUT avec un corps JSON à l'API REST interne.
///
/// Utilisé pour les opérations de mise à jour CRUD (triggers, pipelines,
/// notifications).
pub(crate) async fn http_put_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    http_request_json(port, "PUT", path, Some(body)).await
}

/// Envoie une requête DELETE à l'API REST interne et retourne le corps JSON
/// parsé.
///
/// Utilisé pour les opérations de suppression CRUD (triggers, pipelines,
/// notifications).
pub(crate) async fn http_delete_json(port: u16, path: &str) -> Result<serde_json::Value, String> {
    http_request_json(port, "DELETE", path, None).await
}

/// Fonction interne partagée par `http_post_json`, `http_put_json` et
/// `http_delete_json`.
///
/// Ouvre une connexion HTTP/1.1 vers `localhost:{port}`, envoie la requête avec
/// la méthode et le corps optionnel, et retourne le JSON de réponse ou une
/// erreur descriptive.
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

    let req = if let Some(json_body) = body {
        let body_bytes =
            serde_json::to_vec(json_body).map_err(|e| format!("failed to serialize body: {e}"))?;
        hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost")
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body_bytes)))
            .map_err(|e| format!("failed to build request: {e}"))?
    } else {
        hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost")
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

    serde_json::from_str(&body_str).map_err(|e| format!("invalid JSON response: {e}"))
}
