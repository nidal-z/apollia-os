use std::sync::{Arc, Mutex};

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider, SttConfigRepository};
use apollia_memory::user_memory::UserMemoryRepository;
use tempfile::TempDir;

use super::*;

#[test]
fn a_first_local_model_becomes_the_default_with_its_window() {
    // GIVEN no "local" backend yet
    // WHEN a model is chosen at onboarding with a 16k window
    let cfg = local_backend(
        None,
        "~/.apollia/models/a.gguf",
        "cpu",
        "q4_k_m",
        Some(16_384),
    );

    // THEN the row is the default and carries the window under the key the
    // runtime reads
    assert!(cfg.is_default);
    assert_eq!(cfg.model, "~/.apollia/models/a.gguf");
    assert_eq!(cfg.config_json["context_window"], 16_384);
    assert_eq!(cfg.config_json["model_path"], "~/.apollia/models/a.gguf");
}

#[test]
fn choosing_another_model_updates_the_existing_row() {
    // GIVEN a "local" backend the operator already tuned, not the default,
    // stored with the legacy window key
    let existing = LlmBackendConfig {
        name: "local".into(),
        provider: LlmProvider::LlamaCpp,
        model: "~/.apollia/models/old.gguf".into(),
        config_json: serde_json::json!({
            "model_path": "~/.apollia/models/old.gguf",
            "temperature": 0.4,
            "context_size": 8192,
        }),
        enabled: true,
        is_default: false,
    };

    // WHEN another model is chosen with a new window
    let cfg = local_backend(
        Some(existing),
        "~/.apollia/models/new.gguf",
        "cpu",
        "q8_0",
        Some(65_536),
    );

    // THEN the row points at the new file, keeps its settings and flags, and
    // the window converges on the canonical key
    assert_eq!(cfg.model, "~/.apollia/models/new.gguf");
    assert_eq!(cfg.config_json["model_path"], "~/.apollia/models/new.gguf");
    assert_eq!(cfg.config_json["temperature"], 0.4);
    assert_eq!(cfg.config_json["context_window"], 65_536);
    assert!(cfg.config_json.get("context_size").is_none());
    assert!(!cfg.is_default);
}

#[test]
fn test_infer_quantization_common_patterns() {
    // GIVEN various GGUF filenames
    // WHEN the quantisation is inferred from each name
    // THEN the quantization is correctly inferred
    assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0"), "q8_0");
    assert_eq!(infer_quantization("llama-3-8b-Q4_K_M"), "q4_k_m");
    assert_eq!(infer_quantization("mistral-7b-Q5_K_S"), "q5_k_s");
    assert_eq!(infer_quantization("phi-3-mini-F16"), "f16");
    assert_eq!(infer_quantization("model-Q3_K_M"), "q3_k_m");
}

#[test]
fn test_infer_quantization_fallback() {
    // GIVEN a filename with no recognizable quantization
    // WHEN the quantisation is inferred from it
    // THEN the default is returned
    assert_eq!(infer_quantization("some-random-model"), "q4_k_m");
}

#[tokio::test]
async fn a_chosen_gguf_file_is_copied_into_the_models_folder_and_becomes_the_local_backend() {
    // GIVEN a data directory, and a GGUF file outside it
    let data = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let source = elsewhere.path().join("Qwen3-0.6B-Q8_0.gguf");
    std::fs::write(&source, b"GGUF").unwrap();

    // WHEN it is chosen at onboarding with a 32k window
    let result = setup_local_llm(data.path(), &source.display().to_string(), Some(32_768))
        .await
        .unwrap();

    // THEN the file sits in the models folder, and system.db holds the
    // default "local" backend pointing at it with its quantisation and window
    let copied = apollia_core::config::default_models_dir(data.path()).join("Qwen3-0.6B-Q8_0.gguf");
    assert_eq!(result.model_path, copied.display().to_string());
    assert_eq!(result.quantization, "q8_0");
    assert_eq!(std::fs::read(&copied).unwrap(), b"GGUF");
    let repo = LlmBackendRepository::open(&data.path().join("system.db")).unwrap();
    let local = repo.find_by_name("local").unwrap().unwrap();
    assert!(local.is_default);
    assert_eq!(local.model, "~/.apollia/models/Qwen3-0.6B-Q8_0.gguf");
    assert_eq!(local.config_json["context_window"], 32_768);
}

