//! The sensitive accesses of the agent a batch runs, and their grants.
//!
//! Before a batch starts, the accesses of its agent that governance calls
//! sensitive are listed. Those the user grants are recorded as standing
//! consents scoped to the batch (`lot:<id>` in `family_consents`) and withdrawn
//! when the batch ends. An access is sensitive unless its tool reads only
//! (`read`) or acts on the conversation alone (`exempt`); a tool nobody
//! classified is sensitive, and so is one whose descriptor is unknown, such as
//! an MCP server or one of its tools.

use std::collections::HashMap;

use apollia_tools::{ActionFamily, ResolvedAgent, ToolDescriptor};
use serde::{Deserialize, Serialize};

use super::BatchError;
use crate::governance::{DeclaredAccess, GovernanceContract};

/// One sensitive access of the agent a batch runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchAccess {
    /// The access, as the agent names it: a tool, or `mcp:<server>`.
    pub access_id: String,
    /// What it lets the agent do.
    pub permits: String,
    /// Its action family: `read_personal`, `write_or_delete`, `send_external`,
    /// `execute_code` or `unclassified`.
    pub family: String,
    /// Whether it can be granted for a batch. Executing code asks at every
    /// call, and an unclassified access has no family to grant.
    pub grantable: bool,
}

impl BatchAccess {
    /// Whether the access executes code: no grant covers it, and the user
    /// allows it one call at a time.
    #[must_use]
    pub fn executes_code(&self) -> bool {
        self.family == family_name(ActionFamily::ExecuteCode)
    }
}

/// What a tool descriptor says about one access.
#[derive(Debug, Clone)]
pub(crate) struct KnownTool {
    pub(crate) family: ActionFamily,
    pub(crate) description: String,
}

/// The descriptors known without a running server: the native tools, the
/// connector operations under their bare name, the MCP resource tools. The
/// tool of a connected account (`conn:<provider>:<account>/<operation>`) is
/// read through its operation, see [`lookup`].
pub(crate) fn known_tools() -> HashMap<String, KnownTool> {
    let mut descriptors: Vec<ToolDescriptor> = apollia_tools::tools::native_descriptors();
    descriptors.extend(crate::connectors_bridge::google_tool_descriptors());
    descriptors.extend(crate::connectors_bridge::microsoft_tool_descriptors());
    descriptors.extend(crate::connectors_bridge::imap_tool_descriptors());
    descriptors.extend(apollia_mcp::mcp_resources::mcp_resource_descriptors());
    descriptors
        .into_iter()
        .map(|d| {
            (
                d.name,
                KnownTool {
                    family: d.family,
                    description: d.description,
                },
            )
        })
        .collect()
}

/// What `tools` says about the access `id`. The tool of a connected account
/// takes the family and the description of its operation, the account only
/// choosing which token the call runs with.
fn lookup<'a>(tools: &'a HashMap<String, KnownTool>, id: &str) -> Option<&'a KnownTool> {
    tools.get(id).or_else(|| {
        crate::connectors_bridge::connection_tools::parse_connection_tool(id)
            .and_then(|tool| tools.get(&tool.operation))
    })
}

fn family_name(family: ActionFamily) -> String {
    serde_json::to_value(family)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unclassified".to_owned())
}

fn declared_sensitive(family: ActionFamily) -> bool {
    !matches!(family, ActionFamily::Read | ActionFamily::Exempt)
}

