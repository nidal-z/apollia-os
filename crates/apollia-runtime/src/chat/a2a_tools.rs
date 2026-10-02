//! A2A virtual tool routing for Chat Libre mode.
//!
//! Routes a call to a tool prefixed with `"a2a:"` to the worker agent that
//! declares the skill. No skill is offered to the model on its own: an agent
//! reaches another one through the accesses it was granted.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::AIPPart;
use apollia_llm::ToolInvoker;

use crate::a2a::invoker::A2AError;
use crate::a2a::{A2AInvokeRequest, A2AInvoker};
use crate::chat::builtin_agent::NativeChatToolInvoker;
use crate::hooks::executor::HookExecutor;

/// Prefix used to distinguish A2A virtual tools from native chat tools.
const A2A_PREFIX: &str = "a2a:";

/// Timeout applied to A2A invocations initiated from the chat.
const CHAT_A2A_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum consecutive failures before the circuit breaker opens for a skill.
const A2A_CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

/// Maximum consecutive payload (client) errors before the model is told to stop
/// retrying a skill. Payload errors do not open the circuit (the worker is
/// healthy), but a model that cannot fix its arguments must not loop forever.
const A2A_PAYLOAD_RETRY_LIMIT: u32 = 4;

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
    /// Consecutive failure count per skill_id (worker failures, opens the
    /// circuit breaker). Reset to 0 on success.
    a2a_failures: Mutex<HashMap<String, u32>>,
    /// Consecutive payload (client) error count per skill_id. Reset on success.
    /// These never open the circuit breaker, but bound retry loops.
    a2a_payload_failures: Mutex<HashMap<String, u32>>,
    /// Lifecycle hook executor, fired at the A2A sub-agent boundary
    /// (`SubagentStart` / `SubagentStop`). `None` disables sub-agent hooks.
    hook_executor: Option<Arc<HookExecutor>>,
    /// Session identifier carried in the sub-agent hook payloads.
    session_id: String,
}

impl CompositeToolInvoker {
    /// Create a new composite invoker wrapping a native invoker and an A2A invoker.
    pub fn new(native: NativeChatToolInvoker, a2a: Arc<A2AInvoker>) -> Self {
        Self::with_hooks(native, a2a, None, String::new())
    }

