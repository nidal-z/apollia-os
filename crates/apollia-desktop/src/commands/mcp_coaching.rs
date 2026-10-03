//! Tauri IPC command: post-install coaching examples for a freshly added
//! MCP connection.
//!
//! Returns 2–3 ready-to-send prompts the operator can click to try the newly
//! connected service immediately. The heuristic lives in
//! `apollia_runtime::chat_assists`: embedded, it runs here; attached, the
//! daemon runs it at `POST /api/v1/assists/coaching`.

use apollia_runtime::chat_assists;
use apollia_runtime::chat_assists::{CoachingExample, CoachingRequest};
use tauri::State;

use crate::attached::RuntimeLink;

/// The route of the connector coaching on the runtime.
const COACHING: &str = "/api/v1/assists/coaching";

async fn coaching_on(
    link: &RuntimeLink,
    request: CoachingRequest,
) -> Result<Vec<CoachingExample>, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(chat_assists::coaching(request)),
        RuntimeLink::Attached(remote) => Ok(remote.post(COACHING, &request).await?),
    }
}

/// Generate 2–3 post-install examples for a newly added MCP server.
///
/// The heuristic matches against well-known connector names; unknown servers
/// get a single generic "Explore the tools" card. Attached, a refusal of the
/// daemon comes back in its own words.
#[tauri::command]
pub async fn meta_generate_capabilities_coaching(
    link: State<'_, RuntimeLink>,
    request: CoachingRequest,
) -> Result<Vec<CoachingExample>, String> {
    coaching_on(link.inner(), request).await
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

    fn request() -> CoachingRequest {
        CoachingRequest {
            server_name: "notion".into(),
            server_title: Some("Team Notion".into()),
        }
    }

    #[tokio::test]
    async fn test_coaching_reaches_the_daemon_with_the_token() {
        // GIVEN a daemon coaching a Notion server
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(COACHING))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(serde_json::json!({
                "serverName": "notion",
                "serverTitle": "Team Notion",
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "title": "Summarise my latest Team Notion page",
                    "description": "Open a chat.",
                    "prompt": "Summarise my latest Notion page.",
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached wizard asks for examples
        let cards = coaching_on(&attached(&server), request()).await.unwrap();

        // THEN the daemon's cards come back
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "Summarise my latest Team Notion page");
    }

    #[tokio::test]
    async fn test_coaching_refused_by_the_daemon_shows_its_words() {
        // GIVEN a daemon refusing the request
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(COACHING))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(serde_json::json!({ "error": "runtime is shutting down" })),
            )
            .mount(&server)
            .await;

        // WHEN the attached wizard asks for examples
        let refused = coaching_on(&attached(&server), request()).await;

        // THEN the refusal reads in the daemon's own words
        assert_eq!(refused.unwrap_err(), "runtime is shutting down");
    }
}
