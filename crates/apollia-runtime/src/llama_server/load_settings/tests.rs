use super::*;
use crate::llama_server::config::{build_args, ENV_N_CTX, ENV_N_GPU_LAYERS};
use serde_json::json;

fn no_env(_: &str) -> Option<String> {
    None
}

fn settings(load: Value) -> LoadSettings {
    match load {
        Value::Object(map) => LoadSettings::from_map(&map),
        _ => LoadSettings::default(),
    }
}

/// The launch line of the default configuration under `load`, with no
/// environment and plans that change nothing.
fn args_under(load: &LoadSettings, template: Option<&Path>) -> Vec<String> {
    let base = LlamaServerConfig {
        chat_template_file: template.map(|path| path.display().to_string()),
        ..LlamaServerConfig::default()
    };
    let config = compose(&base, load, no_env, |_| {}, |_| {});
    build_args(&config, 8080)
}

/// The value following `flag`, or the flag itself for a bare one.
fn value_of(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|a| a == flag)?;
    Some(
        args.get(at + 1)
            .filter(|v| !v.starts_with('-') || v.parse::<f64>().is_ok())
            .cloned()
            .unwrap_or_else(|| flag.to_owned()),
    )
}

#[test]
fn test_each_load_setting_reaches_its_flag() {
    // GIVEN one preset key per load setting of the model sheet, with the flag
    // it must produce and the value that flag must carry
    let table: &[(&str, Value, &str, &str)] = &[
        ("n_ctx", json!(8192), "-c", "8192"),
        ("n_gpu_layers", json!(20), "-ngl", "20"),
        ("n_threads", json!(6), "-t", "6"),
        ("n_batch", json!(1024), "-b", "1024"),
        ("n_ubatch", json!(256), "-ub", "256"),
        ("n_parallel", json!(4), "-np", "4"),
        ("kv_unified", json!(true), "--kv-unified", "--kv-unified"),
        ("ctx_checkpoints", json!(8), "--ctx-checkpoints", "8"),
        ("reasoning_budget", json!(0), "--reasoning-budget", "0"),
        (
            "reasoning_budget_message",
            json!("Answer now."),
            "--reasoning-budget-message",
            "Answer now.",
        ),
        (
            "rope_freq_base",
            json!(10000.0),
            "--rope-freq-base",
            "10000",
        ),
        ("rope_freq_scale", json!(0.5), "--rope-freq-scale", "0.5"),
        (
            "kv_offload",
            json!(false),
            "--no-kv-offload",
            "--no-kv-offload",
        ),
        ("mlock", json!(true), "--mlock", "--mlock"),
        ("mmap", json!(false), "--no-mmap", "--no-mmap"),
        ("seed", json!(42), "--seed", "42"),
        (
            "draft_model",
            json!("/models/draft.gguf"),
            "--model-draft",
            "/models/draft.gguf",
        ),
        ("spec_type", json!("draft-mtp"), "--spec-type", "draft-mtp"),
        ("flash_attn", json!("off"), "--flash-attn", "off"),
        ("cache_type_k", json!("q8_0"), "-ctk", "q8_0"),
        ("cache_type_v", json!("q4_0"), "-ctv", "q4_0"),
    ];

    for (key, value, flag, expected) in table {
        // WHEN a preset sets that key alone and the launch line is built
        let load = settings(json!({ *key: value.clone() }));
        let args = args_under(&load, None);

        // THEN the flag is present with the value the preset gave
        assert_eq!(
            value_of(&args, flag).as_deref(),
            Some(*expected),
            "{key} must reach {flag}: {args:?}"
        );
    }
}

#[test]
fn test_an_empty_preset_keeps_the_historical_line() {
    // GIVEN a preset with no load setting
    let load = LoadSettings::default();

    // WHEN the launch line is built
    let args = args_under(&load, None);

    // THEN it is the default line: RoPE stays automatic, nothing is added
    assert_eq!(args, build_args(&LlamaServerConfig::default(), 8080));
    assert!(value_of(&args, "--rope-freq-base").is_none());
    assert!(value_of(&args, "--rope-freq-scale").is_none());
}

