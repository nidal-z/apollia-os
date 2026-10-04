//! Tauri IPC commands behind the workflows of the Agents screen: the list,
//! one workflow with the fields of its inputs, its runs, one run with its
//! steps, a launch and a stop.
//!
//! - [`list_workflows`]: the installed workflows, read from
//!   `GET /api/v1/a2a/skills`
//! - [`get_workflow`]: `GET /api/v1/workflows/{package}/{workflow}`
//! - [`list_workflow_runs`]: `GET /api/v1/workflows/{package}/{workflow}/runs`
//! - [`launch_workflow`]: `POST /api/v1/workflows/{package}/{workflow}/runs`
//! - [`get_workflow_run`]: `GET /api/v1/workflow-runs/{id}`
//! - [`cancel_workflow_run`]: `POST /api/v1/workflow-runs/{id}/cancel`
//!
//! Every command goes through the runtime's HTTP API, so each serves the
//! embedded runtime and the daemon alike. The answers are the routes' own
//! JSON, typed on the interface side.

use apollia_runtime::a2a::SkillListing;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// `kind` of a workflow in the skill listing.
const WORKFLOW_KIND: &str = "workflow";

/// Runs a history asks for when the caller names no limit.
const DEFAULT_HISTORY: u32 = 50;

/// One installed workflow, as the list of the Agents screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListItem {
    /// `<package>/<workflow>`.
    pub workflow_id: String,
    /// The name shown to the user.
    pub name: String,
    /// What the workflow does.
    pub description: String,
}

#[derive(Deserialize)]
struct SkillList {
    skills: Vec<SkillListing>,
}

/// The workflows of `listings`, in their order.
fn workflows_of(listings: Vec<SkillListing>) -> Vec<WorkflowListItem> {
    listings
        .into_iter()
        .filter(|listing| listing.kind == WORKFLOW_KIND)
        .map(|listing| WorkflowListItem {
            workflow_id: listing.agent_name,
            name: listing.skill_name,
            description: listing.description,
        })
        .collect()
}

/// The route of the workflow `workflow_id`, `<package>/<workflow>`, each part
/// encoded as one path segment.
fn workflow_path(workflow_id: &str) -> Result<String, String> {
    match workflow_id.split_once('/') {
        Some((package, workflow)) if !package.is_empty() && !workflow.is_empty() => Ok(format!(
            "/api/v1/workflows/{}/{}",
            segment(package),
            segment(workflow)
        )),
        _ => Err(format!(
            "'{workflow_id}' is not a workflow identifier: expected <package>/<workflow>"
        )),
    }
}

/// The route of the run `run_id`.
fn run_path(run_id: &str) -> String {
    format!("/api/v1/workflow-runs/{}", segment(run_id))
}

/// Every installed workflow.
#[tauri::command]
pub async fn list_workflows(link: State<'_, RuntimeLink>) -> Result<Vec<WorkflowListItem>, String> {
    let skills: SkillList = serde_json::from_value(link.get_json("/api/v1/a2a/skills").await?)
        .map_err(|e| format!("failed to parse the workflow list: {e}"))?;
    Ok(workflows_of(skills.skills))
}

/// The workflow `workflow_id`: what it does, its steps and the fields of its
/// inputs.
#[tauri::command]
pub async fn get_workflow(
    link: State<'_, RuntimeLink>,
    workflow_id: String,
) -> Result<Value, String> {
    link.get_json(&workflow_path(&workflow_id)?).await
}

/// The runs of the workflow `workflow_id`, newest first.
#[tauri::command]
pub async fn list_workflow_runs(
    link: State<'_, RuntimeLink>,
    workflow_id: String,
    limit: Option<u32>,
) -> Result<Value, String> {
    let path = format!(
        "{}/runs?limit={}",
        workflow_path(&workflow_id)?,
        limit.unwrap_or(DEFAULT_HISTORY)
    );
    let mut answer = link.get_json(&path).await?;
    Ok(answer
        .get_mut("runs")
        .map(Value::take)
        .unwrap_or_else(|| json!([])))
}

/// Launches the workflow `workflow_id` on `inputs` and returns the run. An
/// input the workflow refuses comes back as the runtime's message, which
/// names the input and says why.
#[tauri::command]
pub async fn launch_workflow(
    link: State<'_, RuntimeLink>,
    workflow_id: String,
    inputs: Value,
) -> Result<Value, String> {
    let path = format!("{}/runs", workflow_path(&workflow_id)?);
    link.post_json(&path, &json!({ "inputs": inputs })).await
}

/// The run `run_id` with its steps.
#[tauri::command]
pub async fn get_workflow_run(
    link: State<'_, RuntimeLink>,
    run_id: String,
) -> Result<Value, String> {
    link.get_json(&run_path(&run_id)).await
}

/// Stops the run `run_id` and returns it once it has ended.
#[tauri::command]
pub async fn cancel_workflow_run(
    link: State<'_, RuntimeLink>,
    run_id: String,
) -> Result<Value, String> {
    link.post_json(&format!("{}/cancel", run_path(&run_id)), &json!({}))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(id: &str, kind: &str) -> SkillListing {
        serde_json::from_value(json!({
            "skill_id": format!("{kind}:{id}"),
            "agent_name": id,
            "skill_name": format!("name of {id}"),
            "description": format!("about {id}"),
            "kind": kind,
        }))
        .unwrap()
    }

    #[test]
    fn test_only_the_workflows_are_listed_in_their_order() {
        // GIVEN a skill of agent code, an agent and two workflows
        let listings = vec![
            listing("read-excel", "skill"),
            listing("acme/close", WORKFLOW_KIND),
            listing("triage", "agent"),
            listing("acme/audit", WORKFLOW_KIND),
        ];

        // WHEN the workflows are taken from the listing
        let workflows = workflows_of(listings);

        // THEN the two workflows remain, in the order of the listing
        assert_eq!(
            workflows,
            vec![
                WorkflowListItem {
                    workflow_id: "acme/close".to_string(),
                    name: "name of acme/close".to_string(),
                    description: "about acme/close".to_string(),
                },
                WorkflowListItem {
                    workflow_id: "acme/audit".to_string(),
                    name: "name of acme/audit".to_string(),
                    description: "about acme/audit".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_a_workflow_identifier_becomes_its_route_with_each_part_encoded() {
        // GIVEN an identifier whose parts hold characters a path cannot carry
        let id = "ventes pme/relance?v=2";

        // WHEN its route is built
        let path = workflow_path(id);

        // THEN each part is one encoded segment
        assert_eq!(
            path.as_deref(),
            Ok("/api/v1/workflows/ventes%20pme/relance%3Fv%3D2")
        );
    }

    #[test]
    fn test_an_identifier_without_a_package_or_a_name_is_refused() {
        // GIVEN identifiers that name no package, no workflow, or neither
        let malformed = ["relance", "/relance", "ventes/", ""];

        for id in malformed {
            // WHEN the route is built
            let path = workflow_path(id);

            // THEN it is refused, and the message names the identifier
            let error = path.unwrap_err();
            assert!(error.contains(&format!("'{id}'")), "{error}");
        }
    }

    #[test]
    fn test_a_run_identifier_is_encoded_as_one_segment() {
        // GIVEN a run identifier holding a slash
        let id = "run/7";

        // WHEN its route is built
        let path = run_path(id);

        // THEN the slash does not open a second segment
        assert_eq!(path, "/api/v1/workflow-runs/run%2F7");
    }
}
