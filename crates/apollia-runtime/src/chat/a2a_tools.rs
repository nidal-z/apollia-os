//! A2A virtual tool integration for Chat Libre mode.
//!
//! Exposes skills from active A2A agents as virtual tools prefixed with `"a2a:"`,
//! allowing the LLM to invoke worker agents transparently alongside native tools.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::AIPPart;
use apollia_llm::types::ToolSpec;
use apollia_llm::ToolInvoker;
use tracing::warn;

use crate::a2a::invoker::A2AError;
use crate::a2a::A2AInvoker;
use crate::chat::builtin_agent::NativeChatToolInvoker;

/// Prefix used to distinguish A2A virtual tools from native chat tools.
const A2A_PREFIX: &str = "a2a:";

/// Timeout applied to A2A invocations initiated from the chat.
const CHAT_A2A_TIMEOUT: Duration = Duration::from_secs(120);

/// Generate virtual [`ToolSpec`]s from the skills of all active A2A agents.
///
/// Each skill becomes a tool named `"a2a:{skill_id}"` with:
/// - Description: `"{skill_description} (via {agent_name})"`
/// - Input schema: `{"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}`
///
/// Returns an empty list if no A2A agents with skills are available.
pub async fn generate_a2a_tool_specs(a2a_invoker: &A2AInvoker) -> Vec<ToolSpec> {
    let skills = match a2a_invoker.list_skills().await {
        Ok(skills) => skills,
        Err(e) => {
            warn!(error = %e, "Failed to list A2A skills for tool spec generation");
            return vec![];
        }
    };

    skills
        .into_iter()
        .map(|skill| {
            let description = if skill.description.is_empty() {
                format!("{} (via {})", skill.skill_name, skill.agent_name)
            } else {
                format!("{} (via {})", skill.description, skill.agent_name)
            };

            ToolSpec {
                name: format!("{}{}", A2A_PREFIX, skill.skill_id),
                description,
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The request to send to the agent"
                        }
                    },
                    "required": ["text"]
                }),
            }
        })
        .collect()
}

/// Maximum consecutive failures before the circuit breaker opens for a skill.
const A2A_CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

/// [`ToolInvoker`] that composes [`NativeChatToolInvoker`] with an [`A2AInvoker`].
///
/// Routes tool calls prefixed with `"a2a:"` to the A2A invoker, which delegates
/// to the appropriate worker agent. All other tool calls are forwarded to the
/// underlying native invoker unchanged.
///
/// Includes a per-skill circuit breaker: after [`A2A_CIRCUIT_BREAKER_THRESHOLD`]
/// consecutive failures, the skill is short-circuited with an immediate error
/// to prevent the LLM from looping on a broken delegation.
pub struct CompositeToolInvoker {
    native: NativeChatToolInvoker,
    a2a: Arc<A2AInvoker>,
    /// Consecutive failure count per skill_id. Reset to 0 on success.
    a2a_failures: Mutex<HashMap<String, u32>>,
}

