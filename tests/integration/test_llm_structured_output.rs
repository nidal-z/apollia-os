//! End-to-end tests: `ctx.llm.complete(..., schema=...)` on a live local model.
//!
//! The whole structured-output path, from a Python agent down to the embedded
//! `llama-server` and back: the schema crosses into Rust, becomes a GBNF
//! grammar, constrains decoding, and the answer is validated against the same
//! schema before it becomes a Python object.
//!
//! These tests need a running OpenAI-compatible server and a loaded model, so
//! they are opt-in, on the same principle as tracks 2 and 3 of the CLI suite.
//! Without `APOLLIA_TEST_LLAMA_URL` they report that they measured nothing and
//! return, rather than passing over an absent subject.
//!
//! ```sh
//! target/debug/runners/llama-server -m ~/.apollia/models/<model>.gguf \
//!     --port 8899 --host 127.0.0.1 -c 4096 -ngl 99 --jinja &
//! APOLLIA_TEST_LLAMA_URL=http://127.0.0.1:8899/v1 \
//!   PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
//!   cargo test -p apollia-e2e-tests --features python-tests \
//!     --test test_llm_structured_output -- --nocapture
//! ```

use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use apollia_aip::llm::LlmProxy;
use apollia_llm::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};
use apollia_llm::{
    CompletionModel, LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};

/// Environment variable naming a live OpenAI-compatible base URL, API prefix
/// included (`http://127.0.0.1:8899/v1`).
const LIVE_URL_VAR: &str = "APOLLIA_TEST_LLAMA_URL";

/// Tool invoker the helper requires but no test here reaches: these calls carry
/// no tools.
struct NoTools;

#[async_trait::async_trait]
impl ToolInvoker for NoTools {
    async fn invoke(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err(format!("no tool is wired in this test: {tool_name}"))
    }
}

/// The live URL, or `None` when the suite must report that it measured nothing.
fn live_url() -> Option<String> {
    match std::env::var(LIVE_URL_VAR) {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ => {
            println!(
                "skipped: {LIVE_URL_VAR} is unset, so no local model was reached \
                 and nothing here was measured"
            );
            None
        }
    }
}

/// An `LlmProxy` bound to the live server, declaring the llama.cpp extension so
/// the GBNF grammar this workspace builds is the constraint actually applied.
fn live_proxy(url: &str) -> LlmProxy {
    let config = ApiBackendConfig {
        name: "local".to_string(),
        api_url: url.to_string(),
        api_key_env: String::new(),
        model: "local".to_string(),
        context_window: None,
        llama_cpp_extensions: true,
    };
    let client: Arc<dyn CompletionModel> = Arc::new(OpenAICompatibleClient::new(
        &config,
        "sk-no-key".to_string(),
        tokio_util::sync::CancellationToken::new(),
    ));
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert("local".to_string(), Arc::clone(&client));
    let router = Arc::new(LlmRouter::with_backends(backends, "local"));
    let helper = Arc::new(ToolCallHelper::new(client, Arc::new(NoTools)));
    let budget = Arc::new(StepBudgetView::new(Arc::new(AtomicU32::new(0)), u32::MAX));
    LlmProxy::new(
        router,
        helper,
        budget,
        Arc::new(ObservabilityConfig::default()),
        None,
    )
}

/// Run one `ctx.llm.complete(messages, schema=...)` through Python and return
/// the repr of the value it resolved to, or the repr of the exception it raised.
///
/// Going through `asyncio.run` rather than awaiting in Rust is deliberate: the
/// awaitable a Python agent receives is the subject, not the Rust future behind
/// it.
async fn call_with_schema(
    proxy: &LlmProxy,
    prompt: &str,
    schema_json: &str,
) -> Result<String, String> {
    let code = concat!(
        "import asyncio, json\n",
        "def run(proxy, prompt, schema_json):\n",
        "    schema = json.loads(schema_json)\n",
        "    messages = [{'role': 'user', 'content': prompt}]\n",
        "    async def go():\n",
        "        return await proxy.complete(messages, schema=schema, max_tokens=400, temperature=0.0)\n",
        "    return asyncio.run(go())\n",
    );
    // The GIL is released for the duration of the call by `asyncio.run`, which
    // drives the pyo3-async-runtimes future on the Tokio runtime this test owns.
    let handle = tokio::task::spawn_blocking({
        let proxy = proxy.clone();
        let code = code.to_string();
        let prompt = prompt.to_string();
        let schema_json = schema_json.to_string();
        move || {
            Python::with_gil(|py| {
                let ns = PyDict::new(py);
                let source = std::ffi::CString::new(code).map_err(|e| e.to_string())?;
                py.run(source.as_c_str(), Some(&ns), Some(&ns))
                    .map_err(|e| format!("test harness code failed: {e}"))?;
                let run = ns
                    .get_item("run")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "run is not defined".to_string())?;
                let proxy_obj = Py::new(py, proxy).map_err(|e| e.to_string())?;
                match run.call1((proxy_obj, prompt, schema_json)) {
                    Ok(value) => Ok(format!("{value:?}")),
                    Err(err) => Err(format!(
                        "{}: {}",
                        err.get_type(py)
                            .name()
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                        err
                    )),
                }
            })
        }
    });
    handle.await.unwrap_or_else(|e| Err(format!("join: {e}")))
}

