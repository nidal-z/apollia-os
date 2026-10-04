use std::future::Future;
use std::time::Instant;

use apollia_core::vector_search::decode_vector;
use apollia_llm::embeddings_client::EmbeddingError;

use super::*;
use crate::chunk::{chunk_document, Location, Section};

const MODEL: &str = "concept-test-model";
const DIMENSIONS: usize = 64;

/// Words that mean the same thing share one dimension; any other word lands
/// on a dimension of its own among the rest, by a stable hash.
const SYNONYMS: &[&[&str]] = &[
    &["car", "vehicle", "automobile"],
    &["salary", "pay", "wage"],
    &["holiday", "vacation", "leave"],
    &["lawyer", "counsel", "attorney"],
];

/// A stand-in for an embeddings model: one dimension per meaning, so a
/// synonym lands where its twin does and an unrelated word does not.
fn concept_vector(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; DIMENSIONS];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let word = word.to_lowercase();
        let dimension = SYNONYMS
            .iter()
            .position(|group| group.contains(&word.as_str()))
            .unwrap_or_else(|| {
                let hash = word
                    .bytes()
                    .fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
                SYNONYMS.len() + (hash as usize % (DIMENSIONS - SYNONYMS.len()))
            });
        vector[dimension] += 1.0;
    }
    assert!(
        apollia_core::vector_search::normalize(&mut vector),
        "{text}"
    );
    vector
}

struct ConceptEmbedder;

impl QueryEmbedder for ConceptEmbedder {
    fn embed_query(
        &self,
        query: &str,
    ) -> impl Future<Output = Result<QueryVector, EmbeddingError>> + Send {
        let vector = concept_vector(query);
        async move {
            Ok(QueryVector {
                model: MODEL.to_owned(),
                vector,
            })
        }
    }
}

/// An embedder that answers one error, as the runtime's does without a model
/// or with a broken engine.
struct FailingEmbedder(fn() -> EmbeddingError);

impl QueryEmbedder for FailingEmbedder {
    fn embed_query(
        &self,
        _query: &str,
    ) -> impl Future<Output = Result<QueryVector, EmbeddingError>> + Send {
        let error = (self.0)();
        async move { Err(error) }
    }
}

fn record(path: &str) -> DocumentRecord {
    DocumentRecord {
        path: path.to_owned(),
        fingerprint: format!("sha256-of-{path}"),
        modified_at: "2026-10-01T09:00:00Z".to_owned(),
    }
}

/// Writes a one-page document, with concept vectors when `embed` is set.
fn add(index: &mut SourceIndex, path: &str, text: &str, embed: bool) {
    let chunks = chunk_document(&[Section::Page {
        page: 1,
        text: text.to_owned(),
    }]);
    let vectors = embed.then(|| ChunkVectors {
        model: MODEL.to_owned(),
        vectors: chunks.iter().map(|c| concept_vector(&c.text)).collect(),
    });
    index
        .upsert_document(&record(path), &chunks, vectors.as_ref())
        .unwrap();
}

/// A fixture index of thirty short documents of an office, one of which
/// speaks of the company vehicle without ever saying "car".
fn fixture(dir: &Path, embed: bool) -> SourceIndex {
    let mut index = SourceIndex::open(dir, "src-fixture").unwrap();
    let topics = [
        "The supplier invoice is settled within thirty days.",
        "The commercial lease is renewed for nine years.",
        "The committee meets on the first Monday of the month.",
        "The paper stock is ordered every quarter.",
        "The meeting room is booked from the shared calendar.",
    ];
    for i in 0..29 {
        let text = format!("{} File number {i}.", topics[i % topics.len()]);
        add(&mut index, &format!("/office/doc-{i:02}.txt"), &text, embed);
    }
    add(
        &mut index,
        "/office/fleet.txt",
        "The company vehicle is returned with its keys when the employee leaves.",
        embed,
    );
    index
}

