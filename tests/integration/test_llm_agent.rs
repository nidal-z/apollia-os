//! End-to-end tests - Python agent using `ctx.llm`.
//!
//! Checks the full LLM cycle from a Python agent:
//! - an agent calling `ctx.llm.chat()` with a Python mock -> Completed;
//! - `LlmCallCompleted` emitted on the EventBus through `LlmRouter`;
//! - a chunk stream from a mock CompletionModel;
//! - the full ReAct loop with a Python `run_tools` mock.
//!
//! Requires `--features python-tests`. Run it with:
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
// Mock CompletionModel - plain text answer
// ─────────────────────────────────────────────

/// Mock LLM returning fixed content with `FinishReason::Stop`.
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

/// A Python agent with a Python mock ctx completes successfully.
///
/// Exercises the chain: AIPBridge -> agent.run(task, ctx) -> AIPResult::Completed.
/// The ctx is a pure Python object (not the Rust RuntimeContext), with a Python `llm` mock.
#[tokio::test]
async fn test_agent_llm_chat_completed() {
    // GIVEN a decorated Python agent calling ctx.llm.chat()
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

    // AND a Python ctx with a native Python LLM mock
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
        // globals == locals, so MockLlm is visible inside MockCtx
        let ns = pyo3::types::PyDict::new(py);
        let code = std::ffi::CString::new(mock_ctx_code).expect("mock ctx code contains NUL byte");
        py.run(code.as_c_str(), Some(&ns), Some(&ns))
            .expect("mock ctx code should execute without error");
        ns.get_item("ctx_instance")
            .expect("get_item should not raise")
            .expect("ctx_instance must be defined")
            .unbind()
    });

    // WHEN a task is submitted to the agent
    let task = default_task();
    let result = bridge
        .call_run(&task, ctx)
        .await
        .expect("call_run should succeed");

    // THEN the final status is Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "task status must be Completed: {result:?}"
    );
}

/// `LlmCallCompleted` is emitted on the EventBus after an LLM call.
///
/// Exercises the observability layer of `LlmRouter::complete_with_observability`.
/// Pure Rust test - no Python.
#[tokio::test]
async fn test_llm_call_completed_event_emitted() {
    // GIVEN an LlmRouter with a mock backend and an EventBus
    let (backend, _count) = MockLlmBackend::new("mock response");
    let router = make_router_with_mock(backend);

    let (event_sender, _rx) = EventBus::new();
    let mut event_rx = event_sender.subscribe();

    let obs = ObservabilityConfig::default();
    let req = apollia_llm::CompletionRequest {
        messages: vec![apollia_llm::ChatMessage::user("test")],
        ..Default::default()
    };

    // WHEN complete_with_observability is called
    let result = router
        .complete_with_observability(None, req, Some(&event_sender), &obs)
        .await;

    // THEN Ok is returned
    assert!(
        result.is_ok(),
        "complete_with_observability must succeed: {result:?}"
    );

    // AND LlmCallCompleted is emitted on the bus with backend == "mock"
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
    .expect("LlmCallCompleted must be emitted within the deadline");
}

/// The stream of a mock CompletionModel returns the expected chunks.
///
/// Exercises the streaming mode from the public API - pure Rust, no Python.
#[tokio::test]
async fn test_llm_stream_yields_chunks() {
    use futures::StreamExt;

    // GIVEN a mock backend streaming 2 chunks
    let (backend, _count) = MockLlmBackend::new("ignored for stream");
    let req = apollia_llm::CompletionRequest {
        messages: vec![apollia_llm::ChatMessage::user("test stream")],
        ..Default::default()
    };

    // WHEN stream() is called
    let stream = backend.stream(req).await.expect("stream() must succeed");

    let chunks: Vec<StreamChunk> = stream
        .map(|r| r.expect("the chunk must be Ok"))
        .collect()
        .await;

    // THEN 2 chunks come back, in order
    assert_eq!(chunks.len(), 2, "2 chunks attendus");
    assert!(matches!(&chunks[0], StreamChunk::Text(t) if t == "chunk1"));
    assert!(matches!(&chunks[1], StreamChunk::Text(t) if t == "chunk2"));
}

/// Python agent calling `ctx.llm.run_tools()` on a Python mock -> Completed.
///
/// Simulates the full ReAct cycle in pure Python: 2 LLM calls, 1 tool call.
/// Checks that the agent returns `Completed` after the loop.
#[tokio::test]
async fn test_run_tools_full_react_cycle() {
    // GIVEN a decorated Python agent calling ctx.llm.run_tools()
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

    // AND a mock Python ctx whose run_tools simulates 1 ToolCall then Stop
    let mock_ctx_code = concat!(
        "class MockLlm:\n",
        "    def __init__(self):\n",
        "        self.llm_call_count = 0\n",
        "        self.tool_call_count = 0\n",
        "    async def run_tools(self, messages, tools, max_iterations=5):\n",
        "        import asyncio\n",
        "        # Simulates: 1st LLM call -> 1 ToolCall, 2nd call -> Stop\n",
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

    // WHEN the task is submitted
    let task = default_task();
    let result = bridge
        .call_run(&task, ctx)
        .await
        .expect("call_run should succeed");

    // THEN the final status is Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "the ReAct agent must finish with Completed: {result:?}"
    );
}
