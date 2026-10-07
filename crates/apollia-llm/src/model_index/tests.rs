//! Model index tests, on folders built in a temporary directory.
//!
//! The GGUF files carry a real header written by [`gguf`], so the category and
//! parameter detection read the same bytes they read on a real model.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};

use super::{
    ModelCategory, ModelFormat, ModelIndex, ModelIndexError, ModelOrigin, INDEX_FILE_NAME,
    INDEX_FORMAT_VERSION,
};

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

fn llama(name: &str) -> Vec<u8> {
    gguf(&[
        ("general.architecture", "llama"),
        ("general.name", name),
        ("general.size_label", "8B"),
    ])
}

fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, bytes).unwrap();
    path
}

fn rebuilt(root: &Path) -> ModelIndex {
    let mut index = ModelIndex::default();
    index.rebuild(root).unwrap();
    index
}

fn ids(index: &ModelIndex) -> Vec<&str> {
    index.models.iter().map(|m| m.id.as_str()).collect()
}

#[test]
fn rebuild_groups_subfolders_split_parts_projectors_and_mlx_folders() {
    // GIVEN a models folder with a.gguf, sous/b.gguf, a GGUF split in two
    //       parts, a projector beside it, and an MLX folder m/
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "a.gguf", &llama("A"));
    write(root, "sous/b.gguf", &llama("B"));
    let c1 = write(root, "c-00001-of-00002.gguf", &llama("C"));
    let c2 = write(root, "c-00002-of-00002.gguf", b"second part");
    let mmproj = write(
        root,
        "mmproj-c.gguf",
        &gguf(&[("general.architecture", "clip")]),
    );
    write(root, "m/config.json", b"{\"model_type\":\"qwen3\"}");
    write(root, "m/tokenizer.json", b"{}");
    write(root, "m/model.safetensors", b"weights");

    // WHEN the index is rebuilt
    let index = rebuilt(root);

    // THEN it holds four models: a, b, c (one entry, two parts, the projector
    //      as annex) and m in the mlx format
    assert_eq!(ids(&index), vec!["a", "b", "c", "m"]);
    let c = index.get("c").unwrap();
    assert_eq!(c.path, c1);
    assert_eq!(c.parts, vec![c1.clone(), c2.clone()]);
    assert_eq!(c.annexes, vec![mmproj]);
    let parts_size = std::fs::metadata(&c1).unwrap().len() + std::fs::metadata(&c2).unwrap().len();
    assert_eq!(c.size_bytes, parts_size);
    let m = index.get("m").unwrap();
    assert_eq!(m.format, ModelFormat::Mlx);
    assert_eq!(m.path, root.join("m"));
    assert_eq!(index.get("b").unwrap().path, root.join("sous/b.gguf"));
    assert!(index.models.iter().all(|e| e.origin == ModelOrigin::Import));
    assert!(index
        .models
        .iter()
        .all(|e| !e.origin_verified && e.sha256.is_none()));
}

#[test]
fn a_folder_missing_config_or_safetensors_is_not_a_model() {
    // GIVEN a folder with config.json alone and another with weights alone
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "only-config/config.json", b"{}");
    write(root, "only-weights/model.safetensors", b"weights");

    // WHEN the index is rebuilt
    let index = rebuilt(root);

    // THEN neither folder is a model
    assert!(index.models.is_empty(), "{:?}", ids(&index));
}

#[test]
fn a_whisper_gguf_is_filed_as_dictation_by_its_header() {
    // GIVEN a dictation model saved as .gguf, a whisper.cpp .bin, a language
    //       GGUF and a .bin that is not a model
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        root,
        "whisper-small.gguf",
        &gguf(&[("general.architecture", "whisper")]),
    );
    let mut ggml = 0x6767_6d6c_u32.to_le_bytes().to_vec();
    ggml.extend_from_slice(b"hparams");
    write(root, "ggml-base.bin", &ggml);
    write(root, "qwen.gguf", &llama("Qwen"));
    write(root, "notes.bin", b"not a model");

    // WHEN the index is rebuilt
    let index = rebuilt(root);

    // THEN the category comes from the header, not from the extension
    assert_eq!(ids(&index), vec!["ggml-base", "qwen", "whisper-small"]);
    assert_eq!(
        index.get("whisper-small").unwrap().category,
        ModelCategory::Dictation
    );
    assert_eq!(
        index.get("ggml-base").unwrap().category,
        ModelCategory::Dictation
    );
    let qwen = index.get("qwen").unwrap();
    assert_eq!(qwen.category, ModelCategory::Language);
    assert_eq!(qwen.parameters.as_deref(), Some("8B"));
    assert!(qwen.modified_at.is_some());
}

