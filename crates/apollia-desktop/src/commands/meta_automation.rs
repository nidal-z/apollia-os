//! Tauri IPC command for the natural-language → schedule wizard.
//!
//! Single entry point: `meta_parse_automation` takes a free-form description
//! and the list of installed agents, and returns a `ParsedAutomation` that the
//! UI maps onto the wizard steps.
//!
//! The parse lives in `apollia_runtime::chat_assists`: embedded, it runs here;
//! attached, the daemon runs it at `POST /api/v1/assists/automation`.

use apollia_llm::meta::parse_automation::ParsedAutomation;
use apollia_runtime::chat_assists;
use apollia_runtime::chat_assists::ParseAutomationRequest;
use chrono::Utc;
use tauri::State;

use crate::attached::RuntimeLink;

/// The route of the automation parse on the runtime.
const AUTOMATION: &str = "/api/v1/assists/automation";

async fn parse_on(
    link: &RuntimeLink,
    request: ParseAutomationRequest,
) -> Result<ParsedAutomation, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(chat_assists::parse(&request, Utc::now())),
        RuntimeLink::Attached(remote) => Ok(remote.post(AUTOMATION, &request).await?),
    }
}

/// Parses a natural-language description (fr/en) into an automation.
///
/// An empty input returns a `ParsedAutomation` with `confidence = Low` and one
/// ambiguity; the UI gates the step on the ambiguities. Attached, a refusal of
/// the daemon comes back in its own words.
#[tauri::command]
pub async fn meta_parse_automation(
    link: State<'_, RuntimeLink>,
    request: ParseAutomationRequest,
) -> Result<ParsedAutomation, String> {
    parse_on(link.inner(), request).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::AttachedClient;

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn request() -> ParseAutomationRequest {
        ParseAutomationRequest {
            input: "every day at 9am summarise my emails".into(),
            known_agents: vec!["mail-digest".into()],
        }
    }

    #[tokio::test]
    async fn test_an_automation_is_parsed_by_the_daemon_with_the_token() {
        // GIVEN a daemon parsing a daily automation
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(AUTOMATION))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(serde_json::json!({
                "input": "every day at 9am summarise my emails",
                "known_agents": ["mail-digest"],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "schedule": null,
                "agent": null,
                "payload": "summarise my emails",
                "confidence": "low",
                "ambiguities": ["no schedule"],
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached wizard parses the description
        let parsed = parse_on(&attached(&server), request()).await.unwrap();

        // THEN the daemon's parse comes back
        assert_eq!(parsed.payload.as_deref(), Some("summarise my emails"));
        assert_eq!(parsed.ambiguities, ["no schedule"]);
    }

    #[tokio::test]
    async fn test_an_automation_refused_by_the_daemon_shows_its_words() {
        // GIVEN a daemon refusing the request
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(AUTOMATION))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(serde_json::json!({ "error": "runtime is shutting down" })),
            )
            .mount(&server)
            .await;

        // WHEN the attached wizard parses the description
        let refused = parse_on(&attached(&server), request()).await;

        // THEN the refusal reads in the daemon's own words
        assert_eq!(refused.unwrap_err(), "runtime is shutting down");
    }
}
