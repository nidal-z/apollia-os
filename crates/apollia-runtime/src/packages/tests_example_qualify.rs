//! The example that qualifies a batch of requests, as the binary ships it.

use apollia_tools::NATIVE_TOOL_NAMES;

use super::examples::shipped_examples;
use super::tests::Env;
use super::*;

/// Its package, and its folder under `agents/starters/`.
const QUALIFY: &str = "qualify-requests";

/// The entry of the curated connections that opens pages in a browser.
const BROWSER: &str = "@modelcontextprotocol/server-puppeteer";

#[test]
fn test_the_example_that_qualifies_requests_is_shipped_with_what_it_declares() {
    // GIVEN the examples the binary ships
    // WHEN the one that qualifies requests is read through the strict schema
    let (_, manifest) = find_example(shipped_examples(), QUALIFY).unwrap();

    // THEN it delivers one agent with a mission that names the batch, the
    // reference documents and the sources of each result, and with examples
    assert_eq!(manifest.agents.len(), 1);
    let agent = &manifest.agents[0];
    assert_eq!(agent.id, QUALIFY);
    for word in ["workbook", "checked sources", "Sources:", "public page"] {
        assert!(agent.mission.contains(word), "the mission misses `{word}`");
    }
    assert!(!agent.examples.is_empty());
    // AND its accesses are native tools that only read, web search and page
    // reading among them
    assert!(agent
        .defaults
        .access
        .iter()
        .all(|access| NATIVE_TOOL_NAMES.contains(&access.as_str())));
    for access in ["web_search", "web_read", "source_search"] {
        assert!(agent.defaults.access.iter().any(|a| a == access));
    }
    // AND it checks no source of its own: the user plugs a folder, or copies
    // the demonstration dataset
    assert!(agent.defaults.sources.is_empty());
    // AND it requires the browser connection, with its reason, and brings no
    // server of its own
    assert_eq!(manifest.requires.len(), 1);
    assert_eq!(manifest.requires[0].kind, BROWSER);
    assert!(!manifest.requires[0].reason.is_empty());
    assert!(manifest.mcp_servers.is_empty());
}

#[test]
fn test_the_example_that_qualifies_requests_is_proposed_with_its_browser_missing() {
    // GIVEN a fresh data directory, without any connection
    let env = Env::new();

    // WHEN the shipped examples are listed
    let proposed = list_examples(shipped_examples(), &env.packages, &env.connections).unwrap();

    // THEN the example is proposed, not installed, and its browser connection
    // reads as missing and not brought by the package
    let example = proposed
        .iter()
        .find(|example| example.name == QUALIFY)
        .unwrap();
    assert_eq!(example.installed_version, None);
    assert_eq!(example.agents[0].name, "Qualify requests");
    assert_eq!(example.requires[0].kind, BROWSER);
    assert!(!example.requires[0].present);
    assert!(!example.requires[0].brought_by_package);
    assert!(env.packages.list().unwrap().is_empty());
}
