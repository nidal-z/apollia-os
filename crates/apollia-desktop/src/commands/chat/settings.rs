//! The settings one conversation holds for itself.
//!
//! The composer reads them to show how the model reasons in this conversation
//! and replaces them when the user changes it. Both go through the routes the
//! runtime serves, `GET` and `PUT /api/v1/sessions/{id}/settings`, so the
//! answer is the same whether the runtime runs in this process or as a
//! daemon. The route owns the shape: a field it does not declare is refused
//! there, and a field left out is inherited from the agent.

use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// The runtime route of the settings of `session_id`.
pub(crate) fn session_settings_path(session_id: &str) -> String {
    format!("/api/v1/sessions/{}/settings", segment(session_id))
}

/// What `session_id` changes from its agent: only the fields it sets.
#[tauri::command]
pub async fn get_chat_session_settings(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<Value, String> {
    link.get_json(&session_settings_path(&session_id)).await
}

/// Replaces the settings of `session_id` and answers them as stored. They
/// hold for the turns that follow, in this conversation alone.
#[tauri::command]
pub async fn set_chat_session_settings(
    link: State<'_, RuntimeLink>,
    session_id: String,
    settings: Value,
) -> Result<Value, String> {
    link.put_json(&session_settings_path(&session_id), &settings)
        .await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[tokio::test]
    async fn test_the_settings_of_a_conversation_are_read_from_the_daemon() {
        // GIVEN a daemon whose conversation `s1` has reasoning switched off
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/s1/settings"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"thinking": false})),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the composer reads the settings of the conversation
        let settings = attached(&server)
            .get_json(&session_settings_path("s1"))
            .await
            .unwrap();

        // THEN it reads the one field the conversation sets
        assert_eq!(settings, serde_json::json!({"thinking": false}));
    }

    #[tokio::test]
    async fn test_the_settings_of_a_conversation_are_replaced_on_the_daemon() {
        // GIVEN a daemon that stores the settings it is sent
        let server = MockServer::start().await;
        let sent = serde_json::json!({"reasoning_effort": "medium", "temperature": 0.2});
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/s1/settings"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(&sent))
            .respond_with(ResponseTemplate::new(200).set_body_json(&sent))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the composer writes a level of reasoning beside a temperature
        let stored = attached(&server)
            .put_json(&session_settings_path("s1"), &sent)
            .await
            .unwrap();

        // THEN the daemon received both and answers them as stored
        assert_eq!(stored, sent);
    }

    #[tokio::test]
    async fn test_a_refused_write_is_an_error() {
        // GIVEN a daemon whose conversation is closed
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/s1/settings"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(serde_json::json!({"error": "session closed"})),
            )
            .mount(&server)
            .await;

        // WHEN the composer writes a setting
        let written = attached(&server)
            .put_json(
                &session_settings_path("s1"),
                &serde_json::json!({"thinking": true}),
            )
            .await;

        // THEN the refusal is reported
        assert!(written.is_err());
    }

    #[test]
    fn test_the_identifier_is_encoded_as_a_path_segment() {
        // GIVEN an identifier that carries a slash
        // WHEN the route is built
        let route = session_settings_path("a/b");

        // THEN it stays one segment
        assert_eq!(route, "/api/v1/sessions/a%2Fb/settings");
    }

    #[test]
    fn test_both_commands_are_routed_to_a_daemon() {
        // GIVEN the two commands of the settings of a conversation
        // WHEN each passes the gate of an attached window
        // THEN neither is refused
        assert!(gate(LinkMode::Attached, "get_chat_session_settings").is_ok());
        assert!(gate(LinkMode::Attached, "set_chat_session_settings").is_ok());
    }
}
