use apollia_llm::ModelOrigin;

use super::*;

#[test]
fn test_the_installed_list_reads_the_index_and_sees_subfolders() {
    // GIVEN a GGUF kept in a sub-folder of the models folder, the shape the
    //       listing at the top level used to miss
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::create_dir_all(dir.join("vendor")).unwrap();
    std::fs::write(dir.join("vendor/deep.gguf"), b"GGUF").unwrap();

    // WHEN the installed models are listed
    let list = list_in(dir, &[]).unwrap();

    // THEN it is among them, read from the index, which is now written
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "deep");
    assert_eq!(list[0].name, "deep.gguf");
    assert!(index_path(dir).is_file());
}

#[test]
fn test_the_installed_list_carries_the_verified_origin() {
    // GIVEN two indexed models, one whose digest matched its repository and
    //       one imported with an unknown digest
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("checked.gguf"), b"GGUF1").unwrap();
    std::fs::write(dir.join("from-key.gguf"), b"GGUF2").unwrap();
    let mut index = ModelIndex::default();
    let origin = ModelOrigin::Catalog {
        repo: "org/repo".into(),
        file: "checked.gguf".into(),
    };
    index
        .record(dir, &dir.join("checked.gguf"), origin)
        .unwrap();
    index
        .set_digest(&dir.join("checked.gguf"), "a".repeat(64), true)
        .unwrap();
    index
        .record(dir, &dir.join("from-key.gguf"), ModelOrigin::Import)
        .unwrap();
    index
        .set_digest(&dir.join("from-key.gguf"), "b".repeat(64), false)
        .unwrap();
    index.save(&index_path(dir)).unwrap();

    // WHEN the installed models are listed
    let list = list_in(dir, &[]).unwrap();

    // THEN each carries its origin_verified field, as the index holds it
    let json = serde_json::to_value(&list).unwrap();
    assert_eq!(json[0]["name"], "checked.gguf");
    assert_eq!(json[0]["origin_verified"], true);
    assert_eq!(json[1]["name"], "from-key.gguf");
    assert_eq!(json[1]["origin_verified"], false);
}

#[test]
fn test_a_gguf_dictation_model_is_listed_as_stt() {
    // GIVEN a dictation model saved as .gguf, whose header names whisper
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3u32.to_le_bytes());
    header.extend_from_slice(&0u64.to_le_bytes());
    header.extend_from_slice(&1u64.to_le_bytes());
    header.extend_from_slice(&20u64.to_le_bytes());
    header.extend_from_slice(b"general.architecture");
    header.extend_from_slice(&8u32.to_le_bytes());
    header.extend_from_slice(&7u64.to_le_bytes());
    header.extend_from_slice(b"whisper");
    std::fs::write(dir.join("whisper-fr.gguf"), header).unwrap();

    // WHEN the installed models are listed
    let list = list_in(dir, &[]).unwrap();

    // THEN it is filed as STT: the header decides, where the extension alone
    //      used to file every .gguf as an LLM
    let json = serde_json::to_value(&list).unwrap();
    assert_eq!(json[0]["kind"], "stt");
    assert_eq!(json[0]["category"], "dictation");
}

#[test]
fn test_a_model_a_backend_references_is_marked_in_use() {
    // GIVEN a model file and a reference to it by an LLM backend
    let refs = [ModelRef {
        name: "qwen.gguf".to_owned(),
        label: "LLM backend 'local'".to_owned(),
    }];

    // WHEN its owner is looked up, and that of another file
    let owner = used_by(Path::new("/m/sub/qwen.gguf"), &refs);
    let other = used_by(Path::new("/m/other.gguf"), &refs);

    // THEN the backend is named for the first and nobody for the second
    assert_eq!(owner.as_deref(), Some("LLM backend 'local'"));
    assert_eq!(other, None);
}

