//! ToolProxy — Python-facing proxy for invoking Rust tools.
//!
//! Exposes a `#[pyclass]` that agents use via `ctx.tools.call(tool_name, input)`.
//! Handles permission checks, registry lookup, tool execution, audit logging,
//! and tool call counting (STORY-027, ADR-015).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_tools::{
    compute_input_hash, AuditTrailHandle, ToolInvocationRecord, ToolRegistryHandle,
};

/// Errors from tool invocation via the proxy.
#[derive(Debug, thiserror::Error)]
pub enum ToolProxyError {
    /// The requested tool was not found in the registry.
    #[error("tool not found: '{0}'")]
    ToolNotFound(String),

    /// The agent is not allowed to use this tool.
    #[error("tool '{0}' not allowed for this agent")]
    ToolNotAllowed(String),

    /// The tool execution failed.
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
}

/// Trait abstracting tool execution for testability (ADR-015).
///
/// Concrete implementations dispatch to native tools (FileIo, BashExecutor, etc.).
/// Tests use a mock executor.
pub trait ToolExecutor: Send + Sync {
    /// Executes the named tool with the given JSON input.
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

/// Python-facing proxy exposing Rust tools to an agent.
///
/// Each agent receives its own `ToolProxy` instance configured with
/// the tools allowed by its manifest. All calls are permission-checked,
/// audited, and counted.
#[pyclass]
pub struct ToolProxy {
    registry: ToolRegistryHandle,
    audit: AuditTrailHandle,
    executor: Arc<dyn ToolExecutor>,
    allowed_tools: Vec<String>,
    agent_id: String,
    task_id: String,
    tool_calls: AtomicU32,
}

#[pymethods]
impl ToolProxy {
    /// Calls a tool by name with a Python dict as input.
    ///
    /// Returns a Python awaitable that resolves to a dict.
    /// Checks permissions, looks up the tool, executes it,
    /// records an audit entry, and increments the call counter.
    fn call<'py>(
        &self,
        py: Python<'py>,
        tool_name: String,
        input: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert Python dict -> JSON string -> serde_json::Value
        let json_mod = py
            .import_bound("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let input_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&input_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        // Increment counter unconditionally (AC-6)
        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        // Clone fields for the 'static async future
        let registry = self.registry.clone();
        let audit = self.audit.clone();
        let executor = Arc::clone(&self.executor);
        let allowed = self.allowed_tools.clone();
        let agent_id = self.agent_id.clone();
        let task_id = self.task_id.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = execute_tool(
                &ToolCallContext {
                    registry: &registry,
                    audit: &audit,
                    executor: &executor,
                    allowed_tools: &allowed,
                    agent_id: &agent_id,
                    task_id: &task_id,
                },
                &tool_name,
                input_value,
            )
            .await;

            match result {
                Ok(value) => {
                    let json_str = serde_json::to_string(&value).map_err(|e| {
                        PyRuntimeError::new_err(format!("result serialization: {e}"))
                    })?;
                    Python::with_gil(|py| {
                        let json_mod = py
                            .import_bound("json")
                            .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                        let py_obj: PyObject = json_mod
                            .call_method1("loads", (json_str,))
                            .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                            .unbind();
                        Ok(py_obj)
                    })
                }
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Lists the tools available to this agent.
    fn list_tools(&self) -> Vec<String> {
        self.allowed_tools.clone()
    }

    /// Returns the number of tool calls made so far.
    fn tool_call_count(&self) -> u32 {
        self.tool_calls.load(Ordering::Relaxed)
    }
}

#[allow(dead_code)]
impl ToolProxy {
    /// Creates a new ToolProxy for an agent.
    ///
    /// Called by the runtime when constructing a `RuntimeContext` for a task.
    pub fn new(
        registry: ToolRegistryHandle,
        audit: AuditTrailHandle,
        executor: Arc<dyn ToolExecutor>,
        allowed_tools: Vec<String>,
        agent_id: String,
        task_id: String,
    ) -> Self {
        Self {
            registry,
            audit,
            executor,
            allowed_tools,
            agent_id,
            task_id,
            tool_calls: AtomicU32::new(0),
        }
    }

    /// Core tool execution logic — testable without PyO3.
    ///
    /// Performs permission check, registry lookup, execution, and audit recording.
    /// Increments the tool call counter.
    pub(crate) async fn call_inner(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolProxyError> {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        execute_tool(
            &ToolCallContext {
                registry: &self.registry,
                audit: &self.audit,
                executor: &self.executor,
                allowed_tools: &self.allowed_tools,
                agent_id: &self.agent_id,
                task_id: &self.task_id,
            },
            tool_name,
            input,
        )
        .await
    }
}

/// Grouped parameters for [`execute_tool`] to avoid too many function arguments.
struct ToolCallContext<'a> {
    registry: &'a ToolRegistryHandle,
    audit: &'a AuditTrailHandle,
    executor: &'a Arc<dyn ToolExecutor>,
    allowed_tools: &'a [String],
    agent_id: &'a str,
    task_id: &'a str,
}

/// Shared tool execution logic used by both the Python `call()` and Rust `call_inner()`.
async fn execute_tool(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ToolProxyError> {
    let start = Instant::now();
    let input_hash = compute_input_hash(&input);

    // 1. Check permission BEFORE registry lookup (don't reveal tool existence)
    if !ctx.allowed_tools.iter().any(|t| t == tool_name) {
        let duration = start.elapsed();
        record_audit(
            ctx,
            tool_name,
            &input_hash,
            "unknown",
            duration.as_millis() as u64,
            false,
            Some(ToolProxyError::ToolNotAllowed(tool_name.to_string()).to_string()),
        );
        return Err(ToolProxyError::ToolNotAllowed(tool_name.to_string()));
    }

    // 2. Lookup in registry
    let descriptor = ctx
        .registry
        .get(tool_name)
        .await
        .map_err(|e| ToolProxyError::ExecutionFailed(e.to_string()))?
        .ok_or_else(|| ToolProxyError::ToolNotFound(tool_name.to_string()))?;

    let sandbox_profile = format!("{:?}", descriptor.sandbox_profile);

    // 3. Execute the tool
    let exec_result = ctx.executor.execute(tool_name, input);
    let duration = start.elapsed();

    let (success, error_code) = match &exec_result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.clone())),
    };

