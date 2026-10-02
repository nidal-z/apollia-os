//! The embedded documentation against the site it was packed from.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use super::*;

/// The documentation site of this repository.
fn site() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/site")
}

/// The file of the site a page in `language` is built from.
fn site_file(language: &str, source: &str) -> PathBuf {
    match language {
        "fr" => site()
            .join("i18n/fr/docusaurus-plugin-content-docs/current")
            .join(source),
        _ => site().join("docs").join(source),
    }
}

/// An archive holding `manifest` and the given entries, built in memory.
fn archive(manifest: &serde_json::Value, entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file(MANIFEST_ENTRY, options).unwrap();
    writer
        .write_all(serde_json::to_string(manifest).unwrap().as_bytes())
        .unwrap();
    for (name, body) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A manifest with one English-only page written by hand, and one generated
/// reference page.
fn fixture_manifest() -> serde_json::Value {
    let view = |base: &str| {
        serde_json::json!({
            "public_url": base,
            "tree": [{"kind": "doc", "route": "/guide", "label": "Guide"}],
            "navigation": {"/guide": {"kind": "page", "title": "Guide", "previous": null, "next": null}},
        })
    };
    serde_json::json!({
        "format": 1,
        "public_url": "https://docs.example",
        "default_language": "en",
        "languages": ["en", "fr"],
        "openapi": "openapi.json",
        "pages": {
            "/guide": {"source": "guide.md", "languages": ["en"], "generated": false},
            "/reference/sdk/llm": {"source": "reference/sdk/llm.md", "languages": ["en"], "generated": true},
        },
        "locales": {"en": view("https://docs.example"), "fr": view("https://docs.example/fr")},
    })
}

#[test]
fn test_every_page_source_is_byte_identical_to_the_site_file() {
    // GIVEN the archive this binary embeds
    let docs = EmbeddedDocs::embedded().unwrap();
    assert!(
        docs.manifest.pages.len() > 100,
        "the whole site is packed, not a sample"
    );

    // WHEN every route is read in every language it exists in
    for (route, info) in &docs.manifest.pages {
        for language in &info.languages {
            let locale = if language == "fr" {
                UiLocale::Fr
            } else {
                UiLocale::En
            };
            let page = docs.page(route, locale).unwrap();

            // THEN the source is the site's file, byte for byte
            let expected = std::fs::read_to_string(site_file(language, &info.source)).unwrap();
            assert_eq!(page.language, *language, "{route}");
            assert_eq!(page.markdown, expected, "{route} in {language}");
        }
    }
}

#[test]
fn test_mirrored_page_is_served_in_the_application_language() {
    // GIVEN the embedded archive and a page with a French mirror
    let docs = EmbeddedDocs::embedded().unwrap();
    let route = "/architecture/decisions";

    // WHEN it is read with the interface in French
    let page = docs.page(route, UiLocale::Fr).unwrap();

    // THEN the French source is served, unmarked, with its French address
    let expected =
        std::fs::read_to_string(site_file("fr", "architecture/08-decisions.md")).unwrap();
    assert_eq!(page.language, "fr");
    assert!(!page.fallback);
    assert_eq!(page.markdown, expected);
    assert_eq!(
        page.public_url,
        "https://docs.apollia.fr/fr/architecture/decisions"
    );
}

#[test]
fn test_page_without_french_mirror_falls_back_to_english_with_the_mark() {
    // GIVEN an archive whose hand-written page exists in English only
    let bytes = archive(
        &fixture_manifest(),
        &[
            ("docs/en/guide.md", "# Guide\n"),
            ("docs/en/reference/sdk/llm.md", "# llm\n"),
        ],
    );
    let docs = EmbeddedDocs::open(&bytes).unwrap();

    // WHEN it is read with the interface in French, then in English
    let french = docs.page("/guide", UiLocale::Fr).unwrap();
    let english = docs.page("/guide", UiLocale::En).unwrap();

    // THEN French reads the English source marked as a fallback, English does not
    assert_eq!(french.language, "en");
    assert_eq!(french.requested_language, "fr");
    assert!(french.fallback);
    assert_eq!(french.markdown, "# Guide\n");
    assert_eq!(french.public_url, "https://docs.example/fr/guide");
    assert!(!english.fallback);
    assert_eq!(english.public_url, "https://docs.example/guide");
}

#[test]
fn test_generated_references_are_english_and_not_marked_as_fallback() {
    // GIVEN the embedded archive
    let docs = EmbeddedDocs::embedded().unwrap();
    let generated: Vec<(&String, &PageInfo)> = docs
        .manifest
        .pages
        .iter()
        .filter(|(_, info)| info.generated)
        .collect();
    let prefixes: BTreeSet<&str> = generated
        .iter()
        .filter_map(|(_, info)| info.source.split('/').nth(1))
        .collect();
    assert_eq!(prefixes, BTreeSet::from(["cli", "sdk"]));

    // WHEN each generated reference is read with the interface in French
    for (route, info) in generated {
        let page = docs.page(route, UiLocale::Fr).unwrap();

        // THEN it is English, as on the site, and carries no fallback mark
        assert_eq!(info.languages, vec!["en".to_owned()], "{route}");
        assert_eq!(page.language, "en", "{route}");
        assert!(page.generated, "{route}");
        assert!(!page.fallback, "{route}");
    }
    // AND the HTTP reference ships as its OpenAPI document
    let openapi = read_entry(ARCHIVE, &docs.manifest.openapi).unwrap();
    let expected = std::fs::read(site().join("../../clients/openapi.json")).unwrap();
    assert_eq!(openapi, expected);
}

#[test]
fn test_every_route_has_its_source_and_previous_next_form_one_chain() {
    // GIVEN the embedded archive
    let docs = EmbeddedDocs::embedded().unwrap();
    let manifest = &docs.manifest;

    // WHEN its pages and the navigation of each language are walked
    for (route, info) in &manifest.pages {
        for language in &info.languages {
            // THEN each page has its source in the archive, in each language listed
            let name = format!("docs/{language}/{}", info.source);
            assert!(read_entry(ARCHIVE, &name).is_ok(), "{route}: {name}");
        }
    }
    for (language, view) in &manifest.locales {
        let navigation = &view.navigation;
        let starts: Vec<&String> = navigation
            .iter()
            .filter(|(_, entry)| entry.previous.is_none())
            .map(|(route, _)| route)
            .collect();
        assert_eq!(starts.len(), 1, "{language}: one first page");

        // AND following next from the first page visits every route once,
        // each next pointing back through previous
        let mut seen = BTreeSet::new();
        let mut current = Some(starts[0].clone());
        while let Some(route) = current {
            assert!(
                seen.insert(route.clone()),
                "{language}: {route} visited twice"
            );
            let entry = &navigation[&route];
            match entry.kind.as_str() {
                "page" => assert!(manifest.pages.contains_key(&route), "{route}"),
                "generated-index" => assert!(!manifest.pages.contains_key(&route), "{route}"),
                other => panic!("{route}: unknown kind {other}"),
            }
            if let Some(next) = &entry.next {
                let back = navigation[&next.route].previous.as_ref().unwrap();
                assert_eq!(back.route, route, "{language}: {} goes back", next.route);
                assert_eq!(next.title, navigation[&next.route].title);
            }
            current = entry.next.as_ref().map(|link| link.route.clone());
        }
        assert_eq!(seen.len(), navigation.len(), "{language}: one chain");
        for route in manifest.pages.keys() {
            assert!(
                navigation.contains_key(route),
                "{language}: {route} unreachable"
            );
        }
    }
}

#[test]
fn test_manifest_in_french_carries_the_french_labels_and_address() {
    // GIVEN the embedded archive
    let docs = EmbeddedDocs::embedded().unwrap();

    // WHEN its manifest is read in French
    let manifest = docs.manifest(UiLocale::Fr).unwrap();

    // THEN the tree and the address are those of the French site
    assert_eq!(manifest.language, "fr");
    assert_eq!(manifest.view.public_url, "https://docs.apollia.fr/fr");
    let labels: Vec<&str> = manifest
        .view
        .tree
        .iter()
        .filter_map(|item| match item {
            TreeItem::Category { label, .. } => Some(label.as_str()),
            TreeItem::Doc { .. } => None,
        })
        .collect();
    assert!(labels.contains(&"Tutoriels"), "{labels:?}");
    assert!(labels.contains(&"Guides pratiques"), "{labels:?}");
}

#[test]
fn test_unknown_route_is_a_typed_error() {
    // GIVEN the embedded archive
    let docs = EmbeddedDocs::embedded().unwrap();

    // WHEN a route the site does not serve is read
    let error = docs.page("/no/such/page", UiLocale::En).unwrap_err();

    // THEN the error names the route
    assert!(matches!(error, DocsError::UnknownRoute { ref route } if route == "/no/such/page"));
}

#[test]
fn test_manifest_of_another_format_is_refused() {
    // GIVEN an archive whose manifest declares a format this build does not read
    let mut manifest = fixture_manifest();
    manifest["format"] = serde_json::json!(2);
    let bytes = archive(&manifest, &[]);

    // WHEN it is opened
    let error = EmbeddedDocs::open(&bytes).unwrap_err();

    // THEN it is refused rather than misread
    assert!(matches!(error, DocsError::Format { found: 2 }));
}