#[test]
fn the_persisted_index_is_versioned_and_reads_back() {
    // GIVEN a rebuilt index saved to disk
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "a.gguf", &llama("A"));
    let file = root.join(INDEX_FILE_NAME);
    rebuilt(root).save(&file).unwrap();

    // WHEN it is read back, and when a file of another version is read
    let back = ModelIndex::load(&file).unwrap();
    std::fs::write(&file, b"{\"version\": 99, \"models\": []}").unwrap();
    let future = ModelIndex::load(&file);

    // THEN the same index comes back, and the other version is refused
    assert_eq!(back.version, INDEX_FORMAT_VERSION);
    assert_eq!(ids(&back), vec!["a"]);
    assert!(matches!(
        future,
        Err(ModelIndexError::UnsupportedVersion { found: 99, .. })
    ));
    assert!(ModelIndex::load(&root.join("absent.json"))
        .unwrap()
        .models
        .is_empty());
}

#[test]
fn rebuild_drops_a_deleted_model_and_keeps_what_the_disk_cannot_tell() {
    // GIVEN an index where a was recorded from the catalog and pinned
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let a = write(root, "a.gguf", &llama("A"));
    let b = write(root, "b.gguf", &llama("B"));
    let mut index = ModelIndex::default();
    let origin = ModelOrigin::Catalog {
        repo: "org/a-GGUF".to_owned(),
        file: "a.gguf".to_owned(),
    };
    assert_eq!(index.record(root, &a, origin.clone()).unwrap().id, "a");
    let at = Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
    index.pin("a", at).unwrap();

    // WHEN b is deleted from the disk and the index is reconciled
    std::fs::remove_file(&b).unwrap();
    index.rebuild(root).unwrap();

    // THEN b is gone and a kept its origin and pin
    assert_eq!(ids(&index), vec!["a"]);
    let a = index.get("a").unwrap();
    assert_eq!(a.origin, origin);
    assert_eq!(a.pinned_at, Some(at));
}

#[test]
fn two_models_sharing_a_name_get_distinct_identifiers() {
    // GIVEN two files named x.gguf in two folders
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "one/x.gguf", &llama("X"));
    write(root, "two/x.gguf", &llama("X"));

    // WHEN the index is rebuilt
    let index = rebuilt(root);

    // THEN each identifier carries its folder
    assert_eq!(ids(&index), vec!["one/x", "two/x"]);
}

#[test]
fn pin_and_overrides_are_set_cleared_and_persisted() {
    // GIVEN an index with one model and a sibling quantisation of it
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "qwen-q4.gguf", &llama("Qwen"));
    let q8 = write(root, "qwen-q8.gguf", &llama("Qwen"));
    let file = root.join(INDEX_FILE_NAME);
    let mut index = rebuilt(root);
    let at = Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();

    // WHEN the pin and both overrides are set, saved and read back
    index.pin("qwen-q4", at).unwrap();
    index
        .set_category_override("qwen-q4", Some(ModelCategory::Draft))
        .unwrap();
    index
        .set_source_file_override("qwen-q4", Some(q8.clone()))
        .unwrap();
    index.save(&file).unwrap();
    let mut back = ModelIndex::load(&file).unwrap();

    // THEN they are there, and clearing them persists too
    let m = back.get("qwen-q4").unwrap();
    assert_eq!(m.pinned_at, Some(at));
    assert_eq!(m.category_override, Some(ModelCategory::Draft));
    assert_eq!(m.effective_category(), ModelCategory::Draft);
    assert_eq!(m.source_file_override.as_deref(), Some(q8.as_path()));
    back.unpin("qwen-q4").unwrap();
    back.set_category_override("qwen-q4", None).unwrap();
    back.set_source_file_override("qwen-q4", None).unwrap();
    back.save(&file).unwrap();
    let cleared = ModelIndex::load(&file).unwrap();
    let m = cleared.get("qwen-q4").unwrap();
    assert!(m.pinned_at.is_none() && m.category_override.is_none());
    assert!(m.source_file_override.is_none());
    assert!(matches!(
        cleared.clone().pin("absent", at),
        Err(ModelIndexError::UnknownModel { .. })
    ));
}

