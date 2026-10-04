//! `apollia.toml` as the editor of Settings > Configuration reads and writes
//! it: the text with its fingerprint, faults, keys fixed by the policy and
//! keys waiting for a restart; the verdict on a text being typed; the save of
//! a whole text; the default text.
//!
//! Each command is one call to a configuration route of the runtime this
//! application drives (`GET` and `PUT /api/v1/config`, `POST
//! /api/v1/config/validate`, `GET /api/v1/config/default`), so the editor
//! works the same embedded and attached, and the validation, the policy and
//! the fingerprint check are the runtime's, never this window's.

use serde_json::{json, Value};
use tauri::State;

use crate::attached::RuntimeLink;

/// Route that reads and saves the file.
pub(crate) const CONFIG_ROUTE: &str = "/api/v1/config";

/// Route that checks a text without writing it.
pub(crate) const VALIDATE_ROUTE: &str = "/api/v1/config/validate";

/// Route of the default text.
pub(crate) const DEFAULT_ROUTE: &str = "/api/v1/config/default";

/// Reads the file on the runtime this application drives.
pub(crate) async fn read_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(CONFIG_ROUTE).await
}

/// Checks a text on the runtime this application drives.
pub(crate) async fn validate_on(link: &RuntimeLink, text: &str) -> Result<Value, String> {
    link.post_json(VALIDATE_ROUTE, &json!({ "text": text }))
        .await
}

/// Saves a whole text, made from the text whose fingerprint is `sha256`.
pub(crate) async fn save_on(link: &RuntimeLink, text: &str, sha256: &str) -> Result<Value, String> {
    let saved = link
        .put_json(CONFIG_ROUTE, &json!({ "text": text, "sha256": sha256 }))
        .await?;
    tracing::event!(
        tracing::Level::INFO,
        bytes = text.len(),
        "desktop.config_file.saved"
    );
    Ok(saved)
}

/// Reads the default text on the runtime this application drives.
pub(crate) async fn default_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(DEFAULT_ROUTE).await
}

/// The file as the runtime reads it, over `GET /api/v1/config`.
#[tauri::command]
pub async fn read_config_file(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    read_on(link.inner()).await
}

/// The verdict on a text, over `POST /api/v1/config/validate`.
#[tauri::command]
pub async fn validate_config_text(
    link: State<'_, RuntimeLink>,
    text: String,
) -> Result<Value, String> {
    validate_on(link.inner(), &text).await
}

/// Saves a whole text, over `PUT /api/v1/config`.
#[tauri::command]
pub async fn save_config_text(
    link: State<'_, RuntimeLink>,
    text: String,
    sha256: String,
) -> Result<Value, String> {
    save_on(link.inner(), &text, &sha256).await
}

/// The default text, over `GET /api/v1/config/default`.
#[tauri::command]
pub async fn default_config_text(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    default_on(link.inner()).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

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
    async fn test_the_file_is_read_from_the_daemon() {
        // GIVEN a daemon whose file sets the API port
        let server = MockServer::start().await;
        let held = json!({ "path": "/h/.apollia/apollia.toml", "text": "[api]\nport = 7771\n", "sha256": "abc" });
        authorized("GET", CONFIG_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(held.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the editor reads the file
        let read = read_on(&attached(&server)).await.unwrap();

        // THEN it is the daemon's view, as it answered
        assert_eq!(read, held);
    }

    #[tokio::test]
    async fn test_a_text_is_checked_by_the_daemon() {
        // GIVEN a daemon that refuses a port written as a string
        let server = MockServer::start().await;
        let verdict = json!({ "valid": false, "diagnostics": [{ "line": 2, "column": 8, "path": "api.port", "message": "invalid type" }] });
        authorized("POST", VALIDATE_ROUTE)
            .and(body_json(json!({ "text": "[api]\nport = \"x\"\n" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(verdict.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the editor checks the text being typed
        let checked = validate_on(&attached(&server), "[api]\nport = \"x\"\n")
            .await
            .unwrap();

        // THEN the verdict carries the fault at its line
        assert_eq!(checked, verdict);
    }

    #[tokio::test]
    async fn test_a_save_sends_the_text_with_the_fingerprint_it_was_made_from() {
        // GIVEN a daemon that takes the text made from fingerprint abc
        let server = MockServer::start().await;
        let saved = json!({ "text": "[chat]\nplan_mode_default = true\n", "sha256": "def" });
        authorized("PUT", CONFIG_ROUTE)
            .and(body_json(
                json!({ "text": "[chat]\nplan_mode_default = true\n", "sha256": "abc" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(saved.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the editor saves
        let answer = save_on(
            &attached(&server),
            "[chat]\nplan_mode_default = true\n",
            "abc",
        )
        .await
        .unwrap();

        // THEN the answer is the file as saved
        assert_eq!(answer, saved);
    }

    #[tokio::test]
    async fn test_a_save_refused_as_modified_says_the_daemon_sentence() {
        // GIVEN a daemon whose file changed since it was read
        let server = MockServer::start().await;
        authorized("PUT", CONFIG_ROUTE)
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": "apollia.toml changed since it was read",
                "code": "modified",
                "diagnostics": [],
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the editor saves
        let refused = save_on(&attached(&server), "", "abc").await;

        // THEN the error is the daemon's sentence, word for word
        assert_eq!(
            refused.unwrap_err(),
            "apollia.toml changed since it was read"
        );
    }

    #[tokio::test]
    async fn test_the_default_text_is_the_daemon_one() {
        // GIVEN a daemon whose policy fixes nothing
        let server = MockServer::start().await;
        let text = json!({ "text": "# apollia.toml\n" });
        authorized("GET", DEFAULT_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(text.clone()))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the editor asks for the default text
        let read = default_on(&attached(&server)).await.unwrap();

        // THEN it is the daemon's
        assert_eq!(read, text);
    }

    #[test]
    fn test_the_editor_commands_are_routed_to_a_daemon() {
        // GIVEN the four commands of the editor
        let commands = [
            "read_config_file",
            "validate_config_text",
            "save_config_text",
            "default_config_text",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
