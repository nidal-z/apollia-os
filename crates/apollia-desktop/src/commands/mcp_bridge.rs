//! Tauri IPC commands for Apollia as an MCP server: which agents and
//! workflows an MCP client may call.
//!
//! - [`list_mcp_exposable`]: every agent that takes a message and every
//!   workflow, read from `GET /api/v1/a2a/skills`, each with its flag from
//!   `GET /api/v1/mcp/exposed`
//! - [`set_mcp_exposed`]: `PUT /api/v1/mcp/exposed`
//!
//! Both go through the runtime's HTTP API, so they serve the embedded runtime
//! and the daemon alike. The switch that turns the bridge on is the named
//! setting `mcp.server_enabled`, read and written by the settings commands.

use std::collections::BTreeSet;

use apollia_runtime::a2a::SkillListing;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::RuntimeLink;

/// `kind` of an agent that takes a message, in both routes.
const AGENT_KIND: &str = "agent";
/// `kind` of a workflow, in both routes.
const WORKFLOW_KIND: &str = "workflow";

/// One agent or workflow an MCP client may be given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpExposableItem {
    /// `agent` or `workflow`.
    pub kind: String,
    /// The agent id, or `<package>/<workflow>`.
    pub item_id: String,
    /// The name shown to the user.
    pub name: String,
    /// The agent's mission, or the workflow's description.
    pub description: String,
    /// Whether the bridge serves it.
    pub exposed: bool,
}

#[derive(Deserialize)]
struct SkillList {
    skills: Vec<SkillListing>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExposedItem {
    kind: String,
    item_id: String,
}

#[derive(Deserialize)]
struct ExposedList {
    items: Vec<ExposedItem>,
}

/// The agents and workflows of `listings`, in their order, each flagged when
/// `exposed` holds it. Skills of agent code are left out: the bridge does not
/// serve them.
fn exposable(listings: Vec<SkillListing>, exposed: &[ExposedItem]) -> Vec<McpExposableItem> {
    let flagged: BTreeSet<(&str, &str)> = exposed
        .iter()
        .map(|item| (item.kind.as_str(), item.item_id.as_str()))
        .collect();
    listings
        .into_iter()
        .filter(|listing| listing.kind == AGENT_KIND || listing.kind == WORKFLOW_KIND)
        .map(|listing| {
            let exposed = flagged.contains(&(listing.kind.as_str(), listing.agent_name.as_str()));
            McpExposableItem {
                kind: listing.kind,
                item_id: listing.agent_name,
                name: listing.skill_name,
                description: listing.description,
                exposed,
            }
        })
        .collect()
}

/// Every agent and workflow an MCP client may be given, with its flag.
#[tauri::command]
pub async fn list_mcp_exposable(
    state: State<'_, RuntimeLink>,
) -> Result<Vec<McpExposableItem>, String> {
    let skills: SkillList = serde_json::from_value(state.get_json("/api/v1/a2a/skills").await?)
        .map_err(|e| format!("failed to parse the agent list: {e}"))?;
    let exposed: ExposedList = serde_json::from_value(state.get_json("/api/v1/mcp/exposed").await?)
        .map_err(|e| format!("failed to parse the exposed items: {e}"))?;
    Ok(exposable(skills.skills, &exposed.items))
}

/// Marks the agent or workflow `item_id` of kind `kind` exposed, or not.
#[tauri::command]
pub async fn set_mcp_exposed(
    state: State<'_, RuntimeLink>,
    kind: String,
    item_id: String,
    exposed: bool,
) -> Result<(), String> {
    let body = serde_json::json!({ "kind": kind, "itemId": item_id, "exposed": exposed });
    state.put_json("/api/v1/mcp/exposed", &body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(id: &str, kind: &str) -> SkillListing {
        serde_json::from_value(serde_json::json!({
            "skill_id": format!("{kind}:{id}"),
            "agent_name": id,
            "skill_name": format!("name of {id}"),
            "description": format!("about {id}"),
            "kind": kind,
        }))
        .unwrap()
    }

    #[test]
    fn test_agents_and_workflows_are_listed_with_their_flag_and_skills_are_not() {
        // GIVEN a skill of agent code, two agents and a workflow, one agent and
        // the workflow exposed
        let listings = vec![
            listing("worker.summarize", "skill"),
            listing("ledger-clerk", AGENT_KIND),
            listing("scout", AGENT_KIND),
            listing("acme/close", WORKFLOW_KIND),
        ];
        let exposed = vec![
            ExposedItem {
                kind: AGENT_KIND.to_string(),
                item_id: "ledger-clerk".to_string(),
            },
            ExposedItem {
                kind: WORKFLOW_KIND.to_string(),
                item_id: "acme/close".to_string(),
            },
        ];

        // WHEN the exposable items are read
        let items = exposable(listings, &exposed);

        // THEN the skill is gone and each item carries its own flag
        let flags: Vec<(&str, &str, bool)> = items
            .iter()
            .map(|i| (i.kind.as_str(), i.item_id.as_str(), i.exposed))
            .collect();
        assert_eq!(
            flags,
            vec![
                (AGENT_KIND, "ledger-clerk", true),
                (AGENT_KIND, "scout", false),
                (WORKFLOW_KIND, "acme/close", true),
            ]
        );
        assert_eq!(items[0].name, "name of ledger-clerk");
        assert_eq!(items[0].description, "about ledger-clerk");
    }

    #[test]
    fn test_a_flag_of_one_kind_does_not_mark_the_other_kind() {
        // GIVEN an agent and a workflow under the same identifier, only the
        // workflow exposed
        let listings = vec![listing("same", AGENT_KIND), listing("same", WORKFLOW_KIND)];
        let exposed = vec![ExposedItem {
            kind: WORKFLOW_KIND.to_string(),
            item_id: "same".to_string(),
        }];

        // WHEN the exposable items are read
        let items = exposable(listings, &exposed);

        // THEN the agent stays unexposed
        assert!(!items[0].exposed);
        assert!(items[1].exposed);
    }
}
