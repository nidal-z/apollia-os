use std::path::{Path, PathBuf};

use apollia_llm::ModelCategory;
use serde_json::{json, Value};

use super::*;
use crate::llama_server::config::{build_args, LlamaServerConfig};
use crate::llama_server::load_settings::compose;

/// A GGUF header of a language model declaring a vocabulary of `tokens`
/// entries of the family `family`.
fn with_vocabulary(family: &str, tokens: u64) -> Vec<u8> {
    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"GGUF");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&3u64.to_le_bytes());
    for (key, value) in [
        ("general.architecture", "qwen2"),
        ("tokenizer.ggml.model", family),
    ] {
        string(&mut out, key);
        out.extend_from_slice(&8u32.to_le_bytes());
        string(&mut out, value);
    }
    string(&mut out, "tokenizer.ggml.tokens");
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&tokens.to_le_bytes());
    for _ in 0..tokens {
        string(&mut out, "t");
    }
    out
}

/// A models folder holding a main model of 200 tokens, a draft of the same
/// vocabulary (`small`), a draft of 100 tokens (`fewer`) and a model of the
/// same vocabulary that is not filed as a draft (`sibling`), indexed.
fn models(dir: &Path) -> PathBuf {
    for (name, bytes) in [
        ("main.gguf", with_vocabulary("gpt2", 200)),
        ("small.gguf", with_vocabulary("gpt2", 200)),
        ("fewer.gguf", with_vocabulary("gpt2", 100)),
        ("sibling.gguf", with_vocabulary("gpt2", 200)),
    ] {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let mut index = ModelIndex::default();
    index.rebuild(dir).unwrap();
    for id in ["small", "fewer"] {
        index
            .set_category_override(id, Some(ModelCategory::Draft))
            .unwrap();
    }
    index.save(&dir.join(INDEX_FILE_NAME)).unwrap();
    dir.join("main.gguf")
}

fn settings(load: Value) -> LoadSettings {
    LoadSettings::from_map(load.as_object().unwrap())
}

/// The launch line of `main` under `load`, the draft resolved against `dir`
/// as the launch does, with `env` as the environment and plans that change
/// nothing.
fn args_under(
    dir: &Path,
    main: &Path,
    load: &LoadSettings,
    env: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let (draft_model, load) = in_models_dir(dir, main, load);
    let base = LlamaServerConfig {
        model_path: main.display().to_string(),
        draft_model,
        ..LlamaServerConfig::default()
    };
    build_args(&compose(&base, &load, env, |_| {}, |_| {}), 8080)
}

fn values_of(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| *a == flag)
        .filter_map(|(at, _)| args.get(at + 1).cloned())
        .collect()
}

#[test]
fn test_the_draft_model_of_a_preset_reaches_the_engine_by_its_path() {
    // GIVEN a preset naming a compatible draft model by its index identifier
    let dir = tempfile::tempdir().unwrap();
    let main = models(dir.path());
    let load = settings(json!({ "draft_model": "small" }));

    // WHEN the launch line of the main model is built
    let args = args_under(dir.path(), &main, &load, |_| None);

    // THEN the engine is given the path of the draft, in the mode that reads it
    let draft = dir.path().join("small.gguf").display().to_string();
    assert_eq!(values_of(&args, "--model-draft"), [draft]);
    assert_eq!(values_of(&args, "--spec-type"), ["draft-simple"]);
}

#[test]
fn test_a_draft_model_fixes_the_speculative_mode() {
    // GIVEN load settings naming a draft model together with another mode,
    // which a load by hand can send though no preset can store it
    let dir = tempfile::tempdir().unwrap();
    let main = models(dir.path());
    let load = settings(json!({ "draft_model": "small", "spec_type": "draft-mtp" }));

    // WHEN the launch line is built
    let args = args_under(dir.path(), &main, &load, |_| None);

    // THEN the draft model wins: the mode is the one that reads it
    assert_eq!(values_of(&args, "--spec-type"), ["draft-simple"]);
    assert_eq!(values_of(&args, "--model-draft").len(), 1);
}

#[test]
fn test_a_draft_that_cannot_draft_for_the_model_is_left_out() {
    // GIVEN presets naming a draft of another vocabulary, a model that is not
    // filed as a draft, a model the index does not hold, and the model itself
    let dir = tempfile::tempdir().unwrap();
    let main = models(dir.path());

    for id in ["fewer", "sibling", "absent", "main"] {
        let load = settings(json!({ "draft_model": id, "spec_type": "draft-simple" }));

        // WHEN the launch line is built
        let args = args_under(dir.path(), &main, &load, |_| None);

        // THEN the model loads alone, with no speculative mode left behind
        assert!(values_of(&args, "--model-draft").is_empty(), "{id}");
        assert!(values_of(&args, "--spec-type").is_empty(), "{id}");
    }
}

#[test]
fn test_a_mode_without_a_draft_model_is_kept_and_draft_simple_is_not() {
    // GIVEN settings with no draft model: one asks for the model's own
    // prediction head, the other for a draft it does not name
    let dir = tempfile::tempdir().unwrap();
    let main = models(dir.path());
    let own_head = settings(json!({ "spec_type": "draft-mtp" }));
    let unnamed = settings(json!({ "spec_type": "draft-simple" }));

    // WHEN each launch line is built
    let kept = args_under(dir.path(), &main, &own_head, |_| None);
    let dropped = args_under(dir.path(), &main, &unnamed, |_| None);

    // THEN the first mode is passed as it is, the second has nothing to read
    assert_eq!(values_of(&kept, "--spec-type"), ["draft-mtp"]);
    assert!(values_of(&dropped, "--spec-type").is_empty());
}

#[test]
fn test_the_environment_keeps_the_last_word_on_the_draft_model() {
    // GIVEN a preset naming a draft model, and an environment that names
    // another through the extra arguments
    let dir = tempfile::tempdir().unwrap();
    let main = models(dir.path());
    let load = settings(json!({ "draft_model": "small" }));
    let env = |name: &str| {
        (name == "APOLLIA_LLAMA_EXTRA_ARGS").then(|| "--model-draft /env/draft.gguf".to_owned())
    };

    // WHEN the launch line is built
    let args = args_under(dir.path(), &main, &load, env);

    // THEN the environment's draft comes last, where the engine reads it
    let drafts = values_of(&args, "--model-draft");
    assert_eq!(drafts.last().map(String::as_str), Some("/env/draft.gguf"));
    assert_eq!(drafts.len(), 2);
}
