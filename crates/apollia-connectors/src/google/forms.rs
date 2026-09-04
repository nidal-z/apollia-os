//! Google Forms API client, non-sensitive scope (`forms.body`).
//!
//! v0.1.x exposes only form creation. Agents can read responses via a
//! follow-up via Drive (`drive.file`) once the form is owned by Apollia.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest},
};

const BASE: &str = "https://forms.googleapis.com/v1/forms";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormMeta {
    pub form_id: String,
    #[serde(default)]
    pub responder_uri: Option<String>,
    pub info: FormInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormInfo {
    pub title: String,
    #[serde(default)]
    pub document_title: Option<String>,
}

#[derive(Clone)]
pub struct FormsClient {
    http: HttpClient,
    base: String,
}

impl FormsClient {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            base: BASE.to_owned(),
        }
    }

    /// Build a client whose upstream base URL is `base`, used by the replay
    /// harness in [`crate::replay`] to drive the real client methods against a
    /// simulated server.
    ///
    /// Test-only. Production always goes through [`Self::new`], which pins the
    /// real upstream host.
    #[cfg(test)]
    pub fn with_base_url(http: HttpClient, base: &str) -> Self {
        Self {
            http,
            base: base.to_owned(),
        }
    }

    /// Create a new empty form with the given title. The form is owned
    /// by the connected Google account and visible at the `responderUri`
    /// returned in the response.
    pub async fn create<F, Fut>(
        &self,
        title: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<FormMeta, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body = serde_json::json!({
            "info": { "title": title }
        });
        self.http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &self.base,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await
    }
}
