//! Speech-to-text, chat sessions and the SSE line stream they read.

use futures::channel::mpsc;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;

use crate::client::{
    connect_runtime, extract_error, pump_sse_body, AuthorizeToolArgs, ClientError, RuntimeClient,
};

impl RuntimeClient {
    /// Get STT engine status via `GET /api/v1/stt/status`.
    pub async fn stt_status(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/status").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List STT transcriptions via `GET /api/v1/stt/transcriptions`.
    pub async fn stt_transcriptions(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!(
                "/api/v1/stt/transcriptions?limit={limit}&offset={offset}"
            ))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List STT models via `GET /api/v1/stt/models`.
    pub async fn stt_models(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/models").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Create a new chat session via `POST /api/v1/sessions`.
    pub async fn create_chat_session(&self, mode: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "mode": mode });
        let resp = self.post("/api/v1/sessions", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Send a message in a chat session via `POST /api/v1/sessions/:id/messages`.
    pub async fn send_chat_message(
        &self,
        session_id: &str,
        content: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "content": content });
        let resp = self
            .post(
                &format!("/api/v1/sessions/{session_id}/messages"),
                Some(&body),
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// One-shot completion via `POST /api/v1/llm/complete`.
    ///
    /// Optional `system` prompt + `user` message; `grammar` constrains local
    /// decoding with GBNF. Returns the parsed response (`content`, `usage`, ...).
    pub async fn llm_complete(
        &self,
        system: Option<&str>,
        user: &str,
        grammar: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut messages = Vec::new();
        if let Some(s) = system {
            messages.push(serde_json::json!({ "role": "system", "content": s }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": user }));
        let mut body = serde_json::json!({ "messages": messages });
        if let Some(g) = grammar {
            body["grammar"] = serde_json::Value::String(g.to_string());
        }
        let resp = self.post("/api/v1/llm/complete", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get session detail (with message history) via `GET /api/v1/sessions/:id`.
    pub async fn get_chat_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/sessions/{session_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List recent sessions via `GET /api/v1/sessions/recent?limit=N`.
    pub async fn list_recent_chat_sessions(
        &self,
        limit: usize,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/sessions/recent?limit={limit}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fork a session via `POST /api/v1/sessions/:id/fork`.
    ///
    /// When `up_to_index` is `None`, the full history is copied to the child.
    pub async fn fork_chat_session(
        &self,
        session_id: &str,
        up_to_index: Option<usize>,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "up_to_index": up_to_index });
        let resp = self
            .post(&format!("/api/v1/sessions/{session_id}/fork"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List child sessions (forks) of `session_id` via `GET /api/v1/sessions/:id/children`.
    pub async fn list_session_children(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/sessions/{session_id}/children"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resume an existing session via `POST /api/v1/sessions/:id/resume`.
    pub async fn resume_chat_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/sessions/{session_id}/resume"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resolve a pending tool approval via `POST /api/v1/sessions/:id/authorize`.
    pub async fn authorize_tool(
        &self,
        args: &AuthorizeToolArgs<'_>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "message_id": args.message_id,
            "tool_name": args.tool_name,
            "decision": args.decision,
        });
        if let Some(r) = args.reason {
            body["reason"] = serde_json::Value::String(r.to_string());
        }
        if let Some(s) = args.scope {
            body["scope"] = serde_json::Value::String(s.to_string());
        }
        let resp = self
            .post(
                &format!("/api/v1/sessions/{}/authorize", args.session_id),
                Some(&body),
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Open a streaming GET connection for SSE endpoints.
    ///
    /// Unlike [`get`], this method does **not** buffer the entire response body.
    /// It reads HTTP body frames incrementally and yields complete text lines as they
    /// arrive from the server, enabling real-time display of Server-Sent Events.
    ///
    /// Endpoint-agnostic: used both by `GET /api/v1/tasks/{id}/stream` (task
    /// progress) and `GET /api/v1/sessions/{id}/stream` (chat tokens). Dropping
    /// the returned stream closes the connection automatically (the background
    /// reader task exits when the channel receiver is dropped).
    pub async fn stream_sse_lines(
        &self,
        uri: &str,
    ) -> Result<impl futures::Stream<Item = Result<String, ClientError>>, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
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
                tracing::debug!(error = %e, "sse.connection.closed");
            }
        });

        let mut builder = hyper::Request::builder()
            .method("GET")
            .uri(uri)
            .header("host", "localhost")
            .header("accept", "text/event-stream");
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }
        let req = builder
            .body(Full::new(Bytes::new()))
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        // Buffer of 64 events, generous for any realistic task execution.
        let (tx, rx) = mpsc::channel::<Result<String, ClientError>>(64);

        // Background task: reads body frames, accumulates a line buffer, and
        // pushes complete lines (stripped of trailing '\r') to the channel.
        // Exits when the connection closes or when the receiver is dropped.
        tokio::spawn(pump_sse_body(resp.into_body(), tx));

        Ok(rx)
    }
}
