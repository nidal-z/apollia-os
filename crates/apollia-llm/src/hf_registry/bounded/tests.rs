use super::*;
use crate::hardware::AcceleratorProfile;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Pages recorded from the HuggingFace API on 2026-10-02, one per
/// organisation: `models?author=<org>&filter=gguf&search=Qwen3-4B
/// &sort=downloads&limit=3` with the fields of [`EXPAND`]; the `bartowski`
/// page also holds `bartowski/ibm-granite_granite-3.3-8b-instruct-GGUF`,
/// recorded the same day by its own search.
const RECORDED_PAGES: &str = include_str!("../fixtures/bounded_search_qwen3_4b.json");

/// The tree of `bartowski/ibm-granite_granite-3.3-8b-instruct-GGUF`.
const GRANITE_TREE: &str = include_str!("../fixtures/tree_granite_33_8b_bartowski.json");

/// The tree of `unsloth/Qwen3-4B-GGUF`.
const QWEN3_TREE: &str = include_str!("../fixtures/tree_qwen3_4b_unsloth.json");

/// The README of `bartowski/ibm-granite_granite-3.3-8b-instruct-GGUF`, recorded
/// the same day, only to prove none of its sentences reaches a card.
const GRANITE_README: &str = include_str!("../fixtures/readme_granite_33_8b_bartowski.md");

/// The signed index of the model cards, as the release pipeline builds it.
const INDEX: &[u8] = include_bytes!("../../model_cards/fixtures/registry-index.json");

const GRANITE: &str = "bartowski/ibm-granite_granite-3.3-8b-instruct-GGUF";

fn recorded_pages() -> serde_json::Map<String, serde_json::Value> {
    match serde_json::from_str(RECORDED_PAGES) {
        Ok(serde_json::Value::Object(map)) => map,
        other => panic!("the recorded pages are an object: {other:?}"),
    }
}

fn page(org: &str) -> Vec<serde_json::Value> {
    recorded_pages()[org]
        .as_array()
        .expect("a recorded page is a list")
        .clone()
}

fn index() -> RegistryIndex {
    RegistryIndex::parse(INDEX).expect("the fixture index parses")
}

fn mac() -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: 32.0,
        available_ram_gb: 16.0,
        cpu_model: "Apple M3 Pro".to_owned(),
        cpu_cores: 12,
        accelerator: AcceleratorProfile::AppleSilicon {
            chip: "M3 Pro".to_owned(),
            generation: 3,
            vram_gb: 32.0,
        },
        memory_budget_gb: 24.0,
        devices: Vec::new(),
    }
}

fn small_pc() -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: 4.0,
        available_ram_gb: 2.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 4,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: 2.4,
        devices: Vec::new(),
    }
}

fn cuda_box() -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: 64.0,
        available_ram_gb: 40.0,
        cpu_model: "test".to_owned(),
        cpu_cores: 16,
        accelerator: AcceleratorProfile::Cuda {
            device_name: "test".to_owned(),
            vram_gb: 24.0,
            compute_capability: (8, 9),
        },
        memory_budget_gb: 24.0,
        devices: Vec::new(),
    }
}

fn tree(text: &str) -> Vec<TreeEntry> {
    serde_json::from_str(text).expect("the recorded tree parses")
}

/// A Hub that answers every organisation with an empty page, `bartowski` with
/// its recorded page plus an item from an owner on neither list, and the
/// granite tree.
async fn hub() -> MockServer {
    let server = MockServer::start().await;
    let mut bartowski = page("bartowski");
    bartowski.push(serde_json::json!({
        "id": "MaziyarPanahi/Qwen3-4B-GGUF",
        "author": "MaziyarPanahi",
        "downloads": 9_999_999,
        "tags": ["gguf"]
    }));
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .and(query_param("author", "bartowski"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bartowski))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .with_priority(10)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/{GRANITE}/tree/main")))
        .respond_with(ResponseTemplate::new(200).set_body_string(GRANITE_TREE))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .with_priority(20)
        .mount(&server)
        .await;
    server
}

/// The `author` of every listing request the hub received, and the number of
/// listing requests.
async fn listing_authors(server: &MockServer) -> (Vec<String>, usize) {
    let requests = server.received_requests().await.expect("recorded");
    let listings: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path() == "/api/models")
        .collect();
    let authors = listings
        .iter()
        .filter_map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "author")
                .map(|(_, v)| v.into_owned())
        })
        .collect();
    (authors, listings.len())
}

