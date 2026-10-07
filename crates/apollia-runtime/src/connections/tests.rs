use std::collections::HashSet;

use apollia_auth::ConnectorProvider;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpServerStatus;
use apollia_mcp::{ConnectionOrigin, Provenance};
use apollia_tools::{ActionFamily, FamilySource, ToolDescriptor};

use super::*;

fn server(name: &str, transport: &str, url: Option<&str>) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "command": if transport == "stdio" { "npx" } else { "" },
        "transport": transport,
        "url": url,
    }))
    .unwrap()
}

fn with_origin(
    mut config: McpServerConfig,
    provenance: Provenance,
    official: bool,
) -> McpServerConfig {
    config.origin = Some(ConnectionOrigin {
        provenance,
        official,
        entry: None,
    });
    config
}

fn healthy(name: &str) -> McpServerStatus {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "server_info": name,
        "tools_count": 1,
        "connected": true,
        "pid": null,
        "uptime_secs": null,
        "last_call_at": null,
        "error": null,
        "package": null,
        "transport": "stdio",
        "health": { "state": "healthy", "verified": true },
    }))
    .unwrap()
}

fn descriptor(name: &str, family: ActionFamily, source: FamilySource) -> ToolDescriptor {
    let mut d = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    d.name = name.to_owned();
    d.family = family;
    d.family_source = source;
    d
}

fn by_id<'a>(list: &'a [Connection], id: &str) -> &'a Connection {
    list.iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no connection {id}"))
}

