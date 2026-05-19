//! Google Forms API client — non-sensitive scope (`forms.body`).
//!
//! v0.1.x exposes only form creation. Agents can read responses via a
//! follow-up via Drive (`drive.file`) once the form is owned by Apollia.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{error::ConnectorError, http::HttpClient};

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
}

impl FormsClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
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
            .json_request(Method::POST, BASE, &body, bearer, refresh)
            .await
    }
}
