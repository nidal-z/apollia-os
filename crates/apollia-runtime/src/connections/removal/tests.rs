use apollia_connectors::imap::ImapAccount;
use apollia_mcp::config::McpServerConfig;

use super::*;
use crate::connections::{assemble, ConnectionSources};

fn lists(id: &str, by_default: bool, access: Option<&[&str]>, declared: &[&str]) -> AgentLists {
    let owned = |tools: &[&str]| tools.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    AgentLists {
        id: id.to_owned(),
        by_default,
        overrides: access
            .map(|tools| vec![(OverrideField::Access, owned(tools))])
            .unwrap_or_default(),
        declared: owned(declared),
    }
}

/// The MCP server `notion`, stopped, and two mailboxes.
fn connections() -> Vec<Connection> {
    let notion: McpServerConfig =
        serde_json::from_value(serde_json::json!({"name": "notion", "command": "npx"})).unwrap();
    let mailbox = |email: &str| ImapAccount {
        email: email.to_owned(),
        host: "imap.example.org".to_owned(),
        port: 993,
        username: email.to_owned(),
    };
    assemble(&ConnectionSources {
        mcp_configs: vec![notion],
        imap_accounts: vec![mailbox("ada@example.org"), mailbox("support@example.org")],
        ..ConnectionSources::default()
    })
}

fn find<'a>(all: &'a [Connection], id: &str) -> &'a Connection {
    all.iter().find(|c| c.id == id).unwrap()
}

// GIVEN a stopped server and four agents: one whose access override holds
//       a tool of it, a package agent declaring one, an agent on the default
//       access, and one holding none
// WHEN the revocations of its removal are planned
// THEN the override loses the tool, the package agent is given its list
//      without it, and nothing is written for the other two
#[test]
fn test_a_server_is_revoked_from_overrides_and_declarations_only() {
    // GIVEN
    let all = connections();
    let notion = find(&all, "mcp:notion");
    let agents = [
        lists(
            "chosen",
            false,
            Some(&["mcp:notion/search", "file_read"]),
            &[],
        ),
        lists("package", false, None, &["mcp:notion/write", "web_fetch"]),
        lists("default", true, None, &[]),
        lists("other", false, Some(&["mcp:notional/search"]), &[]),
    ];

    // WHEN
    let planned = revocations(notion, false, &agents);

    // THEN
    assert_eq!(
        planned,
        vec![
            Revocation {
                agent: "chosen".into(),
                field: OverrideField::Access,
                kept: vec!["file_read".into()],
                revoked: vec!["mcp:notion/search".into()],
            },
            Revocation {
                agent: "package".into(),
                field: OverrideField::Access,
                kept: vec!["web_fetch".into()],
                revoked: vec!["mcp:notion/write".into()],
            },
        ]
    );
    let items = access_items(&planned);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].agent.as_deref(), Some("chosen"));
    assert_eq!(items[0].kind, RemovedKind::AgentAccess);
}

// GIVEN an agent holding the mailbox search, and two mailboxes
// WHEN one mailbox is removed while the other is left, then the last one
// THEN the search is kept while another mailbox is left, and revoked with
//      the last one
#[test]
fn test_a_mailbox_tool_goes_with_the_last_mailbox_only() {
    // GIVEN
    let all = connections();
    let mailbox = find(&all, "imap:ada@example.org");
    let agents = [lists("triage", false, None, &["imap.search", "file_read"])];

    // WHEN
    let while_shared = revocations(mailbox, true, &agents);
    let last = revocations(mailbox, false, &agents);

    // THEN
    assert!(while_shared.is_empty());
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].revoked, ["imap.search"]);
    assert_eq!(last[0].kept, ["file_read"]);
}

// GIVEN the fields of an agent definition
// WHEN each is asked whether it lists tools
// THEN the access and the tools given on top of the default access do, and
//      no field that names something else than tools (examples, sources)
//      does
#[test]
fn test_only_the_access_fields_list_tools() {
    // GIVEN
    let fields = apollia_tools::OVERRIDE_FIELDS;

    // WHEN
    let listing: Vec<OverrideField> = fields.into_iter().filter(|f| lists_tools(*f)).collect();

    // THEN
    assert_eq!(listing, [OverrideField::Access, OverrideField::AccessAdded]);
}

// GIVEN an agent on the default access given a tool of the server on top of
//       it
// WHEN the revocations of the server's removal are planned
// THEN the tool is taken from the tools given on top, and the default access
//      itself gets no list
#[test]
fn test_a_tool_given_on_top_of_the_default_access_is_revoked() {
    // GIVEN
    let all = connections();
    let notion = find(&all, "mcp:notion");
    let agent = AgentLists {
        id: "writer".into(),
        by_default: true,
        overrides: vec![(
            OverrideField::AccessAdded,
            vec!["mcp:notion/write".into(), "web_fetch".into()],
        )],
        declared: Vec::new(),
    };

    // WHEN
    let planned = revocations(notion, false, &[agent]);

    // THEN
    assert_eq!(
        planned,
        vec![Revocation {
            agent: "writer".into(),
            field: OverrideField::AccessAdded,
            kept: vec!["web_fetch".into()],
            revoked: vec!["mcp:notion/write".into()],
        }]
    );
}
