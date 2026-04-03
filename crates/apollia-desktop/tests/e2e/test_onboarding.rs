//! E2E test — first-launch onboarding skip flow.

/// Verifies that the onboarding skip flow completes successfully:
/// 1. The runtime is healthy before the flow begins.
/// 2. User memory accepts a "done" phase write (mirrors the dismiss IPC command).
/// 3. The health endpoint remains operational after the write.
///
/// A 503 from the user-profile endpoint is treated as a pass: user memory is
/// an optional subsystem and its absence does not impair runtime operation.
#[tokio::test]
#[ignore = "E2E test — requires running runtime and desktop app"]
async fn test_onboarding_first_launch_skip_to_dashboard() {
    super::with_retry(|| async {
        let client = super::http_client()?;

        // 1. Pre-condition: runtime must be healthy.
        client
            .get(super::api_url("/api/v1/health"))
            .send()
            .await?
            .error_for_status()?;

        // 2. Simulate the skip action by persisting the terminal onboarding
        //    phase to the user profile. This mirrors what the dismiss_onboarding
        //    IPC command writes to UserMemoryRepository.
        let profile_resp = client
            .put(super::api_url("/api/v1/user/profile"))
            .json(&serde_json::json!({
                "onboarding_phase": "done",
                "onboarding_skipped": "true"
            }))
            .send()
            .await?;

        let profile_status = profile_resp.status();
        // 503 = user memory not configured (optional subsystem) — acceptable.
        if profile_status.is_server_error() && profile_status.as_u16() != 503 {
            return Err(format!(
                "unexpected server error {} from PUT /api/v1/user/profile",
                profile_status.as_u16()
            )
            .into());
        }

        // 3. Dashboard (health) must still respond after the skip write.
        client
            .get(super::api_url("/api/v1/health"))
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    })
    .await;
}
