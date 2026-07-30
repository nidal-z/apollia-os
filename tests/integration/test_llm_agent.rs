//! Tests e2e - agent Python avec `ctx.llm`.
//!
//! Vérifie le cycle complet LLM depuis un agent Python :
//! - agent qui appelle `ctx.llm.chat()` avec un mock Python → Completed ;
//! - `LlmCallCompleted` émis sur l'EventBus via `LlmRouter` ;
//! - stream de chunks depuis un mock CompletionModel ;
//! - boucle ReAct complète avec mock `run_tools` Python.
//!
//! Nécessite `--features python-tests`. À exécuter avec :
//!   PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
//!   cargo test -p apollia-e2e-tests --features python-tests -- --nocapture

use std::collections::HashMap;
use std::io::Write as _;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use futures::Stream;
use pyo3::prelude::*;

use apollia_aip::{bridge::AIPBridge, loader::load_agent_module, validator::validate_agent};
use apollia_core::{AIPTask, RuntimeEvent, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    ObservabilityConfig, StreamChunk, TokenUsage,
};
use apollia_runtime::eventbus::EventBus;

// ─────────────────────────────────────────────
// Mock CompletionModel - réponse textuelle simple
// ─────────────────────────────────────────────

/// Mock LLM qui retourne un contenu fixe avec `FinishReason::Stop`.
struct MockLlmBackend {
    response: String,
    call_count: Arc<AtomicU32>,
}

