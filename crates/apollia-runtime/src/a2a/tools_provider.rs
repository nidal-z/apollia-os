//! A2AToolsProvider — builds virtual tool descriptors for A2A-enabled agents.
//!
//! Queries the [`A2AInvoker`] for active skills and maps them to
//! [`ToolDescriptor`]s prefixed with `"a2a:"`. Used by the runtime to inject
//! A2A tools into an agent's allowed tool list before each task execution so
//! that the ORIA ReAct loop can discover and invoke Worker Agents transparently.

use std::sync::Arc;

use apollia_core::SandboxProfile;
use apollia_tools::{ToolDescriptor, ToolKind};

use super::invoker::A2AInvoker;

/// Generates virtual tool descriptors for all A2A-enabled agents' skills.
///
/// Each active skill becomes a [`ToolDescriptor`] with name `"a2a:{skill_id}"`.
/// These descriptors are injected into the agent's allowed tool list before
/// task execution, allowing the ORIA ReAct loop to see and invoke them without
/// the Director Agent hard-coding any `a2a_invoke()` calls.
///
/// Virtual tools are **not** persisted in the global [`ToolRegistry`] — they are
/// built fresh for each task by interrogating the live [`A2AInvoker`].
///
/// This is a pure helper struct, not a Tokio actor. Clone cheaply via the
/// inner `Arc`.
pub struct A2AToolsProvider {
    a2a_invoker: Arc<A2AInvoker>,
}

impl A2AToolsProvider {
    /// Constructs the provider with the shared A2A invoker.
    pub fn new(a2a_invoker: Arc<A2AInvoker>) -> Self {
        Self { a2a_invoker }
    }

    /// Builds tool descriptors for all currently active A2A skills.
    ///
    /// Returns a [`Vec<ToolDescriptor>`] where each element corresponds to one
    /// skill from one active A2A-enabled agent.
    ///
    /// - `name` follows the pattern `"a2a:{skill_id}"`.
    /// - `description` follows `"{skill_description} (via {agent_name})"`.
    /// - `input_schema` defaults to `{"type": "object", "properties": {"text": {"type": "string"}}}`.
    ///
    /// Returns an empty `Vec` if no A2A agents are currently active or if the
    /// registry query fails — callers always receive a safe, non-error result.
    pub async fn build_tool_descriptors(&self) -> Vec<ToolDescriptor> {
        let skills = match self.a2a_invoker.list_skills().await {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        skills
            .iter()
            .map(|s| {
                let description = if s.description.is_empty() {
                    format!("(via {})", s.agent_name)
                } else {
                    format!("{} (via {})", s.description, s.agent_name)
                };

                let input_schema = serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Task description or query for the worker agent"
                        }
                    }
                });

                ToolDescriptor {
                    name: format!("a2a:{}", s.skill_id),
                    version: "1.0.0".to_string(),
                    description,
                    kind: ToolKind::Native,
                    input_schema,
                    output_schema: None,
                    sandbox_profile: SandboxProfile::ReadOnly,
                    tags: vec!["a2a".to_string()],
                    dangerous: false,
                    is_read_only: false,
                    risk_score: 0,
                    approval_risk_level: None,
                    impact_description: None,
                    reject_reason_required: false,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::invoker::A2AInvoker;
    use crate::a2a::{A2aDelegateResult, A2aError as LowLevelA2aError};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use apollia_core::{A2AConfig, AgentManifest, AgentSkill, ProcessState};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    fn make_ok_delegate() -> crate::a2a::A2aDelegateFn {
        Arc::new(
            |skill_id: String,
             _input: serde_json::Value,
             _timeout: u64,
             _chain: Vec<apollia_core::AgentId>,
             _caller: apollia_core::AgentId| {
                let fut: Pin<
                    Box<dyn Future<Output = Result<A2aDelegateResult, LowLevelA2aError>> + Send>,
                > = Box::pin(async move {
                    Ok(A2aDelegateResult {
                        task_id: "task-test".to_string(),
                        agent_name: "test-worker".to_string(),
                        output: format!("output for {skill_id}"),
                    })
                });
                fut
            },
        )
    }

    fn make_a2a_manifest(name: &str, skills: &[(&str, &str)]) -> AgentManifest {
        let skill_list = skills
            .iter()
            .map(|(id, desc)| AgentSkill {
                id: id.to_string(),
                name: id.to_string(),
                description: desc.to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
                input_schema: None,
            })
            .collect();

        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: name.to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: true,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec!["worker".to_string()],
            skills: skill_list,
            execution_mode: "direct".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
        }
    }

    #[tokio::test]
    async fn test_build_tool_descriptors_creates_prefixed_names() {
        // GIVEN A2AInvoker with excel-worker (2 skills)
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest(
                "excel-worker",
                &[
                    ("read-excel", "Reads Excel files"),
                    ("edit-excel", "Edits Excel files"),
                ],
            ))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");

        let invoker = Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate(),
            bus_tx,
            A2AConfig::default(),
        ));
        let provider = A2AToolsProvider::new(invoker);

        // WHEN
        let descriptors = provider.build_tool_descriptors().await;

        // THEN 2 ToolDescriptors with names "a2a:read-excel" and "a2a:edit-excel"
        assert_eq!(
            descriptors.len(),
            2,
            "expected 2 descriptors, got: {:?}",
            descriptors.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        let names: Vec<&str> = descriptors.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"a2a:read-excel"),
            "missing a2a:read-excel in {names:?}"
        );
        assert!(
            names.contains(&"a2a:edit-excel"),
            "missing a2a:edit-excel in {names:?}"
        );

        // THEN description contains "(via excel-worker)"
        let read_desc = descriptors
            .iter()
            .find(|d| d.name == "a2a:read-excel")
            .expect("read-excel descriptor not found");
        assert!(
            read_desc.description.contains("(via excel-worker)"),
            "description '{}' does not contain '(via excel-worker)'",
            read_desc.description
        );
    }

    #[tokio::test]
    async fn test_no_a2a_agents_no_descriptors() {
        // GIVEN A2AInvoker with no active A2A agents
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let invoker = Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate(),
            bus_tx,
            A2AConfig::default(),
        ));
        let provider = A2AToolsProvider::new(invoker);

        // WHEN
        let descriptors = provider.build_tool_descriptors().await;

        // THEN empty Vec
        assert!(
            descriptors.is_empty(),
            "expected empty, got {} descriptors",
            descriptors.len()
        );
    }
}
