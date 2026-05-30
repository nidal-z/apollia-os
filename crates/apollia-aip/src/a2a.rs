//! ctx.a2a: agent-to-agent invocation surface.
//!
//! Nested facade consolidating the 6 A2A methods historically flattened onto
//! `RuntimeContext` (`a2a_invoke`, `a2a_discover`, `a2a_list_skills`, `send`,
//! `receive`, `delegate`). It exposes the 3 high-level methods that drive the
//! [`A2AInvoker`]:
//!
//! - [`A2AInterface::invoke`]: synchronous skill call with a typed `dict`
//!   return (equivalent of `a2a_invoke`).
//! - [`A2AInterface::discover`]: agent/skill resolution (equivalent of
//!   `a2a_discover`).
//! - [`A2AInterface::list_skills`]: full runtime inventory (equivalent of
//!   `a2a_list_skills`).
//! - [`A2AInterface::skill_as_tool`]: produces a tool descriptor consumable
//!   by `ctx.react`.
//!
//! The mailbox methods (`send`/`receive`) and the Director-to-Worker
//! delegation (`delegate`) stay on the flat `RuntimeContext`; they will be
//! migrated without a semantic change.
//!
//! The interface shares the same `Arc<A2AInvoker>` as `RuntimeContext`: no
//! state duplication and no second event channel. The depth counter
//! (`a2a_depth`) and the `chain_deadline` are copied at construction time to
//! honor immutability on the Python side.

use std::sync::Arc;
use std::time::Instant;

use apollia_runtime::a2a::A2AInvoker;
use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;

/// Typed facade exposed to the Python agent via `ctx.a2a`.
///
/// Built by [`crate::context::RuntimeContext::new_with_llm`] (or via the
/// `with_*` builders) when the runtime provides an `A2AInvoker`. The agent
/// never instantiates this struct directly.
#[pyclass(name = "A2AInterface", module = "apollia._native")]
pub struct A2AInterface {
    /// A2A orchestrator shared with `RuntimeContext`. `None` = minimal runtime
    /// without A2A support (tests, CLI dry-run).
    invoker: Option<Arc<A2AInvoker>>,
    /// Caller agent id (used for the A2A chain).
    caller_agent_name: String,
    /// Current depth in the chain (0 = root invocation).
    a2a_depth: u32,
    /// Cumulative chain deadline, propagated by the invoker. `None` before the
    /// first invocation from this agent.
    chain_deadline: Option<Instant>,
}

#[pymethods]
impl A2AInterface {
    /// Invokes an A2A skill with typed input and an optional timeout.
    ///
    /// Returns a Python awaitable resolving to a `dict` with keys `result`,
    /// `agent_name`, `skill_id`, `duration_ms` on success, or a failure
    /// `AIPResult` dict if a runtime error occurs (never a Python exception,
    /// matching the historical API semantics).
    #[pyo3(signature = (skill_id, input, timeout_secs=None))]
    fn invoke<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
        input: PyObject,
        timeout_secs: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        // Convert the Python input to serde_json::Value via json.dumps.
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let json_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        let caller = self.caller_agent_name.clone();
        let timeout = timeout_secs.map(std::time::Duration::from_secs);
        let a2a_depth = self.a2a_depth.saturating_add(1);
        let chain_deadline = self.chain_deadline;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let out_json = match invoker
                .invoke(apollia_runtime::a2a::A2AInvokeRequest {
                    skill_id: &skill_id,
                    input: input_value,
                    caller: &caller,
                    a2a_depth,
                    timeout,
                    chain_deadline,
                })
                .await
            {
                Ok(r) => serde_json::to_string(&r)
                    .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?,
                Err(e) => {
                    let failed =
                        apollia_core::AIPResult::failed("a2a_invoke_error", &e.to_string());
                    serde_json::to_string(&failed).map_err(|err| {
                        PyRuntimeError::new_err(format!("serialization error: {err}"))
                    })?
                }
            };

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (out_json,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }

