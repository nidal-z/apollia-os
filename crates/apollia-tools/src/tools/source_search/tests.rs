use std::path::Path;

use apollia_sources::chunk::{Chunk, Location};
use apollia_sources::index::{DocumentRecord, SourceIndex};
use apollia_sources::{NewSource, SourceKind};

use super::*;

/// A registry on a fresh `system.db` under `dir`, holding two folder
/// sources, each indexed with one passage about a lease.
async fn two_indexed_folders(dir: &Path) -> (SourceRegistryHandle, String, String) {
    let registry = SourceRegistryHandle::open(&dir.join("system.db")).unwrap();
    let mut ids = Vec::new();
    for (label, text) in [
        ("Contracts", "The lease of the office ends in June."),
        ("Private", "The lease of the flat is secret."),
    ] {
        let source = registry
            .create(NewSource {
                kind: SourceKind::Folder,
                label: label.to_owned(),
                group: None,
                reference: Some(format!("folder-{label}")),
            })
            .await
            .unwrap();
        let mut index = SourceIndex::open(dir, &source.id).unwrap();
        index
            .upsert_document(
                &DocumentRecord {
                    path: format!("{label}/lease.txt"),
                    fingerprint: "f".to_owned(),
                    modified_at: "2026-10-04T00:00:00Z".to_owned(),
                },
                &[Chunk {
                    text: text.to_owned(),
                    location: Location::Lines { first: 1, last: 1 },
                }],
                None,
            )
            .unwrap();
        ids.push(source.id);
    }
    let private = ids.pop().unwrap();
    let contracts = ids.pop().unwrap();
    (registry, contracts, private)
}

fn input(query: &str, sources: Option<Vec<String>>, limit: Option<u32>) -> SourceSearchInput {
    SourceSearchInput {
        query: query.to_owned(),
        sources,
        limit,
    }
}

#[tokio::test]
async fn test_a_search_reads_the_checked_sources_only() {
    // GIVEN two indexed folders, only the first checked for the agent
    let dir = tempfile::tempdir().unwrap();
    let (registry, contracts, _private) = two_indexed_folders(dir.path()).await;
    let tool = SourceSearchTool::new(registry, vec![contracts.clone()]);

    // WHEN the agent searches without naming a source
    let output = tool.run(input("lease", None, None)).await.unwrap();

    // THEN the passage of the checked folder comes back with its excerpt,
    //      document, location, score, source and the mode, and nothing of the
    //      other folder
    assert_eq!(output.results.len(), 1);
    let hit = &output.results[0];
    assert_eq!(hit.source, contracts);
    assert_eq!(hit.document, "Contracts/lease.txt");
    assert!(hit.excerpt.contains("office"));
    assert_eq!(hit.location, Location::Lines { first: 1, last: 1 });
    assert!(hit.score > 0.0);
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["mode"], "keywords_only");
    for key in ["excerpt", "document", "location", "score", "source"] {
        assert!(
            json["results"][0].get(key).is_some(),
            "{key} missing: {json}"
        );
    }
}

#[tokio::test]
async fn test_a_source_not_checked_is_refused_before_any_read() {
    // GIVEN two indexed folders, only the first checked for the agent
    let dir = tempfile::tempdir().unwrap();
    let (registry, contracts, private) = two_indexed_folders(dir.path()).await;
    let tool = SourceSearchTool::new(registry, vec![contracts.clone()]);

    // WHEN the agent names the other folder, alone or beside the checked one
    let alone = tool
        .run(input("lease", Some(vec![private.clone()]), None))
        .await;
    let beside = tool
        .execute(json!({ "query": "lease", "sources": [contracts, private.clone()] }))
        .await;

    // THEN both calls are refused with the source named, and no result leaks
    match alone {
        Err(SourceSearchError::SourceNotChecked(id)) => assert_eq!(id, private),
        other => panic!("expected a refusal, got {other:?}"),
    }
    match beside {
        Err(ToolExecutionError::ExecutionFailed { code, message }) => {
            assert_eq!(code, "source_not_checked");
            assert!(message.contains(&private), "{message}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn test_the_query_and_the_limit_are_checked_first() {
    // GIVEN a tool over one checked folder
    let dir = tempfile::tempdir().unwrap();
    let (registry, contracts, _) = two_indexed_folders(dir.path()).await;
    let tool = SourceSearchTool::new(registry, vec![contracts]);

    // WHEN it is asked a blank query, a limit of zero, of fifty-one, of fifty
    let blank = tool.run(input("   ", None, None)).await;
    let zero = tool.run(input("lease", None, Some(0))).await;
    let above = tool.run(input("lease", None, Some(51))).await;
    let fifty = tool.run(input("lease", None, Some(50))).await;

    // THEN the first three are refused, the last answers
    assert!(matches!(blank, Err(SourceSearchError::EmptyQuery)));
    assert!(matches!(zero, Err(SourceSearchError::InvalidLimit(0))));
    assert!(matches!(above, Err(SourceSearchError::InvalidLimit(51))));
    assert_eq!(fifty.unwrap().results.len(), 1);
}

#[test]
fn test_the_descriptor_is_a_reading_tool() {
    // GIVEN the descriptor of the tool
    let descriptor = SourceSearchTool::descriptor();

    // WHEN its family and its validity are read
    let valid = descriptor.validate();

    // THEN it is a valid reading tool named source_search
    assert!(valid.is_ok(), "{valid:?}");
    assert_eq!(descriptor.name, SOURCE_SEARCH_TOOL);
    assert_eq!(descriptor.family, ActionFamily::Read);
    assert!(descriptor.is_read_only);
}
