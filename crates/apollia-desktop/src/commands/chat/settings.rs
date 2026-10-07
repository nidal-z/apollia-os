//! The settings of one conversation, its notes, and the preset its settings
//! are kept as.
//!
//! Each command calls one runtime route, so the write is the same whether
//! the runtime runs in this process or as a daemon:
//!
//! - `get_chat_session_settings`: `GET /api/v1/sessions/{id}/settings`;
//! - `set_chat_session_settings`: `PUT /api/v1/sessions/{id}/settings`;
//! - `get_chat_session_notes`: `GET /api/v1/sessions/{id}/notes`;
//! - `set_chat_session_notes`: `PUT /api/v1/sessions/{id}/notes`;
//!
//! The runtime validates what it stores: an unknown field, a value out of
//! bounds or a preset it refuses comes back as the error the panel shows.

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Route of the settings of the conversation `session_id`.
fn settings_path(session_id: &str) -> String {
    format!("/api/v1/sessions/{}/settings", segment(session_id))
}

/// Route of the notes of the conversation `session_id`.
fn notes_path(session_id: &str) -> String {
    format!("/api/v1/sessions/{}/notes", segment(session_id))
}

pub(crate) async fn settings_on(link: &RuntimeLink, session_id: &str) -> Result<Value, String> {
    link.get_json(&settings_path(session_id)).await
}

pub(crate) async fn set_settings_on(
    link: &RuntimeLink,
    session_id: &str,
    settings: &Value,
) -> Result<Value, String> {
    let stored = link.put_json(&settings_path(session_id), settings).await?;
    tracing::event!(
        tracing::Level::INFO,
        session_id = %session_id,
        "chat.session.settings_set"
    );
    Ok(stored)
}

pub(crate) async fn notes_on(link: &RuntimeLink, session_id: &str) -> Result<String, String> {
    let body = link.get_json(&notes_path(session_id)).await?;
    Ok(notes_of(&body))
}

pub(crate) async fn set_notes_on(
    link: &RuntimeLink,
    session_id: &str,
    notes: &str,
) -> Result<String, String> {
    let body = link
        .put_json(&notes_path(session_id), &json!({ "notes": notes }))
        .await?;
    Ok(notes_of(&body))
}

/// The text of a notes answer, empty when the route carries none.
fn notes_of(body: &Value) -> String {
    body.get("notes")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The settings of the conversation `session_id`; a field left out is
/// inherited from its agent.
#[tauri::command]
pub async fn get_chat_session_settings(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<Value, String> {
    settings_on(link.inner(), &session_id).await
}

/// Replaces the settings of the conversation `session_id` and answers them
/// as stored.
#[tauri::command]
pub async fn set_chat_session_settings(
    link: State<'_, RuntimeLink>,
    session_id: String,
    settings: Value,
) -> Result<Value, String> {
    set_settings_on(link.inner(), &session_id, &settings).await
}

/// The notes of the conversation `session_id`.
#[tauri::command]
pub async fn get_chat_session_notes(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<String, String> {
    notes_on(link.inner(), &session_id).await
}

/// Replaces the notes of the conversation `session_id` and answers them as
/// stored.
#[tauri::command]
pub async fn set_chat_session_notes(
    link: State<'_, RuntimeLink>,
    session_id: String,
    notes: String,
) -> Result<String, String> {
    set_notes_on(link.inner(), &session_id, &notes).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};
    use crate::commands::ai_model_detail::create_preset_on;

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
    }

    #[tokio::test]
    async fn test_the_settings_of_a_conversation_are_read_then_replaced() {
        // GIVEN a daemon holding the settings of one conversation
        let server = MockServer::start().await;
        authorized("GET", "/api/v1/sessions/s-1/settings")
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "preset": 4 })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("PUT", "/api/v1/sessions/s-1/settings")
            .and(body_json(json!({ "preset": 4, "temperature": 0.2 })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "preset": 4, "temperature": 0.2 })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the panel reads them, then writes a temperature
        let read = settings_on(&link, "s-1").await.unwrap();
        let stored = set_settings_on(&link, "s-1", &json!({ "preset": 4, "temperature": 0.2 }))
            .await
            .unwrap();

        // THEN each answer is the settings as the daemon holds them
        assert_eq!(read, json!({ "preset": 4 }));
        assert_eq!(stored["temperature"], 0.2);
    }

    #[tokio::test]
    async fn test_a_refused_setting_comes_back_as_the_text_of_the_refusal() {
        // GIVEN a daemon that refuses a temperature out of bounds
        let server = MockServer::start().await;
        authorized("PUT", "/api/v1/sessions/s-1/settings")
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "the temperature must be between 0 and 2, got 3",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the panel writes it
        let refused = set_settings_on(&attached(&server), "s-1", &json!({ "temperature": 3.0 }))
            .await
            .unwrap_err();

        // THEN the refusal names what is wrong
        assert!(
            refused.contains("the temperature must be between 0 and 2"),
            "{refused}"
        );
    }

    #[tokio::test]
    async fn test_the_notes_of_a_conversation_are_read_then_replaced() {
        // GIVEN a daemon holding the notes of one conversation
        let server = MockServer::start().await;
        authorized("GET", "/api/v1/sessions/s-1/notes")
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "notes": "Call back" })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("PUT", "/api/v1/sessions/s-1/notes")
            .and(body_json(json!({ "notes": "Call back on Monday" })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "notes": "Call back on Monday" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the panel reads the notes, then writes them as they are typed
        let read = notes_on(&link, "s-1").await.unwrap();
        let stored = set_notes_on(&link, "s-1", "Call back on Monday")
            .await
            .unwrap();

        // THEN each answer is the text the daemon holds
        assert_eq!(read, "Call back");
        assert_eq!(stored, "Call back on Monday");
    }

    #[tokio::test]
    async fn test_a_preset_is_created_through_the_route_that_validates_it() {
        // GIVEN a daemon that creates one preset and refuses another
        let server = MockServer::start().await;
        let accepted = json!({
            "name": "Precise", "model_source": "local", "model_id": "qwen3-8b",
            "request": { "temperature": 0.2 },
        });
        let refused = json!({
            "name": "Broken", "model_source": "local", "model_id": "qwen3-8b",
            "request": { "system_prompt": "You are" },
        });
        authorized("POST", "/api/v1/llm/presets")
            .and(body_json(accepted.clone()))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": 7, "name": "Precise", "model_source": "local", "model_id": "qwen3-8b",
            })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("POST", "/api/v1/llm/presets")
            .and(body_json(refused.clone()))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "error": "unknown field `system_prompt`",
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the panel saves a preset, then imports a file the route refuses
        let created = create_preset_on(&link, &accepted).await.unwrap();
        let error = create_preset_on(&link, &refused).await.unwrap_err();

        // THEN the first comes back with its identifier, the second with why
        assert_eq!(created["id"], 7);
        assert!(error.contains("unknown field `system_prompt`"), "{error}");
    }

    #[test]
    fn test_the_settings_commands_are_routed_to_a_daemon() {
        // GIVEN the commands of the settings of a conversation
        let commands = [
            "get_chat_session_settings",
            "set_chat_session_settings",
            "get_chat_session_notes",
            "set_chat_session_notes",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
