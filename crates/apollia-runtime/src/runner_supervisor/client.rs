//! Client HTTP vers le runner (loopback TCP).

use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::RunnerError;

/// Formats a `reqwest::Error` with its full source chain.
///
/// `reqwest::Error`'s own `Display` stops at "error sending request for url
/// (...)" and hides the underlying cause (timeout, connection reset, refused).
/// Walking `source()` keeps those causes in the daemon logs.
fn http_err(e: reqwest::Error) -> RunnerError {
    use std::error::Error;
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    RunnerError::Http(msg)
}

/// Cap on a buffered runner answer. The streaming path is not affected: it
/// returns the response untouched and the caller reads the event stream. A
/// non-streaming completion is a JSON document, and 64 MiB is far above the
/// largest one the runner produces.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// Read a bounded JSON answer from the runner.
async fn read_capped<D: DeserializeOwned>(resp: reqwest::Response) -> Result<D, RunnerError> {
    apollia_core::net::read_capped_json(resp, MAX_RESPONSE_BYTES)
        .await
        .map_err(|e| RunnerError::Http(e.to_string()))
}

/// Timeout for control-plane GETs (handshake, health): these must answer fast.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for data-plane POSTs (load_model, complete, transcribe). Local
/// inference is effectively unbounded: a large model compiles Metal kernels on
/// the first call, prefills a big tool-augmented prompt, and may emit a long
/// reasoning block before answering. A short cap would abort legitimate work,
/// surfacing as "error sending request". The generous bound still catches a
/// genuinely wedged runner.
const DATA_TIMEOUT: Duration = Duration::from_secs(600);

/// HTTP client that talks to the runner via 127.0.0.1:<port>.
///
/// Wraps `reqwest::Client`: short per-request timeout on control-plane GETs,
/// generous timeout on data-plane POSTs, base URL pinned to the port selected
/// at spawn, deserialization into the IPC types of the `apollia-runner` crate.
#[derive(Clone)]
pub struct RunnerClient {
    http: Client,
    base_url: String,
}

impl RunnerClient {
    pub fn new(port: u16) -> Result<Self, RunnerError> {
        // No global request timeout: it is applied per-request so inference is
        // not capped by the control-plane bound.
        //
        // `pool_max_idle_per_host(0)` disables idle keep-alive reuse: between
        // the infrequent, long inference calls the runner can close an idle
        // pooled socket, and reusing it surfaces as "error sending request".
        // A fresh connection per call is negligible on loopback.
        // The runner is a child process on loopback, so the
        // public-destination policy is deliberately not applied.
        let http = apollia_core::net::configured_endpoint_client_builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(0)
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
            .timeout(CONTROL_TIMEOUT)
            .send()
            .await
            .map_err(http_err)?;
        if !resp.status().is_success() {
            return Err(RunnerError::Http(format!(
                "GET {} returned {}",
                path,
                resp.status()
            )));
        }
        read_capped(resp).await
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
            .timeout(DATA_TIMEOUT)
            .json(body)
            .send()
            .await
            .map_err(http_err)?;
        if !resp.status().is_success() {
            // Try to parse a normalized ErrorBody.
            let status = resp.status();
            let text = apollia_core::net::read_capped_text(resp, MAX_RESPONSE_BYTES)
                .await
                .unwrap_or_default();
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
        read_capped(resp).await
    }
}
