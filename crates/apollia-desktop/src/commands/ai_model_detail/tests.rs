use std::path::{Path, PathBuf};

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};
use crate::commands::model_index::{current, reconcile};

const TOKEN: &str = "token-of-this-installation";

/// The index of cards the cards module is tested with: one of its cards
/// declares a switch, explained in both languages.
const CARDS_FIXTURE: &str =
    include_str!("../../../../apollia-llm/src/model_cards/fixtures/index.json");

/// A GGUF header (version 3, no tensor) carrying the given string pairs.
fn gguf(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GGUF");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
    for (key, value) in pairs {
        out.extend_from_slice(&(key.len() as u64).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }
    out
}

fn weights(architecture: &str, name: &str) -> Vec<u8> {
    gguf(&[
        ("general.architecture", architecture),
        ("general.name", name),
        ("general.size_label", "8B"),
    ])
}

fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let file = root.join(relative);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&file, bytes).unwrap();
    file
}

/// A folder holding one model under two quantisations, its projector, a split
/// model and another model.
fn models_folder(root: &Path) {
    write(root, "Qwen3-8B-Q4_K_M.gguf", &weights("qwen3", "Qwen3 8B"));
    write(root, "Qwen3-8B-Q8_0.gguf", &weights("qwen3", "Qwen3 8B"));
    write(
        root,
        "mmproj-Qwen3-8B.gguf",
        &gguf(&[("general.architecture", "clip")]),
    );
    write(root, "big-00001-of-00002.gguf", &weights("llama", "Big"));
    write(root, "big-00002-of-00002.gguf", b"second part");
    write(root, "mistral-Q4_K_M.gguf", &weights("llama", "Mistral"));
    reconcile(root).unwrap();
}

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

