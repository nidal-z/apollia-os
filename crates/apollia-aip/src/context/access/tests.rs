//! The access list is the one check before the registry, for every provenance.

use std::sync::Arc;

use apollia_core::SandboxProfile;
use apollia_tools::{AuditTrailHandle, ToolDescriptor, ToolKind, ToolRegistryHandle};

use crate::context::tool_proxy::ToolProxyError;
use crate::context::{ToolExecutor, ToolProxy, ToolProxyConfig};

struct Echo;

impl ToolExecutor for Echo {
    fn execute(
        &self,
        tool_name: &str,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({ "ran": tool_name }))
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
        target_arg: None,
        is_read_only: true,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

type Fixture = (ToolProxy, ToolRegistryHandle, AuditTrailHandle);

/// A proxy granted `granted`, over a registry holding a native tool, an MCP
/// tool, a package tool and the tool of a connected account, each with a
/// sibling that is not granted.
async fn proxy(granted: &[&str]) -> Result<Fixture, Box<dyn std::error::Error>> {
    let registry = ToolRegistryHandle::start();
    for name in [
        "file_read",
        "file_write",
        "mcp:notion/search",
        "mcp:notion/create_page",
        "pkg:crm/lookup",
        "pkg:crm/delete",
        "conn:google:pro@example.com/gmail.send",
        "conn:google:home@example.com/gmail.send",
    ] {
        registry.register(descriptor(name)).await?;
    }
    let audit = AuditTrailHandle::open(&tempfile::tempdir()?.keep().join("audit.db")).await?;
    let proxy = ToolProxy::new(ToolProxyConfig {
        registry: registry.clone(),
        audit: audit.clone(),
        executor: Arc::new(Echo),
        allowed_tools: granted.iter().map(|s| s.to_string()).collect(),
        agent_id: "notes".to_string(),
        task_id: "task-access".to_string(),
        run_id: None,
    });
    Ok((proxy, registry, audit))
}

const GRANTED: [&str; 4] = [
    "file_read",
    "mcp:notion/search",
    "pkg:crm/lookup",
    "conn:google:pro@example.com/gmail.send",
];

// GIVEN an agent granted a native tool, an MCP tool, a package tool and the
// tool of one connected account
// WHEN it calls each of them
// THEN each one runs
#[tokio::test]
async fn test_granted_tool_of_each_provenance_runs() {
    let (proxy, registry, audit) = proxy(&GRANTED).await.unwrap();

    let mut ran = Vec::new();
    for name in GRANTED {
        ran.push(proxy.call_inner(name, serde_json::json!({})).await);
    }

    for (name, result) in GRANTED.iter().zip(ran) {
        assert_eq!(result.unwrap(), serde_json::json!({ "ran": name }));
    }
    registry.shutdown().await;
    audit.shutdown().await;
}

// GIVEN the same agent, and a registered sibling of each provenance it is
// not granted, the same tool of another account among them
// WHEN it calls each sibling
// THEN each call is refused before the registry and the refusal is audited
#[tokio::test]
async fn test_tool_not_granted_is_refused_and_audited() {
    let (proxy, registry, audit) = proxy(&GRANTED).await.unwrap();
    let siblings = [
        "file_write",
        "mcp:notion/create_page",
        "pkg:crm/delete",
        "conn:google:home@example.com/gmail.send",
    ];

    let mut refused = Vec::new();
    for name in siblings {
        refused.push(proxy.call_inner(name, serde_json::json!({})).await);
    }
    let records = audit.query_last(siblings.len()).await;

    for (name, result) in siblings.iter().zip(refused) {
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotAllowed(ref n)) if n == name),
            "{name}"
        );
        let record = records
            .iter()
            .find(|r| r.tool_name == *name)
            .unwrap_or_else(|| panic!("no audit record for {name}"));
        assert!(!record.success);
        assert_eq!(record.agent_id, "notes");
        assert_eq!(record.sandbox_profile, "unknown");
        assert!(record
            .error_code
            .as_deref()
            .is_some_and(|code| code.contains(name)));
    }
    registry.shutdown().await;
    audit.shutdown().await;
}

// GIVEN a run whose governance failed closed, granted two deferred MCP tools
// no descriptor describes, the MCP tool index classifying one read and the
// other writing
// WHEN it calls each of them
// THEN the read runs, on the family of its index entry, and the write is
// refused as sensitive
#[tokio::test]
async fn test_a_deferred_mcp_tool_is_decided_on_its_index_family_when_governance_fails_closed() {
    let registry = ToolRegistryHandle::start();
    let audit = AuditTrailHandle::open(&tempfile::tempdir().unwrap().keep().join("audit.db"))
        .await
        .unwrap();
    let entry = |tool: &str, family: apollia_tools::ActionFamily| {
        apollia_mcp::tool_search::ToolIndexSnapshot {
            server_name: "crm".to_string(),
            tool_name: tool.to_string(),
            family,
            ..Default::default()
        }
    };
    let mut proxy = ToolProxy::new(ToolProxyConfig {
        registry: registry.clone(),
        audit: audit.clone(),
        executor: Arc::new(Echo),
        allowed_tools: vec!["mcp:crm/lookup".to_string(), "mcp:crm/purge".to_string()],
        agent_id: "notes".to_string(),
        task_id: "task-deferred".to_string(),
        run_id: None,
    })
    .with_mcp_index(vec![
        entry("lookup", apollia_tools::ActionFamily::Read),
        entry("purge", apollia_tools::ActionFamily::WriteOrDelete),
    ]);
    proxy.fail_closed = Some("the governance database cannot be read: disk".to_string());

    let read = proxy
        .call_inner("mcp:crm/lookup", serde_json::json!({}))
        .await;
    let write = proxy
        .call_inner("mcp:crm/purge", serde_json::json!({}))
        .await;

    assert_eq!(
        read.unwrap(),
        serde_json::json!({ "ran": "mcp:crm/lookup" })
    );
    assert!(
        matches!(write, Err(ToolProxyError::FailClosed { .. })),
        "{write:?}"
    );
    registry.shutdown().await;
    audit.shutdown().await;
}