/// A flat object comes back as a validated Python dictionary.
#[tokio::test]
async fn test_flat_object_schema_returns_a_validated_object() {
    // GIVEN a live local model and a flat object schema
    let Some(url) = live_url() else { return };
    let proxy = live_proxy(&url);
    let schema = r#"{"type":"object","properties":{"title":{"type":"string"},
        "count":{"type":"integer"}},"required":["title","count"]}"#;

    // WHEN a Python agent completes with that schema
    let value = call_with_schema(&proxy, "Invent a report title and a count.", schema)
        .await
        .expect("a flat object schema must resolve");

    // THEN both declared members are present in the value handed to Python
    assert!(value.contains("title"), "no title in {value}");
    assert!(value.contains("count"), "no count in {value}");
    println!("flat object: {value}");
}

/// An object holding an array of objects, the shape the tool grammar degrades.
#[tokio::test]
async fn test_array_of_objects_schema_returns_a_validated_object() {
    // GIVEN a live local model and a nested schema
    let Some(url) = live_url() else { return };
    let proxy = live_proxy(&url);
    let schema = r#"{"type":"object","properties":{"rows":{"type":"array",
        "items":{"type":"object","properties":{"label":{"type":"string"},
        "n":{"type":"integer"}},"required":["label","n"]}}},"required":["rows"]}"#;

    // WHEN a Python agent completes with that schema
    let value = call_with_schema(&proxy, "List two fruits, each with a number.", schema)
        .await
        .expect("an array of objects must resolve");

    // THEN the nested members are typed rather than degraded to free values
    assert!(value.contains("rows"), "no rows in {value}");
    assert!(value.contains("label"), "no nested label in {value}");
    println!("array of objects: {value}");
}

/// An enumeration of strings comes back as one of its members.
#[tokio::test]
async fn test_string_enumeration_returns_one_member() {
    // GIVEN a live local model and an enumeration of three strings
    let Some(url) = live_url() else { return };
    let proxy = live_proxy(&url);
    let schema = r#"{"type":"string","enum":["low","medium","high"]}"#;

    // WHEN a Python agent completes with that schema
    let value = call_with_schema(
        &proxy,
        "How urgent is a server on fire? Answer with one word.",
        schema,
    )
    .await
    .expect("an enumeration must resolve");

    // THEN the answer is one of the three, and nothing else
    let picked = ["'low'", "'medium'", "'high'"]
        .iter()
        .filter(|member| value.contains(*member))
        .count();
    assert_eq!(picked, 1, "expected exactly one enum member, got {value}");
    println!("enumeration: {value}");
}

/// A schema the model cannot satisfy raises the typed error, not a string.
#[tokio::test]
async fn test_contradictory_schema_raises_the_typed_error() {
    // GIVEN a schema whose own constraints contradict each other: the single
    // enumerated value is shorter than the minimum length it demands. The
    // grammar admits the value, because no grammar expresses a length; the
    // schema refuses it.
    let Some(url) = live_url() else { return };
    let proxy = live_proxy(&url);
    let schema = r#"{"type":"string","enum":["a"],"minLength":5}"#;

    // WHEN a Python agent completes with that schema
    let err = call_with_schema(&proxy, "Answer.", schema)
        .await
        .expect_err("a contradictory schema cannot resolve");

    // THEN the agent sees the SDK's typed exception, carrying its path, rather
    // than a message to parse
    assert!(
        err.contains("StructuredOutputError"),
        "expected the typed SDK exception, got: {err}"
    );
    println!("contradictory schema: {err}");
}
