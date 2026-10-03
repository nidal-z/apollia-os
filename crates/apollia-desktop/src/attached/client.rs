//! The HTTP arm of the seam: the runtime reached over its local API, with the
//! bearer token of this installation.
//!
//! One client per attached application. Every call but the event stream
//! carries a deadline, so a daemon that stops answering turns into a typed
//! error instead of a command that never returns.

use std::time::Duration;

use apollia_runtime::chat::{SessionDetail, SessionInfo};
use serde::de::DeserializeOwned;

use super::AttachedError;

/// How long a connection to the runtime may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a request other than the event stream may take, answer included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a transcription may take, the upload and the model's work
/// included. A processor-bound machine needs several times the length of the
/// audio, so the ordinary deadline would cut a file of a few minutes short.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(600);

/// Boundary of the multipart body a transcription is uploaded in.
const UPLOAD_BOUNDARY: &str = "apollia-desktop-audio-5d1c";

/// How long the liveness request may take before the runtime counts as gone.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// Upper bound on an answer read from the runtime. A full session history is
/// the largest body this client reads.
const MAX_BODY_BYTES: u64 = 32 * 1024 * 1024;

/// A runtime served by another process, reached over HTTP.
#[derive(Clone)]
pub(crate) struct AttachedClient {
    /// `http://127.0.0.1:<port>`, without a trailing slash.
    base: String,
    /// The bearer token of this installation.
    token: String,
    /// The connection pool.
    http: reqwest::Client,
}

