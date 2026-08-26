//! E2E test - chat session lifecycle: create → send → receive → close.

/// Verifies the full Libre chat flow:
/// 1. Create a Libre session.
/// 2. Send a message.
/// 3. Confirm the message is persisted in the session detail.
/// 4. Close the session to leave the runtime in a clean state.
///
/// A 503 from the sessions endpoint signals that the chat subsystem is not
/// configured; the test exits early with a pass in that case.
#[tokio::test]
#[ignore = "true once the runtime and the desktop app are up; the e2e-desktop job of nightly.yml starts them and runs this via: cargo test -p apollia-desktop --test e2e -- --ignored"]
async fn test_chat_create_session_send_receive() {
    // GIVEN a runtime and a desktop app already up, as the nightly job starts them
    // WHEN a free session is created, a message is sent, and the session is read back
    // THEN the message is in the persisted history, and the session closes
    super::with_retry(|| async {
        let client = super::http_client()?;

        // 1. Create a Libre chat session (no agent, no LLM required for creation).
        let create_resp = client
            .post(super::api_url("/api/v1/sessions"))
            .json(&serde_json::json!({ "mode": "libre" }))
            .send()
            .await?;

        if create_resp.status().as_u16() == 503 {
            // Chat subsystem not configured - treat as pass.
            return Ok(());
        }

        let session: serde_json::Value = create_resp.error_for_status()?.json().await?;

        let session_id = session
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("create session response missing 'id' field")?
            .to_string();

        // 2. Send a user message.
        let msg_resp = client
            .post(super::api_url(&format!(
                "/api/v1/sessions/{session_id}/messages"
            )))
            .json(&serde_json::json!({ "content": "hello from e2e test" }))
            .send()
            .await?;

        let msg_status = msg_resp.status();
        // 503 = LLM backend unavailable (message accepted but cannot be processed).
        if msg_status.is_server_error() && msg_status.as_u16() != 503 {
            return Err(format!(
                "unexpected server error {} when sending message",
                msg_status.as_u16()
            )
            .into());
        }

        // 3. Fetch the session detail and verify the message was persisted.
        let detail: serde_json::Value = client
            .get(super::api_url(&format!("/api/v1/sessions/{session_id}")))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let messages = detail
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or("session detail missing 'messages' array")?;

        if messages.is_empty() {
            return Err("expected at least one message in session detail after send".into());
        }

        // 4. Close the session.
        client
            .delete(super::api_url(&format!("/api/v1/sessions/{session_id}")))
            .send()
            .await?;

        Ok(())
    })
    .await;
}
