//! Google Docs API client — non-sensitive scope (`documents`).
//!
//! Per-resource gated: agent can read+write docs it created or that the
//! user opened with Apollia (via Picker). No CASA needed.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{error::ConnectorError, http::HttpClient};

const BASE: &str = "https://docs.googleapis.com/v1/documents";

/// Doc metadata returned on create/read.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMeta {
    pub document_id: String,
    pub title: String,
    /// Full body content tree — kept as raw JSON because the schema is
    /// huge (structural elements, paragraphs, tables, etc.). Agents call
    /// `read_plain_text` for the flattened version.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub revision_id: Option<String>,
}

#[derive(Clone)]
pub struct DocsClient {
    http: HttpClient,
}

impl DocsClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Create a new empty document with the given title.
    pub async fn create<F, Fut>(
        &self,
        title: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<DocumentMeta, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body = serde_json::json!({ "title": title });
        self.http
            .json_request(Method::POST, BASE, &body, bearer, refresh)
            .await
    }

    /// Read a doc and flatten its body into plain text.
    pub async fn read_plain_text<F, Fut>(
        &self,
        document_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<String, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/{document_id}");
        let meta: DocumentMeta = self.http.get_json(&url, bearer, refresh).await?;
        Ok(meta
            .body
            .as_ref()
            .and_then(|b| flatten_body(b))
            .unwrap_or_default())
    }

    /// Append plain text at the end of a doc (single `insertText` request).
    pub async fn append_text<F, Fut>(
        &self,
        document_id: &str,
        text: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<serde_json::Value, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        // `endOfSegmentLocation` inserts at the very end of the body —
        // simplest pattern, no need to compute the current length.
        let url = format!("{BASE}/{document_id}:batchUpdate");
        let body = serde_json::json!({
            "requests": [{
                "insertText": {
                    "text": text,
                    "endOfSegmentLocation": {}
                }
            }]
        });
        self.http
            .json_request(Method::POST, &url, &body, bearer, refresh)
            .await
    }
}

/// Best-effort flattening of the Docs body tree to plain text. Walks
/// `content[].paragraph.elements[].textRun.content` and concatenates.
/// Ignores tables, images, etc. — agents needing structure can fetch the
/// raw JSON via the underlying API.
fn flatten_body(body: &serde_json::Value) -> Option<String> {
    let content = body.get("content")?.as_array()?;
    let mut out = String::new();
    for el in content {
        let Some(para) = el.get("paragraph") else {
            continue;
        };
        let Some(elements) = para.get("elements").and_then(|v| v.as_array()) else {
            continue;
        };
        for piece in elements {
            if let Some(text_run) = piece.get("textRun") {
                if let Some(s) = text_run.get("content").and_then(|v| v.as_str()) {
                    out.push_str(s);
                }
            }
        }
    }
    Some(out)
}
