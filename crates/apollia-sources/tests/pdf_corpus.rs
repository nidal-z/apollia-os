//! The PDF reader against twenty real French documents.
//!
//! The corpus and its witness phrases live in `tests/fixtures/pdf-corpus`,
//! described by its `README.md`. `APOLLIA_PDF_CORPUS` points the test at
//! another directory laid out the same way: PDF files beside a
//! `witnesses.tsv`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use apollia_sources::read_pdf;

/// Fewest documents of the corpus that must yield every witness phrase.
const DOCUMENTS_REQUIRED: usize = 19;

fn corpus() -> PathBuf {
    std::env::var_os("APOLLIA_PDF_CORPUS").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-corpus"),
        PathBuf::from,
    )
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn witnesses(dir: &Path) -> BTreeMap<String, Vec<String>> {
    let table = std::fs::read_to_string(dir.join("witnesses.tsv")).unwrap();
    let mut witnesses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in table.lines().filter(|line| !line.is_empty()) {
        let (file, phrase) = line.split_once('\t').unwrap();
        witnesses
            .entry(file.to_string())
            .or_default()
            .push(collapse(phrase));
    }
    witnesses
}

#[test]
fn test_nineteen_french_documents_out_of_twenty_yield_every_witness_phrase() {
    // GIVEN twenty French documents, each with five accented witness phrases
    let dir = corpus();
    let witnesses = witnesses(&dir);
    assert_eq!(witnesses.len(), 20, "{}", dir.display());
    assert!(witnesses.values().all(|phrases| phrases.len() == 5));

    // WHEN each document is read
    let mut missed = Vec::new();
    for (file, phrases) in &witnesses {
        let text = match read_pdf(&dir.join(file)) {
            Ok(document) => collapse(
                &document
                    .pages
                    .iter()
                    .filter_map(|page| page.text.as_text())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Err(error) => {
                missed.push(format!("{file}: {error}"));
                continue;
            }
        };
        let absent: Vec<&String> = phrases
            .iter()
            .filter(|phrase| !text.contains(phrase.as_str()))
            .collect();
        if !absent.is_empty() {
            missed.push(format!("{file}: {absent:?}"));
        }
    }

    // THEN at least nineteen documents yield all five phrases, accents intact
    let complete = witnesses.len() - missed.len();
    assert!(
        complete >= DOCUMENTS_REQUIRED,
        "{complete} of {} documents complete; missed: {missed:#?}",
        witnesses.len()
    );
}