/// A folder holding a model, its projector, a split model and another model.
fn override_fixture(root: &Path) -> ModelIndex {
    write(root, "qwen.gguf", &llama("Qwen"));
    write(
        root,
        "mmproj-qwen.gguf",
        &gguf(&[("general.architecture", "clip")]),
    );
    write(root, "big-00001-of-00002.gguf", &llama("Big"));
    write(root, "big-00002-of-00002.gguf", b"second part");
    write(root, "mistral.gguf", &llama("Mistral"));
    rebuilt(root)
}

#[test]
fn a_source_override_to_a_projector_is_refused() {
    // GIVEN a model with its projector beside it
    let tmp = tempfile::tempdir().unwrap();
    let mut index = override_fixture(tmp.path());

    // WHEN the source file is pointed at the projector
    let result = index.set_source_file_override("qwen", Some(tmp.path().join("mmproj-qwen.gguf")));

    // THEN it is refused and nothing is recorded
    assert!(matches!(
        result,
        Err(ModelIndexError::SourceIsProjector { .. })
    ));
    assert!(index.get("qwen").unwrap().source_file_override.is_none());
}

#[test]
fn a_source_override_to_a_part_is_refused() {
    // GIVEN a split model beside the model
    let tmp = tempfile::tempdir().unwrap();
    let mut index = override_fixture(tmp.path());

    // WHEN the source file is pointed at the second part of the split model
    let result =
        index.set_source_file_override("big", Some(tmp.path().join("big-00002-of-00002.gguf")));

    // THEN it is refused
    assert!(matches!(result, Err(ModelIndexError::SourceIsPart { .. })));
    assert!(index.get("big").unwrap().source_file_override.is_none());
}

#[test]
fn a_source_override_to_another_model_is_refused() {
    // GIVEN two different models in the same folder
    let tmp = tempfile::tempdir().unwrap();
    let mut index = override_fixture(tmp.path());

    // WHEN the source file of one is pointed at the weights of the other
    let result = index.set_source_file_override("qwen", Some(tmp.path().join("mistral.gguf")));

    // THEN it is refused
    assert!(matches!(
        result,
        Err(ModelIndexError::SourceFromOtherModel { .. })
    ));
    assert!(index.get("qwen").unwrap().source_file_override.is_none());
}

#[test]
fn vision_is_carried_only_by_a_model_the_engine_can_show_an_image() {
    // GIVEN the same GGUF installed twice, once with its projector in its
    //       folder and once alone, and two MLX folders, one multimodal
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let template = "{% if tools %}{{ tools }}{% endif %}<think>";
    let header = gguf(&[
        ("general.architecture", "gemma3"),
        ("tokenizer.chat_template", template),
    ]);
    write(root, "with/gemma-Q4_K_M.gguf", &header);
    write(
        root,
        "with/mmproj-gemma-f16.gguf",
        &gguf(&[("general.architecture", "clip")]),
    );
    write(root, "alone/gemma-Q4_K_M.gguf", &header);
    write(
        root,
        "seeing/config.json",
        br#"{"vision_config":{"hidden_size":1}}"#,
    );
    write(root, "seeing/model.safetensors", b"weights");
    write(root, "blind/config.json", br#"{"model_type":"qwen3"}"#);
    write(root, "blind/model.safetensors", b"weights");

    // WHEN the index is read
    let index = rebuilt(root);
    let caps = |id: &str| {
        index
            .get(id)
            .and_then(|m| m.capabilities.clone())
            .expect("capabilities are computed at rebuild")
    };

    // THEN the GGUF with its projector reads images in the formats of
    //      llama-server, the one without does not, and only the MLX folder
    //      with a vision_config reads images, in the formats of the MLX engine
    let with = caps("with/gemma-Q4_K_M");
    assert!(with.vision);
    assert_eq!(with.image_formats, ["jpeg", "png", "tga", "bmp", "gif"]);
    assert!(with.tools && with.reasoning);
    let alone = caps("alone/gemma-Q4_K_M");
    assert!(!alone.vision);
    assert!(alone.image_formats.is_empty());
    assert!(alone.tools && alone.reasoning);
    let seeing = caps("seeing");
    assert!(seeing.vision);
    assert!(seeing.image_formats.iter().any(|f| f == "webp"));
    assert!(!caps("blind").vision);
}

#[test]
fn an_index_written_before_capabilities_gets_them_at_the_next_rebuild() {
    // GIVEN an index entry without capabilities, the file unchanged since
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        root,
        "a.gguf",
        &gguf(&[
            ("general.architecture", "qwen3"),
            ("tokenizer.chat_template", "{{ tools }}"),
        ]),
    );
    let mut index = rebuilt(root);
    index.models[0].capabilities = None;

    // WHEN the index is rebuilt
    index.rebuild(root).unwrap();

    // THEN the header is read again and the capabilities filled
    let caps = index.models[0].capabilities.clone().expect("filled");
    assert!(caps.tools);
}

