use std::sync::atomic::AtomicUsize;
use std::sync::Mutex as StdMutex;

use apollia_llm::model_index::{IndexedModel, ModelCategory, ModelOrigin};
use apollia_llm::types::{ChatMessage, CompletionRequest};

use super::*;

/// An engine that records which models it was asked to serve and hands back
/// a lease on an address nothing listens on.
struct FakeEngine {
    engine: Engine,
    asked: StdMutex<Vec<String>>,
}

impl FakeEngine {
    fn new(engine: Engine) -> Arc<Self> {
        Arc::new(Self {
            engine,
            asked: StdMutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

impl LocalEngine for FakeEngine {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn default_n_ctx(&self) -> u32 {
        4096
    }

    fn launch_window(&self, _backend: &str, requested: Option<u32>) -> u32 {
        requested.unwrap_or(4096)
    }

    async fn ensure_model(
        &self,
        _backend: &str,
        model_path: String,
        n_ctx: Option<u32>,
    ) -> Result<ModelLease, LocalEngineError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(model_path);
        }
        Ok(ModelLease::new(
            "http://127.0.0.1:9/v1".to_owned(),
            n_ctx.unwrap_or(4096),
            Arc::new(AtomicUsize::new(0)),
        ))
    }

    async fn loaded_models(&self) -> Vec<LoadedModel> {
        Vec::new()
    }

    async fn unload_model(&self, model_path: &str) -> Result<usize, UnloadError> {
        Err(UnloadError::NotLoaded {
            model_path: model_path.to_owned(),
        })
    }

    fn max_loaded(&self) -> usize {
        1
    }

    fn reads_llama_cpp_grammar(&self) -> bool {
        self.engine == Engine::LlamaServer
    }

    fn request_model(&self, model_id: &str, _model_path: &str) -> String {
        model_id.to_owned()
    }
}

fn local_row(name: &str, model: &str) -> LlmBackendConfig {
    LlmBackendConfig {
        name: name.to_owned(),
        provider: LlmProvider::LlamaCpp,
        model: model.to_owned(),
        config_json: serde_json::json!({}),
        is_default: false,
        enabled: true,
    }
}

fn indexed(id: &str, format: ModelFormat, path: &Path) -> IndexedModel {
    IndexedModel {
        id: id.to_owned(),
        format,
        path: path.to_path_buf(),
        parts: Vec::new(),
        annexes: Vec::new(),
        size_bytes: 0,
        origin: ModelOrigin::Import,
        sha256: None,
        origin_verified: false,
        category: ModelCategory::Language,
        parameters: None,
        modified_at: None,
        pinned_at: None,
        category_override: None,
        source_file_override: None,
        capabilities: None,
    }
}

/// Send one request through `backend`; the fake address answers nothing, so
/// only the engine that was asked matters, and the error is returned.
async fn call(backend: &Arc<dyn CompletionModel>) -> Result<CompletionResponse, LlmError> {
    backend
        .complete(CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            ..Default::default()
        })
        .await
}

#[tokio::test]
async fn the_format_of_the_model_picks_the_engine() {
    // GIVEN two fake engines, one per format, and a GGUF row and an MLX row
    let llama = FakeEngine::new(Engine::LlamaServer);
    let mlx = FakeEngine::new(Engine::VllmMlx);
    let gguf_row = local_row("qwen-gguf", "/models/qwen.gguf");
    let mlx_row = local_row("qwen-mlx", "/models/qwen-mlx");

    // WHEN each row is routed and asked once
    let gguf = route_local(&gguf_row, ModelFormat::Gguf, Some(&llama), Some(&mlx), true)
        .expect("a GGUF backend");
    let mlx_backend = route_local(&mlx_row, ModelFormat::Mlx, Some(&llama), Some(&mlx), true)
        .expect("an MLX backend");
    let _ = call(&gguf).await;
    let _ = call(&mlx_backend).await;

    // THEN the GGUF model went to llama-server and the MLX one to vllm-mlx,
    // each engine seeing only its own model
    assert_eq!(engine_for_format(ModelFormat::Gguf), Engine::LlamaServer);
    assert_eq!(engine_for_format(ModelFormat::Mlx), Engine::VllmMlx);
    assert_eq!(llama.asked(), ["/models/qwen.gguf"]);
    assert_eq!(mlx.asked(), ["/models/qwen-mlx"]);
}

#[tokio::test]
async fn an_mlx_model_off_apple_silicon_fails_with_a_readable_reason() {
    // GIVEN both engines, and an MLX row on a machine that is not Apple Silicon
    let llama = FakeEngine::new(Engine::LlamaServer);
    let mlx = FakeEngine::new(Engine::VllmMlx);
    let row = local_row("qwen-mlx", "/models/qwen-mlx");

    // WHEN it is routed and asked
    let backend = route_local(&row, ModelFormat::Mlx, Some(&llama), Some(&mlx), false)
        .expect("a backend that refuses");
    let err = call(&backend).await.expect_err("MLX does not run here");

    // THEN the call fails with the reason, the platform out of scope, and no
    // engine was asked to load the folder
    let message = err.to_string();
    assert!(
        message.contains("only run on a Mac with Apple Silicon"),
        "{message}"
    );
    assert!(message.contains("out of scope"), "{message}");
    assert!(!backend.is_available());
    assert!(llama.asked().is_empty());
    assert!(mlx.asked().is_empty());
}

#[tokio::test]
async fn an_mlx_model_without_an_mlx_engine_says_the_engine_is_missing() {
    // GIVEN Apple Silicon with no MLX engine in this build
    let llama = FakeEngine::new(Engine::LlamaServer);
    let row = local_row("qwen-mlx", "/models/qwen-mlx");

    // WHEN the MLX row is routed and asked
    let backend =
        route_local::<FakeEngine, FakeEngine>(&row, ModelFormat::Mlx, Some(&llama), None, true)
            .expect("a backend that refuses");
    let err = call(&backend).await.expect_err("no engine");

    // THEN it says the MLX engine is not installed, and llama-server is not
    // handed a folder it cannot open
    assert!(
        err.to_string().contains("MLX engine is not installed"),
        "{err}"
    );
    assert!(llama.asked().is_empty());
}

#[test]
fn the_model_index_decides_the_format_and_the_disk_decides_for_an_unindexed_path() {
    // GIVEN an index holding an MLX folder and a GGUF file, and two paths it
    // does not know: a folder and a file
    let tmp = tempfile::tempdir().expect("tempdir");
    let models = tmp.path();
    let mlx_dir = models.join("qwen-mlx");
    let gguf = models.join("qwen.gguf");
    let stray_dir = models.join("stray-mlx");
    std::fs::create_dir_all(&mlx_dir).expect("mkdir");
    std::fs::create_dir_all(&stray_dir).expect("mkdir");
    std::fs::write(&gguf, b"GGUF").expect("write");
    let index = ModelIndex {
        models: vec![
            indexed("qwen-mlx", ModelFormat::Mlx, &mlx_dir),
            indexed("qwen-gguf", ModelFormat::Gguf, &gguf),
        ],
        ..ModelIndex::default()
    };
    index.save(&models.join(INDEX_FILE_NAME)).expect("save");
    let path = |p: &Path| p.display().to_string();

    // WHEN the format of each row is read
    let by_id = model_format(Some(models), "qwen-mlx", "/elsewhere");
    let by_path = model_format(Some(models), "my-local", &path(&gguf));
    let stray = model_format(Some(models), "stray", &path(&stray_dir));
    let unknown_file = model_format(None, "x", "/models/absent.gguf");

    // THEN the index answers for what it knows, by identifier then by path,
    // and an unindexed folder reads as MLX, anything else as GGUF
    assert_eq!(by_id, ModelFormat::Mlx);
    assert_eq!(by_path, ModelFormat::Gguf);
    assert_eq!(stray, ModelFormat::Mlx);
    assert_eq!(unknown_file, ModelFormat::Gguf);
}

#[test]
fn a_provider_other_than_the_local_one_is_left_to_the_router() {
    // GIVEN the factory with no engine at all, and a cloud row
    let factory = local_engine_override(None, None, None);
    let mut row = local_row("cloud", "gpt");
    row.provider = LlmProvider::OpenAi;

    // WHEN the router asks the factory for it
    let built = factory(&row);

    // THEN the factory declines, so the router builds it normally
    assert!(built.is_none());
}