impl std::fmt::Debug for AttachedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachedClient")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl AttachedClient {
    /// A client for the runtime at `base`, authenticated by `token`.
    ///
    /// # Errors
    ///
    /// [`AttachedError::Client`] when the HTTP stack cannot be initialised.
    pub(crate) fn new(
        base: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, AttachedError> {
        let http = apollia_core::net::loopback_client_builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(AttachedError::Client)?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            http,
        })
    }

    /// Creates a chat session from the request the interface sent.
    pub(crate) async fn create_session(
        &self,
        request: &impl serde::Serialize,
    ) -> Result<SessionInfo, AttachedError> {
        let builder = self.http.post(self.url("/api/v1/sessions")).json(request);
        self.send_json(builder).await
    }

    /// Lists chat sessions, optionally filtered by their stored status.
    pub(crate) async fn list_sessions(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<SessionInfo>, AttachedError> {
        let mut builder = self.http.get(self.url("/api/v1/sessions"));
        if let Some(status) = status {
            builder = builder.query(&[("status", status)]);
        }
        self.send_json(builder).await
    }

    /// Reads one chat session with its full history.
    pub(crate) async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<SessionDetail, AttachedError> {
        let path = format!("/api/v1/sessions/{}", urlencoding::encode(session_id));
        self.send_json(self.http.get(self.url(&path))).await
    }

    /// Sends a user message; the answer streams through the event bus.
    pub(crate) async fn send_message(
        &self,
        session_id: &str,
        content: &str,
    ) -> Result<String, AttachedError> {
        #[derive(serde::Deserialize)]
        struct Accepted {
            message_id: String,
        }
        let path = format!(
            "/api/v1/sessions/{}/messages",
            urlencoding::encode(session_id)
        );
        let builder = self
            .http
            .post(self.url(&path))
            .json(&serde_json::json!({ "content": content }));
        let accepted: Accepted = self.send_json(builder).await?;
        Ok(accepted.message_id)
    }

    /// Pauses the turn in flight of a session.
    pub(crate) async fn pause_session(&self, session_id: &str) -> Result<(), AttachedError> {
        let path = format!("/api/v1/sessions/{}/pause", urlencoding::encode(session_id));
        let _: serde_json::Value = self.send_json(self.http.post(self.url(&path))).await?;
        Ok(())
    }

    /// `GET path`, its JSON answer read as `T`.
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AttachedError> {
        self.send_json(self.http.get(self.url(path))).await
    }

    /// `POST path` with a JSON body, its JSON answer read as `T`.
    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, AttachedError> {
        self.send_json(self.http.post(self.url(path)).json(body))
            .await
    }

    /// `PUT path` with a JSON body, its JSON answer read as `T`.
    pub(crate) async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, AttachedError> {
        self.send_json(self.http.put(self.url(path)).json(body))
            .await
    }

    /// `PATCH path` with a JSON body, its JSON answer read as `T`.
    pub(crate) async fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, AttachedError> {
        self.send_json(self.http.patch(self.url(path)).json(body))
            .await
    }

    /// `DELETE path`, its JSON answer read as `T`.
    pub(crate) async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, AttachedError> {
        self.send_json(self.http.delete(self.url(path))).await
    }

    /// `DELETE path` on a route that answers no body.
    pub(crate) async fn delete_empty(&self, path: &str) -> Result<(), AttachedError> {
        let response = self
            .http
            .delete(self.url(path))
            .bearer_auth(&self.token)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(AttachedError::Unreachable)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(refusal(response).await)
        }
    }

    /// Uploads a WAV to `POST /api/v1/stt/transcribe` under `source`, the tag
    /// the history shows; the answer is the row the runtime kept.
    pub(crate) async fn transcribe<T: DeserializeOwned>(
        &self,
        wav: &[u8],
        file_name: &str,
        source: &str,
    ) -> Result<T, AttachedError> {
        let builder = self
            .http
            .post(self.url("/api/v1/stt/transcribe"))
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={UPLOAD_BOUNDARY}"),
            )
            .body(upload_body(wav, file_name, source));
        self.send_json_within(builder, TRANSCRIBE_TIMEOUT).await
    }

    /// Whether the runtime answers its health route, within a short deadline.
    pub(crate) async fn is_alive(&self) -> bool {
        let request = self
            .http
            .get(self.url("/api/v1/health"))
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await;
        matches!(request, Ok(response) if response.status().is_success())
    }

    /// Opens the event stream. The answer is the open-ended SSE body, read by
    /// the relay.
    pub(crate) async fn open_events(&self) -> Result<reqwest::Response, AttachedError> {
        let response = self
            .http
            .get(self.url("/api/v1/events"))
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(AttachedError::Unreachable)?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(refusal(response).await)
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Sends an authenticated request and reads its JSON answer.
    async fn send_json<T: DeserializeOwned>(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<T, AttachedError> {
        self.send_json_within(builder, REQUEST_TIMEOUT).await
    }

    /// [`Self::send_json`] with its own deadline.
    async fn send_json_within<T: DeserializeOwned>(
        &self,
        builder: reqwest::RequestBuilder,
        deadline: Duration,
    ) -> Result<T, AttachedError> {
        let response = builder
            .bearer_auth(&self.token)
            .timeout(deadline)
            .send()
            .await
            .map_err(AttachedError::Unreachable)?;
        if !response.status().is_success() {
            return Err(refusal(response).await);
        }
        apollia_core::net::read_capped_json(response, MAX_BODY_BYTES)
            .await
            .map_err(AttachedError::Body)
    }
}

/// The typed error for a non-success answer, carrying the runtime's own
/// `error` message when the body holds one.
async fn refusal(response: reqwest::Response) -> AttachedError {
    let status = response.status().as_u16();
    let message = apollia_core::net::read_capped_json::<serde_json::Value>(response, 64 * 1024)
        .await
        .ok()
        .and_then(|body| match body.get("error") {
            Some(serde_json::Value::String(text)) => Some(text.clone()),
            Some(serde_json::Value::Object(error)) => error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            _ => None,
        })
        .unwrap_or_default();
    AttachedError::Refused { status, message }
}

/// The multipart body of a transcription: the audio part, then the source.
///
/// The file name is reduced to the characters a header value can carry
/// without quoting, so a name chosen on this machine cannot end the part.
fn upload_body(wav: &[u8], file_name: &str, source: &str) -> Vec<u8> {
    let name: String = file_name
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\r' | '\n'))
        .collect();
    let mut body = Vec::with_capacity(wav.len() + 512);
    body.extend_from_slice(
        format!(
            "--{UPLOAD_BOUNDARY}\r\nContent-Disposition: form-data; name=\"audio\"; \
             filename=\"{name}\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(
        format!(
            "\r\n--{UPLOAD_BOUNDARY}\r\nContent-Disposition: form-data; name=\"source\"\r\n\r\n\
             {source}\r\n--{UPLOAD_BOUNDARY}--\r\n"
        )
        .as_bytes(),
    );
    body
}
