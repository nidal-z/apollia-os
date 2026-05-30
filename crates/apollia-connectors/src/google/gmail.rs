//! Gmail client: `send`, `compose_draft`, `list_drafts`, `delete_draft`.
//!
//! Free-tier scope policy: only non-restricted / "sensitive" scopes are used
//! (`gmail.send`, `gmail.compose`). No inbox read / search / modify in v0.1.0,
//! those require restricted scopes (CASA Tier 2). Power users who want full
//! Gmail access go through Expert Mode (their own OAuth app).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest, RawRequest},
};

/// Base URL for the Gmail API v1.
const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

// ─── Request types ───────────────────────────────────────────────────────────

/// Arguments to `gmail.send` and `gmail.compose_draft`.
///
/// The connector assembles an RFC 5322 email from these fields. Body is plain
/// text; richer HTML / attachments can be added in v0.2+.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeMail {
    /// Recipient email address.
    pub to: String,
    /// Optional CC recipients (comma-separated).
    #[serde(default)]
    pub cc: Option<String>,
    /// Optional BCC recipients (comma-separated).
    #[serde(default)]
    pub bcc: Option<String>,
    /// Subject line.
    pub subject: String,
    /// Plain text body.
    pub body: String,
}

// ─── Response types ──────────────────────────────────────────────────────────

/// Identifier returned by Gmail after a successful send or draft creation.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessageRef {
    /// Server-assigned message id.
    pub id: String,
    /// Server-assigned thread id (when present).
    #[serde(default)]
    pub thread_id: Option<String>,
}

/// Wire format for `users.drafts.create`.
#[derive(Debug, Deserialize, Serialize)]
struct DraftCreate {
    message: GmailMessageRawBody,
}

#[derive(Debug, Deserialize, Serialize)]
struct GmailMessageRawBody {
    raw: String,
}

/// Successful response from `users.drafts.create`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftResponse {
    id: String,
    message: GmailMessageMeta,
}

/// Compact metadata about a Gmail message embedded in API responses.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessageMeta {
    /// Message id.
    pub id: String,
    /// Optional thread id.
    #[serde(default)]
    pub thread_id: Option<String>,
}

/// Successful response from `users.messages.send`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendResponse {
    id: String,
    #[serde(default)]
    thread_id: Option<String>,
}

/// Item returned by `users.drafts.list`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DraftSummary {
    /// Draft id.
    pub id: String,
    /// Message id wrapped by the draft.
    #[serde(default)]
    pub message: Option<GmailMessageMeta>,
}

/// Wire response for `users.drafts.list`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftListResponse {
    #[serde(default)]
    drafts: Vec<DraftSummary>,
    /// Pagination token exposed by the upstream; not yet surfaced by this
    /// client, kept here to ensure it round-trips when added.
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "deserialised so the upstream round-trips cleanly, surfaced once the gmail client exposes paginated drafts"
    )]
    next_page_token: Option<String>,
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Gmail client. Cheap to clone (wraps an [`Arc<reqwest::Client>`]).
#[derive(Clone)]
pub struct GmailClient {
    http: HttpClient,
}