    /// Discovers the agent that exposes `skill_id` and returns its discovery
    /// card.
    ///
    /// Returns a Python awaitable resolving to `dict | None`.
    /// `None` if no available agent declares the skill.
    fn discover<'py>(&self, py: Python<'py>, skill_id: String) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let card_opt = invoker
                .discover(&skill_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            match card_opt {
                None => Ok(Python::with_gil(|py| py.None())),
                Some(card) => {
                    let json_str = serde_json::to_string(&card).map_err(|e| {
                        PyRuntimeError::new_err(format!("serialization error: {e}"))
                    })?;
                    Python::with_gil(|py| {
                        let json_mod = py
                            .import("json")
                            .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                        let py_obj: PyObject = json_mod
                            .call_method1("loads", (json_str,))
                            .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                            .unbind();
                        Ok(py_obj)
                    })
                }
            }
        })
    }

    /// Lists every A2A skill available in the runtime.
    ///
    /// Returns a Python awaitable resolving to `list[dict]`.
    /// Each dict has keys `skill_id`, `agent_name`, `skill_name`,
    /// `description`.
    fn list_skills<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let skills = invoker
                .list_skills()
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            let json_str = serde_json::to_string(&skills)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (json_str,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }

    /// Builds a tool descriptor consumable by the ReAct loop
    /// (`apollia.react`).
    ///
    /// Performs a `discover(skill_id)` call to fetch the target skill's
    /// description and input schema, then assembles a descriptor in the
    /// Anthropic/OpenAI tool-use format:
    ///
    /// ```json
    /// {
    ///   "name": "a2a__pdf__read_text",
    ///   "description": "Read text content from a PDF file",
    ///   "input_schema": { "type": "object", "properties": {...}, "required": [...] }
    /// }
    /// ```
    ///
    /// Naming:
    /// - The tool name replaces `.` with `__` and prefixes `a2a__` to stay
    ///   compatible with OpenAI (which rejects `:` in tool names).
    /// - The bridge [`ToolProxy::call`] recognizes both `a2a__` and `a2a:`
    ///   (legacy) as equivalent prefixes; see `crate::context`.
    ///
    /// Returns a Python awaitable resolving to `dict`.
    ///
    /// Raises [`PyKeyError`] if `skill_id` is unknown (no active A2A agent
    /// exposes it). Raises [`PyRuntimeError`] if the invoker is not
    /// configured.
    fn skill_as_tool<'py>(&self, py: Python<'py>, skill_id: String) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let descriptor = build_skill_tool_descriptor(&invoker, &skill_id).await?;
            let json_str = serde_json::to_string(&descriptor).map_err(|e| {
                PyRuntimeError::new_err(format!("descriptor serialization failed: {e}"))
            })?;

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (json_str,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }
}

/// Pure async helper computing the tool descriptor for `skill_id` from the
/// A2A invoker. Extracted from [`A2AInterface::skill_as_tool`] so it can be
/// unit-tested without requiring an initialized pyo3-async tokio runtime.
///
/// Returns a JSON object with keys `name`, `description`, `input_schema`
/// suitable for Anthropic/OpenAI tool-use APIs.
async fn build_skill_tool_descriptor(
    invoker: &A2AInvoker,
    skill_id: &str,
) -> PyResult<serde_json::Value> {
    let card_opt = invoker
        .discover(skill_id)
        .await
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let card =
        card_opt.ok_or_else(|| PyKeyError::new_err(format!("unknown A2A skill: '{skill_id}'")))?;

    // Find the matching skill in the agent card.
    let skill = card
        .skills
        .iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| {
            PyKeyError::new_err(format!(
                "skill '{skill_id}' not declared by agent '{}'",
                card.name
            ))
        })?;

    // Default input schema: empty object accepting any properties.
    let default_schema = serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": true,
    });
    let input_schema = skill.input_schema.clone().unwrap_or(default_schema);

    // Tool name: a2a__{skill_id with . replaced by __}. OpenAI rejects ':'
    // in tool names; the bridge accepts both `a2a__` and `a2a:` prefixes
    // and reverses the `__` -> `.` encoding before dispatch (see
    // `crate::context::extract_a2a_skill_id`).
    let safe_id = skill_id.replace('.', "__");
    let tool_name = format!("a2a__{safe_id}");

    // Build the descriptor. Examples are only included when present so that
    // skills that don't ship a sample payload don't pollute the LLM context
    // with an empty `examples: []`.
    let mut descriptor = serde_json::Map::new();
    descriptor.insert("name".into(), serde_json::Value::String(tool_name));
    descriptor.insert(
        "description".into(),
        serde_json::Value::String(skill.description.clone()),
    );
    descriptor.insert("input_schema".into(), input_schema);
    if !skill.examples.is_empty() {
        descriptor.insert(
            "examples".into(),
            serde_json::Value::Array(skill.examples.clone()),
        );
    }
    Ok(serde_json::Value::Object(descriptor))
}