impl CompositeToolInvoker {
    /// Create a new composite invoker wrapping a native invoker and an A2A invoker.
    pub fn new(native: NativeChatToolInvoker, a2a: Arc<A2AInvoker>) -> Self {
        Self {
            native,
            a2a,
            a2a_failures: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl ToolInvoker for CompositeToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String> {
        if let Some(skill_id) = tool_name.strip_prefix(A2A_PREFIX) {
            // Circuit breaker: reject immediately if too many consecutive failures.
            {
                let failures = self.a2a_failures.lock().unwrap_or_else(|e| e.into_inner());
                let count = failures.get(skill_id).copied().unwrap_or(0);
                if count >= A2A_CIRCUIT_BREAKER_THRESHOLD {
                    return Err(format!(
                        "[A2A CIRCUIT_OPEN] Le skill '{skill_id}' a échoué {count} fois consécutives. \
                         Ne retente PAS cet appel. Informe l'utilisateur que l'agent worker n'est pas disponible."
                    ));
                }
            }

            let text = arguments
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("{tool_name}: missing required 'text' field"))?;

            let input = serde_json::json!({ "text": text });

            match self
                .a2a
                .invoke(
                    skill_id,
                    input,
                    "chat-libre",
                    0,
                    Some(CHAT_A2A_TIMEOUT),
                    None,
                )
                .await
            {
                Ok(result) => {
                    // Reset circuit breaker on success.
                    {
                        let mut failures =
                            self.a2a_failures.lock().unwrap_or_else(|e| e.into_inner());
                        failures.remove(skill_id);
                    }

                    let output_text: String = result
                        .result
                        .output
                        .iter()
                        .filter_map(|part| {
                            if let AIPPart::Text(t) = part {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(format!(
                        "[{skill_id} via {}]\n{output_text}",
                        result.agent_name
                    ))
                }
                Err(e) => {
                    {
                        let mut failures =
                            self.a2a_failures.lock().unwrap_or_else(|e| e.into_inner());
                        *failures.entry(skill_id.to_string()).or_insert(0) += 1;
                    }

                    let classification = match &e {
                        A2AError::SkillNotFound { .. }
                        | A2AError::AgentNotActive { .. }
                        | A2AError::SelfInvocation { .. }
                        | A2AError::MaxDepthExceeded { .. }
                        | A2AError::ChainTimeoutExceeded { .. } => "PERMANENT",
                        _ => "EXECUTION",
                    };
                    Err(format!(
                        "[A2A {classification}_ERROR] {e}\n\
                         Ne retente PAS cet appel. Informe l'utilisateur de l'échec."
                    ))
                }
            }
        } else {
            self.native.invoke(tool_name, arguments).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use apollia_core::{AgentManifest, AgentSkill, ProcessState};

    use crate::a2a::invoker::A2AInvoker;
    use crate::a2a::{A2aDelegateResult, A2aError};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;

    fn make_a2a_manifest(name: &str, skill_ids: &[(&str, &str)]) -> AgentManifest {
        let skills = skill_ids
            .iter()
            .map(|(id, desc)| AgentSkill {
                id: id.to_string(),
                name: id.to_string(),
                description: desc.to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            })
            .collect();

        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
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
            tags: vec![],
            skills,
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
        }
    }

    fn make_ok_delegate(output: &str) -> crate::a2a::A2aDelegateFn {
        let output = output.to_string();
        Arc::new(
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId| {
                let output = output.clone();
                Box::pin(async move {
                    Ok(A2aDelegateResult {
                        task_id: "t1".into(),
                        agent_name: "excel-worker".into(),
                        output,
                    })
                })
            },
        )
    }

    fn make_err_delegate() -> crate::a2a::A2aDelegateFn {
        Arc::new(
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId| {
                Box::pin(async move {
                    Err(A2aError::WorkerFailed {
                        reason: "agent crashed".to_string(),
                    })
                })
            },
        )
    }

    async fn make_invoker_with_agent(
        name: &str,
        skill_ids: &[(&str, &str)],
        delegate: crate::a2a::A2aDelegateFn,
    ) -> A2AInvoker {
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());

        let manifest = make_a2a_manifest(name, skill_ids);
        let agent_id = registry.register(manifest).await.expect("register");

        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("transition to active");

        A2AInvoker::new_for_test(
            registry,
            delegate,
            bus_tx,
            apollia_core::A2AConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_generate_a2a_tool_specs_creates_prefixed_tools() {
        // GIVEN A2AInvoker with excel-worker (3 skills)
        let invoker = make_invoker_with_agent(
            "excel-worker",
            &[
                ("read-excel", "Read an Excel file"),
                ("edit-excel", "Edit an Excel file"),
                ("analyze-excel", "Analyze an Excel file"),
            ],
            make_ok_delegate("ok"),
        )
        .await;

        // WHEN generate_a2a_tool_specs()
        let specs = super::generate_a2a_tool_specs(&invoker).await;

        // THEN 3 ToolSpec with names "a2a:read-excel", "a2a:edit-excel", "a2a:analyze-excel"
        assert_eq!(specs.len(), 3);
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"a2a:read-excel"),
            "missing a2a:read-excel in {names:?}"
        );
        assert!(
            names.contains(&"a2a:edit-excel"),
            "missing a2a:edit-excel in {names:?}"
        );
        assert!(
            names.contains(&"a2a:analyze-excel"),
            "missing a2a:analyze-excel in {names:?}"
        );

        // AND each description contains "(via excel-worker)"
        for spec in &specs {
            assert!(
                spec.description.contains("(via excel-worker)"),
                "description '{}' does not contain '(via excel-worker)'",
                spec.description
            );
        }

        // AND each input_schema has "text" property as required
        for spec in &specs {
            let props = spec.parameters.get("properties").expect("properties");
            assert!(props.get("text").is_some(), "missing 'text' property");
            let required = spec.parameters.get("required").expect("required");
            assert!(
                required
                    .as_array()
                    .map_or(false, |a| a.iter().any(|v| v == "text")),
                "text must be required"
            );
        }
    }

    #[tokio::test]
    async fn test_no_a2a_agents_no_a2a_specs() {
        // GIVEN A2AInvoker with no registered agents
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let invoker = A2AInvoker::new_for_test(
            registry,
            make_ok_delegate("ok"),
            bus_tx,
            apollia_core::A2AConfig::default(),
        );

        // WHEN generate_a2a_tool_specs()
        let specs = super::generate_a2a_tool_specs(&invoker).await;

        // THEN empty list
        assert!(specs.is_empty(), "expected no specs, got {}", specs.len());
    }

    #[tokio::test]
    async fn test_composite_invoker_routes_a2a_prefix() {
        // GIVEN CompositeToolInvoker with excel-worker
        let invoker = make_invoker_with_agent(
            "excel-worker",
            &[("read-excel", "Read Excel")],
            make_ok_delegate("col A, col B"),
        )
        .await;

        use super::CompositeToolInvoker;
        use crate::chat::builtin_agent::NativeChatToolInvoker;
        use apollia_llm::ToolInvoker;

        let composite = CompositeToolInvoker::new(
            NativeChatToolInvoker::new_with_workspace(None),
            Arc::new(invoker),
        );

        // WHEN invoke("a2a:read-excel", {"text": "Lis ventes.xlsx"})
        let result = composite
            .invoke(
                "a2a:read-excel",
                &serde_json::json!({"text": "Lis ventes.xlsx"}),
            )
            .await;

        // THEN Ok with formatted output
        assert!(result.is_ok(), "expected Ok, got Err: {:?}", result.err());
        let output = result.unwrap();
        assert!(
            output.contains("read-excel via excel-worker"),
            "output missing routing header: {output}"
        );
    }

    #[tokio::test]
    async fn test_composite_invoker_routes_native() {
        // GIVEN CompositeToolInvoker
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let a2a_invoker = Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate("ok"),
            bus_tx,
            apollia_core::A2AConfig::default(),
        ));

        use super::CompositeToolInvoker;
        use crate::chat::builtin_agent::NativeChatToolInvoker;
        use apollia_llm::ToolInvoker;

        let composite =
            CompositeToolInvoker::new(NativeChatToolInvoker::new_with_workspace(None), a2a_invoker);

        // WHEN invoke("unknown_native_tool", ...) — no "a2a:" prefix
        // THEN it is forwarded to NativeChatToolInvoker which returns an error for unknown tools
        let result = composite
            .invoke("unknown_native_tool", &serde_json::json!({}))
            .await;

        // NativeChatToolInvoker returns Err for unknown tools
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("unknown tool"),
            "unexpected error message: {msg}"
        );
    }

    #[tokio::test]
    async fn test_a2a_invocation_failure_returns_err() {
        // GIVEN A2AInvoker that returns WorkerFailed
        let invoker = make_invoker_with_agent(
            "excel-worker",
            &[("read-excel", "Read Excel")],
            make_err_delegate(),
        )
        .await;

        use super::CompositeToolInvoker;
        use crate::chat::builtin_agent::NativeChatToolInvoker;
        use apollia_llm::ToolInvoker;

        let composite = CompositeToolInvoker::new(
            NativeChatToolInvoker::new_with_workspace(None),
            Arc::new(invoker),
        );

        // WHEN invoke("a2a:read-excel", ...)
        let result = composite
            .invoke("a2a:read-excel", &serde_json::json!({"text": "test"}))
            .await;

        // THEN Err with failure message
        assert!(result.is_err(), "expected Err, got Ok");
        let msg = result.unwrap_err();
        assert!(!msg.is_empty(), "error message must not be empty");
    }
}
