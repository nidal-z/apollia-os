//! What the bridge serves: the exposed items that still exist, each under the
//! tool name an MCP client reads.

use std::collections::BTreeMap;

use axum::http::StatusCode;

use apollia_tools::{ExposedItemRepository, ExposedKind, ResolvedAgent};

use super::{error, RouteError};
use crate::agents::task::resolve_runnable;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// The longest tool name the model APIs accept.
const MAX_TOOL_NAME: usize = 64;

/// The longest description handed to the client.
const MAX_DESCRIPTION: usize = 1024;

/// What a bridge tool runs.
pub(super) enum Target {
    /// A stored agent, resolved when the catalogue was read.
    Agent(Box<ResolvedAgent>),
    /// A workflow of an installed package.
    Workflow,
}

/// One exposed item the bridge serves.
pub(super) struct BridgeItem {
    pub(super) kind: ExposedKind,
    pub(super) item_id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) input_schema: serde_json::Value,
    pub(super) target: Target,
}

/// The tool name of `item_id` of kind `kind`: `agent__` or `workflow__`, then
/// the identifier with every character outside `[A-Za-z0-9_-]` written `_`.
/// `None` when the name would be longer than the model APIs accept.
pub(super) fn tool_name(kind: ExposedKind, item_id: &str) -> Option<String> {
    let normalized: String = item_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = format!("{}__{normalized}", kind.as_str());
    (!item_id.is_empty() && name.len() <= MAX_TOOL_NAME).then_some(name)
}

fn bounded(text: &str) -> String {
    match text.char_indices().nth(MAX_DESCRIPTION) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

fn agent_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "What the agent is asked to do."
            }
        },
        "required": ["message"]
    })
}

/// The exposed items that exist now, named, without the ones whose names
/// collide or run too long.
pub(super) async fn catalogue<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<Vec<BridgeItem>, RouteError> {
    let path = apollia_core::paths::DataFile::Agents.path(&state.data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let exposed = tokio::task::spawn_blocking(move || ExposedItemRepository::open(&path)?.list())
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?;

    let mut items = Vec::new();
    for entry in exposed {
        let Some(name) = tool_name(entry.kind, &entry.item_id) else {
            tracing::event!(
                tracing::Level::WARN,
                kind = entry.kind.as_str(),
                item = %entry.item_id,
                "mcp.bridge.name_too_long"
            );
            continue;
        };
        match entry.kind {
            ExposedKind::Agent => {
                let resolved =
                    resolve_runnable(state.data_dir.clone(), &entry.item_id, "the MCP bridge")
                        .await;
                let Ok(agent) = resolved else {
                    continue;
                };
                let description = if agent.mission.trim().is_empty() {
                    agent.name.clone()
                } else {
                    format!("{}: {}", agent.name, agent.mission.trim())
                };
                items.push(BridgeItem {
                    kind: entry.kind,
                    item_id: entry.item_id,
                    name,
                    description: bounded(&description),
                    input_schema: agent_schema(),
                    target: Target::Agent(Box::new(agent)),
                });
            }
            ExposedKind::Workflow => {
                let Some(engine) = state.workflows.as_ref() else {
                    continue;
                };
                let Some(workflow) = engine.describe(&entry.item_id).await else {
                    continue;
                };
                items.push(BridgeItem {
                    kind: entry.kind,
                    item_id: entry.item_id,
                    name,
                    description: bounded(&workflow.description),
                    input_schema: workflow
                        .input_schema
                        .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
                    target: Target::Workflow,
                });
            }
        }
    }
    Ok(without_collisions(items))
}

/// `items` without every item whose tool name another item also carries.
fn without_collisions(items: Vec<BridgeItem>) -> Vec<BridgeItem> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in &items {
        *counts.entry(item.name.clone()).or_default() += 1;
    }
    items
        .into_iter()
        .filter(|item| {
            let unique = counts.get(&item.name).copied().unwrap_or(0) == 1;
            if !unique {
                tracing::event!(
                    tracing::Level::WARN,
                    kind = item.kind.as_str(),
                    item = %item.item_id,
                    "mcp.bridge.name_collision"
                );
            }
            unique
        })
        .collect()
}

/// Whether `item_id` of kind `kind` exists: a stored agent, or a workflow an
/// installed package declares.
pub(super) async fn exists<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    kind: ExposedKind,
    item_id: &str,
) -> Result<bool, RouteError> {
    match kind {
        ExposedKind::Agent => {
            let path = apollia_core::paths::DataFile::Agents.path(&state.data_dir);
            if !path.exists() {
                return Ok(false);
            }
            let name = item_id.to_string();
            tokio::task::spawn_blocking(move || {
                let repo = apollia_tools::AgentRepository::open(&path)?;
                Ok::<bool, apollia_tools::AgentRepositoryError>(repo.stored(&name)?.is_some())
            })
            .await
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))
        }
        ExposedKind::Workflow => match state.workflows.as_ref() {
            Some(engine) => Ok(engine.describe(item_id).await.is_some()),
            None => Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "the workflow engine is not running in this runtime",
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_names_keep_to_the_charset_the_model_apis_accept() {
        // GIVEN an agent id and a workflow id with a slash and a dot
        // WHEN they are named
        let agent = tool_name(ExposedKind::Agent, "ledger-clerk");
        let workflow = tool_name(ExposedKind::Workflow, "acme/close.month");

        // THEN the names are prefixed and normalized
        assert_eq!(agent.as_deref(), Some("agent__ledger-clerk"));
        assert_eq!(workflow.as_deref(), Some("workflow__acme_close_month"));
    }

    #[test]
    fn test_a_name_over_sixty_four_characters_or_empty_is_refused() {
        // GIVEN an identifier that would make a 65-character name, and an empty one
        let long = "x".repeat(MAX_TOOL_NAME - "agent__".len() + 1);

        // WHEN they are named
        // THEN neither gets a name, while one character shorter fits
        assert!(tool_name(ExposedKind::Agent, &long).is_none());
        assert!(tool_name(ExposedKind::Agent, "").is_none());
        assert!(tool_name(ExposedKind::Agent, &long[1..]).is_some());
    }
}
