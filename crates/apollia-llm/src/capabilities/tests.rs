use super::*;

/// The chat template of `Qwen/Qwen3-4B-GGUF`, recorded from the `gguf` block
/// of the HuggingFace model API on 2026-10-02: tools and reasoning.
const QWEN3_TEMPLATE: &str = include_str!("fixtures/chat_template_qwen3_4b.jinja");

/// The chat template of `ggml-org/gemma-3-4b-it-GGUF`, recorded the same way:
/// neither tools nor reasoning.
const GEMMA3_TEMPLATE: &str = include_str!("fixtures/chat_template_gemma3_4b.jinja");

/// The `config.json` of `mlx-community/gemma-3-4b-it-4bit`, a multimodal MLX
/// conversion, recorded on 2026-10-02.
const GEMMA3_MLX_CONFIG: &str = include_str!("fixtures/config_mlx_gemma3_4b_4bit.json");

/// The `config.json` of `Qwen/Qwen3-4B-MLX-4bit`, a text-only MLX conversion.
const QWEN3_MLX_CONFIG: &str = include_str!("fixtures/config_mlx_qwen3_4b_4bit.json");

#[test]
fn a_recorded_template_with_tools_and_reasoning_declares_both() {
    // GIVEN the recorded Qwen3 chat template
    // WHEN its capabilities are read
    let caps = Capabilities::from_chat_template(Some(QWEN3_TEMPLATE));

    // THEN it calls tools and reasons, and declares no vision on its own
    assert!(caps.tools);
    assert!(caps.reasoning);
    assert!(!caps.vision);
    assert!(caps.image_formats.is_empty());
}

#[test]
fn a_recorded_template_without_tools_declares_neither() {
    // GIVEN the recorded Gemma 3 chat template, which renders no tool
    // WHEN its capabilities are read
    let caps = Capabilities::from_chat_template(Some(GEMMA3_TEMPLATE));

    // THEN it declares neither tools nor reasoning
    assert!(!caps.tools);
    assert!(!caps.reasoning);
}

#[test]
fn no_template_declares_nothing() {
    // GIVEN a model with no chat template
    // WHEN its capabilities are read
    let caps = Capabilities::from_chat_template(None);

    // THEN nothing is declared
    assert_eq!(caps, Capabilities::default());
}

#[test]
fn a_recorded_mlx_config_with_a_vision_config_declares_vision() {
    // GIVEN two recorded MLX configurations, one multimodal, one text-only
    // WHEN each is read for a vision tower
    // THEN only the multimodal one declares it
    assert!(mlx_config_has_vision(GEMMA3_MLX_CONFIG));
    assert!(!mlx_config_has_vision(QWEN3_MLX_CONFIG));
    assert!(!mlx_config_has_vision("not json"));
}

#[test]
fn vision_names_the_formats_of_the_engine_that_serves_the_format() {
    // GIVEN a capability set without vision
    let base = Capabilities::from_chat_template(Some(QWEN3_TEMPLATE));

    // WHEN vision is granted by each engine
    let gguf = base.clone().with_vision(VisionEngine::LlamaServer);
    let mlx = base.with_vision(VisionEngine::VllmMlx);

    // THEN each carries the formats its engine decodes
    assert!(gguf.vision);
    assert_eq!(gguf.image_formats, ["jpeg", "png", "tga", "bmp", "gif"]);
    assert!(mlx.vision);
    assert!(mlx.image_formats.iter().any(|f| f == "webp"));
}

#[test]
fn an_mlx_folder_template_is_read_from_its_tokenizer_config() {
    // GIVEN an MLX folder whose template sits in tokenizer_config.json
    let dir = tempfile::tempdir().expect("tempdir");
    let config = serde_json::json!({ "chat_template": QWEN3_TEMPLATE });
    std::fs::write(dir.path().join("tokenizer_config.json"), config.to_string()).expect("write");

    // WHEN its template is looked up
    let template = mlx_chat_template(dir.path());

    // THEN the template is found and declares tools and reasoning
    let caps = Capabilities::from_chat_template(template.as_deref());
    assert!(caps.tools);
    assert!(caps.reasoning);
}

#[test]
fn a_list_of_named_templates_yields_the_default_one() {
    // GIVEN a tokenizer configuration listing named templates
    let text = serde_json::json!({
        "chat_template": [
            { "name": "tool_use", "template": "{{ tools }}" },
            { "name": "default", "template": "{{ messages }}" }
        ]
    })
    .to_string();

    // WHEN the template is read
    // THEN the default one is returned
    assert_eq!(
        chat_template_of_json(&text).as_deref(),
        Some("{{ messages }}")
    );
}
