//! Tauri IPC commands behind the adjustment of an agent on its sheet.
//!
//! What the user changes on the sheet is written as overrides of the agent
//! and as the model assigned to it, through the runtime's routes, so every
//! write is recorded in the history of the agent whether the runtime runs in
//! this process or as a daemon:
//!
//! - `set_agent_override`: `PUT /api/v1/agent-definitions/{name}/overrides/{field}`;
//! - `remove_agent_override`: `DELETE /api/v1/agent-definitions/{name}/overrides/{field}`;
//! - `reset_agent_overrides`: `DELETE /api/v1/agent-definitions/{name}/overrides`,
//!   every override and the assigned model removed;
//! - `set_agent_model`: `PUT /api/v1/llm/agents/{agent}/model`;
//! - `clear_agent_model`: `DELETE /api/v1/llm/agents/{agent}/model`;
//! - `list_model_presets`: `GET /api/v1/llm/presets`.
//!
//! The override writes and the return to defaults answer the agent in force
//! after the write, which the sheet shows without reading it again.

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Route of the override of `field` for the agent `name`.
fn override_path(name: &str, field: &str) -> String {
    format!(
        "/api/v1/agent-definitions/{}/overrides/{}",
        segment(name),
        segment(field)
    )
}

/// Route of every override of the agent `name`.
fn overrides_path(name: &str) -> String {
    format!("/api/v1/agent-definitions/{}/overrides", segment(name))
}

/// Route of the model assigned to `agent`.
fn model_path(agent: &str) -> String {
    format!("/api/v1/llm/agents/{}/model", segment(agent))
}

/// Route of the presets of every model.
const PRESETS_PATH: &str = "/api/v1/llm/presets";

pub(crate) async fn set_override_on(
    link: &RuntimeLink,
    name: &str,
    field: &str,
    value: Value,
) -> Result<Value, String> {
    let resolved = link
        .put_json(&override_path(name, field), &json!({ "value": value }))
        .await?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        field = %field,
        "agent.adjust.override_set"
    );
    Ok(resolved)
}

pub(crate) async fn remove_override_on(
    link: &RuntimeLink,
    name: &str,
    field: &str,
) -> Result<Value, String> {
    let resolved = link.delete_json(&override_path(name, field)).await?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        field = %field,
        "agent.adjust.override_removed"
    );
    Ok(resolved)
}

pub(crate) async fn reset_on(link: &RuntimeLink, name: &str) -> Result<Value, String> {
    let resolved = link.delete_json(&overrides_path(name)).await?;
    tracing::event!(tracing::Level::INFO, agent = %name, "agent.adjust.reset");
    Ok(resolved)
}

pub(crate) async fn set_model_on(
    link: &RuntimeLink,
    agent: &str,
    model_source: &str,
    model_id: &str,
    preset_id: Option<i64>,
) -> Result<Value, String> {
    let body = json!({
        "model_source": model_source,
        "model_id": model_id,
        "preset_id": preset_id,
    });
    let assigned = link.put_json(&model_path(agent), &body).await?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %agent,
        model = %model_id,
        "agent.adjust.model_set"
    );
    Ok(assigned)
}

pub(crate) async fn clear_model_on(link: &RuntimeLink, agent: &str) -> Result<Value, String> {
    let cleared = link.delete_json(&model_path(agent)).await?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %agent,
        "agent.adjust.model_cleared"
    );
    Ok(cleared)
}

