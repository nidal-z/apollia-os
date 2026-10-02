use apollia_mcp::config::McpServerConfig;
use apollia_mcp::{ConnectionOrigin, McpServerRepository, Provenance};
use tempfile::TempDir;

use super::*;
use crate::mcp::load_builtin_enrichments;

fn remote(name: &str, url: &str) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name, "transport": "streamable-http", "url": url,
    }))
    .unwrap()
}

fn npx(name: &str, package: &str) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name, "command": "npx", "args": ["-y", package],
    }))
    .unwrap()
}

fn origin(provenance: Provenance, official: bool) -> Option<ConnectionOrigin> {
    Some(ConnectionOrigin {
        provenance,
        official,
    })
}

#[test]
fn test_a_selection_entry_is_recorded_with_its_official_mark() {
    // GIVEN the official Notion entry and a community one, both from the selection
    let enrichments = load_builtin_enrichments();

    // WHEN each is given its origin
    let notion = with_origin(remote("notion", "https://mcp.notion.com/mcp"), &enrichments);
    let memory = with_origin(
        npx("memory", "@modelcontextprotocol/server-memory"),
        &enrichments,
    );

    // THEN both come from the selection, only the verified one is official
    assert_eq!(notion.origin, origin(Provenance::Selection, true));
    assert_eq!(memory.origin, origin(Provenance::Selection, false));
}

#[test]
fn test_a_registry_entry_keeps_the_origin_the_interface_sent() {
    // GIVEN a registry entry outside the selection, sent as such, recorded official
    let enrichments = load_builtin_enrichments();
    let mut config = remote("weather", "https://weather.example.com/mcp");
    config.origin = origin(Provenance::Registry, true);

    // WHEN it is given its origin
    let config = with_origin(config, &enrichments);

    // THEN the registry origin and its official flag are kept
    assert_eq!(config.origin, origin(Provenance::Registry, true));
}

#[test]
fn test_no_origin_or_a_false_selection_claim_is_custom() {
    // GIVEN a configuration with no origin and one claiming the selection
    // while matching no entry
    let enrichments = load_builtin_enrichments();
    let mut claimed = npx("mine", "@me/my-server");
    claimed.origin = origin(Provenance::Selection, true);

    // WHEN each is given its origin
    let plain = with_origin(npx("plain", "@me/other"), &enrichments);
    let claimed = with_origin(claimed, &enrichments);

    // THEN both are custom and not official
    assert_eq!(plain.origin, origin(Provenance::Custom, false));
    assert_eq!(claimed.origin, origin(Provenance::Custom, false));
}

#[test]
fn test_backfill_matches_by_launch_or_by_name_and_leaves_the_rest_custom() {
    // GIVEN three servers stored without origin: one launching a selection
    // package, one named after a selection label, one of the user's own;
    // and one already recorded from the registry
    let dir = TempDir::new().unwrap();
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    repo.save(&npx("kg", "@modelcontextprotocol/server-memory"))
        .unwrap();
    repo.save(&remote("notion", "https://notion.example.com/proxy"))
        .unwrap();
    repo.save(&npx("mine", "@me/my-server")).unwrap();
    let mut registry = npx("weather", "@acme/weather");
    registry.origin = origin(Provenance::Registry, false);
    repo.save(&registry).unwrap();
    let enrichments = load_builtin_enrichments();

    // WHEN the origins are backfilled
    let written = backfill_origins(&repo, &enrichments).unwrap();

    // THEN the first two read as the selection, the third as custom, and
    // the recorded one is untouched
    assert_eq!(written, 3);
    let read = |name: &str| repo.find_by_name(name).unwrap().unwrap().origin;
    assert_eq!(read("kg"), origin(Provenance::Selection, false));
    assert_eq!(read("notion"), origin(Provenance::Selection, true));
    assert_eq!(read("mine"), origin(Provenance::Custom, false));
    assert_eq!(read("weather"), origin(Provenance::Registry, false));
}

#[test]
fn test_server_names_are_derived_as_the_interface_derives_them() {
    // GIVEN labels as the catalogue shows them
    // WHEN made safe
    // THEN they match the interface's sanitizeServerName
    assert_eq!(sanitize_server_name("Local Files"), "local-files");
    assert_eq!(
        sanitize_server_name("Atlassian (Jira + Confluence)"),
        "atlassian-jira-confluence"
    );
    assert_eq!(sanitize_server_name("com.notion/mcp"), "com-notion-mcp");
    assert_eq!(sanitize_server_name("Time"), "time");
}
