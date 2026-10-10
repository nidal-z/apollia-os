//! The embeddings category of the registry: its two entries, the gate that
//! refuses them as chat models and admits them here, the verified download
//! they share with every model, and the recommendation by machine.

use super::*;
use crate::hardware::AcceleratorProfile;
use crate::recommend::verdict::{assess, Blocker, Verdict};

const GIB: u64 = 1024 * 1024 * 1024;

fn catalogue() -> EmbeddingCatalogue {
    EmbeddingCatalogue::embedded().expect("the shipped embeddings entries must load")
}

/// The header of `multilingual-e5-small-q8_0.gguf`, as read from the file.
fn e5_header() -> GgufHeaderFacts {
    GgufHeaderFacts {
        version: 3,
        architecture: Some("bert".to_owned()),
        tokenizer_model: Some("llama".to_owned()),
        block_count: Some(12),
        context_length: Some(512),
        embedding_length: Some(384),
        head_count: Some(12),
        pooling_type: Some(1),
        ..GgufHeaderFacts::default()
    }
}

/// The header of `Qwen3-Embedding-0.6B-Q8_0.gguf`, as read from the file: the
/// `qwen3` architecture of the chat models, a chat template, and a pooling key.
fn qwen3_embedding_header() -> GgufHeaderFacts {
    GgufHeaderFacts {
        version: 3,
        architecture: Some("qwen3".to_owned()),
        tokenizer_model: Some("gpt2".to_owned()),
        tokenizer_pre: Some("qwen2".to_owned()),
        has_chat_template: true,
        block_count: Some(28),
        context_length: Some(32_768),
        embedding_length: Some(1024),
        head_count: Some(16),
        head_count_kv: Some(8),
        pooling_type: Some(3),
        ..GgufHeaderFacts::default()
    }
}

fn apple(total_ram_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb,
        available_ram_gb: total_ram_gb / 2.0,
        cpu_model: "Apple M2".to_owned(),
        cpu_cores: 8,
        accelerator: AcceleratorProfile::AppleSilicon {
            chip: "M2".to_owned(),
            generation: 2,
            vram_gb: total_ram_gb,
        },
        memory_budget_gb: total_ram_gb * 0.75,
        devices: Vec::new(),
    }
}

fn cuda(vram_gb: f64, total_ram_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb,
        available_ram_gb: total_ram_gb / 2.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 12,
        accelerator: AcceleratorProfile::Cuda {
            device_name: "test".to_owned(),
            vram_gb,
            compute_capability: (8, 6),
        },
        memory_budget_gb: vram_gb,
        devices: Vec::new(),
    }
}

#[test]
fn the_registry_carries_both_embeddings_entries_with_their_technical_facts() {
    // GIVEN the shipped table
    let catalogue = catalogue();

    // WHEN its embeddings category is read
    let e5 = catalogue
        .get("multilingual-e5-small")
        .expect("e5 is in the category");
    let qwen = catalogue
        .get("qwen3-embedding-0.6b")
        .expect("Qwen3 Embedding is in the category");

    // THEN it holds them, each pinned to one file by its digest and declaring
    // dimension, context, pooling and both prefixes
    assert_eq!(catalogue.models().len(), 5);
    assert_eq!(e5.repo, "cstr/multilingual-e5-small-GGUF");
    assert_eq!(e5.quantization, "Q8_0");
    assert_eq!(e5.size_bytes, 131_624_960);
    assert_eq!(
        e5.sha256,
        "0a34067a40f25d3149b36885faa62bee0e5284d0f9edc102acfc00e115d953e8"
    );
    assert_eq!(e5.dimensions, 384);
    assert_eq!(e5.context_length, 512);
    assert_eq!(e5.pooling, Pooling::Mean);
    assert_eq!(e5.license, "mit");
    assert_eq!(e5.query_prefix, "query: ");
    assert_eq!(e5.passage_prefix, "passage: ");

    assert_eq!(qwen.repo, "Qwen/Qwen3-Embedding-0.6B-GGUF");
    assert_eq!(qwen.file, "Qwen3-Embedding-0.6B-Q8_0.gguf");
    assert_eq!(qwen.size_bytes, 639_150_592);
    assert_eq!(qwen.sha256.len(), 64);
    assert_eq!(qwen.dimensions, 1024);
    assert_eq!(qwen.context_length, 32_768);
    assert_eq!(qwen.pooling, Pooling::Last);
    assert!(qwen.query_prefix.starts_with("Instruct: "));
    assert!(qwen.query_prefix.ends_with("\nQuery:"));
    assert_eq!(qwen.passage_prefix, "");
}