#[tokio::test]
async fn a_missing_file_or_one_that_is_not_gguf_is_refused_before_anything_is_written() {
    // GIVEN a data directory, a path no file occupies, and a file of another
    // kind
    let data = TempDir::new().unwrap();
    let absent = data.path().join("absent.gguf").display().to_string();
    let other = data.path().join("notes.txt");
    std::fs::write(&other, b"x").unwrap();

    // WHEN each is chosen
    let missing = setup_local_llm(data.path(), &absent, None).await;
    let wrong = setup_local_llm(data.path(), &other.display().to_string(), None).await;

    // THEN both are refused as a fault of the request, with the words the
    // interface has always shown, and no backend is stored
    let missing = missing.unwrap_err();
    assert!(missing.is_invalid_request());
    assert_eq!(missing.to_string(), format!("file not found: {absent}"));
    let wrong = wrong.unwrap_err();
    assert!(wrong.is_invalid_request());
    assert_eq!(wrong.to_string(), "expected a .gguf file");
    assert!(!data.path().join("system.db").exists());
}

fn onboarding_repo(dir: &TempDir) -> Arc<Mutex<UserMemoryRepository>> {
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    Arc::new(Mutex::new(repo))
}

#[tokio::test]
async fn a_dictation_model_is_stored_enabled_and_marks_the_onboarding_ready() {
    // GIVEN a data directory with an onboarding never started
    let data = TempDir::new().unwrap();
    let repo = onboarding_repo(&data);

    // WHEN a Whisper model is chosen with the French locale
    let done = setup_dictation(
        data.path(),
        Some(ProfileStore(&repo)),
        "/models/ggml-base.bin".to_owned(),
        Some("fr".to_owned()),
    )
    .await
    .unwrap();

    // THEN dictation is enabled on that file in French, and the onboarding
    // progress starts at its first phase with both dictation marks set
    assert_eq!(done.model_path, "/models/ggml-base.bin");
    assert_eq!(done.language.as_deref(), Some("fr"));
    let stt = SttConfigRepository::open(&data.path().join("system.db")).unwrap();
    let row = stt.get_or_default().unwrap();
    assert!(row.enabled);
    assert_eq!(row.model_path, "/models/ggml-base.bin");
    let guard = repo.lock().unwrap();
    assert_eq!(
        guard.get_internal("onboarding_phase").unwrap().as_deref(),
        Some("welcome")
    );
    assert_eq!(
        guard
            .get_internal("onboarding_stt_configured")
            .unwrap()
            .as_deref(),
        Some("true")
    );
    assert_eq!(
        guard
            .get_internal("onboarding_voice_enabled")
            .unwrap()
            .as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn a_language_chosen_before_is_kept_and_a_phase_reached_is_not_reset() {
    // GIVEN dictation already set to English, and an onboarding at the AI
    // setup phase
    let data = TempDir::new().unwrap();
    let stt = SttConfigRepository::open(&data.path().join("system.db")).unwrap();
    let mut row = stt.get_or_default().unwrap();
    row.language = Some("en".to_owned());
    stt.upsert(&row).unwrap();
    let repo = onboarding_repo(&data);
    repo.lock()
        .unwrap()
        .set_internal("onboarding_phase", "ai_setup")
        .unwrap();

    // WHEN a model is chosen with the French locale
    let done = setup_dictation(
        data.path(),
        Some(ProfileStore(&repo)),
        "/models/ggml-small.bin".to_owned(),
        Some("fr".to_owned()),
    )
    .await
    .unwrap();

    // THEN the explicit choice stays, and so does the phase
    assert_eq!(done.language.as_deref(), Some("en"));
    assert_eq!(
        repo.lock()
            .unwrap()
            .get_internal("onboarding_phase")
            .unwrap()
            .as_deref(),
        Some("ai_setup")
    );
}

#[tokio::test]
async fn a_dictation_setup_without_the_onboarding_store_is_refused() {
    // GIVEN a runtime whose user_memory.db did not open
    let data = TempDir::new().unwrap();

    // WHEN a model is chosen
    let refused = setup_dictation(data.path(), None, "/m/ggml-base.bin".to_owned(), None).await;

    // THEN it is refused with the words the onboarding has always shown
    assert_eq!(
        refused.unwrap_err().to_string(),
        "UserMemory database not initialized"
    );
}
