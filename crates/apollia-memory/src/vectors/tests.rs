use std::sync::Mutex;

use super::*;
use crate::episodic::EpisodicMemory;
use crate::search::{MemorySearch, SearchQuery};
use crate::semantic::{RememberInput, SemanticMemory};

/// An embedder that knows two concepts: money (`invoice`, `paid`,
/// `settlement`) and meetings (`meeting`, `appointment`). Texts sharing a
/// concept are close; others are far apart. Its model can be swapped, and
/// a model of more dimensions pads the vector with zeros.
struct ConceptEmbedder {
    model: Mutex<Option<EmbeddingsModel>>,
}

impl ConceptEmbedder {
    fn with(id: &str, dimensions: u32) -> Self {
        Self {
            model: Mutex::new(Some(EmbeddingsModel {
                id: id.to_owned(),
                dimensions,
            })),
        }
    }

    fn none() -> Self {
        Self {
            model: Mutex::new(None),
        }
    }

    fn switch(&self, id: &str, dimensions: u32) {
        *self.model.lock().unwrap() = Some(EmbeddingsModel {
            id: id.to_owned(),
            dimensions,
        });
    }

    fn current(&self) -> Result<EmbeddingsModel, EmbedError> {
        self.model
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| EmbedError::Unavailable("no model".to_owned()))
    }
}

fn concept_vector(text: &str, dimensions: u32) -> Vec<f32> {
    let t = text.to_lowercase();
    let money = ["invoice", "paid", "payment", "settle", "transfer"]
        .iter()
        .any(|w| t.contains(w));
    let meeting = ["meeting", "appointment"].iter().any(|w| t.contains(w));
    let mut v = vec![0.0f32; dimensions as usize];
    v[0] = if money { 1.0 } else { 0.0 };
    v[1] = if meeting { 1.0 } else { 0.0 };
    v[2] = 0.1;
    apollia_core::vector_search::normalize(&mut v);
    v
}

impl MemoryEmbedder for ConceptEmbedder {
    fn model(&self) -> EmbedFuture<'_, EmbeddingsModel> {
        let model = self.current();
        Box::pin(async move { model })
    }

    fn embed<'a>(&'a self, texts: &'a [String], _role: EmbedRole) -> EmbedFuture<'a, Embedded> {
        let model = self.current();
        Box::pin(async move {
            let model = model?;
            let vectors = texts
                .iter()
                .map(|t| concept_vector(t, model.dimensions))
                .collect();
            Ok(Embedded { model, vectors })
        })
    }
}