fn query_value(server_requests: &[wiremock::Request], key: &str) -> Vec<String> {
    server_requests
        .iter()
        .filter(|r| r.url.path() == "/api/models")
        .filter_map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
        })
        .collect()
}

#[tokio::test]
async fn every_request_names_an_author_of_the_lists_and_nothing_else_is_kept() {
    // GIVEN the two embedded lists and a Hub that also slips an owner on
    // neither list into a page
    let authors = search_authors(None);
    let server = hub().await;
    let client = HfRegistryClient::with_hub_base(None, &server.uri());
    let profile = mac();
    let ctx = SearchContext {
        authors: &authors,
        index: None,
        selection: &[],
        hardware: Some(&profile),
    };

    // WHEN a free query that names another organisation is searched
    let search = BoundedSearch {
        query: "MaziyarPanahi/Qwen3-4B".to_owned(),
        ..BoundedSearch::default()
    };
    let page = client
        .search_bounded(&search, &ctx)
        .await
        .expect("the search answers");

    // THEN one listing request per organisation, each carrying an author of
    // the lists, and no result from an owner on neither list
    let (asked, listings) = listing_authors(&server).await;
    assert_eq!(listings, authors.len());
    assert_eq!(
        asked.len(),
        listings,
        "every listing request names an author"
    );
    for org in &asked {
        assert!(
            authors.iter().any(|a| &a.org == org),
            "{org} is not on the lists"
        );
    }
    assert!(!asked
        .iter()
        .any(|a| a.eq_ignore_ascii_case("MaziyarPanahi")));
    for required in [
        "Qwen",
        "mistralai",
        "google",
        "meta-llama",
        "unsloth",
        "bartowski",
    ] {
        assert!(asked.iter().any(|a| a == required), "{required} is asked");
    }
    assert!(!page.models.is_empty());
    for found in &page.models {
        let owner = found.model.repo_id.split('/').next().unwrap_or_default();
        assert!(authors.iter().any(|a| a.org.eq_ignore_ascii_case(owner)));
    }
}

#[tokio::test]
async fn mlx_is_refused_off_apple_silicon_with_its_reason_and_asked_on_it() {
    // GIVEN the lists, a Hub, and a request for MLX
    let authors = search_authors(None);
    let search = BoundedSearch {
        query: "Qwen3".to_owned(),
        format: SearchFormat::Mlx,
        ..BoundedSearch::default()
    };
    let server = hub().await;
    let client = HfRegistryClient::with_hub_base(None, &server.uri());

    for (profile, accepted) in [
        (Some(cuda_box()), false),
        (None, false),
        (Some(mac()), true),
    ] {
        let ctx = SearchContext {
            authors: &authors,
            index: None,
            selection: &[],
            hardware: profile.as_ref(),
        };

        // WHEN the search runs on that machine
        let outcome = client.search_bounded(&search, &ctx).await;

        // THEN only Apple Silicon asks the Hub; elsewhere the refusal says why
        match (accepted, outcome) {
            (true, Ok(page)) => assert_eq!(page.format, SearchFormat::Mlx),
            (false, Err(err @ BoundedSearchError::MlxNeedsAppleSilicon)) => {
                assert_eq!(err.code(), "mlx_needs_apple_silicon");
                assert!(err.to_string().contains("Apple Silicon"));
            }
            (accepted, other) => panic!("accepted={accepted}: unexpected {other:?}"),
        }
    }
    let requests = server.received_requests().await.expect("recorded");
    let filters = query_value(&requests, "filter");
    assert_eq!(
        filters.len(),
        authors.len(),
        "only the Apple Silicon run asked"
    );
    assert!(filters.iter().all(|f| f == "mlx"));
}

#[tokio::test]
async fn the_hub_sorts_are_passed_through_by_their_api_key() {
    // GIVEN the lists and a Hub
    let authors = search_authors(None);
    let profile = mac();
    let ctx = SearchContext {
        authors: &authors,
        index: None,
        selection: &[],
        hardware: Some(&profile),
    };

    for (sort, key) in [
        (SearchSort::Likes, "likes"),
        (SearchSort::Downloads, "downloads"),
        (SearchSort::Updated, "lastModified"),
        (SearchSort::BestMatch, "downloads"),
    ] {
        let server = hub().await;
        let client = HfRegistryClient::with_hub_base(None, &server.uri());
        let search = BoundedSearch {
            sort,
            ..BoundedSearch::default()
        };

        // WHEN the search runs with that order
        client
            .search_bounded(&search, &ctx)
            .await
            .expect("the search answers");

        // THEN every listing request carries the Hub's key
        let requests = server.received_requests().await.expect("recorded");
        let keys = query_value(&requests, "sort");
        assert_eq!(keys.len(), authors.len());
        assert!(keys.iter().all(|k| k == key), "{sort:?} sends {key}");
    }
}