#[test]
fn the_embeddings_entries_stay_out_of_the_chat_ranking() {
    // GIVEN the shipped table, read as the chat ranker reads it
    let manifest = super::super::FamilyManifest::embedded().expect("the table must load");

    // WHEN its families are listed
    // THEN no embeddings model is among them
    for family in manifest.families() {
        for variant in &family.variants {
            assert!(
                !variant.hf_name.to_lowercase().contains("embedding")
                    && !variant.hf_name.to_lowercase().contains("e5"),
                "{} reached the chat families",
                variant.hf_name
            );
        }
    }
}

#[test]
fn both_embeddings_models_are_refused_as_chat_models() {
    // GIVEN the headers of the two entries' files
    let e5 = e5_header();
    let qwen = qwen3_embedding_header();

    // WHEN each is assessed as a chat engine
    let e5_verdict = assess(&e5);
    let qwen_verdict = assess(&qwen);

    // THEN both are rejected: e5 by its architecture, Qwen3 Embedding by its
    // pooling key, since its architecture is the one of the Qwen3 chat models
    let Verdict::Rejected { blockers, .. } = e5_verdict else {
        panic!("e5 must be refused as a chat model");
    };
    assert!(blockers
        .iter()
        .any(|b| matches!(b, Blocker::NotGenerative { architecture } if architecture == "bert")));
    let Verdict::Rejected { blockers, .. } = qwen_verdict else {
        panic!("Qwen3 Embedding must be refused as a chat model");
    };
    assert!(blockers.iter().any(|b| matches!(
        b,
        Blocker::PooledOutput { architecture, pooling_type: 3 } if architecture == "qwen3"
    )));
}

#[test]
fn both_embeddings_models_are_admitted_in_the_embeddings_category() {
    // GIVEN the headers of the two entries' files, and a Qwen3 chat header
    let e5 = e5_header();
    let qwen = qwen3_embedding_header();
    let mut chat = qwen3_embedding_header();
    chat.pooling_type = None;

    // WHEN each is offered to the embeddings category
    // THEN the two embeddings models enter it and the chat model does not
    assert!(admits(&e5));
    assert!(admits(&qwen));
    assert!(!admits(&chat));
    assert!(assess(&chat).is_offerable());
}

#[test]
fn a_reranker_header_is_neither_a_chat_model_nor_an_embeddings_model() {
    // GIVEN a header whose pooling key says rank
    let mut facts = qwen3_embedding_header();
    facts.pooling_type = Some(4);

    // WHEN it is assessed both ways
    // THEN neither category takes it
    assert!(!assess(&facts).is_offerable());
    assert!(!admits(&facts));
}

#[test]
fn a_file_is_recognised_by_its_digest_in_any_case() {
    // GIVEN the Qwen3 Embedding digest, written in capitals
    let catalogue = catalogue();
    let upper = "06507C7B42688469C4E7298B0A1E16DEFF06CAF291CF0A5B278C308249C3E439";

    // WHEN it is looked up
    let found = catalogue.by_sha256(upper);

    // THEN it names the entry, and an unknown digest names none
    assert_eq!(found.map(|m| m.id.as_str()), Some("qwen3-embedding-0.6b"));
    assert!(catalogue.by_sha256(&"0".repeat(64)).is_none());
}

#[test]
fn an_entry_whose_digest_is_not_a_sha256_is_refused_at_load() {
    // GIVEN an entry carrying a short digest
    let text = r#"
[[embedding]]
id = "x"
label = "X"
publisher = "P"
released = "2025-01-01"
license = "mit"
repo = "o/x-GGUF"
file = "x.gguf"
quantization = "Q8_0"
size_bytes = 10
sha256 = "abc"
architecture = "bert"
dimensions = 8
context_length = 16
pooling = "mean"
query_prefix = ""
passage_prefix = ""
source = "https://example.org"
"#;

    // WHEN it is loaded
    let err = EmbeddingCatalogue::from_toml(text).expect_err("a short digest must be refused");

    // THEN the entry is named, since no download could be checked against it
    assert!(matches!(err, ManifestError::InvalidEmbedding { ref id, .. } if id == "x"));
}

