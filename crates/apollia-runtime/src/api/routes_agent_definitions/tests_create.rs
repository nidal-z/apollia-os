use axum::http::StatusCode;
use serde_json::{json, Value};

use super::create::identifier_of;
use super::tests::{call, state};
use crate::audit_journal::JournalEntryKind;
use crate::governance::governance_run_id;

const ROUTE: &str = "/api/v1/agent-definitions";

// GIVEN a runtime with a signed journal
// WHEN an agent is created from a validated sheet
// THEN it is stored with the origin `interface`, under an identifier derived
//      from its name, with the sheet as its definition and no override, and
//      the journal holds its creation then one entry per access granted
#[tokio::test]
async fn test_a_validated_sheet_creates_an_agent_of_origin_interface() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, journal) = state(&dir, true).await;
    let journal = journal.unwrap();

    // WHEN
    let (status, created) = call(
        state.clone(),
        "POST",
        ROUTE,
        Some(json!({
            "name": "  Veille \u{e9}conomique ",
            "mission": "Summarise the press every morning.",
            "examples": ["What is new today?", " ", "What is new today?"],
            "access": ["web_search", "file_read"],
            "sources": ["profile", "memory"],
        })),
    )
    .await;
    let (read, stored) = call(
        state,
        "GET",
        "/api/v1/agent-definitions/veille-economique",
        None,
    )
    .await;

    // THEN
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["id"], "veille-economique");
    assert_eq!(created["origin"], "interface");
    assert_eq!(created["package"], Value::Null);
    assert_eq!(created["enabled"], true);
    assert_eq!(created["name"], "Veille \u{e9}conomique");
    assert_eq!(created["mission"], "Summarise the press every morning.");
    assert_eq!(created["examples"], json!(["What is new today?"]));
    assert_eq!(created["access"], json!(["web_search", "file_read"]));
    assert_eq!(created["sources"], json!(["profile", "memory"]));
    assert_eq!(created["overridden"], json!([]));
    assert_eq!(read, StatusCode::OK);
    assert_eq!(stored, created);
    let kinds: Vec<JournalEntryKind> = journal
        .query_run(&governance_run_id("veille-economique"))
        .await
        .into_iter()
        .map(|entry| entry.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            JournalEntryKind::AgentInstalled,
            JournalEntryKind::AccessChanged,
            JournalEntryKind::AccessChanged,
        ]
    );
}

// GIVEN a runtime whose journal is open
// WHEN a sheet is sent without a name, without a mission, with a source the
//      registry cannot vouch for, and to a runtime without a journal
// THEN they answer 422, 422, 503 and 503, and no agent is stored
#[tokio::test]
async fn test_nothing_is_stored_before_a_valid_sheet() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (with_journal, _) = state(&dir, true).await;
    let other = tempfile::tempdir().unwrap();
    let (without_journal, _) = state(&other, false).await;

    // WHEN
    let (no_name, _) = call(
        with_journal.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "  ", "mission": "Watch."})),
    )
    .await;
    let (no_mission, _) = call(
        with_journal.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Veille", "mission": ""})),
    )
    .await;
    let (unknown_source, _) = call(
        with_journal.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Veille", "mission": "Watch.", "sources": ["src-ghost"]})),
    )
    .await;
    let (no_journal, _) = call(
        without_journal.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Veille", "mission": "Watch."})),
    )
    .await;

    // THEN
    assert_eq!(no_name, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(no_mission, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(unknown_source, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(no_journal, StatusCode::SERVICE_UNAVAILABLE);
    for state in [with_journal, without_journal] {
        let (_, list) = call(state, "GET", ROUTE, None).await;
        let ids: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect();
        assert_eq!(ids, vec!["apollia:chat", "invoices"]);
    }
}

// GIVEN an agent created under a name
// WHEN a second agent is created under the same name, then one whose name
//      reads as the identifier of an installed agent
// THEN each gets an identifier of its own and the first ones are untouched
#[tokio::test]
async fn test_a_name_already_taken_gets_an_identifier_of_its_own() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = state(&dir, true).await;
    let sheet = json!({"name": "Veille", "mission": "Watch the press."});
    let (_, first) = call(state.clone(), "POST", ROUTE, Some(sheet.clone())).await;

    // WHEN
    let (status, second) = call(state.clone(), "POST", ROUTE, Some(sheet)).await;
    let (_, third) = call(
        state.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Invoices", "mission": "Mine."})),
    )
    .await;
    let (_, installed) = call(state, "GET", "/api/v1/agent-definitions/invoices", None).await;

    // THEN
    assert_eq!(first["id"], "veille");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["id"], "veille-2");
    assert_eq!(third["id"], "invoices-2");
    assert_eq!(installed["mission"], "Extract the totals.");
    assert_eq!(installed["origin"], "package");
}