#[test]
fn test_the_detail_reads_the_file_its_quantisation_and_its_architecture() {
    // GIVEN a models folder holding a GGUF model
    let tmp = tempfile::tempdir().unwrap();
    models_folder(tmp.path());

    // WHEN its detail is read
    let detail = detail_in(tmp.path(), "Qwen3-8B-Q4_K_M", "fr").unwrap();

    // THEN the file, the quantisation of its name and the architecture of its
    // header are there, with the category detected and no override
    assert_eq!(detail.file, "Qwen3-8B-Q4_K_M.gguf");
    assert_eq!(detail.format, ModelFormat::Gguf);
    assert_eq!(detail.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(detail.architecture.as_deref(), Some("qwen3"));
    assert_eq!(detail.detected_category, ModelCategory::Language);
    assert!(detail.category_override.is_none());
    assert!(detail.source_file_override.is_none());
    assert!(detail.author_fields.is_empty());
    assert!(detail_in(tmp.path(), "absent", "fr").is_err());
}

#[test]
fn test_the_source_candidates_are_the_other_weights_of_the_same_model_only() {
    // GIVEN a model with another quantisation, a projector, a split model and
    // another model in the same folder
    let tmp = tempfile::tempdir().unwrap();
    models_folder(tmp.path());

    // WHEN its detail is read
    let detail = detail_in(tmp.path(), "Qwen3-8B-Q4_K_M", "fr").unwrap();

    // THEN the other quantisation alone is offered: never the projector, a
    // part of the split model, the other model, or the file itself
    let names: Vec<&str> = detail
        .source_candidates
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Qwen3-8B-Q8_0.gguf"]);
    assert_eq!(
        detail.source_candidates[0].quantization.as_deref(),
        Some("Q8_0")
    );
    assert!(detail.source_candidates[0].size_bytes > 0);
}

#[test]
fn test_the_two_overrides_are_written_to_the_index_and_removed() {
    // GIVEN a model and another quantisation of it
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    models_folder(dir);
    let q8 = dir.join("Qwen3-8B-Q8_0.gguf").display().to_string();

    // WHEN its category and its source file are overridden, then both removed
    override_category_in(dir, "Qwen3-8B-Q4_K_M", Some(ModelCategory::Draft)).unwrap();
    override_source_in(dir, "Qwen3-8B-Q4_K_M", Some(&q8)).unwrap();
    let overridden = detail_in(dir, "Qwen3-8B-Q4_K_M", "fr").unwrap();
    let relaunched = reconcile(dir).unwrap();
    override_category_in(dir, "Qwen3-8B-Q4_K_M", None).unwrap();
    override_source_in(dir, "Qwen3-8B-Q4_K_M", None).unwrap();
    let cleared = current(dir).unwrap();

    // THEN the detail reads them, the file in force is the override, they
    // survive a relaunch, and the removal leaves the detected values
    assert_eq!(overridden.category_override, Some(ModelCategory::Draft));
    assert_eq!(overridden.detected_category, ModelCategory::Language);
    assert_eq!(
        overridden.source_file_override.as_deref(),
        Some(q8.as_str())
    );
    assert_eq!(overridden.file, "Qwen3-8B-Q8_0.gguf");
    assert_eq!(overridden.quantization.as_deref(), Some("Q8_0"));
    let kept = relaunched.get("Qwen3-8B-Q4_K_M").unwrap();
    assert_eq!(kept.category_override, Some(ModelCategory::Draft));
    assert!(kept.source_file_override.is_some());
    let back = cleared.get("Qwen3-8B-Q4_K_M").unwrap();
    assert!(back.category_override.is_none() && back.source_file_override.is_none());
}

#[test]
fn test_a_source_override_to_a_projector_a_part_or_another_model_is_refused() {
    // GIVEN a model, its projector, a split model and another model
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    models_folder(dir);
    let at = |name: &str| dir.join(name).display().to_string();

    // WHEN the source file is pointed at each of them
    let projector = override_source_in(dir, "Qwen3-8B-Q4_K_M", Some(&at("mmproj-Qwen3-8B.gguf")));
    let part = override_source_in(dir, "Qwen3-8B-Q4_K_M", Some(&at("big-00002-of-00002.gguf")));
    let other = override_source_in(dir, "Qwen3-8B-Q4_K_M", Some(&at("mistral-Q4_K_M.gguf")));

    // THEN each is refused and nothing is written
    assert!(projector.is_err() && part.is_err() && other.is_err());
    let index = current(dir).unwrap();
    assert!(index
        .get("Qwen3-8B-Q4_K_M")
        .unwrap()
        .source_file_override
        .is_none());
    assert!(override_category_in(dir, "absent", Some(ModelCategory::Draft)).is_err());
}

#[test]
fn test_an_mlx_model_reads_its_config_and_offers_no_source_file() {
    // GIVEN an MLX folder whose config names its architecture and its width
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write(
        dir,
        "qwen3-mlx/config.json",
        br#"{"model_type":"qwen3","quantization":{"bits":4}}"#,
    );
    write(dir, "qwen3-mlx/tokenizer.json", b"{}");
    write(dir, "qwen3-mlx/model.safetensors", b"weights");
    reconcile(dir).unwrap();

    // WHEN its detail is read
    let detail = detail_in(dir, "qwen3-mlx", "en").unwrap();

    // THEN the config answers, and the folder being the model, no other
    // weights file is offered
    assert_eq!(detail.format, ModelFormat::Mlx);
    assert_eq!(detail.architecture.as_deref(), Some("qwen3"));
    assert_eq!(detail.quantization.as_deref(), Some("4bit"));
    assert!(detail.source_candidates.is_empty());
}

#[test]
fn test_the_author_fields_come_from_the_card_kept_beside_the_model() {
    // GIVEN a model with its card beside it, declaring one switch
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    models_folder(dir);
    let index: Value = serde_json::from_str(CARDS_FIXTURE).unwrap();
    let card = index["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["author_fields"].as_array().is_some_and(|f| !f.is_empty()))
        .unwrap()
        .clone();
    let declared = card["author_fields"][0].clone();
    let beside = apollia_llm::model_cards::card_path_for(&dir.join("Qwen3-8B-Q4_K_M.gguf"));
    std::fs::write(beside, serde_json::to_vec(&card).unwrap()).unwrap();

    // WHEN the detail is read in each language
    let fr = detail_in(dir, "Qwen3-8B-Q4_K_M", "fr").unwrap();
    let en = detail_in(dir, "Qwen3-8B-Q4_K_M", "en").unwrap();

    // THEN the switch is there with its default, explained in that language
    assert_eq!(fr.author_fields.len(), 1);
    assert_eq!(json!(fr.author_fields[0].name), declared["name"]);
    assert_eq!(fr.author_fields[0].default, declared["default"]);
    assert_eq!(
        json!(fr.author_fields[0].explanation),
        declared["explanation"]["fr"]
    );
    assert_eq!(
        json!(en.author_fields[0].explanation),
        declared["explanation"]["en"]
    );
    assert_ne!(
        fr.author_fields[0].explanation,
        en.author_fields[0].explanation
    );
}

#[tokio::test]
async fn test_a_preset_is_created_then_replaced_through_the_preset_routes() {
    // GIVEN a daemon that creates a preset and replaces it
    let server = MockServer::start().await;
    let created = json!({
        "name": "Precise", "model_source": "local", "model_id": "Qwen3-8B-Q4_K_M",
        "load": {}, "request": { "temperature": 0.2 }, "is_model_default": false,
    });
    let replaced = json!({
        "name": "Precise", "load": {}, "request": { "temperature": 0.2 },
        "is_model_default": true,
    });
    Mock::given(method("POST"))
        .and(path("/api/v1/llm/presets"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(&created))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({ "id": 7, "name": "Precise" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/llm/presets/7"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(&replaced))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "id": 7, "name": "Precise", "is_model_default": true })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let link = attached(&server);

    // WHEN the panel saves a preset, then makes it the default of its model
    let saved = create_preset_on(&link, &created).await.unwrap();
    let default = update_preset_on(&link, 7, &replaced).await.unwrap();

    // THEN each answer is the preset the runtime holds
    assert_eq!(saved["id"], 7);
    assert_eq!(default["is_model_default"], true);
}

#[test]
fn test_the_commands_of_the_panel_serve_an_attached_window() {
    // GIVEN the commands of the panel of a model
    let commands = [
        "get_installed_model_detail",
        "set_installed_model_category_override",
        "set_installed_model_source_override",
        "create_model_preset",
        "update_model_preset",
    ];

    // WHEN each passes the gate of an attached window
    // THEN none is refused
    for command in commands {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}
