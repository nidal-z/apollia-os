use super::*;
use apollia_core::SandboxProfile;
use apollia_tools::descriptor::{ToolDescriptor, ToolKind};

struct NoInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoInvoker {
    async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
        Err("not called".into())
    }
}

fn descriptor(name: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("tool {name}"),
        kind: ToolKind::Native,
        input_schema: serde_json::json!({ "type": "object" }),
        output_schema: None,
        sandbox_profile: SandboxProfile::FileSystem,
        tags: vec![],
        family: apollia_tools::ActionFamily::Unclassified,
        family_source: apollia_tools::FamilySource::Unsourced,
        is_read_only: true,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

fn indexed(server: &str, tool: &str) -> ToolIndexSnapshot {
    ToolIndexSnapshot {
        server_name: server.to_string(),
        tool_name: tool.to_string(),
        description: None,
        tags: vec![],
        input_schema: None,
    }
}

async fn registry_of(names: &[&str]) -> ToolRegistryHandle {
    let registry = ToolRegistryHandle::start();
    for name in names {
        registry.register(descriptor(name)).await.unwrap();
    }
    registry
}

fn agent(registry: ToolRegistryHandle) -> BuiltInChatAgent {
    let (event_bus, _rx) = tokio::sync::broadcast::channel(8);
    BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: Arc::new(LlmRouter::empty()),
        tool_registry: registry,
        tool_invoker: Arc::new(NoInvoker),
        event_bus,
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    })
}

fn names(catalog: &Catalog) -> Vec<&str> {
    let mut names: Vec<&str> = catalog.specs.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    names
}

// GIVEN a registry holding natives, an MCP tool and a package tool, and an
// agent granted one of each provenance
// WHEN the catalog of a turn is built
// THEN the model sees the three granted tools and nothing else
#[tokio::test]
async fn test_catalog_offers_the_granted_tools_only() {
    let registry = registry_of(&[
        "file_read",
        "file_write",
        "mcp:notion/search",
        "mcp:notion/create_page",
        "pkg:crm/lookup",
        "pkg:crm/delete",
    ])
    .await;
    let granted = vec![
        "file_read".to_string(),
        "mcp:notion/search".to_string(),
        "pkg:crm/lookup".to_string(),
    ];

    let catalog = agent(registry).catalog(&granted).await;

    assert_eq!(
        names(&catalog),
        vec!["file_read", "mcp:notion/search", "pkg:crm/lookup"]
    );
    assert!(!catalog.callable.contains("file_write"));
    assert!(!catalog.callable.contains("mcp:notion/create_page"));
    assert!(!catalog.callable.contains("pkg:crm/delete"));
}

// GIVEN an agent granted nothing, in deferred MCP mode over an index the
// manager restricted to its access, and a conversation with a task list
// WHEN the catalog of a turn is built
// THEN only the tools of the loop are offered: tool_search and todo_write
#[tokio::test]
async fn test_catalog_adds_only_the_tools_of_the_loop() {
    let registry = registry_of(&["file_read"]).await;
    let todo = crate::chat::todo_actor::spawn_todo_actor(
        rusqlite::Connection::open_in_memory().unwrap(),
        None,
    )
    .unwrap();
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: Arc::new(LlmRouter::empty()),
        tool_registry: registry,
        tool_invoker: Arc::new(NoInvoker),
        event_bus: tokio::sync::broadcast::channel(8).0,
        user_memory: None,
        a2a_invoker: None,
        todo: Some(todo),
        plan: None,
    })
    .with_mcp_index(Some(Vec::new()), 20);

    let catalog = agent.catalog(&[]).await;

    assert_eq!(names(&catalog), vec!["todo_write", "tool_search"]);
}

// GIVEN an agent granted one MCP tool, in deferred mode over an index the
// manager restricted to that grant
// WHEN the catalog of a turn is built
// THEN the granted MCP tool is offered and callable, and no other
#[tokio::test]
async fn test_catalog_offers_a_granted_deferred_mcp_tool() {
    let registry = registry_of(&[]).await;
    let granted = vec!["mcp:notion/search".to_string()];
    let agent = agent(registry).with_mcp_index(Some(vec![indexed("notion", "search")]), 20);

    let catalog = agent.catalog(&granted).await;

    assert_eq!(names(&catalog), vec!["mcp:notion/search", "tool_search"]);
    assert!(catalog.callable.contains("mcp:notion/search"));
    assert_eq!(catalog.callable.len(), 2);
}