pub(crate) async fn presets_on(link: &RuntimeLink) -> Result<Vec<Value>, String> {
    let body = link.get_json(PRESETS_PATH).await?;
    Ok(body
        .get("presets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Writes the user's `value` of `field` for the agent `name`, and answers the
/// agent in force.
#[tauri::command]
pub async fn set_agent_override(
    link: State<'_, RuntimeLink>,
    name: String,
    field: String,
    value: Value,
) -> Result<Value, String> {
    set_override_on(link.inner(), &name, &field, value).await
}

/// Removes the user's value of `field` for the agent `name`, so the package's
/// applies again, and answers the agent in force.
#[tauri::command]
pub async fn remove_agent_override(
    link: State<'_, RuntimeLink>,
    name: String,
    field: String,
) -> Result<Value, String> {
    remove_override_on(link.inner(), &name, &field).await
}

/// Back to defaults: removes every override of the agent `name` and the model
/// assigned to it, and answers the agent in force.
#[tauri::command]
pub async fn reset_agent_overrides(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<Value, String> {
    reset_on(link.inner(), &name).await
}

/// Assigns a model, and optionally a preset of it, to `agent`.
#[tauri::command]
pub async fn set_agent_model(
    link: State<'_, RuntimeLink>,
    agent: String,
    model_source: String,
    model_id: String,
    preset_id: Option<i64>,
) -> Result<Value, String> {
    set_model_on(link.inner(), &agent, &model_source, &model_id, preset_id).await
}

/// Removes the model assigned to `agent`: it follows the default again.
#[tauri::command]
pub async fn clear_agent_model(
    link: State<'_, RuntimeLink>,
    agent: String,
) -> Result<Value, String> {
    clear_model_on(link.inner(), &agent).await
}

/// The presets of every model, sorted by model then name.
#[tauri::command]
pub async fn list_model_presets(link: State<'_, RuntimeLink>) -> Result<Vec<Value>, String> {
    presets_on(link.inner()).await
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

    fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
    }

    #[tokio::test]
    async fn test_an_override_is_written_through_the_route_of_its_field() {
        // GIVEN a daemon that takes the mission of `invoices`
        let server = MockServer::start().await;
        authorized(
            "PUT",
            "/api/v1/agent-definitions/invoices/overrides/mission",
        )
        .and(body_json(json!({ "value": "Sort the invoices" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "invoices", "mission": "Sort the invoices", "overridden": ["mission"],
        })))
        .expect(1)
        .mount(&server)
        .await;

        // WHEN the sheet saves the new mission
        let agent = set_override_on(
            &attached(&server),
            "invoices",
            "mission",
            json!("Sort the invoices"),
        )
        .await
        .unwrap();

        // THEN the agent in force carries it, marked as the user's
        assert_eq!(agent["mission"], "Sort the invoices");
        assert_eq!(agent["overridden"][0], "mission");
    }

    #[tokio::test]
    async fn test_an_override_removed_and_the_return_to_defaults_use_their_routes() {
        // GIVEN a daemon that removes one override, then all of them
        let server = MockServer::start().await;
        authorized(
            "DELETE",
            "/api/v1/agent-definitions/apollia%3Achat/overrides/sources",
        )
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "overridden": ["mission"] })),
        )
        .expect(1)
        .mount(&server)
        .await;
        authorized(
            "DELETE",
            "/api/v1/agent-definitions/apollia%3Achat/overrides",
        )
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "overridden": [], "model": null })),
        )
        .expect(1)
        .mount(&server)
        .await;
        let link = attached(&server);

        // WHEN the sources go back to the package's, then the whole agent
        let after_field = remove_override_on(&link, "apollia:chat", "sources")
            .await
            .unwrap();
        let after_reset = reset_on(&link, "apollia:chat").await.unwrap();

        // THEN each answer is the agent in force
        assert_eq!(after_field["overridden"], json!(["mission"]));
        assert_eq!(after_reset["overridden"], json!([]));
        assert!(after_reset["model"].is_null());
    }

    #[tokio::test]
    async fn test_the_model_and_its_preset_are_assigned_then_cleared() {
        // GIVEN a daemon that assigns and clears the model of `invoices`
        let server = MockServer::start().await;
        authorized("PUT", "/api/v1/llm/agents/invoices/model")
            .and(body_json(json!({
                "model_source": "local", "model_id": "qwen3-8b", "preset_id": 4,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "agent": "invoices", "model_source": "local", "model_id": "qwen3-8b", "preset_id": 4,
            })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("DELETE", "/api/v1/llm/agents/invoices/model")
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "agent": "invoices", "cleared": true,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the sheet assigns a model with a preset, then removes it
        let assigned = set_model_on(&link, "invoices", "local", "qwen3-8b", Some(4))
            .await
            .unwrap();
        let cleared = clear_model_on(&link, "invoices").await.unwrap();

        // THEN both the assignment and its removal are answered
        assert_eq!(assigned["preset_id"], 4);
        assert_eq!(cleared["cleared"], true);
    }

    #[tokio::test]
    async fn test_the_presets_are_read_as_a_list() {
        // GIVEN a daemon holding one preset
        let server = MockServer::start().await;
        authorized("GET", "/api/v1/llm/presets")
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "presets": [{ "id": 4, "name": "Precise", "model_source": "local", "model_id": "qwen3-8b" }],
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the sheet lists the presets
        let presets = presets_on(&attached(&server)).await.unwrap();

        // THEN the one preset comes back
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0]["name"], "Precise");
    }

    #[test]
    fn test_the_adjust_commands_are_routed_to_a_daemon() {
        // GIVEN the commands of the adjustment
        let commands = [
            "set_agent_override",
            "remove_agent_override",
            "reset_agent_overrides",
            "set_agent_model",
            "clear_agent_model",
            "list_model_presets",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