// GIVEN a sheet that names no access, then one that names an empty list
// WHEN each agent is created
// THEN the first reads by default, its definition naming no access, and the
//      second is granted nothing, the empty list being its definition
#[tokio::test]
async fn test_an_absent_access_list_leaves_the_default_and_an_empty_one_replaces_it() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = state(&dir, true).await;

    // WHEN
    let (_, by_default) = call(
        state.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Reader", "mission": "Read."})),
    )
    .await;
    let (_, chosen) = call(
        state,
        "POST",
        ROUTE,
        Some(json!({"name": "Mute", "mission": "Answer.", "access": []})),
    )
    .await;

    // THEN
    assert_eq!(by_default["definition"]["defaults"].get("access"), None);
    assert_eq!(chosen["definition"]["defaults"]["access"], json!([]));
    assert_eq!(chosen["access"], json!([]));
}

// GIVEN display names with accents, punctuation, nothing usable, and length
// WHEN their identifier is derived
// THEN it is lowercase ASCII with single hyphens, `agent` when nothing is
//      left, and never longer than its bound
#[test]
fn test_an_identifier_is_lowercase_ascii_with_single_hyphens() {
    assert_eq!(identifier_of("Veille \u{e9}conomique"), "veille-economique");
    assert_eq!(
        identifier_of("  R\u{e9}ponses / e-mails (v2) !"),
        "reponses-e-mails-v2"
    );
    assert_eq!(identifier_of("\u{152}uvres & C\u{153}ur"), "oeuvres-coeur");
    assert_eq!(identifier_of("日本"), "agent");
    assert_eq!(identifier_of("   "), "agent");
    let long = identifier_of(&"a".repeat(200));
    assert!(long.len() <= 48, "{}", long.len());
    assert!(!identifier_of("a/b").contains('/'));
}

// GIVEN a machine holding a reading tool, a writing tool and a code tool, and
//       a default access the setting widened with the writing tool
// WHEN an agent is created without code, no access chosen for it
// THEN its accesses are the setting: the reading family and the tool added,
//      not as the choice of the person; the setting narrowed back, the same
//      agent follows it
#[tokio::test]
async fn test_an_agent_created_without_code_is_granted_the_default_access_setting() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = state(&dir, true).await;
    let registry = apollia_tools::ToolRegistryHandle::start();
    let mut bash = apollia_tools::tools::file_write::FileWrite::descriptor();
    bash.name = "bash".into();
    bash.family = apollia_tools::descriptor::ActionFamily::ExecuteCode;
    for descriptor in [
        apollia_tools::tools::file_read::FileRead::descriptor(),
        apollia_tools::tools::file_write::FileWrite::descriptor(),
        bash,
    ] {
        registry.register(descriptor).await.unwrap();
    }
    state.tool_registry_handle = Some(registry.clone());
    let config = dir.path().join("apollia.toml");
    std::fs::write(&config, "[tools.new_agent]\naccess = [\"file_write\"]\n").unwrap();
    state.config_path = Some(config.clone());

    // WHEN
    let (status, created) = call(
        state.clone(),
        "POST",
        ROUTE,
        Some(json!({"name": "Reader", "mission": "Read."})),
    )
    .await;
    std::fs::write(&config, "").unwrap();
    let (_, narrowed) = call(state, "GET", "/api/v1/agent-definitions/reader", None).await;

    // THEN
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["origin"], "interface");
    assert_eq!(created["access"], json!(["file_read", "file_write"]));
    assert_eq!(created["overridden"], json!([]));
    assert_eq!(narrowed["access"], json!(["file_read"]));
    registry.shutdown().await;
}