    // 4. Record audit (always, success or failure)
    record_audit(
        ctx,
        tool_name,
        &input_hash,
        &sandbox_profile,
        duration.as_millis() as u64,
        success,
        error_code,
    );

    // 5. Return result
    exec_result.map_err(ToolProxyError::ExecutionFailed)
}

/// Records a tool invocation in the audit trail (fire-and-forget).
fn record_audit(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input_hash: &str,
    sandbox_profile: &str,
    duration_ms: u64,
    success: bool,
    error_code: Option<String>,
) {
    ctx.audit.record(ToolInvocationRecord {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: ctx.agent_id.to_string(),
        task_id: ctx.task_id.to_string(),
        tool_name: tool_name.to_string(),
        input_hash: input_hash.to_string(),
        sandbox_profile: sandbox_profile.to_string(),
        started_at: now_rfc3339(),
        duration_ms: Some(duration_ms),
        exit_code: None,
        success,
        error_code,
        resources_used: None,
    });
}

/// Returns the current UTC time as an RFC3339 string.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day) = epoch_secs_to_ymd(secs);
    let day_secs = (secs % 86400) as u32;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Converts epoch seconds to (year, month, day).
fn epoch_secs_to_ymd(secs: u64) -> (i32, u32, u32) {
    let mut days = (secs / 86400) as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year: i64 = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_lengths: [i64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &ml in &month_lengths {
        if days < ml {
            break;
        }
        days -= ml;
        month += 1;
    }

    (year, month, days as u32 + 1)
}

/// Returns `true` if the given year is a leap year.
fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ─────────────────────────────────────────────
// RuntimeContext
// ─────────────────────────────────────────────

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};

use crate::llm::LlmProxy;

/// Contexte d'exécution exposé à l'agent Python via `run(task, ctx)`.
///
/// Construit par le runtime pour chaque exécution d'agent. Expose les
/// capacités optionnelles du runtime :
/// - `ctx.tools` — [`ToolProxy`] si des outils sont alloués à l'agent, `None` sinon.
/// - `ctx.llm` — [`LlmProxy`] si au moins un backend LLM est disponible,
///   `None` sinon (Principe #6 — l'agent choisit si l'absence est fatale).
/// - `ctx.memory` — réservé, toujours `None` en mode direct (STORY-028 à venir).
///
/// L'absence de LLM est signalée sur l'EventBus via `AgentDegraded`
/// dès la construction (Principe #4 — fail fast).
#[pyclass(name = "RuntimeContext")]
pub struct RuntimeContext {
    /// Proxy outils exposé à Python — `None` si aucun outil alloué.
    tools: Option<pyo3::Py<ToolProxy>>,
    /// Proxy LLM exposé à Python — `None` si aucun backend LLM disponible.
    pub llm: Option<LlmProxy>,
    /// Interface mémoire — réservée, toujours `None` en mode direct.
    memory: Option<pyo3::Py<crate::memory::MemoryInterface>>,
}

impl RuntimeContext {
    /// Construit le contexte avec injection LLM optionnelle et ToolProxy optionnel.
    ///
    /// Si `llm_router` est `None` ou contient un router sans backend,
    /// `ctx.llm` est `None` et `RuntimeEvent::AgentDegraded` est émis
    /// fire-and-forget sur `event_bus` (erreurs `send()` silencieusement ignorées).
    ///
    /// Si `tool_proxy` est `Some`, `ctx.tools` expose les outils alloués à l'agent.
    ///
    /// Le contexte ne panic jamais à la construction : la dégradation est
    /// signalée, mais l'agent décide lui-même si l'absence de LLM est fatale.
    pub fn new_with_llm(
        llm_router: Option<Arc<LlmRouter>>,
        budget_view: Arc<StepBudgetView>,
        tool_helper: Arc<ToolCallHelper>,
        obs_config: Arc<ObservabilityConfig>,
        event_bus: EventBusSender,
        agent_id: AgentId,
        tool_proxy: Option<ToolProxy>,
    ) -> Self {
        let llm = llm_router.and_then(|router| {
            if router.list().is_empty() {
                // AC-3: fire-and-forget — erreurs send() silencieusement ignorées.
                let _ = event_bus.send(RuntimeEvent::AgentDegraded {
                    agent_id,
                    reason: "no LLM backend available".into(),
                });
                None
            } else {
                Some(LlmProxy::new(
                    router,
                    tool_helper,
                    budget_view,
                    obs_config,
                    Some(event_bus),
                ))
            }
        });

        // Wrap ToolProxy in a Py<> handle so it can be cheaply cloned in the getter.
        let tools = tool_proxy.and_then(|proxy| {
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, proxy).ok())
        });

