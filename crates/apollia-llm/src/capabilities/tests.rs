use super::*;

/// The chat template of `Qwen/Qwen3-4B-GGUF`, recorded from the `gguf` block
/// of the HuggingFace model API on 2026-10-02: tools and reasoning.
const QWEN3_TEMPLATE: &str = include_str!("fixtures/chat_template_qwen3_4b.jinja");

/// The chat template of `ggml-org/gemma-3-4b-it-GGUF`, recorded the same way:
/// neither tools nor reasoning.
const GEMMA3_TEMPLATE: &str = include_str!("fixtures/chat_template_gemma3_4b.jinja");

/// The chat template of `openai/gpt-oss-20b` (`chat_template.jinja` of the
/// repository), recorded on 2026-10-03: it reads `reasoning_effort`.
const GPT_OSS_TEMPLATE: &str = include_str!("fixtures/chat_template_gpt_oss_20b.jinja");

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

    // THEN nothing is declared, and the model offers no reasoning setting
    assert!(!caps.tools && !caps.reasoning && !caps.vision);
    assert_eq!(caps.reasoning_profile, Some(ReasoningProfile::None));
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

#[test]
fn each_recorded_template_yields_its_reasoning_profile() {
    // GIVEN three recorded chat templates: Qwen3 reads `enable_thinking`,
    // gpt-oss reads `reasoning_effort`, Gemma 3 reads neither
    // WHEN the capabilities of each are read
    let qwen3 = Capabilities::from_chat_template(Some(QWEN3_TEMPLATE));
    let gpt_oss = Capabilities::from_chat_template(Some(GPT_OSS_TEMPLATE));
    let gemma3 = Capabilities::from_chat_template(Some(GEMMA3_TEMPLATE));

    // THEN Qwen3 is a switch, gpt-oss a level, Gemma 3 offers no setting, and
    // the earlier `reasoning` flag is unchanged beside the profile
    assert_eq!(qwen3.reasoning_profile, Some(ReasoningProfile::Toggle));
    assert_eq!(gpt_oss.reasoning_profile, Some(ReasoningProfile::Levels));
    assert_eq!(gemma3.reasoning_profile, Some(ReasoningProfile::None));
    assert!(qwen3.reasoning && gpt_oss.reasoning && !gemma3.reasoning);
}

#[test]
fn a_reasoning_template_without_a_variable_offers_no_setting() {
    // GIVEN a template that opens a reasoning block but reads neither variable
    let template = "{{ '<think>' }}{% for m in messages %}{{ m.content }}{% endfor %}";

    // WHEN its capabilities are read
    let caps = Capabilities::from_chat_template(Some(template));

    // THEN the model reasons, and no setting is offered: a variable the
    // template never reads would change nothing
    assert!(caps.reasoning);
    assert_eq!(caps.reasoning_profile, Some(ReasoningProfile::None));
}

#[test]
fn the_family_table_settles_what_the_template_leaves_open() {
    // GIVEN a template reading both variables, and a model with no template
    let both = "{% if enable_thinking %}{% endif %}{{ reasoning_effort }}";

    // WHEN each is read for a gpt-oss model (as a GGUF architecture and as an
    // MLX `model_type`), and for a family the table does not list
    let gguf = Capabilities::from_chat_template_of_family(Some(both), Some("gpt-oss"));
    let mlx = Capabilities::from_chat_template_of_family(None, Some("gpt_oss"));
    let unlisted = Capabilities::from_chat_template_of_family(None, Some("llama"));

    // THEN the table names gpt-oss a level model, and an unlisted family with
    // no template offers no setting
    assert_eq!(gguf.reasoning_profile, Some(ReasoningProfile::Levels));
    assert_eq!(mlx.reasoning_profile, Some(ReasoningProfile::Levels));
    assert_eq!(unlisted.reasoning_profile, Some(ReasoningProfile::None));
    assert_eq!(family_profile("GPT_OSS"), Some(ReasoningProfile::Levels));
}

#[test]
fn an_mlx_config_names_its_family() {
    // GIVEN the recorded MLX configuration of Qwen3, and unreadable text
    // WHEN their `model_type` is read
    // THEN the family is named, and nothing is named for unreadable text
    assert_eq!(
        mlx_config_model_type(QWEN3_MLX_CONFIG).as_deref(),
        Some("qwen3")
    );
    assert_eq!(mlx_config_model_type("not json"), None);
}

#[test]
fn a_record_without_a_profile_is_not_current() {
    // GIVEN a capability record written before the profile existed
    let old: Capabilities =
        serde_json::from_str(r#"{"tools":true,"reasoning":true,"vision":false}"#)
            .expect("an older record still parses");

    // WHEN it is checked against this build
    // THEN it reads as stale, so the index reads the model again
    assert_eq!(old.reasoning_profile, None);
    assert!(!old.is_current());
    assert!(Capabilities::from_chat_template(Some(QWEN3_TEMPLATE)).is_current());
}
