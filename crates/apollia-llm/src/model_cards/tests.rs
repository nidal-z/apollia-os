//! Model cards: the signed index, its fetch and its fallbacks, the copy next
//! to a downloaded model, and the README read on demand.
//!
//! The fixture index under `fixtures/` was signed by the release tooling
//! (`cargo tauri signer generate`, then `cargo tauri signer sign`) with a
//! throwaway key whose private half was not kept. `test-dist.key.pub` is its
//! public half, `other.key.pub` a key that signed nothing here.

use std::path::{Path, PathBuf};

use super::*;

const INDEX: &str = include_str!("fixtures/index.json");
const SIGNATURE: &str = include_str!("fixtures/index.json.minisig");
const TEST_KEY: &str = include_str!("fixtures/test-dist.key.pub");
const OTHER_KEY: &str = include_str!("fixtures/other.key.pub");

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn the_fixture_index_holds_with_its_key_and_lists_its_cards() {
    // GIVEN the fixture index, its signature and the key that made it
    // WHEN the index is checked and parsed
    let index = verify_index(INDEX.as_bytes(), SIGNATURE, Some(TEST_KEY));

    // THEN it holds, and both cards are there
    let index = index.expect("the fixture signature holds");
    assert_eq!(index.cards.len(), 2);
    assert!(index.card("qwen3/Qwen3-8B").is_some());
    assert_eq!(index.publishers.converters, vec!["unsloth".to_owned()]);
}

#[test]
fn another_key_or_one_changed_byte_is_refused() {
    // GIVEN the fixture index under a key that did not sign it, and the same
    //       index with one byte changed
    let mut altered = INDEX.as_bytes().to_vec();
    altered[40] ^= 0x01;

    // WHEN each is checked
    let other = verify_index(INDEX.as_bytes(), SIGNATURE, Some(OTHER_KEY));
    let changed = verify_index(&altered, SIGNATURE, Some(TEST_KEY));
    let no_key = verify_index(INDEX.as_bytes(), SIGNATURE, None);

    // THEN each is a readable refusal
    assert!(matches!(other, Err(CardError::Signature(_))), "{other:?}");
    assert!(
        matches!(changed, Err(CardError::Signature(_))),
        "{changed:?}"
    );
    assert!(matches!(no_key, Err(CardError::NoTrustedKey)), "{no_key:?}");
    let message = other.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("signature"), "{message}");
}

#[test]
fn a_card_reads_in_french_and_falls_back_on_english() {
    // GIVEN a card with both languages, and one whose French text is empty
    let index = RegistryIndex::parse(INDEX.as_bytes()).expect("parses");
    let qwen = index.card("qwen3/Qwen3-8B").expect("qwen card");
    let e5 = index.card("multilingual-e5-small").expect("e5 card");

    // WHEN each is read in French, and the first in an unknown language
    let qwen_fr = qwen.localized("fr-FR");
    let e5_fr = e5.localized("fr");
    let qwen_de = qwen.localized("de");

    // THEN French is served when written, English otherwise, and the source
    //      address of the model goes with every reading
    assert_eq!(qwen_fr.language, "fr");
    assert!(qwen_fr.description.starts_with("Modèle"));
    assert_eq!(
        qwen_fr.author_fields[0].explanation,
        "Le modèle réfléchit avant de répondre."
    );
    assert_eq!(e5_fr.language, "en");
    assert_eq!(qwen_de.language, "en");
    assert_eq!(qwen_de.source_url, "https://huggingface.co/Qwen/Qwen3-8B");
}

#[test]
fn an_index_of_another_schema_is_refused() {
    // GIVEN an index declaring schema version 2
    let text = INDEX.replacen("\"schema_version\": 1", "\"schema_version\": 2", 1);

    // WHEN it is parsed
    let parsed = RegistryIndex::parse(text.as_bytes());

    // THEN it is refused by name
    assert!(
        matches!(parsed, Err(CardError::UnsupportedSchema(2))),
        "{parsed:?}"
    );
}

#[test]
fn the_card_copy_is_beside_a_file_and_inside_a_folder() {
    // GIVEN a GGUF file and an MLX folder
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("Qwen3-8B-Q4_K_M.gguf");
    std::fs::write(&file, b"GGUF").expect("write");
    let folder = tmp.path().join("Qwen3-8B-4bit");
    std::fs::create_dir_all(&folder).expect("mkdir");

    // WHEN their card paths are asked
    let beside = card_path_for(&file);
    let inside = card_path_for(&folder);

    // THEN the first sits next to the weights, the second in the folder
    assert_eq!(beside, tmp.path().join("Qwen3-8B-Q4_K_M.gguf.card.json"));
    assert_eq!(inside, folder.join(FOLDER_CARD_FILE_NAME));
}