#[test]
fn test_a_speculative_mode_the_engine_lacks_is_refused() {
    // GIVEN presets naming DSpark, a LM Studio mode the bundled engine does not
    // have, and a mode spelled outside the engine's vocabulary
    for mode in ["dspark", "mtp", "draft-eagle3"] {
        let load = settings(json!({ "spec_type": mode }));

        // WHEN the launch line is built
        let args = args_under(&load, None);

        // THEN no speculative mode is passed, and the model still loads
        assert_eq!(load.spec_type, None, "{mode}");
        assert!(value_of(&args, "--spec-type").is_none(), "{mode}");
    }
}

#[test]
fn test_a_value_of_the_wrong_shape_is_dropped_alone() {
    // GIVEN a preset whose thread count is a word and whose seed is valid
    let load = settings(json!({ "n_threads": "many", "seed": 7, "unknown": 1 }));

    // WHEN the launch line is built
    let args = args_under(&load, None);

    // THEN the bad key is left out and the good one still applies
    assert!(value_of(&args, "-t").is_none());
    assert_eq!(value_of(&args, "--seed").as_deref(), Some("7"));
}

#[test]
fn test_the_environment_wins_over_the_preset_for_the_window() {
    // GIVEN APOLLIA_LLAMA_N_CTX and a preset that fixes the window
    let get = |name: &str| (name == ENV_N_CTX).then(|| "4096".to_owned());
    let load = settings(json!({ "n_ctx": 8192 }));

    // WHEN the configuration is composed
    let config = compose(&LlamaServerConfig::default(), &load, get, |_| {}, |_| {});

    // THEN the environment wins
    assert_eq!(config.n_ctx, 4096);
}

#[test]
fn test_the_environment_wins_over_the_preset_for_the_offload() {
    // GIVEN APOLLIA_LLAMA_N_GPU_LAYERS and a preset that fixes the offload
    let get = |name: &str| (name == ENV_N_GPU_LAYERS).then(|| "10".to_owned());
    let load = settings(json!({ "n_gpu_layers": 30 }));

    // WHEN the configuration is composed with a plan that would answer 50
    let config = compose(
        &LlamaServerConfig::default(),
        &load,
        get,
        |_| {},
        |c| c.n_gpu_layers = 50,
    );

    // THEN the environment wins and the plan is not consulted
    assert_eq!(config.n_gpu_layers, 10);
}

#[test]
fn test_the_preset_wins_over_the_automatic_plans() {
    // GIVEN a preset that fixes the offload and the slot count, and no
    // variable
    let load = settings(json!({ "n_gpu_layers": 30, "n_parallel": 2 }));

    // WHEN the configuration is composed with plans that would answer
    // otherwise
    let mut slots_planned = false;
    let config = compose(
        &LlamaServerConfig::default(),
        &load,
        no_env,
        |c| {
            slots_planned = true;
            c.n_parallel = Some(8);
        },
        |c| c.n_gpu_layers = 50,
    );

    // THEN the preset stands and neither plan changed it
    assert_eq!(config.n_gpu_layers, 30);
    assert_eq!(config.n_parallel, Some(2));
    assert!(!slots_planned);
}

#[test]
fn test_a_preset_fixing_the_slots_keeps_the_resolved_window() {
    // GIVEN a preset that fixes two slots and leaves the window, and a window
    // of 16384 resolved for the model
    let load = settings(json!({ "n_parallel": 2 }));
    let base = LlamaServerConfig {
        n_ctx: 16_384,
        ..LlamaServerConfig::default()
    };

    // WHEN the configuration is composed
    let config = compose(&base, &load, no_env, |c| c.n_parallel = Some(8), |_| {});

    // THEN the resolved window is launched beside the preset's slots
    assert_eq!(config.n_ctx, 16_384);
    assert_eq!(config.n_parallel, Some(2));
}

