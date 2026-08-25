//! E2E tests for the Apollia desktop application.
//!
//! Tests connect to the embedded runtime HTTP API at `localhost:7771` and
//! exercise the five critical user flows. They are marked `#[ignore]` and
//! only run when a live runtime is present.
//!
//! Run with:
//!
//! ```text
//! cargo test -p apollia-desktop --test e2e -- --ignored
//! ```

mod test_agents;
mod test_chat;
mod test_navigation;
mod test_onboarding;
mod test_startup;

use std::time::Duration;

/// Default TCP port for the embedded runtime HTTP API.
pub const RUNTIME_PORT: u16 = 7771;

/// Pause between retry attempts when a test fails due to timing.
const RETRY_PAUSE: Duration = Duration::from_secs(2);

/// Builds the full URL for an embedded runtime API path.
pub fn api_url(path: &str) -> String {
    format!("http://127.0.0.1:{RUNTIME_PORT}{path}")
}

/// Expands a leading `~/` to the value of `$HOME`.
///
/// Falls back to `/tmp` when `HOME` is not set (CI edge case).
pub fn expand_home(path: &str) -> String {
    if let Some(suffix) = path.strip_prefix("~/") {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        format!("{home}/{suffix}")
    } else {
        path.to_string()
    }
}

/// Reads the API Bearer token from `~/.apollia/api-token`.
///
/// Returns an error if the file is absent or empty - which indicates the
/// runtime has not started or the data directory is misconfigured.
pub fn read_api_token() -> Result<String, Box<dyn std::error::Error>> {
    let token_path = expand_home("~/.apollia/api-token");
    let raw = std::fs::read_to_string(&token_path)
        .map_err(|e| format!("cannot read api-token at {token_path}: {e}"))?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        return Err(format!("api-token file at {token_path} is empty").into());
    }
    Ok(token)
}

/// Builds a `reqwest::Client` pre-configured with the runtime Bearer token
/// and a 10-second per-request timeout.
pub fn http_client() -> Result<reqwest::Client, Box<dyn std::error::Error>> {
    let token = read_api_token()?;
    let mut headers = reqwest::header::HeaderMap::new();
    let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
    headers.insert(reqwest::header::AUTHORIZATION, auth_value);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// Executes a test closure with retry.
///
/// Re-runs the closure once after `RETRY_PAUSE` on failure before declaring
/// the test definitively failed. Protects against transient timing issues
/// in async flows without inflating the normal happy-path duration.
pub async fn with_retry<F, Fut>(test_fn: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    for attempt in 1_u32..=2 {
        match test_fn().await {
            Ok(()) => return,
            Err(e) if attempt < 2 => {
                println!(
                    "[e2e] attempt {attempt} failed ({e}), retrying after {}s …",
                    RETRY_PAUSE.as_secs()
                );
                tokio::time::sleep(RETRY_PAUSE).await;
            }
            Err(e) => panic!("test failed after 2 attempts: {e}"),
        }
    }
}