/// The index built from `registry/models/` by
/// `scripts/build_model_registry_index.py --test-fixture`, signed by a key made
/// for the occasion whose private half was deleted at once.
const REGISTRY_INDEX: &str = include_str!("fixtures/registry-index.json");
const REGISTRY_SIGNATURE: &str = include_str!("fixtures/registry-index.json.minisig");
const REGISTRY_KEY: &str = include_str!("fixtures/registry-test.key.pub");

fn registry_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../registry/models")
}

#[test]
fn the_index_built_from_the_registry_holds_with_its_test_key() {
    // GIVEN the index built from the card files of the repository, and the
    // signature a throwaway distribution key made over it
    // WHEN the distribution module checks it with that key, and with another
    let checked = verify_index(
        REGISTRY_INDEX.as_bytes(),
        REGISTRY_SIGNATURE,
        Some(REGISTRY_KEY),
    );
    let foreign = verify_index(
        REGISTRY_INDEX.as_bytes(),
        REGISTRY_SIGNATURE,
        Some(OTHER_KEY),
    );

    // THEN it holds with its key only
    assert!(checked.is_ok(), "{:?}", checked.err());
    assert!(
        matches!(foreign, Err(CardError::Signature(_))),
        "{foreign:?}"
    );
}

#[test]
fn every_entry_of_the_selection_has_its_card_in_the_index() {
    // GIVEN the signed index built from the repository, the embedded model
    // table and the embedded embeddings catalogue
    let index = verify_index(
        REGISTRY_INDEX.as_bytes(),
        REGISTRY_SIGNATURE,
        Some(REGISTRY_KEY),
    )
    .expect("the fixture index holds");
    let families = crate::recommend::FamilyManifest::embedded().expect("families");
    let embeddings =
        crate::recommend::embeddings::EmbeddingCatalogue::embedded().expect("embeddings");

    // WHEN each entry looks for its card
    let mut missing: Vec<String> = Vec::new();
    for family in families.families() {
        for variant in &family.variants {
            let id = format!("{}/{}", family.id, variant.hf_name);
            if index.card(&id).map(|c| c.kind) != Some(CardKind::Language) {
                missing.push(id);
            }
        }
    }
    for model in embeddings.models() {
        if index.card(&model.id).map(|c| c.kind) != Some(CardKind::Embeddings) {
            missing.push(model.id.clone());
        }
    }

    // THEN none is missing, and dictation has cards of its own
    assert!(missing.is_empty(), "entries without a card: {missing:?}");
    assert!(index.cards.iter().any(|c| c.kind == CardKind::Dictation));
}

#[test]
fn the_fixture_index_is_the_card_files_of_the_repository() {
    // GIVEN the card files under registry/models
    let files: Vec<PathBuf> = files_under(&registry_dir())
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    let index = RegistryIndex::parse(REGISTRY_INDEX.as_bytes()).expect("parse");

    // WHEN each file is read as a card and looked up in the index
    let mut stale: Vec<String> = Vec::new();
    for file in &files {
        let bytes = std::fs::read(file).expect("read card file");
        let card: ModelCard = serde_json::from_slice(&bytes).expect("card file parses");
        if index.card(&card.id) != Some(&card) {
            stale.push(card.id);
        }
    }

    // THEN every file is in the index as written, and the index holds nothing
    // else
    assert!(!files.is_empty());
    assert!(
        stale.is_empty(),
        "rebuild the fixture, stale cards: {stale:?}"
    );
    assert_eq!(index.cards.len(), files.len());
}

#[cfg(feature = "cloud")]
mod network {
    use std::net::TcpListener;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::model_cards::fetch::{load_index, IndexOrigin, RegistryError, RegistrySource};
    use crate::model_cards::readme::{fetch_readme, ReadmeError, MAX_README_BYTES};

