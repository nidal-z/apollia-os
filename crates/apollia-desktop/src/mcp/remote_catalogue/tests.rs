use super::*;

use crate::mcp::enrichments::compiled_enrichments;

const INDEX: &[u8] = include_bytes!("fixtures/index.json");
const SIGNATURE: &str = include_str!("fixtures/index.json.minisig");
const KEY: &str = include_str!("fixtures/test-dist.key.pub");

fn entry(package: &str, label: &str) -> RemoteEntry {
    let mut entry = compiled_enrichments()
        .into_iter()
        .find(|e| e.package_identifier == "@playwright/mcp")
        .expect("browser entry");
    entry.package_identifier = package.to_owned();
    entry
        .operator_label
        .insert("en".to_owned(), label.to_owned());
    RemoteEntry {
        entry,
        npm_lock: None,
    }
}

fn catalogue(entries: Vec<RemoteEntry>) -> RemoteCatalogue {
    RemoteCatalogue {
        schema_version: SCHEMA_VERSION,
        entries,
    }
}

#[test]
fn the_published_index_is_accepted_with_the_key_it_was_signed_with() {
    // GIVEN the index built from the selection, signed by a throwaway key
    // WHEN it is checked with that key
    let accepted = verify(INDEX, SIGNATURE, Some(KEY)).expect("accepted");

    // THEN it holds every compiled entry, and the lock of the pinned browser
    assert_eq!(accepted.entries.len(), compiled_enrichments().len());
    let lock = accepted.npm_lock("@playwright/mcp").expect("browser lock");
    assert!(lock.contains("\"@playwright/mcp\""));
}

#[test]
fn an_index_altered_after_signing_is_refused() {
    // GIVEN the signed index with one byte changed
    let mut altered = INDEX.to_vec();
    let at = altered.iter().position(|b| *b == b'W').expect("a letter W");
    altered[at] = b'w';

    // WHEN it is checked
    let refused = verify(&altered, SIGNATURE, Some(KEY));

    // THEN the signature does not hold
    assert!(matches!(refused, Err(CatalogueError::Signature(_))));
}

#[test]
fn without_a_key_no_index_is_accepted() {
    // GIVEN a build that carries no distribution key
    // WHEN the signed index is checked
    let refused = verify(INDEX, SIGNATURE, None);

    // THEN nothing is accepted
    assert!(matches!(refused, Err(CatalogueError::NoTrustedKey)));
}

#[test]
fn an_index_in_another_format_is_refused() {
    // GIVEN an index of a format this build does not read
    let later = br#"{"schema_version":2,"entries":[]}"#;

    // WHEN it is parsed
    let refused = RemoteCatalogue::parse(later);

    // THEN it is refused, naming the format
    assert!(matches!(refused, Err(CatalogueError::Schema { found: 2 })));
}

#[test]
fn an_entry_of_the_index_replaces_the_compiled_one_and_a_new_one_is_added() {
    // GIVEN the compiled selection and an index that relabels the browser
    //       and brings a connection the build does not know
    let compiled = compiled_enrichments();
    let remote = catalogue(vec![
        entry("@playwright/mcp", "Browser"),
        entry("@example/new-connection", "New"),
    ]);

    // WHEN the index is laid over the selection
    let merged = merge(compiled.clone(), &remote);

    // THEN the browser keeps its place with the new label, and the new entry ends the list
    assert_eq!(merged.len(), compiled.len() + 1);
    let at = compiled
        .iter()
        .position(|e| e.package_identifier == "@playwright/mcp")
        .expect("browser compiled");
    assert_eq!(merged[at].operator_label["en"], "Browser");
    let last = merged.last().expect("an entry");
    assert_eq!(last.package_identifier, "@example/new-connection");
}

#[test]
fn a_kept_copy_is_read_back_and_one_that_no_longer_holds_is_refused() {
    // GIVEN a cache holding the signed index
    let dir = tempfile::tempdir().expect("temp dir");
    store(dir.path(), INDEX, SIGNATURE).expect("kept");

    // WHEN it is read back with the key, then with another index in its place
    let read = cached(dir.path(), Some(KEY))
        .expect("read")
        .expect("a copy");
    std::fs::write(dir.path().join(CACHED_INDEX_FILE_NAME), b"{}").expect("overwritten");
    let refused = cached(dir.path(), Some(KEY));

    // THEN the first is the index, and the replaced copy is refused
    assert_eq!(read.entries.len(), compiled_enrichments().len());
    assert!(refused.is_err());
}

#[test]
fn no_copy_kept_reads_as_none() {
    // GIVEN an empty cache directory
    let dir = tempfile::tempdir().expect("temp dir");

    // WHEN the kept copy is read
    let read = cached(dir.path(), Some(KEY)).expect("read");

    // THEN there is none
    assert!(read.is_none());
}

#[test]
fn the_source_defaults_to_the_project_registry_and_its_mirror() {
    // GIVEN no apollia.toml
    let dir = tempfile::tempdir().expect("temp dir");

    // WHEN the source is resolved without a policy
    let source =
        CatalogueSource::resolve(&dir.path().join("apollia.toml"), &PolicySettings::none());

    // THEN it reads the project registry, then the release mirror
    assert_eq!(source.url, DEFAULT_MCP_CATALOGUE_URL);
    assert_eq!(
        source.mirror_url.as_deref(),
        Some(DEFAULT_MCP_CATALOGUE_MIRROR_URL)
    );
    assert!(source.enabled);
}

#[test]
fn a_catalogue_turned_off_asks_nothing() {
    // GIVEN an apollia.toml that turns the online catalogue off
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("apollia.toml");
    std::fs::write(&path, "[mcp]\ncatalogue_enabled = false\n").expect("written");
    let source = CatalogueSource::resolve(&path, &PolicySettings::none());

    // WHEN the index is refreshed
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let refreshed = runtime
        .block_on(refresh(&source, dir.path(), Some(KEY)))
        .expect("refreshed");

    // THEN no address was asked
    assert!(!source.enabled);
    assert_eq!(refreshed, Refreshed::NotAsked);
}

#[test]
fn the_fixture_is_the_index_the_selection_builds() {
    // GIVEN the selection compiled into this build
    // WHEN the fixture index is read
    let fixture = RemoteCatalogue::parse(INDEX).expect("parses");

    // THEN it lists the same packages, in the same order: the fixture was
    //      rebuilt with `scripts/build_mcp_catalogue_index.py --test-fixture`
    let fixture_ids: Vec<&str> = fixture
        .entries
        .iter()
        .map(|e| e.entry.package_identifier.as_str())
        .collect();
    let compiled = compiled_enrichments();
    let compiled_ids: Vec<&str> = compiled
        .iter()
        .map(|e| e.package_identifier.as_str())
        .collect();
    assert_eq!(fixture_ids, compiled_ids);
}
