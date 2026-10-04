//! Governance for the loop tests that need a tool to run without asking.
//!
//! Governance decides every call, so a test that wants a tool to run says
//! why it may: a code executor runs a single simple command under an allow
//! rule in developer mode, any other tool is registered as a writing tool the
//! run grants.

use apollia_permissions::{PermissionScope, PrefixRule, RuleAction};
use apollia_tools::{ActionFamily, ToolRegistryHandle};

use crate::governance::{GovernanceHandle, GovernanceState};

/// Governance in developer mode, with an allow rule on an empty prefix for
/// each code executor: every single simple command they are given runs.
pub(super) fn code_runs() -> GovernanceHandle {
    let rules = apollia_permissions::CODE_EXECUTOR_TOOLS
        .iter()
        .enumerate()
        .map(|(i, tool)| PrefixRule {
            id: i as i64 + 1,
            tool_name: (*tool).to_owned(),
            arg_prefix: Some(String::new()),
            action: RuleAction::Allow,
            scope: PermissionScope::Global,
            ..PrefixRule::default()
        })
        .collect();
    GovernanceHandle::detached(
        GovernanceState {
            rules,
            developer_mode: true,
            ..GovernanceState::default()
        },
        None,
    )
}

/// Registers `name` as a writing tool with an open schema, so that a run
/// granting it lets it run.
pub(super) async fn register_writing(registry: &ToolRegistryHandle, name: &str) {
    let mut descriptor = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    descriptor.name = name.to_owned();
    descriptor.family = ActionFamily::WriteOrDelete;
    descriptor.is_read_only = false;
    descriptor.input_schema = serde_json::json!({ "type": "object" });
    registry.register(descriptor).await.expect("register tool");
}