    /// Create a composite invoker that also fires sub-agent lifecycle hooks at
    /// the A2A boundary, tagged with `session_id`.
    pub fn with_hooks(
        native: NativeChatToolInvoker,
        a2a: Arc<A2AInvoker>,
        hook_executor: Option<Arc<HookExecutor>>,
        session_id: String,
    ) -> Self {
        Self {
            native,
            a2a,
            a2a_failures: Mutex::new(HashMap::new()),
            a2a_payload_failures: Mutex::new(HashMap::new()),
            hook_executor,
            session_id,
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
                        "[A2A CIRCUIT_OPEN] Skill '{skill_id}' failed {count} times in a row. \
                         Do NOT retry this call. Tell the user the worker agent is unavailable."
                    ));
                }
            }

            // Full pass-through of the `arguments` dict as the A2A payload.
            // The A2A runtime wraps it in a `DataPart` on the `delegate_inner`
            // side, and the worker reads it via `extract_a2a_payload(task)`.
            let input = arguments.clone();

            // SubagentStart (non-blocking, best-effort): the worker agent name is
            // only resolved by the A2A invoker, so it is reported on stop. The
            // skill_id identifies the delegation target at start.
            if let Some(executor) = self.hook_executor.as_ref() {
                executor
                    .run_subagent_start("", skill_id, &self.session_id)
                    .await;
            }

            let outcome = self
                .a2a
                .invoke(A2AInvokeRequest {
                    skill_id,
                    input,
                    caller: "chat-libre",
                    a2a_depth: 0,
                    timeout: Some(CHAT_A2A_TIMEOUT),
                    chain_deadline: None,
                    invocation_id: None,
                })
                .await;

            if let Some(executor) = self.hook_executor.as_ref() {
                let agent_id = outcome
                    .as_ref()
                    .map(|r| r.agent_name.as_str())
                    .unwrap_or("");
                executor
                    .run_subagent_stop(agent_id, skill_id, outcome.is_ok(), &self.session_id)
                    .await;
            }

            match outcome {
                Ok(result) => {
                    // Reset both failure counters on success.
                    {
                        let mut failures =
                            self.a2a_failures.lock().unwrap_or_else(|e| e.into_inner());
                        failures.remove(skill_id);
                    }
                    {
                        let mut payload = self
                            .a2a_payload_failures
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        payload.remove(skill_id);
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
                    // A payload / schema-validation rejection is a CLIENT error:
                    // the worker is healthy, the model just sent arguments that
                    // do not match the skill schema (e.g. a wrong field name). It
                    // must NOT trip the circuit breaker, that would block recovery
                    // for the rest of the turn, and the model should correct the
                    // arguments and retry (the worker often suggests the right
                    // field). The step budget bounds any retry loop.
                    let e_str = e.to_string();
                    let is_payload_error = e_str.contains("PAYLOAD_ERROR")
                        || e_str.contains("Unexpected field")
                        || e_str.contains("Expected fields");
                    if is_payload_error {
                        let count = {
                            let mut payload = self
                                .a2a_payload_failures
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            let c = payload.entry(skill_id.to_string()).or_insert(0);
                            *c += 1;
                            *c
                        };
                        if count >= A2A_PAYLOAD_RETRY_LIMIT {
                            return Err(format!(
                                "[A2A PAYLOAD_ERROR] {e}\n\
                                 You failed {count} times to build a valid payload for this \
                                 skill. Do not retry it the same way: take another approach, \
                                 or tell the user this skill could not be called \
                                 correctly."
                            ));
                        }
                        return Err(format!(
                            "[A2A PAYLOAD_ERROR] {e}\n\
                             Fix the argument names and values to match the schema the skill \
                             expects (use EXACTLY the expected field names), then retry the \
                             call with the corrected payload."
                        ));
                    }

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
                         Do NOT retry this call. Tell the user about the failure."
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
                input_schema: None,
                examples: vec![],
            })
            .collect();

        AgentManifest {
            autonomy_level: None,
            format_version: 1,
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
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            model: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    fn make_ok_delegate(output: &str) -> crate::a2a::A2aDelegateFn {
        let output = output.to_string();
        Arc::new(
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId,
                  _invocation: Option<crate::a2a::ClientInvocation>| {
                let output = output.clone();
                Box::pin(async move {
                    Ok(A2aDelegateResult {
                        task_id: "t1".into(),
                        run_id: None,
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
                  _caller: apollia_core::AgentId,
                  _invocation: Option<crate::a2a::ClientInvocation>| {
                Box::pin(async move {
                    Err(A2aError::WorkerFailed {
                        reason: "agent crashed".to_string(),
                    })
                })
            },
        )
    }

    fn make_payload_err_delegate() -> crate::a2a::A2aDelegateFn {
        Arc::new(
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId,
                  _invocation: Option<crate::a2a::ClientInvocation>| {
                Box::pin(async move {
                    Err(A2aError::WorkerFailed {
                        reason: "[PAYLOAD_ERROR] Unexpected field 'max_chars'. \
                                 Expected fields: ['url', 'max_words']. Did you mean 'max_words'?"
                            .to_string(),
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

        // WHEN invoke("unknown_native_tool", ...), no "a2a:" prefix
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

    #[tokio::test]
    async fn test_a2a_payload_error_does_not_trip_circuit_breaker() {
        // GIVEN a worker that rejects the payload (a client error: the worker is
        // healthy, the model sent a wrong field name)
        let invoker = make_invoker_with_agent(
            "md-worker",
            &[("summarize", "Summarize a URL")],
            make_payload_err_delegate(),
        )
        .await;

        use super::CompositeToolInvoker;
        use crate::chat::builtin_agent::NativeChatToolInvoker;
        use apollia_llm::ToolInvoker;

        let composite = CompositeToolInvoker::new(
            NativeChatToolInvoker::new_with_workspace(None),
            Arc::new(invoker),
        );

        // WHEN the skill is invoked more times than the breaker threshold
        for _ in 0..(super::A2A_CIRCUIT_BREAKER_THRESHOLD + 2) {
            let result = composite
                .invoke("a2a:summarize", &serde_json::json!({"max_chars": 100}))
                .await;

            // THEN every attempt is a corrective payload error, and the circuit
            // breaker never opens, so the model can keep correcting and retrying.
            let msg = result.expect_err("payload validation should error");
            assert!(
                msg.contains("PAYLOAD_ERROR"),
                "expected a payload error, got: {msg}"
            );
            assert!(
                !msg.contains("CIRCUIT_OPEN"),
                "payload errors must not trip the circuit breaker: {msg}"
            );
        }
    }
}