fn recorded_pages_merged(sort: SearchSort, selection: &[String]) -> Vec<HfModelCard> {
    recorded_pages_merged_for("Qwen3-4B", sort, selection)
}

fn recorded_pages_merged_for(
    query: &str,
    sort: SearchSort,
    selection: &[String],
) -> Vec<HfModelCard> {
    let authors = search_authors(None);
    let ctx = SearchContext {
        authors: &authors,
        index: None,
        selection,
        hardware: None,
    };
    let search = BoundedSearch {
        query: query.to_owned(),
        sort,
        ..BoundedSearch::default()
    };
    let pages = ["Qwen", "unsloth", "bartowski", "lmstudio-community"]
        .iter()
        .map(|org| page(org))
        .collect();
    merge(pages, &search, &ctx)
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

#[test]
fn without_words_the_selection_does_not_lead_the_list() {
    // GIVEN the recorded pages and a selection holding one converter's copy
    let selection = vec!["bartowski/Qwen_Qwen3-4B-GGUF".to_owned()];

    // WHEN they are merged under the best match with no word typed
    let merged = recorded_pages_merged_for("", SearchSort::BestMatch, &selection);
    let ids: Vec<&str> = merged.iter().map(|m| m.repo_id.as_str()).collect();

    // THEN the official publisher comes first, and the copy of the selection
    // stands where its downloads put it among the converters
    assert!(ids[0].starts_with("Qwen/"), "{ids:?}");
    assert_ne!(ids[0], "bartowski/Qwen_Qwen3-4B-GGUF");
    let converters: Vec<u64> = merged
        .iter()
        .filter(|m| !m.repo_id.starts_with("Qwen/"))
        .map(|m| m.downloads)
        .collect();
    assert!(converters.windows(2).all(|w| w[0] >= w[1]), "{ids:?}");
}

#[test]
fn best_match_orders_exact_name_then_selection_then_official_then_downloads() {
    // GIVEN the recorded pages and a selection holding one converter's copy
    let selection = vec!["bartowski/Qwen_Qwen3-4B-GGUF".to_owned()];

    // WHEN they are merged under the best match for "Qwen3-4B"
    let merged = recorded_pages_merged(SearchSort::BestMatch, &selection);
    let ids: Vec<&str> = merged.iter().map(|m| m.repo_id.as_str()).collect();

    // THEN the three repositories named exactly come first: the one in the
    // selection, then the official one, then the converter's; the rest put
    // the official publisher first, then downloads
    assert_eq!(
        &ids[..3],
        &[
            "bartowski/Qwen_Qwen3-4B-GGUF",
            "Qwen/Qwen3-4B-GGUF",
            "unsloth/Qwen3-4B-GGUF"
        ]
    );
    assert_eq!(ids[3], "Qwen/Qwen3-VL-4B-Instruct-GGUF");
    assert_eq!(ids[4], "Qwen/Qwen3-Embedding-4B-GGUF");
    assert_eq!(ids[5], "unsloth/Qwen3.5-4B-GGUF");
    let rest: Vec<u64> = merged[5..].iter().map(|m| m.downloads).collect();
    assert!(rest.windows(2).all(|w| w[0] >= w[1]), "then downloads");
    assert_eq!(ids.len(), 13, "every recorded repository, once");
}

#[test]
fn hub_sorts_order_the_merged_pages_and_cards_carry_date_and_kind() {
    // GIVEN the recorded pages
    // WHEN they are merged under each Hub order
    let likes = recorded_pages_merged(SearchSort::Likes, &[]);
    let downloads = recorded_pages_merged(SearchSort::Downloads, &[]);
    let updated = recorded_pages_merged(SearchSort::Updated, &[]);

    // THEN each order holds across organisations, and every card carries its
    // update date and the kind of its owner
    assert!(likes.windows(2).all(|w| w[0].likes >= w[1].likes));
    assert!(downloads
        .windows(2)
        .all(|w| w[0].downloads >= w[1].downloads));
    assert!(updated
        .windows(2)
        .all(|w| w[0].last_modified >= w[1].last_modified));
    assert_eq!(updated[0].repo_id, "bartowski/Qwen_Qwen3.5-4B-GGUF");
    for card in &updated {
        assert!(card.last_modified.is_some(), "{} has a date", card.repo_id);
        assert!(card.publisher_kind.is_some(), "{} has a kind", card.repo_id);
    }
    let qwen = updated
        .iter()
        .find(|c| c.repo_id == "Qwen/Qwen3-4B-GGUF")
        .expect("listed");
    assert_eq!(qwen.publisher_kind, Some(PublisherKind::Official));
    assert_eq!(
        qwen.last_modified.as_deref(),
        Some("2025-05-21T06:32:23.000Z")
    );
}

fn found(repo: &str, tree_text: &str, profile: Option<&HardwareProfile>) -> FoundModel {
    found_in(repo, tree_text, profile, &index())
}

/// The fixture index without the card `id`: what the registry is for a
/// repository whose origin has no card.
fn index_without(id: &str) -> RegistryIndex {
    let mut value: serde_json::Value =
        serde_json::from_slice(INDEX).expect("the fixture index is JSON");
    let cards = value["cards"]
        .as_array_mut()
        .expect("the fixture index lists its cards");
    let before = cards.len();
    cards.retain(|card| card["id"] != id);
    assert_eq!(
        cards.len() + 1,
        before,
        "the fixture index holds the card {id}"
    );
    RegistryIndex::parse(value.to_string().as_bytes()).expect("the index parses without the card")
}

fn found_in(
    repo: &str,
    tree_text: &str,
    profile: Option<&HardwareProfile>,
    index: &RegistryIndex,
) -> FoundModel {
    let authors = search_authors(None);
    let ctx = SearchContext {
        authors: &authors,
        index: Some(index),
        selection: &[],
        hardware: profile,
    };
    let item = recorded_pages()
        .values()
        .filter_map(|v| v.as_array())
        .flatten()
        .find(|m| m["id"] == repo)
        .cloned()
        .expect("the repository is recorded");
    let mut model = parse_model_list_item(&item, None).expect("the item parses");
    model.publisher_kind = crate::recommend::publisher_kind(repo);
    found_model(model, &item, &tree(tree_text), SearchFormat::Gguf, &ctx)
}

#[test]
fn a_converter_copy_of_a_model_with_a_card_inherits_it() {
    // GIVEN a converter's repository whose origin, Qwen/Qwen3-4B, has an
    // Apollia card
    let profile = mac();

    // WHEN its result is read
    let found = found("unsloth/Qwen3-4B-GGUF", QWEN3_TREE, Some(&profile));
    let card = &found.card;
    let origin = index();
    let origin = origin.card("qwen3/Qwen3-4B").expect("the card exists");

    // THEN the card inherits the description, the author fields and the
    // defaults, and adds converter, format, variants, size and date
    assert_eq!(card.origin_repo.as_deref(), Some("Qwen/Qwen3-4B"));
    assert_eq!(card.origin_card_id.as_deref(), Some("qwen3/Qwen3-4B"));
    assert_eq!(card.description, origin.description);
    assert_eq!(card.author_fields, origin.author_fields);
    assert_eq!(card.defaults.as_ref(), Some(&origin.defaults));
    assert_eq!(card.owner, "unsloth");
    assert_eq!(card.publisher_kind, Some(PublisherKind::Converter));
    assert_eq!(card.format, SearchFormat::Gguf);
    assert!(!card.variants.is_empty());
    assert!(card.size_bytes.is_some_and(|s| s > 0));
    assert_eq!(
        card.last_modified.as_deref(),
        Some("2025-06-08T08:08:49.000Z")
    );
    assert_eq!(
        card.source_url,
        "https://huggingface.co/unsloth/Qwen3-4B-GGUF"
    );
    assert!(
        card.capabilities.tools,
        "computed from the recorded template"
    );
}

#[test]
fn a_repository_without_a_card_gets_facts_only_and_no_readme_sentence() {
    // GIVEN a converter's repository whose origin has no Apollia card: the
    // registry as it is without the card of that model
    let profile = mac();
    let index = index_without("granite3.3/granite-3.3-8b-instruct");

    // WHEN its result is read
    let found = found_in(GRANITE, GRANITE_TREE, Some(&profile), &index);
    let card = &found.card;

    // THEN the card is facts: a sentence Apollia builds, parameters,
    // architecture, context, capabilities, licence
    assert_eq!(card.origin_card_id, None);
    assert_eq!(
        card.origin_repo.as_deref(),
        Some("ibm-granite/granite-3.3-8b-instruct")
    );
    assert_eq!(
        card.description.en,
        "GGUF conversion of ibm-granite/granite-3.3-8b-instruct by bartowski."
    );
    assert!(card
        .description
        .fr
        .contains("ibm-granite/granite-3.3-8b-instruct"));
    assert!(card.author_fields.is_empty());
    assert_eq!(card.defaults, None);
    assert_eq!(card.parameters, Some(8_170_864_640));
    assert_eq!(card.architecture.as_deref(), Some("granite"));
    assert_eq!(card.context_length, Some(131_072));
    assert_eq!(card.license.as_deref(), Some("apache-2.0"));

    // AND no sentence of the recorded README is in it
    let serialized = serde_json::to_string(card).expect("the card serializes");
    let sentences: Vec<&str> = GRANITE_README
        .split(['.', '\n', '!', '?'])
        .map(str::trim)
        .filter(|s| s.split_whitespace().count() >= 4)
        .collect();
    assert!(sentences.len() > 10, "the README has prose to compare");
    for sentence in sentences {
        assert!(
            !serialized.contains(sentence),
            "README sentence copied: {sentence}"
        );
    }
}

#[test]
fn fits_on_this_machine_keeps_the_cards_with_a_recommendation() {
    // GIVEN the granite repository (Q4_K_M near 5 GB) and two machines
    let table = [(mac(), true), (small_pc(), false)];

    for (profile, fits) in table {
        // WHEN its result is read on that machine
        let found = found(GRANITE, GRANITE_TREE, Some(&profile));

        // THEN a variant is recommended exactly when the machine holds it
        assert_eq!(
            found.card.has_recommendation(),
            fits,
            "{} GB machine",
            profile.total_ram_gb
        );
    }
}

#[tokio::test]
async fn the_fit_parameter_filters_the_page_and_the_page_carries_the_capacity() {
    // GIVEN a Hub with the granite repository and two machines
    let authors = search_authors(None);
    let search = BoundedSearch {
        query: "granite-3.3-8b-instruct".to_owned(),
        fits_only: true,
        ..BoundedSearch::default()
    };

    for (profile, kept) in [(mac(), true), (small_pc(), false)] {
        let server = hub().await;
        let client = HfRegistryClient::with_hub_base(None, &server.uri());
        let ctx = SearchContext {
            authors: &authors,
            index: None,
            selection: &[],
            hardware: Some(&profile),
        };

        // WHEN the search asks for what fits
        let page = client
            .search_bounded(&search, &ctx)
            .await
            .expect("the search answers");

        // THEN the granite card is kept only where it fits, and the page
        // carries the machine's estimated capacity
        let has_granite = page.models.iter().any(|r| r.model.repo_id == GRANITE);
        assert_eq!(has_granite, kept, "{} GB machine", profile.total_ram_gb);
        assert!(page.models.iter().all(|r| r.card.has_recommendation()));
        assert_eq!(
            page.capacity_gb,
            Some(crate::recommend::estimated_capacity_gb(&profile))
        );
    }
}

#[tokio::test]
async fn offline_the_search_is_unavailable_by_name_never_an_empty_page() {
    // GIVEN a Hub address nothing can listen on: port 0 is refused at once
    let authors = search_authors(None);
    let client = HfRegistryClient::with_hub_base(None, "http://127.0.0.1:0");
    let profile = mac();
    let ctx = SearchContext {
        authors: &authors,
        index: None,
        selection: &[],
        hardware: Some(&profile),
    };

    // WHEN a search runs
    let outcome = client.search_bounded(&BoundedSearch::default(), &ctx).await;

    // THEN it says the search is unavailable offline
    match outcome {
        Err(err @ BoundedSearchError::Offline) => {
            assert_eq!(err.code(), "offline");
            assert_eq!(err.to_string(), "search is unavailable offline");
        }
        other => panic!("expected the offline error, got {other:?}"),
    }
}

#[test]
fn the_index_lists_replace_the_embedded_ones_when_it_carries_them() {
    // GIVEN an index carrying the lists, and one carrying none
    let with_lists = index();
    let mut without = index();
    without.publishers = crate::model_cards::PublisherLists::default();

    // WHEN the authors are read from each
    let from_index = search_authors(Some(&with_lists));
    let embedded = search_authors(Some(&without));

    // THEN both name the official publishers and the converters, the
    // converters marked as such
    for authors in [&from_index, &embedded] {
        assert!(authors
            .iter()
            .any(|a| a.org == "Qwen" && a.kind == PublisherKind::Official));
        assert!(authors
            .iter()
            .any(|a| a.org == "mlx-community" && a.kind == PublisherKind::Converter));
    }
    assert_eq!(embedded, search_authors(None));
}
