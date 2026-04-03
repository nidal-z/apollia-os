//! E2E test — bundled agent lifecycle: list → start → verify active.

use std::time::Duration;

/// Path to the bundled onboarding agent, relative to `~/.apollia/`.
const BUNDLED_AGENT_RELPATH: &str = "agents/onboarding-agent/agent.py";

/// Maximum time to wait for an agent to reach the `active` state.
const AGENT_ACTIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// Poll interval while waiting for agent state transitions.
const AGENT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Verifies the full bundled-agent lifecycle:
/// 1. The agent list endpoint is operational.
/// 2. The onboarding agent is started (or confirmed already running).
/// 3. The agent reaches the `active` state within the allowed window.
///
/// If the agent is already active when the test runs (e.g. desktop app
/// auto-started it), the test exits early after confirming the state.
#[tokio::test]
#[ignore = "E2E test — requires running runtime and desktop app"]
async fn test_agents_list_start_bundled_verify_active() {
    super::with_retry(|| async {
        let client = super::http_client()?;

        // 1. Agent list endpoint must return a JSON object with an "agents" array.
        let list: serde_json::Value = client
            .get(super::api_url("/api/v1/agents"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let agents = list
            .get("agents")
            .and_then(|v| v.as_array())
            .ok_or("agent list response missing 'agents' array")?;

        // Fast path: the onboarding-agent is already active.
        let already_active = agents.iter().any(|a| {
            a.get("name").and_then(|n| n.as_str()) == Some("onboarding-agent")
                && a.get("state").and_then(|s| s.as_str()) == Some("active")
        });

        if already_active {
            return Ok(());
        }

        // 2. Start the bundled agent via its on-disk path.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let agent_path = format!("{home}/.apollia/{BUNDLED_AGENT_RELPATH}");

        let start_resp = client
            .post(super::api_url("/api/v1/agents"))
            .json(&serde_json::json!({ "agent_path": agent_path }))
            .send()
            .await?;

        let start_status = start_resp.status();

        let agent_id: String = if start_status.is_success() {
            // Newly started — extract the assigned agent_id.
            let started: serde_json::Value = start_resp.json().await?;
            started
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or("start agent response missing 'agent_id' field")?
                .to_string()
        } else if start_status.as_u16() == 409 {
            // Already registered in the registry — look it up by name.
            let list2: serde_json::Value = client
                .get(super::api_url("/api/v1/agents"))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            list2
                .get("agents")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    arr.iter().find(|a| {
                        a.get("name").and_then(|n| n.as_str()) == Some("onboarding-agent")
                    })
                })
                .and_then(|a| a.get("agent_id").and_then(|v| v.as_str()))
                .ok_or("onboarding-agent not found in registry after 409 conflict")?
                .to_string()
        } else {
            let body = start_resp.text().await.unwrap_or_default();
            return Err(format!(
                "POST /api/v1/agents returned {}: {body}",
                start_status.as_u16()
            )
            .into());
        };

        // 3. Poll until the agent reaches the `active` state or the deadline.
        let deadline = tokio::time::Instant::now() + AGENT_ACTIVE_TIMEOUT;

        loop {
            let detail: serde_json::Value = client
                .get(super::api_url(&format!("/api/v1/agents/{agent_id}")))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            let state = detail
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");

            if state == "active" {
                return Ok(());
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "agent '{agent_id}' did not reach 'active' state within {}s (last: {state})",
                    AGENT_ACTIVE_TIMEOUT.as_secs()
                )
                .into());
            }

            tokio::time::sleep(AGENT_POLL_INTERVAL).await;
        }
    })
    .await;
}