#[test]
fn test_an_index_is_one_versioned_file_per_source_under_the_data_directory() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN the index of a source is opened, then opened again
    drop(SourceIndex::open(dir.path(), "src-abc").unwrap());
    let index = SourceIndex::open(dir.path(), "src-abc").unwrap();

    // THEN it is `<data>/sources/src-abc.db`, at version 1, with its tables
    let path = dir.path().join("sources").join("src-abc.db");
    assert!(path.exists());
    let version: i64 = index
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, i64::from(INDEX_SCHEMA_VERSION));
    let mut tables: Vec<String> = index
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('documents', 'chunks', 'chunks_fts', 'index_meta') ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables.sort();
    assert_eq!(tables, ["chunks", "chunks_fts", "documents", "index_meta"]);
    let tokenizer: String = index
        .conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chunks_fts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(tokenizer.contains("unicode61"), "{tokenizer}");
}

#[test]
fn test_an_index_written_by_a_newer_binary_is_refused() {
    // GIVEN an index file stamped one version ahead
    let dir = tempfile::tempdir().unwrap();
    let path = index_path(dir.path(), "src-new").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", INDEX_SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);

    // WHEN it is opened
    let opened = SourceIndex::open(dir.path(), "src-new");

    // THEN it is refused rather than misread
    assert!(
        matches!(
            opened,
            Err(IndexError::Schema(SchemaError::NewerThanBinary { .. }))
        ),
        "{opened:?}"
    );
}

#[test]
fn test_a_hostile_identifier_names_no_index() {
    // GIVEN an identifier that climbs out of the sources directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN its index is opened or removed
    let opened = SourceIndex::open(dir.path(), "../system");
    let removed = remove_index(dir.path(), "../system");

    // THEN both are refused
    assert!(matches!(opened, Err(IndexError::InvalidSourceId(_))));
    assert!(matches!(removed, Err(IndexError::InvalidSourceId(_))));
}

#[test]
fn test_removing_an_index_erases_its_file() {
    // GIVEN an index holding one document
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-gone").unwrap();
    add(&mut index, "/a.txt", "The contract is signed.", false);
    drop(index);
    let path = index_path(dir.path(), "src-gone").unwrap();
    assert!(path.exists());

    // WHEN it is removed, twice
    let first = remove_index(dir.path(), "src-gone").unwrap();
    let second = remove_index(dir.path(), "src-gone").unwrap();

    // THEN the file is gone, and the second removal finds nothing
    assert!(first);
    assert!(!second);
    assert!(!path.exists());
}

#[test]
fn test_a_document_written_again_replaces_its_chunks_and_their_words() {
    // GIVEN an index where a document speaks of a lease
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-up").unwrap();
    add(&mut index, "/lease.txt", "The lease is signed.", false);

    // WHEN the same path is written again about an invoice
    add(&mut index, "/lease.txt", "The invoice is paid.", false);

    // THEN one document remains, the old word is not found, the new one is
    assert_eq!(index.counts().unwrap(), (1, 1));
    assert!(index.search("lease", None, None).unwrap().hits.is_empty());
    let found = index.search("invoice", None, None).unwrap();
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].document, "/lease.txt");
    assert_eq!(
        index.document("/lease.txt").unwrap(),
        Some(record("/lease.txt"))
    );
}

#[test]
fn test_removing_a_document_removes_its_chunks_from_the_search() {
    // GIVEN an index of two documents
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-rm").unwrap();
    add(&mut index, "/a.txt", "The contract is signed.", false);
    add(&mut index, "/b.txt", "The contract is terminated.", false);

    // WHEN one is removed
    let removed = index.remove_document("/a.txt").unwrap();

    // THEN only the other is found
    assert!(removed);
    let hits = index.search("contract", None, None).unwrap().hits;
    let documents: Vec<&str> = hits.iter().map(|h| h.document.as_str()).collect();
    assert_eq!(documents, ["/b.txt"]);
}