    /// An address on this machine where nothing listens.
    fn closed_address() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        format!("http://127.0.0.1:{port}/models/stable/index.json")
    }

    async fn registry() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models/stable/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(INDEX))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/models/stable/index.json.minisig"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SIGNATURE))
            .mount(&server)
            .await;
        server
    }

    fn source(url: String, mirror: Option<String>) -> RegistrySource {
        RegistrySource {
            url,
            mirror_url: mirror,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn the_index_is_fetched_checked_and_kept_and_the_request_says_nothing() {
        // GIVEN a registry serving the signed fixture, and an empty cache
        let server = registry().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let url = format!("{}/models/stable/index.json", server.uri());

        // WHEN the catalogue reads the index
        let loaded = load_index(&source(url, None), cache.path(), Some(TEST_KEY)).await;

        // THEN it is the main address's index, a copy is kept, and the two
        //      requests carried no body, no query and nothing identifying
        let loaded = loaded.expect("the index loads");
        assert_eq!(loaded.origin, IndexOrigin::Primary);
        assert_eq!(loaded.index.cards.len(), 2);
        assert!(cache.path().join(CACHED_INDEX_FILE_NAME).is_file());
        assert!(cache.path().join(CACHED_SIGNATURE_FILE_NAME).is_file());
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 2);
        for request in &requests {
            assert_eq!(request.method.as_str(), "GET");
            assert!(request.body.is_empty(), "a body was sent");
            assert_eq!(request.url.query(), None, "a query was sent");
            for header in ["authorization", "cookie", "user-agent", "x-api-key"] {
                assert!(
                    !request.headers.contains_key(header),
                    "the request carried {header}"
                );
            }
        }
    }

    #[tokio::test]
    async fn the_mirror_is_read_when_the_main_address_does_not_answer() {
        // GIVEN a main address where nothing listens, and a mirror serving the
        //       signed fixture
        let mirror = registry().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let mirror_url = format!("{}/models/stable/index.json", mirror.uri());

        // WHEN the catalogue reads the index
        let loaded = load_index(
            &source(closed_address(), Some(mirror_url)),
            cache.path(),
            Some(TEST_KEY),
        )
        .await;

        // THEN the mirror's index is returned, checked the same way
        let loaded = loaded.expect("the mirror answers");
        assert_eq!(loaded.origin, IndexOrigin::Mirror);
        assert_eq!(loaded.index.cards.len(), 2);
    }

    #[tokio::test]
    async fn a_signature_that_does_not_hold_is_refused_and_nothing_is_kept() {
        // GIVEN a registry whose signature was made by another key
        let cache = tempfile::tempdir().expect("tempdir");
        let server = registry().await;
        let url = format!("{}/models/stable/index.json", server.uri());

        // WHEN the catalogue reads it with the key of this build
        let loaded = load_index(&source(url, None), cache.path(), Some(OTHER_KEY)).await;

        // THEN it is a readable refusal, and no copy is kept
        let error = loaded.expect_err("refused");
        assert!(
            matches!(error, RegistryError::Card(CardError::Signature(_))),
            "{error:?}"
        );
        assert!(error.to_string().contains("signature"), "{error}");
        assert!(files_under(cache.path()).is_empty());
    }

    #[tokio::test]
    async fn offline_the_kept_copy_is_read_and_without_it_the_catalogue_is_empty() {
        // GIVEN a first read that kept a copy, then no address answering
        let server = registry().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let url = format!("{}/models/stable/index.json", server.uri());
        load_index(&source(url, None), cache.path(), Some(TEST_KEY))
            .await
            .expect("first read");
        let offline = source(closed_address(), Some(closed_address()));
        let empty_cache = tempfile::tempdir().expect("tempdir");

        // WHEN the catalogue reads the index with and without the copy
        let from_copy = load_index(&offline, cache.path(), Some(TEST_KEY)).await;
        let without = load_index(&offline, empty_cache.path(), Some(TEST_KEY)).await;

        // THEN the copy answers, and without it the error says offline
        assert_eq!(from_copy.expect("the copy").origin, IndexOrigin::Cache);
        let error = without.expect_err("offline");
        assert!(matches!(error, RegistryError::Offline { .. }), "{error:?}");
    }

    #[tokio::test]
    async fn a_registry_turned_off_is_never_asked() {
        // GIVEN a registry the configuration turned off
        let server = MockServer::start().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let mut off = source(format!("{}/models/stable/index.json", server.uri()), None);
        off.enabled = false;

        // WHEN the catalogue opens
        let loaded = load_index(&off, cache.path(), Some(TEST_KEY)).await;

        // THEN nothing is requested, and the answer says the registry is off
        assert!(matches!(loaded, Err(RegistryError::Disabled)));
        let requests = server.received_requests().await.expect("recorded");
        assert!(requests.is_empty());
    }

    #[tokio::test]
    async fn without_a_compiled_key_no_request_is_made() {
        // GIVEN a build that carries no distribution key
        let server = MockServer::start().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let url = format!("{}/models/stable/index.json", server.uri());

        // WHEN the catalogue opens
        let loaded = load_index(&source(url, None), cache.path(), None).await;

        // THEN it is refused by name, before any request
        assert!(
            matches!(loaded, Err(RegistryError::Card(CardError::NoTrustedKey))),
            "{loaded:?}"
        );
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[tokio::test]
    async fn a_downloaded_model_gets_its_card_and_reads_it_with_the_network_cut() {
        // GIVEN a kept index, and a model just downloaded from one of the
        //       repositories its first card describes
        let server = registry().await;
        let data = tempfile::tempdir().expect("tempdir");
        let cache = default_cache_dir(data.path());
        let url = format!("{}/models/stable/index.json", server.uri());
        load_index(&source(url, None), &cache, Some(TEST_KEY))
            .await
            .expect("first read");
        let models = data.path().join("models");
        std::fs::create_dir_all(&models).expect("mkdir");
        let model = models.join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::write(&model, b"GGUF").expect("write");

        // WHEN the card is copied, the registry goes away, and the card of the
        //      installed model is read
        let copied = copy_card_beside(&cache, &model, "qwen/qwen3-8b-gguf", Some(TEST_KEY));
        drop(server);
        let no_copy = tempfile::tempdir().expect("tempdir");
        let offline = load_index(
            &source(closed_address(), None),
            no_copy.path(),
            Some(TEST_KEY),
        )
        .await;
        let read = installed_card(&model);

        // THEN the copy sits next to the weights and reads back whole, while
        //      the registry itself is unreachable
        assert_eq!(
            copied.expect("copied"),
            Some(models.join("Qwen3-8B-Q4_K_M.gguf.card.json"))
        );
        assert!(matches!(offline, Err(RegistryError::Offline { .. })));
        let card = read.expect("readable").expect("present");
        assert_eq!(card.id, "qwen3/Qwen3-8B");
        assert_eq!(card.defaults.temperature, Some(1.0));
    }

    #[tokio::test]
    async fn a_model_no_card_describes_gets_no_copy() {
        // GIVEN a kept index, and a model from a repository it does not list
        let server = registry().await;
        let cache = tempfile::tempdir().expect("tempdir");
        let url = format!("{}/models/stable/index.json", server.uri());
        load_index(&source(url, None), cache.path(), Some(TEST_KEY))
            .await
            .expect("first read");
        let models = tempfile::tempdir().expect("tempdir");
        let model = models.path().join("other.gguf");
        std::fs::write(&model, b"GGUF").expect("write");

        // WHEN the card is copied
        let copied = copy_card_beside(cache.path(), &model, "someone/other-GGUF", Some(TEST_KEY));

        // THEN nothing is written
        assert_eq!(copied.expect("no error"), None);
        assert_eq!(files_under(models.path()), vec![model]);
    }

    async fn readme_server(body: String) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Qwen/Qwen3-8B/resolve/main/README.md"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn the_readme_is_returned_as_text_and_nothing_is_written() {
        // GIVEN a hub serving a README, and an empty data directory
        let server =
            readme_server("---\nlicense: apache-2.0\n---\n# Qwen3-8B\nSome text.\n".to_owned())
                .await;
        let data = tempfile::tempdir().expect("tempdir");

        // WHEN the README is read
        let readme = fetch_readme(&server.uri(), "Qwen/Qwen3-8B").await;

        // THEN the text comes back with the repository, the author and the
        //      licence, and the data directory is still empty
        let readme = readme.expect("read");
        assert_eq!(readme.repo, "Qwen/Qwen3-8B");
        assert_eq!(readme.author, "Qwen");
        assert_eq!(readme.license.as_deref(), Some("apache-2.0"));
        assert_eq!(readme.source_url, "https://huggingface.co/Qwen/Qwen3-8B");
        assert!(readme.text.contains("Some text."));
        assert!(files_under(data.path()).is_empty());
    }

    #[tokio::test]
    async fn a_readme_over_the_cap_is_refused() {
        // GIVEN a hub serving a README one byte over 512 KiB
        let size = usize::try_from(MAX_README_BYTES).expect("fits") + 1;
        let server = readme_server("a".repeat(size)).await;

        // WHEN the README is read
        let readme = fetch_readme(&server.uri(), "Qwen/Qwen3-8B").await;

        // THEN it is refused, with the cap in the message
        let error = readme.expect_err("refused");
        assert!(matches!(error, ReadmeError::TooLarge { .. }), "{error:?}");
        assert!(error.to_string().contains("524288"), "{error}");
    }

    #[tokio::test]
    async fn offline_the_readme_is_unavailable_by_name() {
        // GIVEN a hub address where nothing listens
        let base = closed_address().replace("/models/stable/index.json", "");

        // WHEN the README is read
        let readme = fetch_readme(&base, "Qwen/Qwen3-8B").await;

        // THEN the error says the README is unavailable offline
        let error = readme.expect_err("offline");
        assert!(matches!(error, ReadmeError::Offline), "{error:?}");
        assert_eq!(error.to_string(), "README unavailable offline");
    }
}
