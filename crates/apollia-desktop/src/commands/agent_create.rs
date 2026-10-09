//! Tauri IPC commands behind the creation of an agent without code.
//!
//! The Agents screen shows a short form; the agent workshop, a workflow
//! shipped with Apollia, turns it into a sheet; the person reads the sheet
//! over in the form of an agent and saves it. Three commands serve that path,
//! each a call to the runtime, whether it runs in this process or as a
//! daemon:
//!
//! - `agent_draft_offer` reads what a sheet may hold, which the workshop is
//!   handed (`GET /api/v1/agent-drafts/offer`);
//! - `check_agent_draft` has the sheet the workshop wrote checked against the
//!   machine (`POST /api/v1/agent-drafts`); nothing is written;
//! - `create_agent_definition` sends the sheet the person saved to
//!   `POST /api/v1/agent-definitions`: the runtime stores the agent with the
//!   origin `interface` and records its creation. The answer is the agent in
//!   force.

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::RuntimeLink;

/// Route of the stored agents.
const DEFINITIONS_PATH: &str = "/api/v1/agent-definitions";

/// Route of what the draft of an agent may hold.
const DRAFT_OFFER_PATH: &str = "/api/v1/agent-drafts/offer";

/// Route that checks a proposed sheet.
const DRAFT_CHECK_PATH: &str = "/api/v1/agent-drafts";

/// The sheet the person validated.
#[derive(Debug, Deserialize)]
pub struct AgentSheetDraft {
    /// Display name.
    pub name: String,
    /// What the agent is for.
    pub mission: String,
    /// Example requests.
    #[serde(default)]
    pub examples: Vec<String>,
    /// Accesses granted, as checked in the form.
    #[serde(default)]
    pub access: Vec<String>,
    /// Sources checked in the form.
    #[serde(default)]
    pub sources: Vec<String>,
}

pub(crate) async fn create_on(
    link: &RuntimeLink,
    sheet: &AgentSheetDraft,
) -> Result<Value, String> {
    let body = json!({
        "name": sheet.name,
        "mission": sheet.mission,
        "examples": sheet.examples,
        "access": sheet.access,
        "sources": sheet.sources,
    });
    let created = link.post_json(DEFINITIONS_PATH, &body).await?;
    let id = created
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    tracing::event!(tracing::Level::INFO, agent = %id, "agent.create.saved");
    Ok(created)
}

/// What the draft of an agent may hold: the accesses, the sources and the
/// models that exist on the machine.
#[tauri::command]
pub async fn agent_draft_offer(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(DRAFT_OFFER_PATH).await
}

/// Has the proposed `sheet` checked against the machine and answers the
/// draft the form opens. Nothing is written.
#[tauri::command]
pub async fn check_agent_draft(
    link: State<'_, RuntimeLink>,
    sheet: Value,
) -> Result<Value, String> {
    link.post_json(DRAFT_CHECK_PATH, &sheet).await
}

/// Creates the agent the validated `sheet` describes, and answers the agent
/// in force.
#[tauri::command]
pub async fn create_agent_definition(
    link: State<'_, RuntimeLink>,
    sheet: AgentSheetDraft,
) -> Result<Value, String> {
    create_on(link.inner(), &sheet).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn sheet() -> AgentSheetDraft {
        AgentSheetDraft {
            name: "Press watch".into(),
            mission: "Summarise the press.".into(),
            examples: vec!["What is new today?".into()],
            access: vec!["web_search".into()],
            sources: vec!["profile".into()],
        }
    }

    #[tokio::test]
    async fn test_a_validated_sheet_is_sent_to_the_creation_route() {
        // GIVEN a daemon that creates the agent of the sheet it is sent
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(DEFINITIONS_PATH))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(json!({
                "name": "Press watch",
                "mission": "Summarise the press.",
                "examples": ["What is new today?"],
                "access": ["web_search"],
                "sources": ["profile"],
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "press-watch", "origin": "interface", "name": "Press watch",
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the form is saved
        let created = create_on(&link, &sheet()).await.unwrap();

        // THEN the answer is the agent the runtime created, of origin interface
        assert_eq!(created["id"], "press-watch");
        assert_eq!(created["origin"], "interface");
    }

    #[tokio::test]
    async fn test_a_refused_sheet_reports_the_reason_of_the_runtime() {
        // GIVEN a daemon that refuses a sheet without a mission
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(DEFINITIONS_PATH))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "error": { "code": "unprocessable", "message": "field 'mission' is empty" },
            })))
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the form is saved
        let refused = create_on(&link, &sheet()).await.unwrap_err();

        // THEN the reason reaches the form
        assert!(refused.contains("mission"), "{refused}");
    }

    #[tokio::test]
    async fn test_a_proposed_sheet_is_sent_to_the_check_route_as_it_is() {
        // GIVEN a daemon that checks the sheet it is sent
        let server = MockServer::start().await;
        let proposed = json!({"name": "Quotes", "mission": "Read the quotes.", "access": ["x"]});
        Mock::given(method("POST"))
            .and(path(DRAFT_CHECK_PATH))
            .and(body_json(proposed.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "Quotes", "access": [], "unavailable": ["x"],
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the sheet is checked
        let draft = link.post_json(DRAFT_CHECK_PATH, &proposed).await.unwrap();

        // THEN the answer is the draft of the runtime
        assert_eq!(draft["unavailable"], json!(["x"]));
    }

    #[test]
    fn test_the_commands_are_served_in_attached_mode() {
        // GIVEN the desktop attached to a daemon
        // WHEN the gate is asked about each command of the creation
        let verdicts = [
            gate(LinkMode::Attached, "agent_draft_offer"),
            gate(LinkMode::Attached, "check_agent_draft"),
            gate(LinkMode::Attached, "create_agent_definition"),
        ];

        // THEN it lets each through
        assert!(verdicts.iter().all(Result::is_ok));
    }
}