#[test]
fn test_vectors_are_stored_as_normalised_f32_blobs() {
    // GIVEN one chunk and a vector of norm five
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-vec").unwrap();
    let chunks = chunk_document(&[Section::Text {
        text: "Un passage.".to_owned(),
    }]);
    let vectors = ChunkVectors {
        model: MODEL.to_owned(),
        vectors: vec![vec![3.0, 4.0]],
    };

    // WHEN the document is written
    index
        .upsert_document(&record("/v.txt"), &chunks, Some(&vectors))
        .unwrap();

    // THEN the BLOB holds two little-endian f32 of a unit vector, and the
    //      index records the model
    let blob: Vec<u8> = index
        .conn
        .query_row("SELECT vector FROM chunks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(blob.len(), 8);
    let stored = decode_vector(&blob).unwrap();
    assert!((stored[0] - 0.6).abs() < 1e-6 && (stored[1] - 0.8).abs() < 1e-6);
    assert_eq!(index.embedding_model().unwrap().as_deref(), Some(MODEL));
}

#[test]
fn test_vectors_that_do_not_fit_are_refused_and_nothing_is_written() {
    // GIVEN an index holding vectors of one model, of two dimensions
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-mix").unwrap();
    let one = chunk_document(&[Section::Text {
        text: "Un passage.".to_owned(),
    }]);
    let first = ChunkVectors {
        model: MODEL.to_owned(),
        vectors: vec![vec![1.0, 0.0]],
    };
    index
        .upsert_document(&record("/a.txt"), &one, Some(&first))
        .unwrap();

    // WHEN vectors of another model, of another dimension, of another count,
    //      or of no direction are written
    let other_model = ChunkVectors {
        model: "another-model".to_owned(),
        vectors: vec![vec![1.0, 0.0]],
    };
    let other_dimension = ChunkVectors {
        model: MODEL.to_owned(),
        vectors: vec![vec![1.0, 0.0, 0.0]],
    };
    let too_many = ChunkVectors {
        model: MODEL.to_owned(),
        vectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
    };
    let zero = ChunkVectors {
        model: MODEL.to_owned(),
        vectors: vec![vec![0.0, 0.0]],
    };
    let b = record("/b.txt");
    let model = index.upsert_document(&b, &one, Some(&other_model));
    let dimension = index.upsert_document(&b, &one, Some(&other_dimension));
    let count = index.upsert_document(&b, &one, Some(&too_many));
    let zeroed = index.upsert_document(&b, &one, Some(&zero));

    // THEN each is refused and the index still holds one document
    assert!(
        matches!(model, Err(IndexError::ModelChanged { .. })),
        "{model:?}"
    );
    assert!(
        matches!(
            dimension,
            Err(IndexError::Dimensions {
                stored: 2,
                given: 3
            })
        ),
        "{dimension:?}"
    );
    assert!(
        matches!(
            count,
            Err(IndexError::VectorCount {
                chunks: 1,
                vectors: 2
            })
        ),
        "{count:?}"
    );
    assert!(
        matches!(zeroed, Err(IndexError::ZeroVector(0))),
        "{zeroed:?}"
    );
    assert_eq!(index.counts().unwrap(), (1, 1));
}

#[tokio::test]
async fn test_a_synonym_finds_the_passage_among_the_first_ten() {
    // GIVEN a fixture index of thirty documents with their vectors, one of
    //       which speaks of the "company vehicle"
    let dir = tempfile::tempdir().unwrap();
    let index = fixture(dir.path(), true);

    // WHEN a search asks for the "car", a word no document holds
    let query = "car";
    let vector = query_vector(&ConceptEmbedder, query).await.unwrap();
    let outcome = index.search(query, vector.as_ref(), None).unwrap();
    let words_alone = index.search(query, None, None).unwrap();

    // THEN the search is hybrid and the passage is in the first ten, with its
    //      document, location, score and source, while the words alone find
    //      nothing
    assert_eq!(outcome.mode, SearchMode::Hybrid);
    assert!(outcome.hits.len() <= DEFAULT_LIMIT);
    let found = outcome
        .hits
        .iter()
        .find(|h| h.document == "/office/fleet.txt")
        .unwrap_or_else(|| panic!("not in the first ten: {:?}", outcome.hits));
    assert!(found.excerpt.contains("company vehicle"));
    assert_eq!(found.location, Location::Page { page: 1 });
    assert!(found.score > 0.0);
    assert_eq!(found.source, "src-fixture");
    assert!(words_alone.hits.is_empty(), "{:?}", words_alone.hits);
}

#[tokio::test]
async fn test_without_an_embeddings_model_the_search_is_by_keywords_and_says_so() {
    // GIVEN a fixture index, and a machine with no embeddings model chosen
    let dir = tempfile::tempdir().unwrap();
    let index = fixture(dir.path(), false);
    let embedder = FailingEmbedder(|| EmbeddingError::NoModel);

    // WHEN a search runs
    let vector = query_vector(&embedder, "supplier invoice").await.unwrap();
    let outcome = index
        .search("supplier invoice", vector.as_ref(), None)
        .unwrap();

    // THEN no vector was computed, the BM25 results come back, and the mode
    //      says keywords only
    assert!(vector.is_none());
    assert_eq!(outcome.mode, SearchMode::KeywordsOnly);
    assert!(!outcome.hits.is_empty());
    assert!(outcome
        .hits
        .iter()
        .all(|h| h.excerpt.contains("supplier invoice")));
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["mode"], "keywords_only");
}

