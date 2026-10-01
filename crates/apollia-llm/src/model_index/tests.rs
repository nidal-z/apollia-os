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