impl GmailClient {
    /// Build a Gmail client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Send a freshly composed email.
    ///
    /// Uses the `gmail.send` scope. The user must have granted it through the
    /// OAuth flow; otherwise the upstream returns 403 and this returns
    /// [`ConnectorError::Upstream`] with status 403.
    pub async fn send<F, Fut>(
        &self,
        mail: &ComposeMail,
        bearer: &str,
        refresh: F,
    ) -> Result<GmailMessageRef, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let raw = encode_rfc5322(mail);
        let body = GmailMessageRawBody { raw };
        let url = format!("{BASE}/messages/send");
        let response: SendResponse = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(GmailMessageRef {
            id: response.id,
            thread_id: response.thread_id,
        })
    }

    /// Create a draft message visible in the user's Drafts folder.
    ///
    /// Uses the `gmail.compose` scope.
    pub async fn compose_draft<F, Fut>(
        &self,
        mail: &ComposeMail,
        bearer: &str,
        refresh: F,
    ) -> Result<GmailMessageRef, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let raw = encode_rfc5322(mail);
        let payload = DraftCreate {
            message: GmailMessageRawBody { raw },
        };
        let url = format!("{BASE}/drafts");
        let response: DraftResponse = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: &payload,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(GmailMessageRef {
            id: response.id, // The draft id, not the message id; caller can use either.
            thread_id: response.message.thread_id,
        })
    }

    /// List drafts in the user's account. Returns up to `max_results` entries.
    pub async fn list_drafts<F, Fut>(
        &self,
        max_results: u32,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DraftSummary>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/drafts?maxResults={max_results}");
        let response: DraftListResponse = self.http.get_json(&url, bearer, refresh).await?;
        Ok(response.drafts)
    }

    /// Delete a draft by id.
    pub async fn delete_draft<F, Fut>(
        &self,
        draft_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<(), ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/drafts/{draft_id}");
        self.http
            .send(
                RawRequest {
                    method: Method::DELETE,
                    url: &url,
                    body: None,
                    content_type: None,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(())
    }
}

// ─── RFC 5322 + base64url encoding ───────────────────────────────────────────

/// Build the base64url-encoded raw message expected by the Gmail API.
fn encode_rfc5322(mail: &ComposeMail) -> String {
    let mut headers = format!(
        "To: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n",
        mail.to, mail.subject
    );
    if let Some(cc) = &mail.cc {
        headers.push_str(&format!("Cc: {cc}\r\n"));
    }
    if let Some(bcc) = &mail.bcc {
        headers.push_str(&format!("Bcc: {bcc}\r\n"));
    }
    let rfc5322 = format!("{headers}\r\n{}", mail.body);
    URL_SAFE_NO_PAD.encode(rfc5322)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_encode_rfc5322_includes_to_subject_body() {
        let mail = ComposeMail {
            to: "alice@example.com".into(),
            cc: None,
            bcc: None,
            subject: "Hello".into(),
            body: "Body line.".into(),
        };
        let raw = encode_rfc5322(&mail);
        // Decode and inspect
        let bytes = URL_SAFE_NO_PAD.decode(&raw).expect("decode");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(text.contains("To: alice@example.com"));
        assert!(text.contains("Subject: Hello"));
        assert!(text.contains("Body line."));
        assert!(!text.contains("Cc: "));
        assert!(!text.contains("Bcc: "));
    }

    #[test]
    fn test_encode_rfc5322_includes_cc_and_bcc_when_set() {
        let mail = ComposeMail {
            to: "to@example.com".into(),
            cc: Some("cc@example.com".into()),
            bcc: Some("bcc@example.com".into()),
            subject: "S".into(),
            body: "B".into(),
        };
        let raw = encode_rfc5322(&mail);
        let bytes = URL_SAFE_NO_PAD.decode(&raw).expect("decode");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(text.contains("Cc: cc@example.com"));
        assert!(text.contains("Bcc: bcc@example.com"));
    }

    // The endpoint constants are fixed to gmail.googleapis.com; we cannot
    // easily redirect them to a mock server without a builder. The tests
    // below exercise the encode_rfc5322 path which is the integration-prone
    // piece. The HTTP layer is already exhaustively tested in http.rs.

    #[tokio::test]
    async fn test_gmail_client_constructs_from_http_client() {
        // Sanity check that the client type wires up without panicking.
        let http = HttpClient::new("google").expect("http");
        let _client = GmailClient::new(http);
    }

    // Smoke test for the send wire format using a wiremock server proxied
    // through a custom client. We construct the request manually because the
    // GmailClient hard-codes the upstream URL.
    #[tokio::test]
    async fn test_send_request_uses_post_to_send_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages/send"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg-1",
                "threadId": "thr-1"
            })))
            .mount(&server)
            .await;

        // Manually verify the wire shape using the helper directly.
        let http = HttpClient::new("google").expect("http");
        let url = format!("{}/messages/send", server.uri());
        let raw = encode_rfc5322(&ComposeMail {
            to: "alice@example.com".into(),
            cc: None,
            bcc: None,
            subject: "Hi".into(),
            body: "Body".into(),
        });
        let body = GmailMessageRawBody { raw };
        let response: SendResponse = http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: &body,
                },
                "tok",
                || async { Ok::<_, ConnectorError>("unused".to_owned()) },
            )
            .await
            .expect("send");
        assert_eq!(response.id, "msg-1");
    }
}
