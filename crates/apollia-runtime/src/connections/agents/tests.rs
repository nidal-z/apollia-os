use apollia_auth::ConnectorProvider;
use apollia_connectors::imap::ImapAccount;

use super::*;
use crate::connections::{assemble, ConnectionSources};

fn mailbox(email: &str) -> ImapAccount {
    ImapAccount {
        email: email.to_owned(),
        host: "imap.example.org".to_owned(),
        port: 993,
        username: email.to_owned(),
    }
}

fn agent(id: &str, access: &[&str]) -> AgentAccessView {
    AgentAccessView {
        id: id.to_owned(),
        name: id.to_uppercase(),
        enabled: true,
        by_default: false,
        access: access.iter().map(|a| (*a).to_owned()).collect(),
    }
}

/// Two Google accounts and two mailboxes.
fn connections() -> Vec<Connection> {
    assemble(&ConnectionSources {
        native_accounts: vec![(
            ConnectorProvider::Google,
            vec!["ada@example.com".to_owned(), "pro@example.com".to_owned()],
        )],
        imap_accounts: vec![mailbox("ada@example.org"), mailbox("support@example.org")],
        ..ConnectionSources::default()
    })
}

fn find<'a>(all: &'a [Connection], id: &str) -> &'a Connection {
    all.iter().find(|c| c.id == id).unwrap()
}

// GIVEN two mailboxes, and one agent holding the mailbox search tool
// WHEN the holders of each mailbox are read
// THEN each mailbox answers for itself: the agent under both, and each names
//      the other as reached by the same tools
#[test]
fn test_each_mailbox_lists_the_agent_and_names_the_other() {
    // GIVEN
    let all = connections();
    let agents = vec![agent("triage", &["imap.search", "file_read"])];

    // WHEN
    let first = holders(find(&all, "imap:ada@example.org"), &all, &agents);
    let second = holders(find(&all, "imap:support@example.org"), &all, &agents);

    // THEN
    assert_eq!(first.connection, "imap:ada@example.org");
    assert_eq!(second.connection, "imap:support@example.org");
    assert_eq!(first.agents.len(), 1);
    assert_eq!(first.agents[0].tools, ["imap.search"]);
    assert_eq!(second.agents, first.agents);
    assert_eq!(first.shared_with, ["imap:support@example.org"]);
    assert_eq!(second.shared_with, ["imap:ada@example.org"]);
}

// GIVEN two Google accounts, and agents holding the tools of one or the other
// WHEN the holders of each account are read
// THEN each lists only the agent of its own account, and names no other
#[test]
fn test_a_native_account_lists_only_its_own_holders() {
    // GIVEN
    let all = connections();
    let agents = vec![
        agent("mailer", &["conn:google:ada@example.com/gmail.send"]),
        agent("pro", &["conn:google:pro@example.com/gmail.compose_draft"]),
    ];

    // WHEN
    let ada = holders(find(&all, "native:google:ada@example.com"), &all, &agents);
    let pro = holders(find(&all, "native:google:pro@example.com"), &all, &agents);

    // THEN
    assert_eq!(ada.agents.len(), 1);
    assert_eq!(ada.agents[0].id, "mailer");
    assert_eq!(pro.agents.len(), 1);
    assert_eq!(pro.agents[0].id, "pro");
    assert!(ada.shared_with.is_empty());
}

// GIVEN an MCP server that lists no tool, and an agent holding one of its
//       tools by name
// WHEN its holders are read
// THEN the agent is listed through the prefix of the server
#[test]
fn test_a_server_listing_no_tool_still_finds_its_holders() {
    // GIVEN
    let server: apollia_mcp::config::McpServerConfig =
        serde_json::from_value(serde_json::json!({"name": "notion", "command": "npx"})).unwrap();
    let all = assemble(&ConnectionSources {
        mcp_configs: vec![server],
        ..ConnectionSources::default()
    });
    let agents = vec![agent("notes", &["mcp:notion/search", "mcp:notionx/search"])];

    // WHEN
    let found = holders(&all[0], &all, &agents);

    // THEN
    assert_eq!(found.agents.len(), 1);
    assert_eq!(found.agents[0].tools, ["mcp:notion/search"]);
}

// GIVEN an access list holding one of two tools
// WHEN both are granted, then the same two again
// THEN the missing one is appended once, and the second grant adds nothing
#[test]
fn test_with_granted_appends_only_what_is_missing() {
    // GIVEN
    let access = vec!["file_read".to_owned(), "imap.search".to_owned()];
    let tools = vec!["imap.search".to_owned(), "imap.read".to_owned()];

    // WHEN
    let granted = with_granted(&access, &tools).unwrap();
    let again = with_granted(&granted, &tools);

    // THEN
    assert_eq!(granted, ["file_read", "imap.search", "imap.read"]);
    assert_eq!(again, None);
}

// GIVEN a Google account
// WHEN tools of that account, of the other account, and of a mailbox are
//      checked against it
// THEN only the tool of the account belongs to it
#[test]
fn test_foreign_tools_names_what_the_connection_does_not_bring() {
    // GIVEN
    let all = connections();
    let ada = find(&all, "native:google:ada@example.com");
    let tools = vec![
        "conn:google:ada@example.com/gmail.send".to_owned(),
        "conn:google:pro@example.com/gmail.send".to_owned(),
        "imap.search".to_owned(),
    ];

    // WHEN
    let foreign = foreign_tools(ada, &tools);

    // THEN
    assert_eq!(
        foreign,
        ["conn:google:pro@example.com/gmail.send", "imap.search"]
    );
}
