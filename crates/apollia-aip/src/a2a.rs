//! ctx.a2a — agent-to-agent invocation surface.
//!
//! Façade nestée consolidant les 6 méthodes A2A historiquement aplaties sur
//! `RuntimeContext` (`a2a_invoke`, `a2a_discover`, `a2a_list_skills`,
//! `send`, `receive`, `delegate`). On expose les 3 méthodes
//! "haut niveau" qui pilotent l'[`A2AInvoker`] :
//!
//! - [`A2AInterface::invoke`] — appel synchrone d'un skill avec retour
//!   typé `dict` (équivalent `a2a_invoke`).
//! - [`A2AInterface::discover`] — résolution agent/skill (équivalent
//!   `a2a_discover`).
//! - [`A2AInterface::list_skills`] — inventaire complet du runtime
//!   (équivalent `a2a_list_skills`).
//! - [`A2AInterface::skill_as_tool`] — produit un descriptor
//!   tool consommable par `ctx.react`.
//!
//! Les méthodes mailbox (`send`/`receive`) et la délégation Director→Worker
//! (`delegate`) restent sur `RuntimeContext` flat — elles
//! seront migrées sans changement de sémantique.
//!
//! L'interface partage le même `Arc<A2AInvoker>` que `RuntimeContext` —
//! pas de duplication d'état ni de second canal d'événements. Le compteur
//! de profondeur (`a2a_depth`) et le `chain_deadline` sont copiés au moment
//! de la construction pour respecter l'immuabilité côté Python.

use std::sync::Arc;
use std::time::Instant;

use apollia_runtime::a2a::A2AInvoker;
use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;

/// Façade typée exposée à l'agent Python via `ctx.a2a`.
///
/// Construite par [`crate::context::RuntimeContext::new_with_llm`] (ou via
/// les builders `with_*`) lorsque le runtime fournit un `A2AInvoker`.
/// L'agent n'a jamais à instancier cette structure directement.
#[pyclass(name = "A2AInterface", module = "apollia._native")]
pub struct A2AInterface {
    /// Orchestrateur A2A partagé avec `RuntimeContext`. `None` = runtime
    /// minimal sans support A2A (tests, CLI dry-run).
    invoker: Option<Arc<A2AInvoker>>,
    /// Identifiant de l'agent caller (utilisé pour la chaîne A2A).
    caller_agent_name: String,
    /// Profondeur actuelle dans la chaîne (0 = invocation racine).
    a2a_depth: u32,
    /// Deadline cumulé de la chaîne, propagé par l'invoker. `None` avant la
    /// première invocation depuis cet agent.
    chain_deadline: Option<Instant>,
}

#[pymethods]
impl A2AInterface {
    /// Invoque un skill A2A avec entrée typée et timeout optionnel.
    ///
    /// Retourne un Python awaitable qui résout en `dict` avec les clés
    /// `result`, `agent_name`, `skill_id`, `duration_ms` en cas de succès,
    /// ou un dict `AIPResult` d'échec si une erreur runtime survient (jamais
    /// d'exception Python — sémantique alignée sur l'API historique).
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

        // Convertir input Python → serde_json::Value via json.dumps.
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
                .invoke(
                    &skill_id,
                    input_value,
                    &caller,
                    a2a_depth,
                    timeout,
                    chain_deadline,
                )
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

    /// Découvre l'agent qui expose `skill_id` et retourne sa carte de
    /// découverte.
    ///
    /// Retourne un Python awaitable qui résout en `dict | None`.
    /// `None` si aucun agent disponible ne déclare le skill.
    fn discover<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
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

    /// Liste tous les skills A2A disponibles dans le runtime.
    ///
    /// Retourne un Python awaitable qui résout en `list[dict]`.
    /// Chaque dict a les clés `skill_id`, `agent_name`, `skill_name`,
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

    /// Construit un descripteur tool consommable par la boucle ReAct
    /// (`apollia.react`).
    ///
    /// Effectue un appel `discover(skill_id)` pour récupérer la description
    /// et le schéma d'entrée du skill cible, puis assemble un descripteur au
    /// format Anthropic/OpenAI tool-use :
    ///
    /// ```json
    /// {
    ///   "name": "a2a__pdf__read_text",
    ///   "description": "Read text content from a PDF file",
    ///   "input_schema": { "type": "object", "properties": {...}, "required": [...] }
    /// }
    /// ```
    ///
    /// Naming :
    /// - Le tool name remplace `.` par `__` et préfixe `a2a__` pour rester
    ///   compatible avec OpenAI (qui refuse les `:` dans les noms d'outils).
    /// - Le bridge [`ToolProxy::call`] reconnaît `a2a__` ET `a2a:` (legacy)
    ///   comme préfixes équivalents — voir `crate::context`.
    ///
    /// Retourne un Python awaitable qui résout en `dict`.
    ///
    /// Lève [`PyKeyError`] si `skill_id` est inconnu (aucun agent A2A actif
    /// ne l'expose). Lève [`PyRuntimeError`] si l'invoker n'est pas
    /// configuré.
    fn skill_as_tool<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
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

    let card = card_opt.ok_or_else(|| {
        PyKeyError::new_err(format!("unknown A2A skill: '{skill_id}'"))
    })?;

    // Find the matching skill in the agent card.
    let skill = card.skills.iter().find(|s| s.id == skill_id).ok_or_else(|| {
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

    // Tool name: a2a__{skill_id with . replaced by __} — OpenAI rejects ':'
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
    /// Construit une nouvelle interface A2A liée au caller.
    ///
    /// `invoker = None` désactive complètement la surface (toutes les
    /// méthodes lèvent `RuntimeError("A2A invoker not available …")`), sauf
    /// `skill_as_tool` qui reste constructible (utile pour les tests
    /// unitaires builder).
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
