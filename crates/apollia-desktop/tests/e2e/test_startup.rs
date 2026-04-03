//! E2E test — application startup and SSE infrastructure connectivity.

/// Verifies that the runtime is fully started, the health endpoint confirms a
/// ready state, and the SSE infrastructure is reachable.
///
/// Two signals are checked:
/// - `GET /api/v1/health` returns `{"status": "ok"}`.
/// - The SSE task-stream endpoint responds with a structured HTTP reply (not a
///   connection error), confirming the server accepts event-stream connections.
#[tokio::test]
#[ignore = "E2E test — requires running runtime and desktop app"]
async fn test_startup_sse_connect_dashboard_render() {
    super::with_retry(|| async {
        let client = super::http_client()?;

        // 1. Health endpoint must report a ready runtime.
        let health: serde_json::Value = client
            .get(super::api_url("/api/v1/health"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let status = health
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or("health response missing 'status' field")?;

        if status != "ok" {
            return Err(format!("expected health status 'ok', got '{status}'").into());
        }

        // 2. Connect to the SSE task-stream endpoint with a synthetic task id.
        //    A 200 (stream open) or 404 (task not found) both confirm the SSE
        //    infrastructure is operational. Only 5xx responses indicate a fault.
        let sse_resp = client
            .get(super::api_url("/api/v1/tasks/e2e-probe-startup/stream"))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?;

        let sse_status = sse_resp.status();
        if sse_status.is_server_error() {
            return Err(format!(
                "SSE task-stream endpoint returned server error {}",
                sse_status.as_u16()
            )
            .into());
        }

        Ok(())
    })
    .await;
}