#[tokio::test]
async fn test_a_failing_engine_is_reported_rather_than_hidden() {
    // GIVEN an embeddings engine that exists and answers an error
    let embedder = FailingEmbedder(|| EmbeddingError::Status {
        status: 500,
        body: "out of memory".to_owned(),
    });

    // WHEN a query vector is asked for
    let vector = query_vector(&embedder, "invoice").await;

    // THEN the error comes back
    assert!(matches!(
        vector,
        Err(EmbeddingError::Status { status: 500, .. })
    ));
}

#[test]
fn test_a_vector_of_another_model_searches_by_keywords() {
    // GIVEN an index built with one model, and a query vector of another
    let dir = tempfile::tempdir().unwrap();
    let index = fixture(dir.path(), true);
    let vector = QueryVector {
        model: "another-model".to_owned(),
        vector: concept_vector("invoice"),
    };

    // WHEN it searches
    let outcome = index.search("invoice", Some(&vector), None).unwrap();

    // THEN the vectors are not compared, and it says so
    assert_eq!(outcome.mode, SearchMode::KeywordsOnly);
    assert!(!outcome.hits.is_empty());
}

#[test]
fn test_ten_results_by_default_and_fifty_at_most() {
    // GIVEN an index of sixty documents sharing one word
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-many").unwrap();
    for i in 0..60 {
        add(
            &mut index,
            &format!("/n{i:02}.txt"),
            &format!("Contract number {i}."),
            true,
        );
    }
    let vector = QueryVector {
        model: MODEL.to_owned(),
        vector: concept_vector("contract"),
    };

    // WHEN it is searched with no limit, a limit of 500 and a limit of zero
    let default = index.search("contract", Some(&vector), None).unwrap();
    let most = index.search("contract", Some(&vector), Some(500)).unwrap();
    let least = index.search("contract", Some(&vector), Some(0)).unwrap();

    // THEN ten, fifty and one come back, best first
    assert_eq!(default.hits.len(), DEFAULT_LIMIT);
    assert_eq!(most.hits.len(), MAX_LIMIT);
    assert_eq!(least.hits.len(), 1);
    assert!(most.hits.windows(2).all(|w| w[0].score >= w[1].score));
}

