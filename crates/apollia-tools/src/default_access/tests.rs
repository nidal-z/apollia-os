use super::*;
use crate::agent_definition::{resolve, AgentOverride, StoredAgent};
use crate::tools::{file_read, file_write, notebook_read};
use apollia_core::AutonomyLevel;
use serde_json::json;

fn stored(origin: AgentOrigin, definition: serde_json::Value) -> StoredAgent {
    StoredAgent {
        name: "helper".into(),
        origin,
        package: None,
        version: "1".into(),
        definition,
        enabled: true,
        installed_at: "2026-10-02T00:00:00Z".into(),
        updated_at: "2026-10-02T00:00:00Z".into(),
    }
}

fn resolved(stored: &StoredAgent, overrides: &[AgentOverride]) -> ResolvedAgent {
    resolve(stored, overrides, None, AutonomyLevel::Assisted)
}

/// A registry holding `descriptors` and nothing else.
async fn registry_with(descriptors: Vec<crate::descriptor::ToolDescriptor>) -> ToolRegistryHandle {
    let registry = ToolRegistryHandle::start();
    for descriptor in descriptors {
        registry.register(descriptor).await.unwrap();
    }
    registry
}

fn mcp_tool(name: &str, family: ActionFamily) -> crate::descriptor::ToolDescriptor {
    let mut descriptor = file_read::FileRead::descriptor();
    descriptor.name = name.into();
    descriptor.family = family;
    descriptor
}

// GIVEN an agent created in the interface whose definition names no access
// WHEN its access is read
// THEN it is the reading family
#[test]
fn test_an_agent_created_without_code_reads_by_default() {
    let agent = resolved(
        &stored(AgentOrigin::Interface, json!({"name": "helper"})),
        &[],
    );

    let access = AgentAccess::of(&agent);

    assert_eq!(access, AgentAccess::ReadingFamily);
}

// GIVEN an agent created in the interface whose access the user chose
// WHEN its access is read
// THEN it is the user's list, the default replaced whole
#[test]
fn test_an_access_override_replaces_the_default() {
    let agent = resolved(
        &stored(AgentOrigin::Interface, json!({"name": "helper"})),
        &[AgentOverride {
            field: OverrideField::Access,
            value: json!(["file_write"]),
        }],
    );

    let access = AgentAccess::of(&agent);

    assert_eq!(access, AgentAccess::Listed(vec!["file_write".into()]));
}

// GIVEN a package agent that declares no access, and one that declares two
// WHEN their access is read
// THEN each gets what its package declares, nothing by default
#[test]
fn test_a_package_agent_gets_what_its_package_declares() {
    let silent = resolved(&stored(AgentOrigin::Package, json!({"defaults": {}})), &[]);
    let declaring = resolved(
        &stored(
            AgentOrigin::Package,
            json!({"defaults": {"access": ["file_read", "mcp:notion/search"]}}),
        ),
        &[],
    );

    let silent_access = AgentAccess::of(&silent);
    let declared_access = AgentAccess::of(&declaring);

    assert_eq!(silent_access, AgentAccess::Listed(vec![]));
    assert_eq!(
        declared_access,
        AgentAccess::Listed(vec!["file_read".into(), "mcp:notion/search".into()])
    );
}

// GIVEN an agent created in the interface whose definition lists its access
// WHEN its access is read
// THEN the listed access holds: the default applies only when nobody chose
#[test]
fn test_a_listed_definition_is_not_replaced_by_the_default() {
    let agent = resolved(
        &stored(
            AgentOrigin::Interface,
            json!({"defaults": {"access": ["web_search"]}}),
        ),
        &[],
    );

    let access = AgentAccess::of(&agent);

    assert_eq!(access, AgentAccess::Listed(vec!["web_search".into()]));
}

// GIVEN a registry with reading tools (native and MCP), a writing tool, a
//   personal read and an MCP tool classified as a write
// WHEN the reading family is read
// THEN only the reading tools come out, by name
#[cfg(all(feature = "web-search", feature = "web-read"))]
#[tokio::test]
async fn test_the_reading_family_is_the_active_read_tools_only() {
    use crate::tools::{web_read, web_search};
    let registry = registry_with(vec![
        web_search::WebSearch::descriptor(),
        web_read::WebRead::descriptor(),
        file_read::FileRead::descriptor(),
        notebook_read::NotebookRead::descriptor(),
        file_write::FileWrite::descriptor(),
        mcp_tool("mcp:notion/search", ActionFamily::Read),
        mcp_tool("mcp:notion/create_page", ActionFamily::WriteOrDelete),
        mcp_tool("mcp:gmail/list", ActionFamily::ReadPersonal),
    ])
    .await;

    let family = reading_family(&registry, &[]).await;

    assert_eq!(
        family,
        vec![
            "file_read",
            "mcp:notion/search",
            "notebook_read",
            "web_read",
            "web_search"
        ]
    );
    registry.shutdown().await;
}

// GIVEN a registry where web_search and web_read are absent (not compiled)
// WHEN the reading family is read, file_read being disabled
// THEN neither inactive web tool nor the disabled tool is granted
#[tokio::test]
async fn test_an_inactive_tool_is_not_in_the_reading_family() {
    let registry = registry_with(vec![
        file_read::FileRead::descriptor(),
        notebook_read::NotebookRead::descriptor(),
    ])
    .await;

    let family = reading_family(&registry, &["file_read".to_string()]).await;

    assert_eq!(family, vec!["notebook_read"]);
    registry.shutdown().await;
}

// GIVEN an agent created without code and a registry with notebook_read active
// WHEN its default access is written in for its card
// THEN the card shows the reading family, the access still not the user's
#[tokio::test]
async fn test_the_card_shows_the_default_access() {
    let registry = registry_with(vec![
        notebook_read::NotebookRead::descriptor(),
        file_write::FileWrite::descriptor(),
    ])
    .await;
    let agent = resolved(
        &stored(AgentOrigin::Interface, json!({"name": "helper"})),
        &[],
    );

    let shown = with_default_access(agent, &registry).await;

    assert_eq!(shown.access, vec!["notebook_read"]);
    assert!(shown.overridden.is_empty());
    registry.shutdown().await;
}
