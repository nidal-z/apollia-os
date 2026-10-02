use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use apollia_auth::ConnectorProvider;
use apollia_tools::{ActionFamily, ToolRegistryHandle};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use super::*;
use crate::connectors_bridge::google_tool_descriptors;

/// The fields of one event, rendered.
type Seen = Vec<(String, String)>;

/// The fields of every event a subscriber receives.
struct Recorder(Arc<Mutex<Vec<Seen>>>);

struct Fields(Vec<(String, String)>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

fn google(accounts: &[&str]) -> NativeAccounts {
    (
        ConnectorProvider::Google,
        accounts.iter().map(|a| (*a).to_owned()).collect(),
    )
}

fn microsoft(accounts: &[&str]) -> NativeAccounts {
    (
        ConnectorProvider::Microsoft,
        accounts.iter().map(|a| (*a).to_owned()).collect(),
    )
}

// GIVEN two connected Google accounts
// WHEN their descriptors are built
// THEN each account publishes its own set, named after it, each tool with the
// family of its operation, and no bare operation name is published
#[test]
fn test_two_accounts_publish_two_distinct_sets() {
    let accounts = [google(&["pro@example.com", "home@example.com"])];

    let descriptors = connector_descriptors_for(&accounts);

    let operations = google_tool_descriptors();
    assert_eq!(descriptors.len(), 2 * operations.len());
    let names: HashSet<&str> = descriptors.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names.len(), descriptors.len(), "every name is distinct");
    for account in ["pro@example.com", "home@example.com"] {
        for op in &operations {
            let name = format!("conn:google:{account}/{}", op.name);
            let d = descriptors.iter().find(|d| d.name == name).unwrap();
            assert_eq!(d.family, op.family, "{name}");
            assert_ne!(d.family, ActionFamily::Unclassified, "{name}");
            assert!(d.description.contains(account), "{name}");
            d.validate().unwrap();
        }
    }
    assert!(descriptors.iter().all(|d| d.name.starts_with("conn:")));
}

// GIVEN no connected account, then providers with an empty account list
// WHEN the descriptors are built
// THEN none is published
#[test]
fn test_no_account_publishes_no_connector_tool() {
    assert!(connector_descriptors_for(&[]).is_empty());
    assert!(connector_descriptors_for(&[google(&[]), microsoft(&[])]).is_empty());
}

// GIVEN a registry, and the boot registration with no connected account
// WHEN the registry is listed
// THEN it holds no Google or Microsoft tool, bare or per account
#[tokio::test]
async fn test_boot_without_account_announces_no_connector_tool() {
    let registry = ToolRegistryHandle::start();

    crate::supervisor::register_builtin_tools(&registry, &[]).await;

    let names: Vec<String> = registry
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(!names.is_empty(), "the native tools are registered");
    assert!(
        names
            .iter()
            .all(|n| !n.starts_with("conn:") && connector_of(n).is_none()),
        "{names:?}"
    );
    registry.shutdown().await;
}

// GIVEN a registry, and the boot registration with one Google account
// WHEN an account connected later registers its tools, twice
// THEN both accounts' tools are announced, once each
#[tokio::test]
async fn test_an_account_connected_later_announces_its_tools() {
    let registry = ToolRegistryHandle::start();
    crate::supervisor::register_builtin_tools(&registry, &[google(&["pro@example.com"])]).await;

    register_account_tools(&registry, ConnectorProvider::Google, "home@example.com")
        .await
        .unwrap();
    register_account_tools(&registry, ConnectorProvider::Google, "home@example.com")
        .await
        .unwrap();

    let names: Vec<String> = registry
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let per_account = |account: &str| {
        names
            .iter()
            .filter(|n| n.starts_with(&format!("conn:google:{account}/")))
            .count()
    };
    assert_eq!(
        per_account("pro@example.com"),
        google_tool_descriptors().len()
    );
    assert_eq!(
        per_account("home@example.com"),
        google_tool_descriptors().len()
    );
    registry.shutdown().await;
}

// GIVEN the name of a tool of an account
// WHEN it is read back
// THEN its provider, account and operation come out; a malformed name or an
// operation of another provider does not parse
#[test]
fn test_a_connection_tool_name_reads_back() {
    let name = connection_tool_name(ConnectorProvider::Google, "pro@example.com", "gmail.send");

    let parsed = parse_connection_tool(&name).unwrap();

    assert_eq!(name, "conn:google:pro@example.com/gmail.send");
    assert_eq!(parsed.provider, ConnectorProvider::Google);
    assert_eq!(parsed.account.as_str(), "pro@example.com");
    assert_eq!(parsed.operation, "gmail.send");
    for bad in [
        "gmail.send",
        "mcp:notion/search",
        "conn:google:/gmail.send",
        "conn:google:pro@example.com/outlook.send",
        "conn:yahoo:pro@example.com/gmail.send",
        "conn:google:pro@example.com",
    ] {
        assert!(parse_connection_tool(bad).is_none(), "{bad}");
    }
}

// GIVEN an access list naming a bare connector tool, and a single Google
// account
// WHEN the access is resolved
// THEN the bare name becomes the tool of that account, and the rest is kept
#[test]
fn test_bare_access_is_rewritten_to_the_single_account() {
    let access = vec![
        "file_read".to_owned(),
        "gmail.send".to_owned(),
        "mcp:notion/search".to_owned(),
    ];

    let resolved = resolve_bare_connector_access("mailer", access, &[google(&["pro@example.com"])]);

    assert_eq!(
        resolved,
        vec![
            "file_read",
            "conn:google:pro@example.com/gmail.send",
            "mcp:notion/search"
        ]
    );
}

// GIVEN an access list naming a bare Google tool and a bare Microsoft tool,
// with two Google accounts and no Microsoft one
// WHEN the access is resolved
// THEN both bare names are removed, each removal is logged naming the access,
// and the rest is kept
#[test]
fn test_bare_access_is_removed_without_a_single_account() {
    let access = vec![
        "gmail.send".to_owned(),
        "outlook.send".to_owned(),
        "conn:google:home@example.com/gmail.send".to_owned(),
    ];
    let accounts = [
        google(&["pro@example.com", "home@example.com"]),
        microsoft(&[]),
    ];
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder(Arc::clone(&events));

    let resolved = tracing::subscriber::with_default(recorder, || {
        resolve_bare_connector_access("mailer", access, &accounts)
    });

    assert_eq!(resolved, vec!["conn:google:home@example.com/gmail.send"]);
    let events = events.lock().unwrap();
    let removed: Vec<&str> = events
        .iter()
        .filter(|f| field(f, "message") == Some("agent.access.connector.removed"))
        .inspect(|f| assert_eq!(field(f, "agent"), Some("mailer")))
        .filter_map(|f| field(f, "access"))
        .collect();
    assert_eq!(removed, vec!["gmail.send", "outlook.send"]);
}

// GIVEN a bare access and the tool of the same account already granted
// WHEN the access is resolved against that single account
// THEN the tool is listed once
#[test]
fn test_rewritten_access_is_not_listed_twice() {
    let access = vec![
        "conn:google:pro@example.com/gmail.send".to_owned(),
        "gmail.send".to_owned(),
    ];

    let resolved = resolve_bare_connector_access("mailer", access, &[google(&["pro@example.com"])]);

    assert_eq!(resolved, vec!["conn:google:pro@example.com/gmail.send"]);
}
