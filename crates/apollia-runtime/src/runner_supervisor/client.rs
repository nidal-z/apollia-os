//! Client HTTP vers le runner (loopback TCP).

use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use super::error::RunnerError;

/// HTTP client that talks to the runner via 127.0.0.1:<port>.
///
/// Wraps `reqwest::Client` with: default 60-second timeout, base URL pinned to
/// the port selected at spawn, deserialization into the IPC types of the
/// `apollia-runner` crate.
#[derive(Clone)]
pub struct RunnerClient {
    http: Client,
    base_url: String,
}

impl RunnerClient {
    pub fn new(port: u16) -> Result<Self, RunnerError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| RunnerError::Http(e.to_string()))?;
        Ok(Self {
            http,
            base_url: format!("http://127.0.0.1:{port}"),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET un endpoint, parse JSON.
    pub async fn get<D: DeserializeOwned>(&self, path: &str) -> Result<D, RunnerError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| RunnerError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(RunnerError::Http(format!(
                "GET {} returned {}",
                path,
                resp.status()
            )));
        }
        resp.json::<D>()
            .await
            .map_err(|e| RunnerError::Http(e.to_string()))
    }

    /// POST JSON, parse JSON response.
    pub async fn post<P: Serialize, D: DeserializeOwned>(
        &self,
        path: &str,
        body: &P,
    ) -> Result<D, RunnerError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| RunnerError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            // Try to parse a normalized ErrorBody.
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(err) = parsed.get("error") {
                    let code = err
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UNKNOWN")
                        .to_string();
                    let message = err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    return Err(RunnerError::Ipc { code, message });
                }
            }
            return Err(RunnerError::Http(format!(
                "POST {} returned {}: {}",
                path, status, text
            )));
        }
        resp.json::<D>()
            .await
            .map_err(|e| RunnerError::Http(e.to_string()))
    }

    /// Generate a UUID v4 `request_id` for log correlation.
    pub fn new_request_id() -> Uuid {
        Uuid::new_v4()
    }
}
