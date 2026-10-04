use std::sync::Mutex;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::store::MemoryStore;

use super::*;

/// An embedder whose model can be swapped; every text gets the same unit
/// vector of the model's dimension.
struct SwitchableEmbedder {
    model: Mutex<EmbeddingsModel>,
}

impl SwitchableEmbedder {
    fn new(id: &str, dimensions: u32) -> Self {
        Self {
            model: Mutex::new(EmbeddingsModel {
                id: id.to_owned(),
                dimensions,
            }),
        }
    }

    fn switch(&self, id: &str, dimensions: u32) {
        *self.model.lock().unwrap() = EmbeddingsModel {
            id: id.to_owned(),
            dimensions,
        };
    }

    fn current(&self) -> EmbeddingsModel {
        self.model.lock().unwrap().clone()
    }
}

impl MemoryEmbedder for SwitchableEmbedder {
    fn model(&self) -> EmbedFuture<'_, EmbeddingsModel> {
        let model = self.current();
        Box::pin(async move { Ok(model) })
    }

    fn embed<'a>(&'a self, texts: &'a [String], _role: EmbedRole) -> EmbedFuture<'a, Embedded> {
        let model = self.current();
        Box::pin(async move {
            let mut unit = vec![0.0f32; model.dimensions as usize];
            unit[0] = 1.0;
            Ok(Embedded {
                vectors: vec![unit; texts.len()],
                model,
            })
        })
    }
}

fn record(dir: &Path, content: &str) {
    let store = MemoryStore::open(&dir.join("agent.db")).unwrap();
    EpisodicMemory::new(&store)
        .record("agent", "agent", content, 0.5, None, None, None)
        .unwrap();
}

/// `(model, dimension)` of every stored vector.
fn vectors(dir: &Path) -> Vec<(String, i64)> {
    let path = dir.join("agent.db");
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT model, dimension FROM memory_vectors")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Asks the actor for a pass and waits for its count.
async fn run_pass(mailbox: &mpsc::Sender<Pass>) -> usize {
    let (reply, answer) = oneshot::channel();
    mailbox.send(Pass(Some(reply))).await.unwrap();
    answer.await.unwrap()
}

#[tokio::test]
async fn test_the_actor_vectorizes_new_entries_and_again_after_a_change_of_model() {
    // GIVEN a memory with two entries written before any vector, and the
    // actor started with a model of 3 dimensions
    let dir = tempfile::tempdir().unwrap();
    record(dir.path(), "first");
    record(dir.path(), "second");
    let embedder = Arc::new(SwitchableEmbedder::new("small", 3));
    let mailbox = MemoryVectorsActor::spawn(
        dir.path().to_path_buf(),
        Arc::clone(&embedder) as Arc<dyn MemoryEmbedder>,
    );

    // WHEN a pass has run
    run_pass(&mailbox).await;

    // THEN both entries carry a vector of that model
    assert_eq!(vectors(dir.path()), vec![("small".to_owned(), 3); 2]);

    // WHEN an entry is written and a pass runs
    record(dir.path(), "third");
    let stored = run_pass(&mailbox).await;

    // THEN the new entry alone is vectorized
    assert_eq!(stored, 1);

    // WHEN another model, of another dimension, is installed
    embedder.switch("large", 5);
    let stored = run_pass(&mailbox).await;

    // THEN every entry is vectorized again in the new dimension
    assert_eq!(stored, 3);
    assert_eq!(vectors(dir.path()), vec![("large".to_owned(), 5); 3]);
}

#[tokio::test]
async fn test_without_a_default_the_memory_embedder_is_unavailable() {
    // GIVEN a data directory where no embeddings model was chosen
    let tmp = tempfile::tempdir().unwrap();
    let embedder = LocalEmbedder::new(tmp.path().to_path_buf(), None);

    // WHEN the memory asks for the model and for a vector
    let model = MemoryEmbedder::model(&embedder).await;
    let texts = ["facture".to_owned()];
    let vectors = MemoryEmbedder::embed(&embedder, &texts, EmbedRole::Query).await;

    // THEN both answer "unavailable", which the memory reads as "words alone"
    assert!(matches!(model, Err(EmbedError::Unavailable(_))));
    assert!(matches!(vectors, Err(EmbedError::Unavailable(_))));
}

#[test]
fn test_only_a_failing_engine_is_a_failure() {
    // GIVEN an engine that answered an error, and a machine without a model
    let failed = EmbeddingError::Transport("reset".to_owned());
    let absent = EmbeddingError::NoModel;

    // WHEN they are read by the memory
    let failed = embed_error(failed);
    let absent = embed_error(absent);

    // THEN only the first is a failure
    assert!(matches!(failed, EmbedError::Failed(_)));
    assert!(matches!(absent, EmbedError::Unavailable(_)));
}