/// A GGUF header carrying the given string pairs, then one `u32` pair: what an
/// embeddings conversion writes for `{arch}.pooling_type`.
fn gguf_with_u32(pairs: &[(&str, &str)], key: &str, value: u32) -> Vec<u8> {
    let mut out = gguf(pairs);
    let count = (pairs.len() as u64 + 1).to_le_bytes();
    out[16..24].copy_from_slice(&count);
    out.extend_from_slice(&(key.len() as u64).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&value.to_le_bytes());
    out
}

/// The SHA-256 the registry pins for multilingual-e5-small.
const E5_DIGEST: &str = "0a34067a40f25d3149b36885faa62bee0e5284d0f9edc102acfc00e115d953e8";

#[test]
fn a_gguf_whose_header_carries_a_pooling_key_is_filed_as_embeddings() {
    // GIVEN two qwen3 GGUFs, one carrying an embeddings pooling key (as
    //       Qwen3-Embedding does) and one without (a chat model), and a bert
    //       one pooled by mean (as multilingual-e5-small is)
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        root,
        "qwen3-embedding.gguf",
        &gguf_with_u32(
            &[("general.architecture", "qwen3")],
            "qwen3.pooling_type",
            3,
        ),
    );
    write(
        root,
        "e5.gguf",
        &gguf_with_u32(&[("general.architecture", "bert")], "bert.pooling_type", 1),
    );
    write(
        root,
        "qwen3-chat.gguf",
        &gguf(&[("general.architecture", "qwen3")]),
    );

    // WHEN the index is rebuilt
    let index = rebuilt(root);

    // THEN the pooled ones are embeddings and the chat model a language model
    assert_eq!(
        index.get("qwen3-embedding").unwrap().category,
        ModelCategory::Embeddings
    );
    assert_eq!(index.get("e5").unwrap().category, ModelCategory::Embeddings);
    assert_eq!(
        index.get("qwen3-chat").unwrap().category,
        ModelCategory::Language
    );
}

#[test]
fn a_digest_pinned_by_an_embeddings_entry_files_the_model_as_embeddings() {
    // GIVEN an imported GGUF whose header carries no pooling key
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let path = write(
        root,
        "e5-old-conversion.gguf",
        &gguf(&[("general.architecture", "bert")]),
    );
    let mut index = ModelIndex::default();
    index.record(root, &path, ModelOrigin::Import).unwrap();
    assert_eq!(
        index.get("e5-old-conversion").unwrap().category,
        ModelCategory::Language
    );

    // WHEN its digest, the one the registry pins for e5, is stored, and the
    //      index is rebuilt with the file unchanged
    index.set_digest(&path, E5_DIGEST.to_owned(), true).unwrap();
    let after_digest = index.get("e5-old-conversion").unwrap().category;
    index.rebuild(root).unwrap();

    // THEN the entry is recognised by its digest, before and after the rebuild
    assert_eq!(after_digest, ModelCategory::Embeddings);
    assert_eq!(
        index.get("e5-old-conversion").unwrap().category,
        ModelCategory::Embeddings
    );
}

#[test]
fn a_digest_no_entry_pins_leaves_the_header_category() {
    // GIVEN an imported chat GGUF
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let path = write(root, "qwen.gguf", &llama("Qwen"));
    let mut index = ModelIndex::default();
    index.record(root, &path, ModelOrigin::Import).unwrap();

    // WHEN a digest no embeddings entry pins is stored
    index.set_digest(&path, "1".repeat(64), false).unwrap();

    // THEN it stays a language model
    assert_eq!(index.get("qwen").unwrap().category, ModelCategory::Language);
}

/// A GGUF header of a language model declaring a vocabulary of `tokens`
/// entries of the family `family`.
fn with_vocabulary(family: &str, tokens: u64) -> Vec<u8> {
    let mut out = gguf(&[
        ("general.architecture", "qwen2"),
        ("tokenizer.ggml.model", family),
    ]);
    // One more pair than `gguf` declared: the vocabulary array.
    out[16..24].copy_from_slice(&3u64.to_le_bytes());
    let key = "tokenizer.ggml.tokens";
    out.extend_from_slice(&(key.len() as u64).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&tokens.to_le_bytes());
    for _ in 0..tokens {
        out.extend_from_slice(&1u64.to_le_bytes());
        out.push(b't');
    }
    out
}