fn memory_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn seed(dir: &Path) -> PathBuf {
    let path = dir.join("agent.db");
    let store = MemoryStore::open(&path).unwrap();
    EpisodicMemory::new(&store)
        .record(
            "agent",
            "agent",
            "Customer Dupont paid the March invoice",
            0.8,
            None,
            None,
            None,
        )
        .unwrap();
    SemanticMemory::new(&store)
        .remember(RememberInput {
            namespace: "agent",
            key: "next_meeting",
            value: &serde_json::json!("meeting on Monday with Martin"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
    path
}

fn vectors_of(path: &Path) -> Vec<(String, String, String, i64)> {
    let store = MemoryStore::open(path).unwrap();
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT source_table, source_id, model, dimension FROM memory_vectors \
             ORDER BY source_table",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn search(path: &Path, query: &str, meaning: Option<&QueryVector>) -> Vec<String> {
    let store = MemoryStore::open(path).unwrap();
    MemorySearch::new(&store)
        .query(SearchQuery {
            namespace: "agent",
            query,
            limit: 10,
            sources: None,
            min_importance: None,
            meaning,
        })
        .unwrap()
        .into_iter()
        .map(|r| r.content)
        .collect()
}

#[tokio::test]
async fn test_written_entries_get_a_vector_when_a_model_is_present() {
    // GIVEN an episode and a fact written, and an embeddings model present
    let dir = memory_dir();
    let path = seed(dir.path());
    assert!(vectors_of(&path).is_empty());
    let embedder = ConceptEmbedder::with("concept-small", 3);

    // WHEN the background pass runs
    let stored = vectorize_dir(dir.path(), &embedder).await;

    // THEN both entries carry a vector of that model and dimension
    assert_eq!(stored, 2);
    let vectors = vectors_of(&path);
    assert_eq!(vectors.len(), 2);
    assert_eq!(vectors[0].0, "episodic");
    assert_eq!(vectors[1].0, "semantic");
    assert!(vectors.iter().all(|v| v.2 == "concept-small" && v.3 == 3));
}

#[tokio::test]
async fn test_no_vector_is_computed_without_a_model() {
    // GIVEN entries and no embeddings model on this machine
    let dir = memory_dir();
    let path = seed(dir.path());

    // WHEN the background pass runs
    let stored = vectorize_dir(dir.path(), &ConceptEmbedder::none()).await;

    // THEN nothing is stored and the memory stays searchable by words
    assert_eq!(stored, 0);
    assert!(vectors_of(&path).is_empty());
    assert_eq!(search(&path, "invoice", None).len(), 1);
}

#[tokio::test]
async fn test_a_second_pass_has_nothing_left_to_do() {
    // GIVEN a memory whose entries all have a vector
    let dir = memory_dir();
    seed(dir.path());
    let embedder = ConceptEmbedder::with("concept-small", 3);
    vectorize_dir(dir.path(), &embedder).await;

    // WHEN the pass runs again with the same model
    let stored = vectorize_dir(dir.path(), &embedder).await;

    // THEN it stores nothing
    assert_eq!(stored, 0);
}

#[tokio::test]
async fn test_a_change_of_model_revectorizes_every_entry_in_the_new_dimension() {
    // GIVEN a memory vectorized by one model
    let dir = memory_dir();
    let path = seed(dir.path());
    let embedder = ConceptEmbedder::with("concept-small", 3);
    vectorize_dir(dir.path(), &embedder).await;

    // WHEN another model of another dimension is installed and the pass runs
    embedder.switch("concept-large", 4);
    let stored = vectorize_dir(dir.path(), &embedder).await;

    // THEN every entry carries a vector of the new model and dimension
    assert_eq!(stored, 2);
    assert!(vectors_of(&path)
        .iter()
        .all(|v| v.2 == "concept-large" && v.3 == 4));
}

#[tokio::test]
async fn test_hybrid_search_finds_an_entry_by_its_meaning() {
    // GIVEN a vectorized memory, and a query sharing no word with the episode
    let dir = memory_dir();
    let path = seed(dir.path());
    let embedder = ConceptEmbedder::with("concept-small", 3);
    vectorize_dir(dir.path(), &embedder).await;
    let query = "settlement received";

    // WHEN it is searched by words alone, then with its vector
    let by_words = search(&path, query, None);
    let meaning = query_vector(Some(&embedder), query).await.unwrap();
    let hybrid = search(&path, query, Some(&meaning));

    // THEN only the hybrid search finds the payment, ranked first
    assert!(by_words.is_empty());
    assert_eq!(hybrid[0], "Customer Dupont paid the March invoice");
}

#[tokio::test]
async fn test_hybrid_search_keeps_the_entries_found_by_words() {
    // GIVEN a vectorized memory
    let dir = memory_dir();
    let path = seed(dir.path());
    let embedder = ConceptEmbedder::with("concept-small", 3);
    vectorize_dir(dir.path(), &embedder).await;

    // WHEN a query matching the fact by its words is searched with its vector
    let meaning = query_vector(Some(&embedder), "Martin").await.unwrap();
    let hybrid = search(&path, "Martin", Some(&meaning));

    // THEN the fact found by words is in the results
    assert!(hybrid.iter().any(|c| c.contains("Martin")));
}

#[tokio::test]
async fn test_a_query_vector_of_another_model_searches_by_words_alone() {
    // GIVEN a memory vectorized by one model, and a query vector of another
    let dir = memory_dir();
    let path = seed(dir.path());
    vectorize_dir(dir.path(), &ConceptEmbedder::with("concept-small", 3)).await;
    let other = query_vector(Some(&ConceptEmbedder::with("other", 3)), "settlement")
        .await
        .unwrap();

    // WHEN the search runs
    let results = search(&path, "settlement", Some(&other));

    // THEN no vector is compared, and no word matches
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_query_vector_is_none_without_embedder_or_model() {
    // GIVEN no embedder, then an embedder without a model
    let none = ConceptEmbedder::none();

    // WHEN the query vector is asked
    let without_embedder = query_vector(None, "invoice").await;
    let without_model = query_vector(Some(&none), "invoice").await;

    // THEN the search goes on by words alone
    assert!(without_embedder.is_none());
    assert!(without_model.is_none());
}

#[tokio::test]
async fn test_a_vector_goes_when_its_entry_is_deleted_or_its_fact_changes() {
    // GIVEN a vectorized memory
    let dir = memory_dir();
    let path = seed(dir.path());
    vectorize_dir(dir.path(), &ConceptEmbedder::with("concept-small", 3)).await;
    let store = MemoryStore::open(&path).unwrap();
    let episode_id: String = store
        .conn()
        .query_row("SELECT id FROM episodic_memories", [], |r| r.get(0))
        .unwrap();

    // WHEN the episode is deleted and the fact gets a new value
    assert!(store.delete_entry_by_id(&episode_id).unwrap());
    SemanticMemory::new(&store)
        .remember(RememberInput {
            namespace: "agent",
            key: "next_meeting",
            value: &serde_json::json!("appointment on Tuesday"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();

    // THEN no vector is left, and the changed fact is pending again
    assert!(vectors_of(&path).is_empty());
    let pending = pending_entries(
        &store,
        &EmbeddingsModel {
            id: "concept-small".to_owned(),
            dimensions: 3,
        },
        10,
    )
    .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, "next_meeting \"appointment on Tuesday\"");
}

#[test]
fn test_a_long_entry_is_embedded_by_its_beginning() {
    // GIVEN an entry longer than the model reads
    let text = '\u{e9}'.to_string().repeat(MAX_EMBEDDED_CHARS + 10);

    // WHEN its excerpt is taken
    let cut = excerpt(&text);

    // THEN it stops at the limit, on a character boundary
    assert_eq!(cut.chars().count(), MAX_EMBEDDED_CHARS);
}

#[test]
fn test_a_version_3_database_gains_the_vectors_table() {
    // GIVEN a namespace database as version 3 left it, with an entry
    let dir = memory_dir();
    let path = seed(dir.path());
    {
        let store = MemoryStore::open(&path).unwrap();
        store
            .conn()
            .execute_batch(
                "DROP TRIGGER memory_vectors_episodic_deleted;
                 DROP TRIGGER memory_vectors_episodic_changed;
                 DROP TRIGGER memory_vectors_semantic_deleted;
                 DROP TRIGGER memory_vectors_semantic_changed;
                 DROP TABLE memory_vectors;
                 UPDATE _schema_version SET version = 3;",
            )
            .unwrap();
    }

    // WHEN this binary opens it
    let store = MemoryStore::open(&path).unwrap();

    // THEN it is at version 4, keeps its entries, and they are all pending
    assert_eq!(store.schema_version().unwrap(), 4);
    let model = EmbeddingsModel {
        id: "concept-small".to_owned(),
        dimensions: 3,
    };
    assert_eq!(pending_entries(&store, &model, 10).unwrap().len(), 2);
}
