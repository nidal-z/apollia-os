//! Tauri IPC commands behind the sheet of an agent.
//!
//! The sheet is drawn from the agent the runtime resolves (its package
//! definition, then the user's overrides), the last time its model was not
//! available, and the governance history the journal holds for it. Each
//! command reads one runtime route, so the sheet reads the same thing whether
//! the runtime runs in this process or as a daemon:
//!
//! - `get_agent_definition`: `GET /api/v1/agent-definitions/{name}`;
//! - `get_agent_model_fallback`: `GET /api/v1/llm/agents/{agent}/model/fallback`;
//! - `list_agent_changes`: `GET /api/v1/audit/journal/governance:{agent}`.
//!
//! An agent the runtime does not hold, a model that never fell back and an
//! agent with no history answer an empty value rather than an error: each is
//! a state the sheet shows, not a failure.

use serde_json::Value;
use tauri::State;

use crate::attached::{segment, AttachedError, RuntimeLink};

/// Prefix of the journal run holding the governance history of one agent,
/// as `apollia_runtime::governance::governance_run_id` writes it.
const GOVERNANCE_RUN_PREFIX: &str = "governance:";

/// `GET path` on the runtime, `Ok(None)` when the route answers 404.
pub(crate) async fn get_optional(link: &RuntimeLink, path: &str) -> Result<Option<Value>, String> {
    match link {
        RuntimeLink::Embedded(runtime) => {
            match super::http_get_json(runtime.api_port, path).await {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.starts_with("API error (404)") => Ok(None),
                Err(e) => Err(e),
            }
        }
        RuntimeLink::Attached(remote) => match remote.get::<Value>(path).await {
            Ok(value) => Ok(Some(value)),
            Err(AttachedError::Refused { status: 404, .. }) => Ok(None),
            Err(e) => Err(e.into()),
        },
    }
}

/// Route of the resolved agent `name`.
fn definition_path(name: &str) -> String {
    format!("/api/v1/agent-definitions/{}", segment(name))
}

/// Route of the last model fallback of `agent`.
fn fallback_path(agent: &str) -> String {
    format!("/api/v1/llm/agents/{}/model/fallback", segment(agent))
}

/// Route of the governance history of `agent` in the journal.
fn changes_path(agent: &str) -> String {
    let run_id = format!("{GOVERNANCE_RUN_PREFIX}{agent}");
    format!("/api/v1/audit/journal/{}", segment(&run_id))
}

pub(crate) async fn definition_on(link: &RuntimeLink, name: &str) -> Result<Option<Value>, String> {
    get_optional(link, &definition_path(name)).await
}

pub(crate) async fn fallback_on(link: &RuntimeLink, agent: &str) -> Result<Option<Value>, String> {
    get_optional(link, &fallback_path(agent)).await
}

pub(crate) async fn changes_on(link: &RuntimeLink, agent: &str) -> Result<Vec<Value>, String> {
    let page = get_optional(link, &changes_path(agent)).await?;
    Ok(page
        .and_then(|body| body.get("entries").and_then(Value::as_array).cloned())
        .unwrap_or_default())
}