        Self { llm, tools, memory: None }
    }
}

#[pymethods]
impl RuntimeContext {
    /// Proxy outils injecté — `None` si aucun outil alloué.
    ///
    /// Propriété Python `ctx.tools`. Retourne `None` Python (pas d'exception)
    /// si l'agent n'a pas de `tools_required` ou que la factory n'a pas fourni de proxy.
    #[getter]
    fn tools(&self, py: Python<'_>) -> PyObject {
        match &self.tools {
            Some(proxy) => proxy.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Proxy LLM injecté — `None` si aucun backend LLM disponible.
    ///
    /// Propriété Python `ctx.llm`. Retourne `None` Python (pas d'exception)
    /// si le runtime a démarré sans backend LLM configuré ou disponible.
    #[getter]
    fn llm(&self, py: Python<'_>) -> PyObject {
        match &self.llm {
            Some(proxy) => Py::new(py, proxy.clone())
                .map(|p| p.into_any())
                .unwrap_or_else(|_| py.None()),
            None => py.None(),
        }
    }

    /// Interface mémoire — `None` en mode direct (accès mémoire via STORY-028).
    ///
    /// Propriété Python `ctx.memory`. Toujours `None` tant que le MemoryManager
    /// n'est pas injecté dans le contexte d'exécution.
    #[getter]
    fn memory(&self, py: Python<'_>) -> PyObject {
        match &self.memory {
            Some(mem) => mem.clone_ref(py).into_any(),
            None => py.None(),
        }
    }
}

#[cfg(test)]
mod runtime_context_tests {
    use super::*;
    use apollia_core::events::RuntimeEvent;
    use apollia_llm::{ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker};
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    use apollia_llm::{
        CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
        TokenUsage,
    };
    use futures::Stream;

    // ── Mocks pour la construction du ToolCallHelper (jamais réellement appelés) ──

    struct NoopModel;

    #[async_trait::async_trait]
    impl CompletionModel for NoopModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: String::new(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "noop"
        }
        fn model_id(&self) -> &str {
            "noop"
        }
    }

    struct NoopInvoker;

    #[async_trait::async_trait]
    impl ToolInvoker for NoopInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(String::new())
        }
    }

    fn make_tool_helper() -> Arc<ToolCallHelper> {
        Arc::new(ToolCallHelper::new(
            Arc::new(NoopModel),
            Arc::new(NoopInvoker),
        ))
    }

    /// AC-2 — `ctx.llm` est `None` si le router n'a aucun backend.
    #[tokio::test]
    async fn test_ac2_ctx_llm_none_if_no_backends() {
        // GIVEN un LlmRouter vide (0 backends)
        let router = Arc::new(LlmRouter::empty());
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        // WHEN
        let ctx = RuntimeContext::new_with_llm(
            Some(router),
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            AgentId::new_v4(),
            None,
        );
        // THEN
        assert!(ctx.llm.is_none());
    }

    /// AC-3 — `AgentDegraded` émis sur EventBus si aucun backend LLM.
    #[tokio::test]
    async fn test_ac3_agent_degraded_emitted_if_no_llm() {
        // GIVEN un router vide et un bus avec receiver
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let router = Arc::new(LlmRouter::empty());
        let agent_id = AgentId::new_v4();
        // WHEN
        let _ctx = RuntimeContext::new_with_llm(
            Some(router),
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            agent_id,
            None,
        );
        // THEN un événement AgentDegraded est présent sur le bus
        let event = rx.try_recv().expect("un événement doit être présent");
        assert!(
            matches!(
                event,
                RuntimeEvent::AgentDegraded { ref reason, .. }
                    if reason.contains("no LLM backend")
            ),
            "événement inattendu : {event:?}"
        );
    }

    /// AC-2 (variante) — `ctx.llm` est `None` si `llm_router` est `None`.
    #[tokio::test]
    async fn test_ac2_ctx_llm_none_if_router_option_is_none() {
        // GIVEN llm_router = None (Supervisor n'a pas pu initialiser le LLM)
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        // WHEN
        let ctx = RuntimeContext::new_with_llm(
            None,
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            AgentId::new_v4(),
            None,
        );
        // THEN
        assert!(ctx.llm.is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SandboxProfile;
    use apollia_tools::{ToolDescriptor, ToolKind};

    /// Mock executor that returns a configurable result.
    struct MockExecutor {
        result: Result<serde_json::Value, String>,
    }

    impl ToolExecutor for MockExecutor {
        fn execute(
            &self,
            _tool_name: &str,
            _input: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            self.result.clone()
        }
    }

    fn file_io_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_io".to_string(),
            version: "1.0.0".to_string(),
            description: "File system operations".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
        }
    }

    fn bash_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
        }
    }

    async fn open_test_audit() -> AuditTrailHandle {
        let db_path =
            std::env::temp_dir().join(format!("apollia_proxy_test_{}.db", uuid::Uuid::new_v4()));
        AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit trail")
    }

    async fn make_proxy(
        allowed_tools: Vec<&str>,
        executor_result: Result<serde_json::Value, String>,
    ) -> (ToolProxy, ToolRegistryHandle, AuditTrailHandle) {
        let registry = ToolRegistryHandle::start();
        let audit = open_test_audit().await;
        let executor = Arc::new(MockExecutor {
            result: executor_result,
        });

        let proxy = ToolProxy::new(
            registry.clone(),
            audit.clone(),
            executor,
            allowed_tools.into_iter().map(String::from).collect(),
            "test-agent".to_string(),
            "task-001".to_string(),
        );

        (proxy, registry, audit)
    }

    // AC-1: Nominal tool call
    #[tokio::test]
    async fn test_call_tool_nominal() {
        // GIVEN a ToolProxy with "file_io" allowed and registered
        let expected = serde_json::json!({"files": ["a.txt", "b.txt"]});
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(expected.clone())).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let input = serde_json::json!({"action": "list", "path": "."});
        let result = proxy.call_inner("file_io", input).await;

        // THEN the result is Ok with the executor's output
        assert!(result.is_ok());
        assert_eq!(result.expect("should succeed"), expected);

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // AC-2: Tool not found in registry
    #[tokio::test]
    async fn test_call_tool_not_found() {
        // GIVEN a ToolProxy with "inexistant" allowed but NOT in registry
        let (proxy, registry, audit) =
            make_proxy(vec!["inexistant"], Ok(serde_json::json!({}))).await;

        // WHEN we call "inexistant"
        let result = proxy.call_inner("inexistant", serde_json::json!({})).await;

        // THEN we get ToolNotFound
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotFound(ref name)) if name == "inexistant")
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // AC-3: Tool not allowed for this agent
    #[tokio::test]
    async fn test_call_tool_not_allowed() {
        // GIVEN a ToolProxy with allowed_tools = ["file_io"]
        //   and "bash_executor" registered in the registry
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        registry
            .register(bash_descriptor())
            .await
            .expect("register failed");

        // WHEN we call "bash_executor" (exists but not allowed)
        let result = proxy
            .call_inner("bash_executor", serde_json::json!({}))
            .await;

        // THEN we get ToolNotAllowed
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotAllowed(ref name)) if name == "bash_executor")
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // AC-4: Tool execution failed
    #[tokio::test]
    async fn test_call_tool_execution_failed() {
        // GIVEN a ToolProxy with an executor that fails
        let (proxy, registry, audit) =
            make_proxy(vec!["file_io"], Err("permission denied".to_string())).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let result = proxy
            .call_inner("file_io", serde_json::json!({"action": "read"}))
            .await;

        // THEN we get ExecutionFailed with the error message
        assert!(
            matches!(result, Err(ToolProxyError::ExecutionFailed(ref msg)) if msg.contains("permission denied"))
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // AC-5: Audit trail records each call
    #[tokio::test]
    async fn test_call_records_audit_trail() {
        // GIVEN a ToolProxy with a connected AuditTrailHandle
        let (proxy, registry, audit) =
            make_proxy(vec!["file_io"], Ok(serde_json::json!({"ok": true}))).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let input = serde_json::json!({"action": "list", "path": "."});
        let input_hash = compute_input_hash(&input);
        let _ = proxy.call_inner("file_io", input).await;

        // Wait for fire-and-forget audit to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // THEN an audit record is stored
        let records = audit.query_last(1).await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool_name, "file_io");
        assert_eq!(records[0].agent_id, "test-agent");
        assert_eq!(records[0].task_id, "task-001");
        assert_eq!(records[0].input_hash, input_hash);
        assert!(records[0].success);
        assert!(records[0].duration_ms.is_some());

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // AC-6: Tool call counter increments
    #[tokio::test]
    async fn test_tool_call_count_increments() {
        // GIVEN a ToolProxy with tool_calls = 0
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        assert_eq!(proxy.tool_call_count(), 0);

        // WHEN we call call_inner() 3 times
        for _ in 0..3 {
            let _ = proxy.call_inner("file_io", serde_json::json!({})).await;
        }

        // THEN tool_call_count() returns 3
        assert_eq!(proxy.tool_call_count(), 3);

        registry.shutdown().await;
        audit.shutdown().await;
    }
}