/// A card as the catalogue copies it beside a model, marked as a draft model
/// or not.
fn card(draft: bool) -> String {
    let mark = if draft {
        r#", "draft": { "source": "https://example.org/draft" }"#
    } else {
        ""
    };
    format!(
        r#"{{
  "id": "qwen2.5/Small", "kind": "language", "label": "Small", "publisher": "Qwen",
  "license": {{ "id": "apache-2.0", "source": "https://example.org/licence" }},
  "source_url": "https://huggingface.co/Qwen/Small",
  "description": {{ "fr": "Petit.", "en": "Small." }},
  "capabilities": {{ "tools": false, "reasoning": false, "vision": false, "source": "s" }},
  "memory": {{ "min_gb": 0.6, "source": "s" }},
  "defaults": {{ "source": "" }},
  "author_fields": [], "repos": ["Qwen/Small"]{mark}
}}"#
    )
}

#[test]
fn a_model_whose_card_marks_it_is_filed_as_a_draft_and_stays_a_language_model() {
    // GIVEN two language models, each with its card beside it, one card marked
    // as published to draft for a larger model
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "small.gguf", &llama("small"));
    write(root, "small.gguf.card.json", card(true).as_bytes());
    write(root, "plain.gguf", &llama("plain"));
    write(root, "plain.gguf.card.json", card(false).as_bytes());

    // WHEN the index is rebuilt, then again once the mark is gone
    let mut index = rebuilt(root);
    let marked = index.get("small").unwrap().clone();
    let plain = index.get("plain").unwrap().clone();
    write(root, "small.gguf.card.json", card(false).as_bytes());
    index.rebuild(root).unwrap();

    // THEN the marked model is a draft a conversation can still run on, the
    // other is a language model, and the category follows the card
    assert_eq!(marked.category, ModelCategory::Draft);
    assert!(marked.is_language_model());
    assert_eq!(plain.category, ModelCategory::Language);
    assert!(plain.is_language_model());
    assert_eq!(
        index.get("small").unwrap().category,
        ModelCategory::Language
    );
}

#[test]
fn a_model_overridden_as_a_draft_stays_a_language_model_and_others_do_not() {
    // GIVEN a language model the developer mode files as a draft, and another
    // it files as an embeddings model
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "small.gguf", &llama("small"));
    write(root, "vectors.gguf", &llama("vectors"));
    let mut index = rebuilt(root);

    // WHEN the overrides are set
    index
        .set_category_override("small", Some(ModelCategory::Draft))
        .unwrap();
    index
        .set_category_override("vectors", Some(ModelCategory::Embeddings))
        .unwrap();

    // THEN the draft is in force and a conversation can run on it; the
    // embeddings model is no language model
    let small = index.get("small").unwrap();
    assert_eq!(small.effective_category(), ModelCategory::Draft);
    assert!(small.is_language_model());
    assert!(!index.get("vectors").unwrap().is_language_model());
}

#[test]
fn only_the_drafts_of_the_same_vocabulary_are_proposed_for_a_model() {
    // GIVEN a main model of 200 tokens, a draft of the same vocabulary, a
    // draft of 100 tokens, a draft of another family, and a model of the same
    // vocabulary that is not filed as a draft
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "main.gguf", &with_vocabulary("gpt2", 200));
    write(root, "same.gguf", &with_vocabulary("gpt2", 200));
    write(root, "fewer.gguf", &with_vocabulary("gpt2", 100));
    write(root, "family.gguf", &with_vocabulary("llama", 200));
    write(root, "sibling.gguf", &with_vocabulary("gpt2", 200));
    let mut index = rebuilt(root);
    for id in ["same", "fewer", "family"] {
        index
            .set_category_override(id, Some(ModelCategory::Draft))
            .unwrap();
    }

    // WHEN the drafts of the main model are listed, and one is looked up by
    // the weights about to be loaded
    let proposed: Vec<&str> = index
        .compatible_drafts("main")
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    let main_path = root.join("main.gguf");
    let found = index.draft_for(&main_path, "same").map(|m| m.id.clone());
    let refused = index.draft_for(&main_path, "fewer").map(|m| m.id.clone());

    // THEN only the draft of the same vocabulary is proposed, and never the
    // model itself
    assert_eq!(proposed, ["same"]);
    assert_eq!(found.as_deref(), Some("same"));
    assert_eq!(refused, None);
    assert!(index.compatible_drafts("absent").is_empty());
}