#[test]
fn test_a_query_is_never_read_as_search_syntax() {
    // GIVEN a fixture index
    let dir = tempfile::tempdir().unwrap();
    let index = fixture(dir.path(), false);

    // WHEN queries carry quotes, operators, parentheses, or no word at all
    let hostile = [
        "\"lease",
        "lease AND OR NOT",
        "(lease*",
        "col:lease",
        "  ",
        "!!!",
    ];

    // THEN each one answers, without an error
    for query in hostile {
        let outcome = index.search(query, None, None);
        assert!(outcome.is_ok(), "{query}: {outcome:?}");
    }
    assert!(!index.search("(lease*", None, None).unwrap().hits.is_empty());
}

/// Words of the synthetic corpus of the measurement.
const LEXICON: &[&str] = &[
    "contract",
    "invoice",
    "customer",
    "supplier",
    "delivery",
    "payment",
    "lease",
    "employee",
    "leave",
    "meeting",
    "budget",
    "project",
    "report",
    "quote",
    "order",
    "stock",
    "audit",
    "insurance",
    "dispute",
    "amendment",
    "deadline",
    "reminder",
    "discount",
    "price",
    "warranty",
    "mission",
    "schedule",
    "training",
    "hiring",
    "file",
];

/// A pseudo-random generator, so the corpus is the same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

#[tokio::test]
#[ignore = "measurement: indexes 10 000 synthetic documents; run with --ignored --nocapture"]
async fn measure_query_latency_over_ten_thousand_documents() {
    // GIVEN 10 000 documents of two pages each, 384-dimension unit vectors
    let dir = tempfile::tempdir().unwrap();
    let mut index = SourceIndex::open(dir.path(), "src-measure").unwrap();
    let mut random = Lcg(42);
    let dimensions = 384;
    let started = Instant::now();
    for doc in 0..10_000 {
        let pages: Vec<Section> = (1..=2)
            .map(|page| Section::Page {
                page,
                text: (0..120)
                    .map(|_| LEXICON[random.next() as usize % LEXICON.len()])
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect();
        let chunks = chunk_document(&pages);
        let vectors = chunks
            .iter()
            .map(|_| {
                (0..dimensions)
                    .map(|_| (random.next() % 2001) as f32 / 1000.0 - 1.0)
                    .collect()
            })
            .collect();
        index
            .upsert_document(
                &record(&format!("/corpus/doc-{doc:05}.txt")),
                &chunks,
                Some(&ChunkVectors {
                    model: MODEL.to_owned(),
                    vectors,
                }),
            )
            .unwrap();
    }
    let indexing = started.elapsed();
    let (documents, chunks) = index.counts().unwrap();

    // WHEN twenty hybrid queries run
    let mut latencies = Vec::new();
    for q in 0..20u64 {
        let mut vector: Vec<f32> = (0..dimensions)
            .map(|_| (random.next() % 2001) as f32 / 1000.0 - 1.0)
            .collect();
        apollia_core::vector_search::normalize(&mut vector);
        let query = format!(
            "{} {}",
            LEXICON[(q as usize * 7) % LEXICON.len()],
            LEXICON[(q as usize * 13 + 3) % LEXICON.len()]
        );
        let started = Instant::now();
        let outcome = index
            .search(
                &query,
                Some(&QueryVector {
                    model: MODEL.to_owned(),
                    vector,
                }),
                None,
            )
            .unwrap();
        latencies.push(started.elapsed());
        assert_eq!(outcome.mode, SearchMode::Hybrid);
        assert_eq!(outcome.hits.len(), DEFAULT_LIMIT);
    }

    // THEN the median and the worst latency are reported
    latencies.sort();
    let median = latencies[latencies.len() / 2];
    let worst = latencies[latencies.len() - 1];
    eprintln!(
        "documents={documents} chunks={chunks} dimensions={dimensions} indexing={indexing:?} median={median:?} worst={worst:?}"
    );
    assert_eq!(documents, 10_000);
}
