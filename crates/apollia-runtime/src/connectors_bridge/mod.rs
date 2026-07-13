//! Bridge layer that turns connector operations (Gmail, Calendar, Drive, …)
//! into [`ToolDescriptor`]s the agent runtime can register at boot.
//!
//! Descriptors are STATIC metadata, they make the LLM aware that the tool
//! exists and what it accepts. The actual EXECUTION is plugged in by
//! `apollia-desktop` (which owns the `AuthManager` singleton and the live
//! `GoogleConnector` instance) at dispatcher-build time, via a parallel
//! `connectors_bridge` module that produces [`ToolExecutor`] implementations
//! sharing the same operation IDs.
//!
//! Splitting the descriptor side (runtime) from the executor side (desktop)
//! avoids a cyclic dependency on `apollia-desktop` from within the runtime
//! while still letting the supervisor register the tool names at startup so
//! agents see them in their tool catalogue regardless of when (or whether) a
//! Google account is connected.

use std::sync::Arc;

use apollia_auth::AuthManager;
use apollia_connectors::google::GoogleConnector;
use serde_json::Value;
use tokio::sync::OnceCell;

mod descriptors;
mod google;
mod microsoft;

pub use descriptors::{
    all_connector_descriptors, google_tool_descriptors, microsoft_tool_descriptors,
};
pub use google::{build_google_executors, GoogleChatToolInvoker};
pub use microsoft::build_microsoft_executors;

// ─── Tests ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;

// ─── Chat Libre invoker (Google) ────────────────────────────────────────────
//
// `NativeChatToolInvoker` uses a hardcoded match on tool names. Connector
// tools are not in that match, so we surface an injected [`ToolInvoker`]
// implementation it can delegate to when the LLM calls `gmail.send`,
// `gcal.list_events`, `gdrive.workspace_write`, etc.
//
// The invoker owns lazy-initialised [`AuthManager`] + [`GoogleConnector`]
// singletons. They share the OS keychain with the parallel singleton in
// `apollia-desktop::commands::integrations`, so OAuth tokens written by the
// desktop side are picked up here transparently. Singleflight refresh inside
// `AuthManager` keeps the two caches from racing.

static GOOGLE_AUTH: OnceCell<Arc<AuthManager>> = OnceCell::const_new();
static GOOGLE_CONNECTOR: OnceCell<Arc<GoogleConnector>> = OnceCell::const_new();

async fn get_google_connector() -> Result<Arc<GoogleConnector>, String> {
    let auth = GOOGLE_AUTH
        .get_or_try_init(|| async {
            AuthManager::new()
                .map(Arc::new)
                .map_err(|e| format!("auth init failed: {e}"))
        })
        .await?
        .clone();
    GOOGLE_CONNECTOR
        .get_or_try_init(|| async {
            GoogleConnector::new(auth.clone())
                .map(Arc::new)
                .map_err(|e| format!("google connector init failed: {e}"))
        })
        .await
        .cloned()
}

async fn get_auth() -> Result<Arc<AuthManager>, String> {
    GOOGLE_AUTH
        .get_or_try_init(|| async {
            AuthManager::new()
                .map(Arc::new)
                .map_err(|e| format!("auth init failed: {e}"))
        })
        .await
        .cloned()
}

fn get_str(input: &Value, key: &str) -> Result<String, String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing required field `{key}`"))
}

fn get_str_opt(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn get_u32_or(input: &Value, key: &str, default: u32) -> u32 {
    input
        .get(key)
        .and_then(Value::as_u64)
        .map(|n| n.min(u32::MAX as u64) as u32)
        .unwrap_or(default)
}

fn parse_rfc3339(s: &str, label: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| format!("`{label}` must be RFC 3339 ({e})"))
}

fn get_str_array(input: &Value, key: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