impl MockLlmBackend {
    fn new(response: impl Into<String>) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                response: response.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockLlmBackend {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.response.clone(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                cost_usd: Some(0.001),
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let chunks = vec![
            StreamChunk::Text("chunk1".to_owned()),
            StreamChunk::Text("chunk2".to_owned()),
        ];
        let stream = futures::stream::iter(chunks.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn make_router_with_mock(backend: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert("mock".to_owned(), backend);
    Arc::new(LlmRouter::with_backends(backends, "mock"))
}

fn default_task() -> AIPTask {
    AIPTask::default()
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

/// Un agent Python avec un mock ctx Python complète avec succès.
///
/// Teste la chaîne : AIPBridge → agent.run(task, ctx) → AIPResult::Completed.
/// Le ctx est un objet Python pur (pas RuntimeContext Rust) avec un `llm` mock Python.
#[tokio::test]
async fn test_agent_llm_chat_completed() {
    // GIVEN un agent Python decorateur qui appelle ctx.llm.chat()
    let agent_code = concat!(
        "from apollia import agent, on_message\n",
        "\n",
        "@agent(name='llm-chat-agent', version='1.0.0', description='test')\n",
        "class LlmChatAgent:\n",
        "    @on_message\n",
        "    async def handle(self, message, history, ctx):\n",
        "        response = await ctx.llm.chat(system='', user='hello')\n",
        "        return response.content\n",
        "\n",
        "agent = LlmChatAgent()\n"
    );

    let mut tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("failed to create temp file");
    tmp.write_all(agent_code.as_bytes())
        .expect("failed to write agent code");

    let agent_obj = load_agent_module(tmp.path()).expect("load_agent_module should succeed");
    let validated = validate_agent(&agent_obj).expect("agent should pass validation");
    let bridge = Arc::new(AIPBridge::new(validated).expect("AIPBridge init failed"));

    // AND un ctx Python avec un mock LLM natif Python
    let mock_ctx_code = concat!(
        "class MockLlm:\n",
        "    async def chat(self, system='', user='', backend=None):\n",
        "        import asyncio\n",
        "        await asyncio.sleep(0)\n",
        "        class R:\n",
        "            content = 'mock:' + user\n",
        "            latency_ms = 0\n",
        "        return R()\n",
        "\n",
        "class MockCtx:\n",
        "    def __init__(self):\n",
        "        self.llm = MockLlm()\n",
        "\n",
        "ctx_instance = MockCtx()\n"
    );

    let ctx: PyObject = Python::with_gil(|py| -> PyObject {
        // globals == locals pour que MockLlm soit visible dans MockCtx
        let ns = pyo3::types::PyDict::new(py);
        let code = std::ffi::CString::new(mock_ctx_code).expect("mock ctx code contains NUL byte");
        py.run(code.as_c_str(), Some(&ns), Some(&ns))
            .expect("mock ctx code should execute without error");
        ns.get_item("ctx_instance")
            .expect("get_item should not raise")
            .expect("ctx_instance must be defined")
            .unbind()
    });

    // WHEN une tâche est soumise à l'agent
    let task = default_task();
    let result = bridge
        .call_run(&task, ctx)
        .await
        .expect("call_run should succeed");

    // THEN le statut final est Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "task status must be Completed: {result:?}"
    );
}

/// `LlmCallCompleted` est émis sur l'EventBus après un appel LLM.
///
/// Teste la couche d'observabilité de `LlmRouter::complete_with_observability`.
/// Test Rust pur - pas de Python.
#[tokio::test]
async fn test_llm_call_completed_event_emitted() {
    // GIVEN un LlmRouter avec un mock backend et un EventBus
    let (backend, _count) = MockLlmBackend::new("mock response");
    let router = make_router_with_mock(backend);

    let (event_sender, _rx) = EventBus::new();
    let mut event_rx = event_sender.subscribe();

    let obs = ObservabilityConfig::default();
    let req = apollia_llm::CompletionRequest {
        messages: vec![apollia_llm::ChatMessage::user("test")],
        ..Default::default()
    };

    // WHEN complete_with_observability est appelé
    let result = router
        .complete_with_observability(None, req, Some(&event_sender), &obs)
        .await;

    // THEN Ok est retourné
    assert!(
        result.is_ok(),
        "complete_with_observability doit réussir : {result:?}"
    );

    // ET LlmCallCompleted est émis sur le bus avec backend == "mock"
    tokio::time::timeout(Duration::from_millis(100), async {
        loop {
            match event_rx.try_recv() {
                Ok(RuntimeEvent::LlmCallCompleted { ref backend, .. }) if backend == "mock" => {
                    return;
                }
                Ok(_) => continue,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        }
    })
    .await
    .expect("LlmCallCompleted doit être émis dans le délai imparti");
}

/// Le stream d'un mock CompletionModel retourne les chunks attendus.
///
/// Teste le mode streaming depuis l'API publique - Rust pur, pas de Python.
#[tokio::test]
async fn test_llm_stream_yields_chunks() {
    use futures::StreamExt;

    // GIVEN un mock backend avec stream de 2 chunks
    let (backend, _count) = MockLlmBackend::new("ignored for stream");
    let req = apollia_llm::CompletionRequest {
        messages: vec![apollia_llm::ChatMessage::user("test stream")],
        ..Default::default()
    };

    // WHEN stream() est appelé
    let stream = backend.stream(req).await.expect("stream() doit réussir");

    let chunks: Vec<StreamChunk> = stream
        .map(|r| r.expect("chunk doit être Ok"))
        .collect()
        .await;

    // THEN 2 chunks sont retournés dans l'ordre
    assert_eq!(chunks.len(), 2, "2 chunks attendus");
    assert!(matches!(&chunks[0], StreamChunk::Text(t) if t == "chunk1"));
    assert!(matches!(&chunks[1], StreamChunk::Text(t) if t == "chunk2"));
}

/// Agent Python appelant `ctx.llm.run_tools()` sur un mock Python → Completed.
///
/// Simule le cycle ReAct complet en Python pur : 2 appels LLM, 1 appel outil.
/// Vérifie que l'agent retourne `Completed` après la boucle.
#[tokio::test]
async fn test_run_tools_full_react_cycle() {
    // GIVEN un agent Python decorateur qui appelle ctx.llm.run_tools()
    let agent_code = concat!(
        "from apollia import agent, on_message\n",
        "\n",
        "@agent(name='run-tools-agent', version='1.0.0', description='ReAct test')\n",
        "class RunToolsAgent:\n",
        "    @on_message\n",
        "    async def handle(self, message, history, ctx):\n",
        "        result = await ctx.llm.run_tools(\n",
        "            messages=[{'role': 'user', 'content': 'test'}],\n",
        "            tools=[{'name': 'echo', 'description': 'test', 'parameters': {}}],\n",
        "            max_iterations=5,\n",
        "        )\n",
        "        return str(result)\n",
        "\n",
        "agent = RunToolsAgent()\n"
    );

    let mut tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("failed to create temp file");
    tmp.write_all(agent_code.as_bytes())
        .expect("failed to write agent code");

    let agent_obj = load_agent_module(tmp.path()).expect("load should succeed");
    let validated = validate_agent(&agent_obj).expect("validation should succeed");
    let bridge = Arc::new(AIPBridge::new(validated).expect("AIPBridge init failed"));

    // AND un mock ctx Python avec run_tools qui simule 1 ToolCall puis Stop
    let mock_ctx_code = concat!(
        "class MockLlm:\n",
        "    def __init__(self):\n",
        "        self.llm_call_count = 0\n",
        "        self.tool_call_count = 0\n",
        "    async def run_tools(self, messages, tools, max_iterations=5):\n",
        "        import asyncio\n",
        "        # Simule : 1er appel LLM → 1 ToolCall, 2ème appel → Stop\n",
        "        self.llm_call_count += 1\n",
        "        await asyncio.sleep(0)\n",
        "        self.tool_call_count += 1\n",
        "        self.llm_call_count += 1\n",
        "        return 'final result'\n",
        "\n",
        "class MockCtxReAct:\n",
        "    def __init__(self):\n",
        "        self.llm = MockLlm()\n",
        "\n",
        "ctx_react = MockCtxReAct()\n"
    );

    let ctx: PyObject = Python::with_gil(|py| -> PyObject {
        let ns = pyo3::types::PyDict::new(py);
        let code = std::ffi::CString::new(mock_ctx_code).expect("mock ctx code contains NUL byte");
        py.run(code.as_c_str(), Some(&ns), Some(&ns))
            .expect("mock ctx code should execute");
        ns.get_item("ctx_react")
            .expect("get_item should not raise")
            .expect("ctx_react must be defined")
            .unbind()
    });

    // WHEN la tâche est soumise
    let task = default_task();
    let result = bridge
        .call_run(&task, ctx)
        .await
        .expect("call_run should succeed");

    // THEN le statut final est Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "ReAct agent doit se terminer avec Completed : {result:?}"
    );
}