impl A2AInterface {
    /// Builds a new A2A interface bound to the caller.
    ///
    /// `invoker = None` fully disables the surface (all methods raise
    /// `RuntimeError("A2A invoker not available ...")`), except
    /// `skill_as_tool` which stays constructible (useful for builder unit
    /// tests).
    pub fn new(
        invoker: Option<Arc<A2AInvoker>>,
        caller_agent_name: String,
        a2a_depth: u32,
        chain_deadline: Option<Instant>,
    ) -> Self {
        Self {
            invoker,
            caller_agent_name,
            a2a_depth,
            chain_deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{A2AConfig, AgentManifest, AgentSkill, ProcessState};
    use apollia_runtime::a2a::A2aError as LowLevelA2aError;
    use apollia_runtime::registry::AgentRegistry;
    use apollia_runtime::{A2AInvoker, A2aDelegateFn, A2aDelegateResult, EventBus};
    use std::future::Future;
    use std::pin::Pin;

    fn make_ok_delegate() -> A2aDelegateFn {
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
                        task_id: "task-a2a".to_string(),
                        agent_name: "pdf-worker".to_string(),
                        output: format!("processed {skill_id}"),
                    })
                });
                fut
            },
        )
    }

    fn make_pdf_manifest() -> AgentManifest {
        AgentManifest {
            name: "pdf-worker".to_string(),
            version: "0.1.0".to_string(),
            description: "PDF worker".to_string(),
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
            skills: vec![AgentSkill {
                id: "pdf.read_text".to_string(),
                name: "Read PDF text".to_string(),
                description: "Read text content from a PDF file".to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                })),
                examples: vec![],
            }],
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
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
        }
    }

    async fn make_invoker_with_pdf() -> Arc<A2AInvoker> {
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_pdf_manifest())
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state");

        Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate(),
            bus_tx,
            A2AConfig::default(),
        ))
    }

    #[test]
    fn test_invoke_without_invoker_raises() {
        pyo3::prepare_freethreaded_python();
        let a2a = A2AInterface::new(None, "tester".to_string(), 0, None);
        Python::with_gil(|py| {
            let input = py.None();
            let res = a2a.invoke(py, "x".to_string(), input, None);
            assert!(res.is_err(), "expected RuntimeError without invoker");
        });
    }

    #[test]
    fn test_skill_as_tool_without_invoker_raises() {
        pyo3::prepare_freethreaded_python();
        let a2a = A2AInterface::new(None, "tester".to_string(), 0, None);
        Python::with_gil(|py| {
            let res = a2a.skill_as_tool(py, "summarize".to_string());
            assert!(
                res.is_err(),
                "expected RuntimeError when invoker is missing"
            );
        });
    }

    /// GIVEN an invoker exposing `pdf.read_text`
    /// WHEN `build_skill_tool_descriptor("pdf.read_text")` is awaited
    /// THEN the returned JSON value has the right `name`, `description`
    /// and `input_schema` (Anthropic/OpenAI tool-use compatible).
    #[tokio::test]
    async fn test_skill_as_tool_returns_full_descriptor() {
        let invoker = make_invoker_with_pdf().await;
        let descriptor = build_skill_tool_descriptor(&invoker, "pdf.read_text")
            .await
            .expect("descriptor should be built");

        assert_eq!(descriptor["name"], "a2a__pdf__read_text");
        assert_eq!(
            descriptor["description"],
            "Read text content from a PDF file"
        );
        assert_eq!(descriptor["input_schema"]["type"], "object");
        assert_eq!(
            descriptor["input_schema"]["properties"]["path"]["type"],
            "string"
        );
        assert_eq!(descriptor["input_schema"]["required"][0], "path");
    }

    /// GIVEN an invoker that does NOT expose `unknown.skill`
    /// WHEN `build_skill_tool_descriptor("unknown.skill")` is awaited
    /// THEN a `KeyError` is raised.
    #[tokio::test]
    async fn test_skill_as_tool_unknown_skill_raises_keyerror() {
        pyo3::prepare_freethreaded_python();
        let invoker = make_invoker_with_pdf().await;
        let result = build_skill_tool_descriptor(&invoker, "unknown.skill").await;

        assert!(result.is_err(), "expected KeyError for unknown skill");
        let err = result.unwrap_err();
        Python::with_gil(|py| {
            assert!(
                err.is_instance_of::<PyKeyError>(py),
                "expected PyKeyError, got: {err}"
            );
        });
    }

    /// Sanity check : skill_id without dots still produces the canonical
    /// `a2a__{id}` name.
    #[tokio::test]
    async fn test_skill_as_tool_no_dots_in_skill_id() {
        // Register a worker with a single-word skill id.
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let mut manifest = make_pdf_manifest();
        manifest.skills[0].id = "summarize".to_string();
        manifest.skills[0].input_schema = None;
        let agent_id = registry.register(manifest).await.expect("register");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state");
        let invoker = Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate(),
            bus_tx,
            A2AConfig::default(),
        ));

        let descriptor = build_skill_tool_descriptor(&invoker, "summarize")
            .await
            .expect("descriptor");
        assert_eq!(descriptor["name"], "a2a__summarize");
        // Default schema when none declared.
        assert_eq!(descriptor["input_schema"]["type"], "object");
        assert_eq!(descriptor["input_schema"]["additionalProperties"], true);
    }
}