#[test]
fn the_recommendation_follows_the_room_left_beside_the_language_model() {
    // GIVEN three machines, each with the size of the language model it runs
    struct Case {
        name: &'static str,
        profile: HardwareProfile,
        language_model_bytes: Option<u64>,
        expected: &'static str,
    }
    let cases = [
        Case {
            // an 8 GB Mac whose language model (3 GB of weights) fills most of
            // its 6 GB budget
            name: "8 GB unified",
            profile: apple(8.0),
            language_model_bytes: Some(3 * GIB),
            expected: "multilingual-e5-small",
        },
        Case {
            // a 32 GB Mac running a 14B at four bits (9 GB of weights)
            name: "32 GB unified",
            profile: apple(32.0),
            language_model_bytes: Some(9 * GIB),
            expected: "bge-m3",
        },
        Case {
            // an 8 GB card in a 16 GB machine running an 8B at four bits
            name: "8 GB card, 16 GB system",
            profile: cuda(8.0, 16.0),
            language_model_bytes: Some(5 * GIB),
            expected: "bge-m3",
        },
    ];
    let catalogue = catalogue();

    for case in cases {
        // WHEN the category recommends an entry
        let recommendation = catalogue
            .recommend(&case.profile, case.language_model_bytes)
            .expect("a non-empty category recommends");

        // THEN it is the larger model when it fits beside the language model,
        // the small one otherwise, and the reason says which
        assert_eq!(recommendation.model, case.expected, "{}", case.name);
        match &recommendation.reason {
            EmbeddingReason::FitsBeside {
                language_gb,
                embeddings_gb,
                capacity_gb,
            } => {
                assert_eq!(case.expected, "bge-m3", "{}", case.name);
                assert!(language_gb + embeddings_gb <= *capacity_gb, "{}", case.name);
            }
            EmbeddingReason::NoRoomBeside {
                language_gb,
                larger,
                larger_gb,
                capacity_gb,
            } => {
                assert_eq!(larger, "bge-m3", "{}", case.name);
                assert!(language_gb + larger_gb > *capacity_gb, "{}", case.name);
            }
        }
    }
}

#[test]
fn the_footprints_compared_are_the_ones_the_estimator_gives() {
    // GIVEN the 8 GB Mac and its 3 GB language model
    let profile = apple(8.0);
    let catalogue = catalogue();

    // WHEN the recommendation is made
    let recommendation = catalogue
        .recommend(&profile, Some(3 * GIB))
        .expect("a non-empty category recommends");

    // THEN the language figure is the one estimate_memory gives for those
    // weights, at the runtime's own launch shape, on an accelerated machine
    let expected = estimate_memory(
        3 * GIB,
        &GgufHeaderFacts::default(),
        &RuntimeShape::default(),
        true,
    )
    .total_gb;
    let EmbeddingReason::NoRoomBeside { language_gb, .. } = recommendation.reason else {
        panic!("the 8 GB Mac has no room beside its language model");
    };
    assert!((language_gb - expected).abs() < f64::EPSILON);
}

#[test]
fn with_no_language_model_counted_the_larger_entry_is_offered_when_it_fits() {
    // GIVEN the 8 GB Mac with no language model counted
    let profile = apple(8.0);

    // WHEN the category recommends an entry
    let recommendation = catalogue()
        .recommend(&profile, None)
        .expect("a non-empty category recommends");

    // THEN the larger entry fits on its own
    assert_eq!(recommendation.model, "bge-m3");
    assert!(matches!(
        recommendation.reason,
        EmbeddingReason::FitsBeside { language_gb, .. } if language_gb == 0.0
    ));
}

#[test]
fn an_entry_listed_by_request_is_never_the_one_recommended() {
    // GIVEN the shipped table, whose largest file is an entry listed by
    // request, and a machine with room for every entry
    let catalogue = catalogue();
    let largest = catalogue
        .models()
        .iter()
        .max_by_key(|m| m.size_bytes)
        .expect("the category is not empty");
    assert!(largest.by_request);
    let profile = apple(128.0);

    // WHEN the category recommends an entry
    let recommendation = catalogue
        .recommend(&profile, None)
        .expect("a non-empty category recommends");

    // THEN it is the largest of the entries not listed by request
    assert_eq!(recommendation.model, "bge-m3");
    assert!(catalogue
        .models()
        .iter()
        .filter(|m| m.by_request)
        .all(|m| m.id != recommendation.model));
}

#[test]
fn the_later_entries_carry_the_pooling_their_header_declares() {
    // GIVEN the three entries added after the first two
    let catalogue = catalogue();

    // WHEN each is read
    let granite = catalogue
        .get("granite-embedding-107m-multilingual")
        .expect("Granite Embedding is in the category");
    let bge = catalogue.get("bge-m3").expect("BGE-M3 is in the category");
    let qwen = catalogue
        .get("qwen3-embedding-4b")
        .expect("Qwen3 Embedding 4B is in the category");

    // THEN the two BERT models pool on the first token with no prefix, and the
    // Qwen3 one on the last token with the query instruction of its family
    assert_eq!(granite.pooling, Pooling::Cls);
    assert_eq!((granite.dimensions, granite.context_length), (384, 512));
    assert_eq!(bge.pooling, Pooling::Cls);
    assert_eq!((bge.dimensions, bge.context_length), (1024, 8192));
    assert_eq!(
        (bge.query_prefix.as_str(), bge.passage_prefix.as_str()),
        ("", "")
    );
    assert_eq!(qwen.pooling, Pooling::Last);
    assert_eq!((qwen.dimensions, qwen.context_length), (2560, 32_768));
    assert!(qwen.query_prefix.ends_with("\nQuery:"));
}