#[test]
fn test_a_downloaded_model_names_its_source_page_and_an_imported_one_none() {
    // GIVEN a model downloaded from a catalogue repository and one imported
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("fetched.gguf"), b"GGUF1").unwrap();
    std::fs::write(dir.join("brought.gguf"), b"GGUF2").unwrap();
    let mut index = ModelIndex::default();
    let origin = ModelOrigin::Catalog {
        repo: "org/model-GGUF".into(),
        file: "fetched.gguf".into(),
    };
    index
        .record(dir, &dir.join("fetched.gguf"), origin)
        .unwrap();
    index
        .record(dir, &dir.join("brought.gguf"), ModelOrigin::Import)
        .unwrap();
    index.save(&index_path(dir)).unwrap();

    // WHEN the installed models are listed
    let list = list_in(dir, &[]).unwrap();

    // THEN the downloaded one points at its repository page, the other at nothing
    let by_name = |n: &str| list.iter().find(|m| m.name == n).unwrap();
    assert_eq!(
        by_name("fetched.gguf").source_url.as_deref(),
        Some("https://huggingface.co/org/model-GGUF")
    );
    assert_eq!(by_name("brought.gguf").source_url, None);
}

#[test]
fn test_an_embeddings_file_of_the_registry_is_named_by_its_entry() {
    // GIVEN the file of a registry embeddings entry, filed as embeddings, and a
    //       language model carrying no entry
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("multilingual-e5-small-q8_0.gguf"), b"GGUF1").unwrap();
    std::fs::write(dir.join("chat.gguf"), b"GGUF2").unwrap();
    let mut index = ModelIndex::default();
    let id = index
        .record(
            dir,
            &dir.join("multilingual-e5-small-q8_0.gguf"),
            ModelOrigin::Import,
        )
        .unwrap()
        .id
        .clone();
    index
        .set_category_override(&id, Some(ModelCategory::Embeddings))
        .unwrap();
    index
        .record(dir, &dir.join("chat.gguf"), ModelOrigin::Import)
        .unwrap();
    index.save(&index_path(dir)).unwrap();

    // WHEN the installed models are listed
    let list = list_in(dir, &[]).unwrap();

    // THEN the embeddings file names its registry entry, the other none
    let by_name = |n: &str| list.iter().find(|m| m.name == n).unwrap();
    assert_eq!(
        by_name("multilingual-e5-small-q8_0.gguf")
            .embedding_entry
            .as_deref(),
        Some("multilingual-e5-small")
    );
    assert_eq!(by_name("chat.gguf").embedding_entry, None);
}

#[test]
fn test_the_default_chat_and_search_models_are_marked() {
    // GIVEN a language model, an embeddings file of the registry and another
    //       language model, the first the default of chat and the registry
    //       entry the default of search
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    for name in ["a.gguf", "b.gguf", "multilingual-e5-small-q8_0.gguf"] {
        std::fs::write(dir.join(name), name.as_bytes()).unwrap();
    }
    let mut index = ModelIndex::default();
    index
        .record(dir, &dir.join("a.gguf"), ModelOrigin::Import)
        .unwrap();
    index
        .record(dir, &dir.join("b.gguf"), ModelOrigin::Import)
        .unwrap();
    let id = index
        .record(
            dir,
            &dir.join("multilingual-e5-small-q8_0.gguf"),
            ModelOrigin::Import,
        )
        .unwrap()
        .id
        .clone();
    index
        .set_category_override(&id, Some(ModelCategory::Embeddings))
        .unwrap();
    index.save(&index_path(dir)).unwrap();
    let mut list = list_in(dir, &[]).unwrap();

    // WHEN the defaults are marked
    mark_defaults(
        &mut list,
        Some(&dir.join("a.gguf")),
        Some("multilingual-e5-small"),
    );

    // THEN only the chat default and the search default carry a mark
    let marks: Vec<(&str, bool, bool)> = list
        .iter()
        .map(|m| (m.name.as_str(), m.default_chat, m.default_search))
        .collect();
    assert_eq!(
        marks,
        vec![
            ("a.gguf", true, false),
            ("b.gguf", false, false),
            ("multilingual-e5-small-q8_0.gguf", false, true),
        ]
    );
}
