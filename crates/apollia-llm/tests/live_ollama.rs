//! Live checks of the native Ollama client against a running server.
//!
//! Ignored by default: they need Ollama on 127.0.0.1:11434 with the model named
//! by `APOLLIA_TEST_OLLAMA_MODEL` pulled (default `qwen3.5`). They prove the one
//! thing the offline tests cannot, which is that the server honours the window
//! the client asks for, since the whole point of the native client is that the
//! OpenAI-compatible endpoint did not.
//!
//! ```sh
//! cargo test -p apollia-llm --test live_ollama -- --ignored --nocapture
//! ```

#![cfg(feature = "cloud")]

use apollia_llm::backends::ollama::{OllamaClient, OllamaConfig, DEFAULT_NUM_CTX};
use apollia_llm::types::{
    ChatMessage, CompletionModel, CompletionRequest, StreamChunk, ToolCall, ToolSpec,
};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

const ROOT: &str = "http://127.0.0.1:11434";

fn model() -> String {
    std::env::var("APOLLIA_TEST_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3.5".to_owned())
}

fn tool() -> ToolSpec {
    ToolSpec {
        name: "fs_read_dir".into(),
        description: "List the entries of a local directory".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }
}

async fn loaded_window(model: &str) -> Option<u64> {
    let ps: serde_json::Value = reqwest::get(format!("{ROOT}/api/ps"))
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    ps["models"].as_array()?.iter().find_map(|m| {
        let name = m["name"].as_str()?;
        (name == model || name.strip_suffix(":latest") == Some(model))
            .then(|| m["context_length"].as_u64())
            .flatten()
    })
}

async fn collect(client: &OllamaClient, req: CompletionRequest) -> (String, Vec<ToolCall>, u32) {
    let mut stream = client
        .stream(req)
        .await
        .expect("the server accepts the call");
    let (mut text, mut calls, mut prompt_tokens) = (String::new(), Vec::new(), 0);
    while let Some(item) = stream.next().await {
        match item.expect("a clean stream") {
            StreamChunk::Text(t) => text.push_str(&t),
            StreamChunk::ToolCall(c) => calls.push(c),
            StreamChunk::Usage(u) => prompt_tokens = u.prompt_tokens,
            StreamChunk::Timings(_) => {}
        }
    }
    (text, calls, prompt_tokens)
}

#[tokio::test]
#[ignore = "needs a running Ollama; run with --ignored"]
async fn the_server_runs_at_the_window_the_client_asks_for() {
    // GIVEN the window the client chooses for this model
    let model = model();
    let num_ctx = OllamaClient::num_ctx(ROOT, &model, None).await;
    assert!(num_ctx <= DEFAULT_NUM_CTX);
    let client = OllamaClient::new(
        OllamaConfig {
            name: "ollama".into(),
            root: ROOT.into(),
            model: model.clone(),
            num_ctx,
        },
        CancellationToken::new(),
        std::time::Duration::from_secs(300),
    );

    // WHEN a turn that should call a tool is streamed
    let history = vec![
        ChatMessage::system("You are an assistant with access to the local file system."),
        ChatMessage::user("List the files in C:/Users/example/Downloads"),
    ];
    let (text, calls, prompt_tokens) = collect(
        &client,
        CompletionRequest {
            messages: history.clone(),
            tools: vec![tool()],
            max_tokens: Some(512),
            ..Default::default()
        },
    )
    .await;
    println!("window {num_ctx}, prompt {prompt_tokens} tokens, text {text:?}, calls {calls:?}");

    // THEN the model saw its tool and called it, and the server loaded the
    // model at the requested window rather than at its own 4096
    assert!(!calls.is_empty(), "the tool schema did not reach the model");
    assert_eq!(calls[0].name, "fs_read_dir");
    assert_eq!(loaded_window(&model).await, Some(u64::from(num_ctx)));

    // WHEN the result is sent back
    let mut follow_up = history;
    follow_up.push(ChatMessage::assistant_with_calls("", &calls));
    follow_up.push(ChatMessage::tool_result(
        &calls[0].id,
        "report.pdf\nphoto.jpg\nsetup.exe",
    ));
    let (answer, _, _) = collect(
        &client,
        CompletionRequest {
            messages: follow_up,
            tools: vec![tool()],
            max_tokens: Some(512),
            ..Default::default()
        },
    )
    .await;
    println!("answer {answer:?}");

    // THEN the answer uses the result, and any reasoning arrives bracketed in
    // the pipeline's own tags rather than as loose text
    let visible = apollia_llm::reasoning_markers::strip_reasoning(&answer);
    assert!(
        visible.contains("report.pdf"),
        "the history did not reach the model"
    );
    assert_eq!(
        answer.matches("<think>").count(),
        answer.matches("</think>").count()
    );
}
