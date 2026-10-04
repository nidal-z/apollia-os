use apollia_memory::injection_tracker::{global_record, InjectedEntry};
use apollia_memory::semantic::RememberInput;
use tempfile::TempDir;

use super::*;

/// A memory directory holding the namespace `agent-x`, with one episode and
/// one fact.
fn seeded() -> TempDir {
    let dir = TempDir::new().unwrap();
    let store = MemoryStore::open(&dir.path().join("agent-x.db")).unwrap();
    EpisodicMemory::new(&store)
        .record(
            "agent-x",
            "agent-x",
            "Quote sent to Dupont",
            0.8,
            None,
            None,
            None,
        )
        .unwrap();
    SemanticMemory::new(&store)
        .remember(RememberInput {
            namespace: "agent-x",
            key: "client.dupont.budget",
            value: &serde_json::json!(15000),
            confidence: 0.9,
            source: None,
            expires_at: None,
        })
        .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a namespace").unwrap();
    dir
}

#[tokio::test]
async fn test_namespaces_are_the_db_files_sorted_and_a_missing_directory_has_none() {
    // GIVEN a memory directory with two namespaces and a stray file
    let dir = seeded();
    MemoryStore::open(&dir.path().join("a-first.db")).unwrap();

    // WHEN the namespaces are listed, then those of a directory never created
    let listed = list_namespaces(dir.path().to_path_buf()).await.unwrap();
    let none = list_namespaces(dir.path().join("absent")).await.unwrap();

    // THEN only the database files count, in order, and the absent directory
    // is empty rather than an error
    assert_eq!(listed, ["a-first", "agent-x"]);
    assert!(none.is_empty());
}

#[tokio::test]
async fn test_entries_of_a_namespace_carry_each_type() {
    // GIVEN a namespace holding one episode and one fact
    let dir = seeded();

    // WHEN its entries are listed
    let entries = namespace_entries(dir.path().to_path_buf(), "agent-x".to_owned())
        .await
        .unwrap();

    // THEN both come back with their type, key and value
    let mut types: Vec<&str> = entries.iter().map(|e| e.entry_type.as_str()).collect();
    types.sort_unstable();
    assert_eq!(types, ["episodic", "semantic"]);
    let fact = entries.iter().find(|e| e.entry_type == "semantic").unwrap();
    assert_eq!(fact.key, "client.dupont.budget");
    assert_eq!(fact.value, "15000");
}

#[tokio::test]
async fn test_a_search_finds_the_episode_and_an_unknown_namespace_has_no_result() {
    // GIVEN a namespace whose episode mentions Dupont
    let dir = seeded();

    // WHEN Dupont is searched there, then in a namespace without a file
    let found = search(
        dir.path().to_path_buf(),
        "agent-x".to_owned(),
        "Dupont".to_owned(),
    )
    .await
    .unwrap();
    let none = search(
        dir.path().to_path_buf(),
        "ghost".to_owned(),
        "Dupont".to_owned(),
    )
    .await
    .unwrap();

    // THEN the episode is found, and the unknown namespace is empty
    assert!(found
        .iter()
        .any(|r| r.entry_type == "episodic" && r.content.contains("Dupont")));
    assert!(none.is_empty());
}

#[tokio::test]
async fn test_a_namespace_that_names_a_path_is_refused() {
    // GIVEN a memory directory
    let dir = seeded();

    // WHEN a namespace climbing out of it is asked for
    let error = namespace_entries(dir.path().join("sub"), "../agent-x".to_owned())
        .await
        .unwrap_err();

    // THEN it is refused before any file is opened
    assert!(matches!(error, MemoryViewError::InvalidNamespace(_)));
    assert_eq!(
        error.to_string(),
        "VALIDATION_ERROR: invalid memory namespace '../agent-x'"
    );
}

#[test]
fn test_injected_entries_are_read_from_the_tracker_of_this_process() {
    // GIVEN a turn with two entries recorded by the agent bridge
    let turn = "turn-memory-view-test";
    for (id, score) in [("sem-1", 0.92_f32), ("ep-1", 0.71)] {
        global_record(
            turn,
            InjectedEntry {
                id: id.to_owned(),
                content_preview: "client dupont budget 15000".to_owned(),
                namespace: "agent-x".to_owned(),
                injection_reason: "Matched query: budget client".to_owned(),
                relevance_score: score,
            },
        );
    }

    // WHEN the entries of that turn, then of a turn never seen, are read
    let entries = injected_entries(turn);
    let none = injected_entries("turn-memory-view-never-seen");

    // THEN both entries come back with their fields, the unknown turn has none
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, "sem-1");
    assert_eq!(entries[0].namespace, "agent-x");
    assert!((entries[0].relevance_score - 0.92).abs() < f32::EPSILON);
    assert_eq!(entries[1].id, "ep-1");
    assert!(none.is_empty());
}

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

#[tokio::test]
async fn test_the_memory_names_the_embeddings_model_only_when_it_is_installed() {
    // GIVEN a data directory without a default embeddings model
    let data = TempDir::new().unwrap();
    let none = embeddings_model(data.path().to_path_buf()).await.unwrap();

    // WHEN a model is chosen whose file is absent, then once its file is in
    // the model index
    crate::embeddings::set_default_model(data.path(), "multilingual-e5-small").unwrap();
    let absent = embeddings_model(data.path().to_path_buf()).await.unwrap();
    let models = crate::installed_models::models_dir(data.path());
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join("multilingual-e5-small-q8_0.gguf"), gguf("bert")).unwrap();
    let mut index = apollia_llm::ModelIndex::default();
    index.rebuild(&models).unwrap();
    index
        .save(&models.join(apollia_llm::model_index::INDEX_FILE_NAME))
        .unwrap();
    let installed = embeddings_model(data.path().to_path_buf()).await.unwrap();

    // THEN the memory names no model until its file is there, then names it
    // with the dimension of its vectors
    assert_eq!(none, MemoryEmbeddingsView::default());
    assert_eq!(absent, MemoryEmbeddingsView::default());
    assert_eq!(installed.model.as_deref(), Some("multilingual-e5-small"));
    assert_eq!(installed.dimensions, Some(384));
}

#[test]
fn test_an_unknown_or_uninstalled_model_reads_as_words_alone() {
    // GIVEN what a daemon may answer for its default embeddings model
    // WHEN the desktop reads it
    let unknown = embeddings_view(Some("not-a-model"), true);
    let uninstalled = embeddings_view(Some("multilingual-e5-small"), false);
    let unset = embeddings_view(None, false);

    // THEN none of them names a model
    assert_eq!(unknown, MemoryEmbeddingsView::default());
    assert_eq!(uninstalled, MemoryEmbeddingsView::default());
    assert_eq!(unset, MemoryEmbeddingsView::default());
}