/// The accesses of `agent`, each with whether governance calls it sensitive.
/// `governance` resolves them when there is one; without it, an access is
/// sensitive when its tool declares it so, which is the rule in force when no
/// policy is deposited.
pub(crate) fn classify(
    agent: &ResolvedAgent,
    tools: &HashMap<String, KnownTool>,
    governance: Option<&dyn GovernanceContract>,
) -> Vec<(BatchAccess, bool)> {
    let declared: Vec<DeclaredAccess> = agent
        .access
        .iter()
        .map(|id| {
            let tool = lookup(tools, id);
            DeclaredAccess {
                access_id: id.clone(),
                permits: tool
                    .map(|t| first_line(&t.description))
                    .unwrap_or_else(|| format!("use {id}")),
                declared_sensitive: declared_sensitive(
                    tool.map_or(ActionFamily::Unclassified, |t| t.family),
                ),
            }
        })
        .collect();
    let effective: Vec<(String, String, bool)> = match governance {
        Some(governance) => governance
            .effective_accesses(&agent.id, &declared)
            .into_iter()
            .map(|a| (a.access_id, a.permits, a.sensitive))
            .collect(),
        None => declared
            .into_iter()
            .map(|a| (a.access_id, a.permits, a.declared_sensitive))
            .collect(),
    };
    effective
        .into_iter()
        .map(|(access_id, permits, sensitive)| {
            let family = lookup(tools, &access_id).map_or(ActionFamily::Unclassified, |t| t.family);
            let grantable = family
                .consent_family()
                .is_some_and(|f| f != apollia_permissions::ConsentFamily::ExecuteCode);
            (
                BatchAccess {
                    access_id,
                    permits,
                    family: family_name(family),
                    grantable,
                },
                sensitive,
            )
        })
        .collect()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

/// The sensitive accesses among `classified`.
pub(crate) fn sensitive(classified: &[(BatchAccess, bool)]) -> Vec<BatchAccess> {
    classified
        .iter()
        .filter(|(_, sensitive)| *sensitive)
        .map(|(access, _)| access.clone())
        .collect()
}

/// The sensitive accesses `requested` names, each checked against the list of
/// the agent.
///
/// # Errors
///
/// [`BatchError::Invalid`] for a name that is not a sensitive access of the
/// agent, or one that cannot be granted for a batch.
pub(crate) fn resolve_grants(
    sensitive: &[BatchAccess],
    requested: &[String],
) -> Result<Vec<BatchAccess>, BatchError> {
    let mut out: Vec<BatchAccess> = Vec::new();
    for name in requested {
        let Some(access) = sensitive.iter().find(|a| &a.access_id == name) else {
            let listed: Vec<&str> = sensitive.iter().map(|a| a.access_id.as_str()).collect();
            return Err(BatchError::Invalid(format!(
                "'{name}' is not a sensitive access of this agent; the accesses to grant are: {}",
                if listed.is_empty() {
                    "none".to_owned()
                } else {
                    listed.join(", ")
                }
            )));
        };
        if !access.grantable {
            return Err(BatchError::Invalid(format!(
                "'{name}' cannot be granted for a batch: its family is {}",
                access.family
            )));
        }
        if !out.contains(access) {
            out.push(access.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(access: &[&str]) -> ResolvedAgent {
        let repo_dir = tempfile::tempdir().unwrap();
        let repo =
            apollia_tools::AgentRepository::open(&repo_dir.path().join("agents.db")).unwrap();
        repo.save_definition(&apollia_tools::StoredAgent {
            name: "clerk".into(),
            origin: apollia_tools::AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: serde_json::json!({
                "id": "clerk",
                "name": "Clerk",
                "mission": "Answer.",
                "defaults": {"access": access}
            }),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .unwrap();
        repo.resolved("clerk", None, crate::agents::global_default_autonomy())
            .unwrap()
            .unwrap()
    }

    fn tools() -> HashMap<String, KnownTool> {
        HashMap::from([
            (
                "web_search".to_owned(),
                KnownTool {
                    family: ActionFamily::Read,
                    description: "Search the web.".into(),
                },
            ),
            (
                "send_email".to_owned(),
                KnownTool {
                    family: ActionFamily::SendExternal,
                    description: "Send an email.\nMore detail.".into(),
                },
            ),
            (
                "bash_executor".to_owned(),
                KnownTool {
                    family: ActionFamily::ExecuteCode,
                    description: "Run a shell command.".into(),
                },
            ),
        ])
    }

    #[test]
    fn test_only_the_sensitive_accesses_are_listed_with_what_they_permit() {
        // GIVEN an agent that reads the web, sends mail, runs code and uses an MCP server
        let agent = agent(&["web_search", "send_email", "bash_executor", "mcp:crm"]);
        // WHEN its accesses are classified without a deposited policy
        let classified = classify(&agent, &tools(), None);
        let listed = sensitive(&classified);
        // THEN reading is not listed; sending is grantable; code and the unknown
        //      server are listed but cannot be granted for a batch
        let ids: Vec<(&str, &str, bool)> = listed
            .iter()
            .map(|a| (a.access_id.as_str(), a.family.as_str(), a.grantable))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("send_email", "send_external", true),
                ("bash_executor", "execute_code", false),
                ("mcp:crm", "unclassified", false),
            ]
        );
        assert_eq!(listed[0].permits, "Send an email.");
    }

    #[test]
    fn test_a_grant_must_name_a_grantable_sensitive_access() {
        // GIVEN the sensitive accesses of an agent
        let listed = sensitive(&classify(
            &agent(&["web_search", "send_email", "bash_executor"]),
            &tools(),
            None,
        ));
        // WHEN grants name a listed access, a plain one, and code
        let granted = resolve_grants(&listed, &["send_email".into(), "send_email".into()]);
        let plain = resolve_grants(&listed, &["web_search".into()]);
        let code = resolve_grants(&listed, &["bash_executor".into()]);
        // THEN only the grantable sensitive access is kept, once
        assert_eq!(granted.unwrap().len(), 1);
        assert!(
            matches!(plain, Err(BatchError::Invalid(m)) if m.contains("not a sensitive access"))
        );
        assert!(matches!(code, Err(BatchError::Invalid(m)) if m.contains("execute_code")));
    }

    #[test]
    fn test_the_tool_of_a_connected_account_is_classified_by_its_operation() {
        // GIVEN an agent granted the sending tool of one connected Google account
        let tool = crate::connectors_bridge::connection_tools::account_descriptors(
            apollia_auth::ConnectorProvider::Google,
            "ada@example.com",
        )
        .into_iter()
        .find(|d| d.family == ActionFamily::SendExternal)
        .map(|d| d.name)
        .unwrap();
        let agent = agent(&[tool.as_str()]);
        // WHEN its accesses are classified against the descriptors known at launch
        let listed = sensitive(&classify(&agent, &known_tools(), None));
        // THEN the account's tool takes the family of its operation and can be granted
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].access_id, tool);
        assert_eq!(listed[0].family, "send_external");
        assert!(listed[0].grantable);
    }
}