#[test]
fn test_the_environment_wins_over_the_resolved_window() {
    // GIVEN APOLLIA_LLAMA_N_CTX and a window of 16384 resolved for the model
    let get = |name: &str| (name == ENV_N_CTX).then(|| "4096".to_owned());

    let base = LlamaServerConfig {
        n_ctx: 16_384,
        ..LlamaServerConfig::default()
    };

    // WHEN the configuration is composed
    let config = compose(&base, &LoadSettings::default(), get, |_| {}, |_| {});

    // THEN the environment wins
    assert_eq!(config.n_ctx, 4096);
}

#[test]
fn test_without_a_preset_the_plans_decide() {
    // GIVEN no preset setting and no variable
    let load = LoadSettings::default();

    // WHEN the configuration is composed
    let config = compose(
        &LlamaServerConfig::default(),
        &load,
        no_env,
        |c| {
            c.n_parallel = Some(8);
            c.kv_unified = true;
        },
        |c| c.n_gpu_layers = 50,
    );

    // THEN the plans' answers are launched
    assert_eq!(config.n_gpu_layers, 50);
    assert_eq!(config.n_parallel, Some(8));
    assert!(config.kv_unified);
}

#[test]
fn test_a_preset_unified_cache_survives_the_slot_plan() {
    // GIVEN a preset that turns the unified cache off without fixing the slots
    let load = settings(json!({ "kv_unified": false }));

    // WHEN the slot plan answers eight unified slots
    let config = compose(
        &LlamaServerConfig::default(),
        &load,
        no_env,
        |c| {
            c.n_parallel = Some(8);
            c.kv_unified = true;
        },
        |_| {},
    );

    // THEN the planned count is kept and the preset's choice of buffer holds
    assert_eq!(config.n_parallel, Some(8));
    assert!(!config.kv_unified);
}

#[test]
fn test_a_chat_template_is_written_and_passed_by_file() {
    // GIVEN a preset carrying a chat template and a directory to write it in
    let dir = tempfile::tempdir().unwrap();
    let template = "{% for m in messages %}{{ m.content }}{% endfor %}";
    let load = settings(json!({ "chat_template": template }));

    // WHEN the template is written and the launch line is built
    let file = write_template(dir.path(), load.chat_template.as_deref().unwrap()).unwrap();
    let args = args_under(&load, Some(&file));

    // THEN the line passes the file, and the file holds the template verbatim
    let passed = value_of(&args, "--chat-template-file").unwrap();
    assert_eq!(passed, file.display().to_string());
    assert_eq!(std::fs::read_to_string(&passed).unwrap(), template);
}

#[test]
fn test_the_default_preset_of_the_model_is_the_one_read() {
    // GIVEN a model with a default preset and another one, and a second model
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let repo = apollia_core::LlmBackendRepository::open(&db).unwrap();
    let draft = |name: &str, threads: u32, default: bool| apollia_core::model_preset::PresetDraft {
        name: name.to_owned(),
        load: json!({ "n_threads": threads })
            .as_object()
            .cloned()
            .unwrap(),
        request: apollia_core::model_preset::RequestSettings::default(),
        is_model_default: default,
    };
    repo.create_preset(ModelSource::Local, "qwen", &draft("fast", 2, false))
        .unwrap();
    repo.create_preset(ModelSource::Local, "qwen", &draft("main", 6, true))
        .unwrap();
    repo.create_preset(ModelSource::Local, "gemma", &draft("main", 4, true))
        .unwrap();
    drop(repo);

    // WHEN the load settings of each are read
    let qwen = read_default(&db, "qwen");
    let absent = read_default(&db, "mistral");

    // THEN the default preset of that model answers, and a model without one
    // reads as no setting
    assert_eq!(qwen.n_threads, Some(6));
    assert_eq!(absent, LoadSettings::default());
}