#[test]
fn test_two_mcp_servers_and_one_google_account_are_three_connections_with_tools() {
    // GIVEN two MCP servers, one running with a registered tool and one
    // stopped with its selection classification, and one Google account
    let mut stopped = with_origin(
        server(
            "notion",
            "streamable-http",
            Some("https://mcp.notion.com/mcp"),
        ),
        Provenance::Selection,
        true,
    );
    stopped
        .tool_families
        .insert("notion-search".to_owned(), ActionFamily::ReadPersonal);
    let sources = ConnectionSources {
        mcp_configs: vec![server("files", "stdio", None), stopped],
        mcp_statuses: vec![healthy("files")],
        descriptors: vec![descriptor(
            "mcp:files/read_file",
            ActionFamily::Read,
            FamilySource::Server,
        )],
        package_servers: HashSet::new(),
        native_accounts: vec![(
            ConnectorProvider::Google,
            vec!["ada@example.com".to_owned()],
        )],
        imap_accounts: Vec::new(),
        forbidden: HashSet::new(),
        folders: Vec::new(),
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN there are three, each with its tools
    assert_eq!(list.len(), 3);
    let google = by_id(&list, "native:google:ada@example.com");
    assert_eq!(google.kind, ConnectionKind::Native);
    assert_eq!(google.account.as_deref(), Some("ada@example.com"));
    assert!(!google.tools.is_empty());
    assert!(google
        .tools
        .iter()
        .all(|t| t.family_source == FamilySource::Builtin));

    let files = by_id(&list, "mcp:files");
    assert_eq!(files.kind, ConnectionKind::Mcp);
    assert_eq!(files.state, ConnectionState::Connected);
    assert_eq!(files.tools.len(), 1);
    assert_eq!(files.tools[0].id, "mcp:files/read_file");

    let notion = by_id(&list, "mcp:notion");
    assert_eq!(notion.state, ConnectionState::Stopped);
    assert_eq!(notion.tools.len(), 1);
    assert_eq!(notion.tools[0].id, "mcp:notion/notion-search");
    assert_eq!(notion.tools[0].family, ActionFamily::ReadPersonal);
    assert_eq!(notion.tools[0].family_source, FamilySource::Selection);
}

#[test]
fn test_imap_mailbox_is_a_connection_with_its_read_tools() {
    // GIVEN a Gmail mailbox and a mailbox served on this machine
    let account = |email: &str, host: &str| apollia_connectors::imap::ImapAccount {
        email: email.to_owned(),
        host: host.to_owned(),
        port: 993,
        username: email.to_owned(),
    };
    let sources = ConnectionSources {
        imap_accounts: vec![
            account("ana@gmail.com", "imap.gmail.com"),
            account("ana@home.lan", "127.0.0.1"),
        ],
        ..ConnectionSources::default()
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN each is a mailbox connection labelled by its provider or server,
    // remote unless its server is local, with three read tools pre-checked
    assert_eq!(list.len(), 2);
    let gmail = by_id(&list, "imap:ana@gmail.com");
    assert_eq!(gmail.kind, ConnectionKind::Imap);
    assert_eq!(gmail.label, "Gmail");
    assert_eq!(gmail.account.as_deref(), Some("ana@gmail.com"));
    assert!(gmail.remote);
    assert_eq!(gmail.tools.len(), 3);
    assert!(gmail
        .tools
        .iter()
        .all(|t| t.family == ActionFamily::ReadPersonal && !t.prechecked));
    let home = by_id(&list, "imap:ana@home.lan");
    assert_eq!(home.label, "127.0.0.1");
    assert!(!home.remote);
    assert!(!is_remote_host("localhost"));
    assert!(!is_remote_host("::1"));
}

#[test]
fn test_remote_is_a_network_transport_off_loopback() {
    // GIVEN transports and addresses, with the answer each must give
    let cases = [
        ("streamable-http", Some("https://mcp.notion.com/mcp"), true),
        ("sse", Some("https://example.com/sse"), true),
        ("sse", Some("http://127.0.0.1:3845/sse"), false),
        ("streamable-http", Some("http://localhost:8080/mcp"), false),
        ("streamable-http", Some("http://[::1]:8080/mcp"), false),
        ("stdio", None, false),
        ("stdio", Some("https://example.com/mcp"), false),
    ];
    for (transport, url, expected) in cases {
        // WHEN the attribute is computed
        let remote = is_remote(transport, url);

        // THEN it matches
        assert_eq!(remote, expected, "{transport} {url:?}");
    }
}

#[test]
fn test_a_connection_reports_the_remote_attribute_it_computes() {
    // GIVEN a local Figma server on loopback and a public one
    let sources = ConnectionSources {
        mcp_configs: vec![
            server("figma", "sse", Some("http://127.0.0.1:3845/sse")),
            server("linear", "sse", Some("https://mcp.linear.app/sse")),
        ],
        ..ConnectionSources::default()
    };

    // WHEN listed
    let list = assemble(&sources);

    // THEN only the public one is remote
    assert!(!by_id(&list, "mcp:figma").remote);
    assert!(by_id(&list, "mcp:linear").remote);
}

#[test]
fn test_provenance_comes_from_the_package_then_the_stored_origin_then_custom() {
    // GIVEN a server with no origin, one from the selection, one from the
    // registry, and one a package brought though it was stored as custom
    let sources = ConnectionSources {
        mcp_configs: vec![
            server("legacy", "stdio", None),
            with_origin(server("time", "stdio", None), Provenance::Selection, false),
            with_origin(
                server("weather", "stdio", None),
                Provenance::Registry,
                false,
            ),
            with_origin(server("crm", "stdio", None), Provenance::Custom, false),
        ],
        package_servers: HashSet::from(["crm".to_owned()]),
        ..ConnectionSources::default()
    };

    // WHEN listed
    let list = assemble(&sources);

    // THEN each reads its provenance
    assert_eq!(by_id(&list, "mcp:legacy").provenance, Provenance::Custom);
    assert_eq!(by_id(&list, "mcp:time").provenance, Provenance::Selection);
    assert_eq!(by_id(&list, "mcp:weather").provenance, Provenance::Registry);
    assert_eq!(by_id(&list, "mcp:crm").provenance, Provenance::Package);
}

#[test]
fn test_official_is_what_was_recorded_at_addition_and_false_otherwise() {
    // GIVEN a registry entry recorded official, one recorded not, and one
    // with no record
    let sources = ConnectionSources {
        mcp_configs: vec![
            with_origin(server("github", "stdio", None), Provenance::Registry, true),
            with_origin(server("fork", "stdio", None), Provenance::Registry, false),
            server("legacy", "stdio", None),
        ],
        ..ConnectionSources::default()
    };

    // WHEN listed
    let list = assemble(&sources);

    // THEN only the recorded one is official
    assert!(by_id(&list, "mcp:github").official);
    assert!(!by_id(&list, "mcp:fork").official);
    assert!(!by_id(&list, "mcp:legacy").official);
}

#[test]
fn test_prechecked_only_for_a_read_from_the_selection_or_an_official_source() {
    // GIVEN every family against every provenance, official or not
    let families = [
        ActionFamily::Read,
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
    ];
    let provenances = [
        Provenance::Selection,
        Provenance::Registry,
        Provenance::Custom,
        Provenance::Package,
    ];
    for family in families {
        for provenance in provenances {
            for official in [false, true] {
                // WHEN the precheck is decided
                let checked = is_prechecked(family, provenance, official);

                // THEN it is checked if and only if the tool reads and its
                // connection is from the selection or official
                let expected = family == ActionFamily::Read
                    && (provenance == Provenance::Selection || official);
                assert_eq!(checked, expected, "{family:?} {provenance:?} {official}");
            }
        }
    }
}

#[test]
fn test_a_server_read_tool_is_unchecked_outside_the_selection() {
    // GIVEN a custom server whose tool declares itself read-only
    let sources = ConnectionSources {
        mcp_configs: vec![server("files", "stdio", None)],
        mcp_statuses: vec![healthy("files")],
        descriptors: vec![descriptor(
            "mcp:files/read_file",
            ActionFamily::Read,
            FamilySource::Server,
        )],
        ..ConnectionSources::default()
    };

    // WHEN listed
    let list = assemble(&sources);

    // THEN the tool is classified as a read but offered unchecked
    let tool = &by_id(&list, "mcp:files").tools[0];
    assert_eq!(tool.family, ActionFamily::Read);
    assert!(!tool.prechecked);
}

#[test]
fn test_a_stored_folder_is_listed_last_as_a_local_folder_connection() {
    // GIVEN one MCP server and one stored folder
    let sources = ConnectionSources {
        mcp_configs: vec![server("files", "stdio", None)],
        folders: vec![apollia_core::FolderConnection {
            id: "f1".to_owned(),
            path: "/Users/ada/Clients".to_owned(),
            label: "Clients".to_owned(),
            added_at: "2026-10-04T10:00:00.000Z".to_owned(),
        }],
        ..ConnectionSources::default()
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN the folder comes last, local, of the selection, with its path and
    // the native file tools
    let folder = &list[1];
    assert_eq!(folder.id, "folder:f1");
    assert_eq!(folder.kind, ConnectionKind::Folder);
    assert_eq!(folder.label, "Clients");
    assert_eq!(folder.path.as_deref(), Some("/Users/ada/Clients"));
    assert!(!folder.remote);
    assert_eq!(folder.provenance, Provenance::Selection);
    assert!(folder
        .tools
        .iter()
        .any(|t| t.id == "conn:folder:f1/file_read" && t.prechecked));
    assert_eq!(folder_id_of(&folder.id), Some("f1"));
    assert_eq!(folder_id_of("mcp:files"), None);
}

#[test]
fn test_the_wire_shape_is_camel_case() {
    // GIVEN one connection with a tool
    let sources = ConnectionSources {
        mcp_configs: vec![server("files", "stdio", None)],
        mcp_statuses: vec![healthy("files")],
        descriptors: vec![descriptor(
            "mcp:files/read_file",
            ActionFamily::Read,
            FamilySource::Server,
        )],
        ..ConnectionSources::default()
    };

    // WHEN serialized
    let json = serde_json::to_value(assemble(&sources)).unwrap();

    // THEN the fields read as the clients expect
    let c = &json[0];
    assert_eq!(c["id"], "mcp:files");
    assert_eq!(c["kind"], "mcp");
    assert_eq!(c["provenance"], "custom");
    assert_eq!(c["remote"], false);
    assert_eq!(c["state"], "connected");
    assert_eq!(c["tools"][0]["familySource"], "server");
    assert_eq!(c["tools"][0]["family"], "read");
    assert_eq!(c["tools"][0]["prechecked"], false);
}

// A connection the policy forbade at boot is marked so, whatever its state
// would otherwise be; the others keep theirs.
#[test]
fn test_a_forbidden_connection_is_marked_forbidden_by_policy() {
    // GIVEN a live server, a stopped one and an account, the stopped server
    // and the account forbidden at boot
    let sources = ConnectionSources {
        mcp_configs: vec![
            server("files", "stdio", None),
            server("fetch", "stdio", None),
        ],
        mcp_statuses: vec![healthy("files")],
        native_accounts: vec![(
            ConnectorProvider::Google,
            vec!["ada@example.com".to_owned()],
        )],
        forbidden: HashSet::from([
            "mcp:fetch".to_owned(),
            "native:google:ada@example.com".to_owned(),
        ]),
        ..ConnectionSources::default()
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN both forbidden ones are listed, never dropped, and marked
    assert_eq!(list.len(), 3);
    assert_eq!(
        by_id(&list, "mcp:fetch").state,
        ConnectionState::ForbiddenByPolicy
    );
    assert_eq!(
        by_id(&list, "native:google:ada@example.com").state,
        ConnectionState::ForbiddenByPolicy
    );
    assert_eq!(by_id(&list, "mcp:files").state, ConnectionState::Connected);
}
