//! HTTP client for communicating with the Apollia runtime via Unix socket.
//!
//! Uses hyper 1.x over `tokio::net::UnixStream` for lightweight HTTP/1.1
//! requests without pulling in reqwest or a full HTTP client stack.

use std::path::{Path, PathBuf};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

/// Default Unix socket path for the Apollia runtime.
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/apollia.sock";

/// Default TCP port for the Apollia runtime.
pub const DEFAULT_TCP_PORT: u16 = 7771;

/// Client for communicating with the Apollia runtime via Unix socket.
///
/// Each method opens a new connection (HTTP/1.1 per-request). This is fine
/// for CLI usage where request frequency is low.
pub struct RuntimeClient {
    socket_path: PathBuf,
}

/// Errors that can occur when communicating with the runtime.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Failed to connect to the Unix socket (runtime not started).
    #[error("runtime not started (connection refused)")]
    ConnectionRefused,

    /// IO error during communication.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP protocol error.
    #[error("http error: {0}")]
    Http(String),

    /// Failed to parse response body as JSON.
    #[error("invalid JSON response: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// Server returned a non-success status code.
    #[error("server error ({status}): {body}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },
}

/// Raw HTTP response from the runtime.
#[derive(Debug)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as string.
    pub body: String,
}

impl RuntimeClient {
    /// Create a new client targeting the given Unix socket path.
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Create a client with the default socket path (`/tmp/apollia.sock`).
    pub fn default_client() -> Self {
        Self::new(PathBuf::from(DEFAULT_SOCKET_PATH))
    }

    /// Return the socket path this client connects to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Send a GET request and return the raw response.
    pub async fn get(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("GET", uri, None).await
    }

    /// Send a POST request with an optional JSON body and return the raw response.
    pub async fn post(
        &self,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        self.request("POST", uri, body).await
    }

    /// Send a DELETE request and return the raw response.
    pub async fn delete(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("DELETE", uri, None).await
    }

    /// Check if the runtime is healthy by calling `GET /api/v1/health`.
    pub async fn health(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/health").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Request shutdown via `POST /api/v1/shutdown`.
    pub async fn shutdown(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/shutdown", None).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List all agents via `GET /api/v1/agents`.
    pub async fn list_agents(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agents").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Start (register) a new agent via `POST /api/v1/agents`.
    pub async fn start_agent(&self, agent_path: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "agent_path": agent_path });
        let resp = self.post("/api/v1/agents", Some(&body)).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Get agent detail via `GET /api/v1/agents/{id}`.
    pub async fn get_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/agents/{agent_id}")).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Stop an agent via `DELETE /api/v1/agents/{id}`.
    pub async fn stop_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/agents/{agent_id}")).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Submit a task via `POST /api/v1/tasks`.
    pub async fn submit_task(
        &self,
        agent_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": input,
        });
        let resp = self.post("/api/v1/tasks", Some(&body)).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Get task status via `GET /api/v1/tasks/{id}`.
    pub async fn get_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/tasks/{task_id}")).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Hot-reload triggers via `POST /api/v1/triggers/reload`.
    ///
    /// Returns the JSON response on success (`{ "reloaded": <count> }`).
    pub async fn reload_triggers(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/triggers/reload", None).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Cancel a task via `DELETE /api/v1/tasks/{id}`.
    pub async fn cancel_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/tasks/{task_id}")).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }
        Ok(json)
    }

    /// Internal: send an HTTP request over Unix socket.
    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "HTTP connection closed");
            }
        });

        let req_body = match body {
            Some(json) => Full::new(Bytes::from(
                serde_json::to_vec(json).map_err(|e| ClientError::Http(e.to_string()))?,
            )),
            None => Full::new(Bytes::new()),
        };

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost");

        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }

        let req = builder
            .body(req_body)
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(RawResponse {
            status,
            body: body_str,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_client_connection_refused() {
        // GIVEN a nonexistent socket
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-nonexistent.sock"));

        // WHEN health() is called
        let result = client.health().await;

        // THEN connection refused error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ClientError::ConnectionRefused),
            "expected ConnectionRefused, got: {err}"
        );
    }

    #[test]
    fn test_default_socket_path() {
        let client = RuntimeClient::default_client();
        assert_eq!(client.socket_path(), Path::new("/tmp/apollia.sock"));
    }

    #[test]
    fn test_custom_socket_path() {
        let client = RuntimeClient::new(PathBuf::from("/custom/path.sock"));
        assert_eq!(client.socket_path(), Path::new("/custom/path.sock"));
    }
}