/// The agent `name` as the runtime resolves it, `null` when it holds no
/// definition for that name.
#[tauri::command]
pub async fn get_agent_definition(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<Option<Value>, String> {
    definition_on(link.inner(), &name).await
}

/// The last time the model of `agent` was not available since the runtime
/// started, `null` when it never fell back.
#[tauri::command]
pub async fn get_agent_model_fallback(
    link: State<'_, RuntimeLink>,
    agent: String,
) -> Result<Option<Value>, String> {
    fallback_on(link.inner(), &agent).await
}

/// The journal entries of the governance history of `agent`, oldest first:
/// accesses granted and revoked, overrides written and removed,
/// installations and updates. Empty when nothing was ever recorded.
#[tauri::command]
pub async fn list_agent_changes(
    link: State<'_, RuntimeLink>,
    agent: String,
) -> Result<Vec<Value>, String> {
    changes_on(link.inner(), &agent).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    async fn answer(server: &MockServer, route: &str, status: u16, body: Value) {
        Mock::given(method("GET"))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn test_the_sheet_reads_the_resolved_agent() {
        // GIVEN a daemon holding the agent `invoices` with an overridden mission
        let server = MockServer::start().await;
        answer(
            &server,
            "/api/v1/agent-definitions/invoices",
            200,
            serde_json::json!({
                "id": "invoices", "name": "Invoices", "mission": "Sort the invoices",
                "access": ["file_read"], "sources": ["profile"], "overridden": ["mission"],
            }),
        )
        .await;

        // WHEN the sheet reads the agent
        let agent = definition_on(&attached(&server), "invoices").await.unwrap();

        // THEN the resolved agent comes back as the runtime wrote it
        let agent = agent.expect("the agent is held");
        assert_eq!(agent["mission"], "Sort the invoices");
        assert_eq!(agent["sources"][0], "profile");
    }

    #[tokio::test]
    async fn test_an_agent_without_definition_reads_as_absent() {
        // GIVEN a daemon that holds no definition for `scribe`
        let server = MockServer::start().await;
        answer(
            &server,
            "/api/v1/agent-definitions/scribe",
            404,
            serde_json::json!({ "error": "agent 'scribe' not found" }),
        )
        .await;

        // WHEN the sheet reads the agent
        let agent = definition_on(&attached(&server), "scribe").await.unwrap();

        // THEN it is absent rather than an error
        assert!(agent.is_none());
    }

    #[tokio::test]
    async fn test_the_last_fallback_of_the_model_is_read_and_its_absence_is_none() {
        // GIVEN a daemon where `mail` fell back once and `notes` never did
        let server = MockServer::start().await;
        answer(
            &server,
            "/api/v1/llm/agents/mail/model/fallback",
            200,
            serde_json::json!({
                "agent": "mail", "requested": "qwen3-8b", "used": "default",
                "at": "2026-10-03T09:12:00Z",
            }),
        )
        .await;
        answer(
            &server,
            "/api/v1/llm/agents/notes/model/fallback",
            404,
            serde_json::json!({ "error": "not_found" }),
        )
        .await;
        let link = attached(&server);

        // WHEN the sheet reads the fallback of each agent
        let mail = fallback_on(&link, "mail").await.unwrap();
        let notes = fallback_on(&link, "notes").await.unwrap();

        // THEN the fallback of `mail` names both models and `notes` has none
        let mail = mail.expect("mail fell back");
        assert_eq!(mail["requested"], "qwen3-8b");
        assert_eq!(mail["used"], "default");
        assert!(notes.is_none());
    }

    #[tokio::test]
    async fn test_the_changes_are_the_governance_run_of_the_agent() {
        // GIVEN a daemon whose journal holds one access granted to `invoices`
        let server = MockServer::start().await;
        answer(
            &server,
            "/api/v1/audit/journal/governance%3Ainvoices",
            200,
            serde_json::json!({
                "run_id": "governance:invoices",
                "entries": [{ "kind": "access_changed", "payload": { "access_id": "file_read" } }],
            }),
        )
        .await;

        // WHEN the sheet lists the changes of the agent
        let changes = changes_on(&attached(&server), "invoices").await.unwrap();

        // THEN the one entry of its governance run comes back
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["kind"], "access_changed");
    }

    #[tokio::test]
    async fn test_an_agent_with_no_history_has_no_change() {
        // GIVEN a daemon whose journal holds nothing for `scribe`
        let server = MockServer::start().await;
        answer(
            &server,
            "/api/v1/audit/journal/governance%3Ascribe",
            404,
            serde_json::json!({ "error": "not_found" }),
        )
        .await;

        // WHEN the sheet lists the changes of the agent
        let changes = changes_on(&attached(&server), "scribe").await.unwrap();

        // THEN the list is empty rather than an error
        assert!(changes.is_empty());
    }

    #[test]
    fn test_the_sheet_commands_are_routed_to_a_daemon() {
        // GIVEN the three commands of the sheet
        let commands = [
            "get_agent_definition",
            "get_agent_model_fallback",
            "list_agent_changes",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
