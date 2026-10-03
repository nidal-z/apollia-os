use super::*;

/// A minimal GGUF header naming `arch`, with no tensor.
fn gguf(arch: &str) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes());
    let key = "general.architecture";
    out.extend_from_slice(&(key.len() as u64).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(arch.len() as u64).to_le_bytes());
    out.extend_from_slice(arch.as_bytes());
    out
}

/// A data directory whose model index holds the e5 entry's file.
fn data_dir_with_e5() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let models = crate::installed_models::models_dir(tmp.path());
    std::fs::create_dir_all(&models).expect("models dir");
    std::fs::write(models.join("multilingual-e5-small-q8_0.gguf"), gguf("bert")).expect("write");
    let mut index = ModelIndex::default();
    index.rebuild(&models).expect("rebuild");
    index.save(&models.join(INDEX_FILE_NAME)).expect("save");
    tmp
}

#[tokio::test]
async fn test_without_a_default_the_embedder_answers_no_model() {
    // GIVEN a data directory where no embeddings model was ever chosen
    let tmp = tempfile::tempdir().expect("tempdir");
    let embedder = LocalEmbedder::new(tmp.path().to_path_buf(), None);

    // WHEN texts are embedded
    let err = embedder
        .embed(&["invoice".to_owned()], EmbedPurpose::Query)
        .await
        .expect_err("nothing to embed with");

    // THEN the error is the typed "no embeddings model", which a caller reads
    // as "keywords alone"
    assert_eq!(err, EmbeddingError::NoModel);
    assert!(err.falls_back_to_keywords());
    assert_eq!(default_model(tmp.path()).expect("readable"), None);
}

#[test]
fn test_only_a_registry_entry_can_become_the_default_and_it_persists() {
    // GIVEN an empty data directory
    let tmp = tempfile::tempdir().expect("tempdir");

    // WHEN an unknown identifier, then a registry entry, are set as default
    let refused = set_default_model(tmp.path(), "not-a-model");
    set_default_model(tmp.path(), "qwen3-embedding-0.6b").expect("a registry entry");

    // THEN the first is refused, and the second is read back from system.db
    assert!(matches!(refused, Err(SetDefaultError::UnknownModel(id)) if id == "not-a-model"));
    assert_eq!(
        default_model(tmp.path()).expect("readable").as_deref(),
        Some("qwen3-embedding-0.6b")
    );
}

#[tokio::test]
async fn test_a_default_whose_file_is_absent_is_not_installed() {
    // GIVEN a default set but no model file on the machine
    let tmp = tempfile::tempdir().expect("tempdir");
    set_default_model(tmp.path(), "multilingual-e5-small").expect("set");
    let embedder = LocalEmbedder::new(tmp.path().to_path_buf(), None);

    // WHEN texts are embedded
    let err = embedder
        .embed(&["invoice".to_owned()], EmbedPurpose::Query)
        .await
        .expect_err("no file");

    // THEN the error names the model as not installed, and keywords stay
    assert_eq!(
        err,
        EmbeddingError::NotInstalled {
            model: "multilingual-e5-small".to_owned()
        }
    );
    assert!(err.falls_back_to_keywords());
}

#[tokio::test]
async fn test_embedding_with_the_default_keeps_the_chat_model_loaded() {
    // GIVEN the e5 entry installed and chosen, a chat model loaded, and the
    // residency ceiling at one
    let data = data_dir_with_e5();
    set_default_model(data.path(), "multilingual-e5-small").expect("set");
    let engine_dir = tempfile::tempdir().expect("tempdir");
    let sup = LlamaServerSupervisor::with_binary_for_test(
        crate::llama_server::fake_engine(engine_dir.path()),
        1,
        &["/models/chat.gguf"],
    )
    .await;
    let embedder = LocalEmbedder::new(data.path().to_path_buf(), Some(Arc::clone(&sup)));

    // WHEN two queries are embedded
    let vectors = embedder
        .embed(
            &["march invoice".to_owned(), "contract".to_owned()],
            EmbedPurpose::Query,
        )
        .await
        .expect("embedded");

    // THEN two unit vectors come back, the engine got the query prefix and the
    // pooling of the entry, and the chat model is still loaded
    assert_eq!(vectors.len(), 2);
    for v in &vectors {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "{v:?}");
    }
    let launched = crate::llama_server::fake_engine_launches(engine_dir.path());
    assert!(launched[0].contains("multilingual-e5-small-q8_0.gguf --embeddings --pooling mean"));
    let bodies = std::fs::read_to_string(engine_dir.path().join("bodies.txt")).expect("bodies");
    assert!(bodies.contains("\"query: march invoice\""), "{bodies}");
    let loaded = sup.loaded_models().await;
    assert!(loaded
        .iter()
        .any(|m| m.model_path == "/models/chat.gguf"
            && m.kind == crate::llama_server::ModelKind::Chat));
    sup.shutdown_in_place().await;
}
