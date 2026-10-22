//! Tauri IPC commands for the connection list, the folder connections and the
//! mailbox connection.
//!
//! Each delegates to a route of the embedded runtime, or of the daemon in
//! attached mode, so the desktop and any other client read and change the same
//! connections:
//!
//! - `list_connections`: `GET /api/v1/connections`.
//! - `grant_connection`: `POST /api/v1/connections/{id}/grants`, tools of a
//!   connection given to agents; an agent on the default access keeps it.
//! - `check_connection`: `POST /api/v1/connections/{id}/check`, the state found
//!   now, kept and announced by the runtime.
//! - `pause_connection`, `resume_connection`: `POST
//!   /api/v1/connections/{id}/pause` and `/resume`; the accesses of its agents
//!   stay as they are.
//! - `remove_connection`: `DELETE /api/v1/connections/{id}`, whatever its
//!   kind; with `dry_run` nothing is erased and the answer lists what would be.
//! - `add_folder_connection`: `POST /api/v1/connections/folders`. A folder is
//!   admitted, stored and journaled by the runtime; a refusal comes back as its
//!   reason.
//! - `repoint_folder_connection`: `PUT /api/v1/connections/folders/{id}`, the
//!   folder of a connection chosen again, its identifier and accesses kept.
//! - `list_imap_providers`: `GET /api/v1/connectors/imap/providers`, the
//!   providers whose server and port the add form fills from the address.
//! - `add_imap_mailbox`: `POST /api/v1/connectors/imap/accounts`, which logs in
//!   for real before it stores anything.

use std::time::Duration;

use crate::attached::RuntimeLink;
use apollia_runtime::connections::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

/// Longest wait for a check: an MCP server that had no session is started,
/// and its handshake may take as long as at its addition.
const CHECK_DEADLINE: Duration = Duration::from_secs(120);

/// Longest wait for a mailbox addition. The runtime gives the server fifteen
/// seconds to connect and sixty to answer the login, so the default deadline
/// of the attached arm would cut a slow but working server short.
const MAILBOX_ADD_DEADLINE: Duration = Duration::from_secs(90);

/// Every connection of this machine: native accounts, MCP servers, folders.
#[tauri::command]
pub async fn list_connections(state: State<'_, RuntimeLink>) -> Result<Vec<Connection>, String> {
    let json = state.get_json("/api/v1/connections").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse connection list: {e}"))
}

/// Give `tools` of the connection `connection_id` to every agent of `agents`.
/// The runtime gives each agent what it does not hold yet and journals it; an
/// agent on the default access keeps it and holds the tools on top of it.
/// Answers the agents of the connection after the grant.
#[tauri::command]
pub async fn grant_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
    agents: Vec<String>,
    tools: Vec<String>,
) -> Result<Value, String> {
    state
        .post_json(
            &grant_path(&connection_id),
            &json!({ "agents": agents, "tools": tools }),
        )
        .await
}

/// The route that grants tools of the connection `connection_id`.
fn grant_path(connection_id: &str) -> String {
    format!(
        "/api/v1/connections/{}/grants",
        crate::attached::segment(connection_id)
    )
}

/// The route of `action` on the connection `connection_id`, its identifier one
/// encoded segment.
fn connection_path(connection_id: &str, action: &str) -> String {
    format!(
        "/api/v1/connections/{}/{action}",
        crate::attached::segment(connection_id)
    )
}

/// The route that removes the connection `connection_id`, or on a dry run
/// answers what its removal would erase.
fn remove_path(connection_id: &str, dry_run: bool) -> String {
    let query = if dry_run { "?dry_run=1" } else { "" };
    format!(
        "/api/v1/connections/{}{query}",
        crate::attached::segment(connection_id)
    )
}

/// The body of a check: the read-only probe a server's connector declares, if
/// any; nothing for another kind.
fn check_body(probe: Option<apollia_mcp::manager::ProbeSpec>) -> Value {
    match probe {
        Some(probe) => json!({ "probe": probe }),
        None => json!({}),
    }
}

fn parse_connection(json: Value) -> Result<Connection, String> {
    serde_json::from_value(json).map_err(|e| format!("failed to parse the connection: {e}"))
}

/// Check the connection `connection_id` now, whatever its kind, and answer it
/// with the state found. A server is probed with the read-only call its
/// connector declares, when it declares one.
#[tauri::command]
pub async fn check_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
) -> Result<Connection, String> {
    let probe = match connection_id.strip_prefix("mcp:") {
        Some(name) => crate::commands::mcp::resolve_health_probe(&state, name).await,
        None => None,
    };
    let json = state
        .post_json_within(
            &connection_path(&connection_id, "check"),
            &check_body(probe),
            CHECK_DEADLINE,
        )
        .await?;
    parse_connection(json)
}

/// Put the connection `connection_id` in pause: none of its tools runs until
/// it is resumed. Answers the connection as the list now shows it.
#[tauri::command]
pub async fn pause_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
) -> Result<Connection, String> {
    let json = state
        .post_json(&connection_path(&connection_id, "pause"), &json!({}))
        .await?;
    parse_connection(json)
}

/// Resume the connection `connection_id`: its tools run again.
#[tauri::command]
pub async fn resume_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
) -> Result<Connection, String> {
    let json = state
        .post_json(&connection_path(&connection_id, "resume"), &json!({}))
        .await?;
    parse_connection(json)
}

/// Remove the connection `connection_id`, whatever its kind, with what it
/// leaves behind: its configuration, token or password, installed files, the
/// sources made on a folder, and the tools granted from it to agents. With
/// `dry_run` nothing is erased and the answer lists what would be.
#[tauri::command]
pub async fn remove_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
    dry_run: bool,
) -> Result<Value, String> {
    state
        .delete_json(&remove_path(&connection_id, dry_run))
        .await
}

/// The providers whose IMAP server and port are known from the address, and
/// the port of any other server. Read-only.
#[tauri::command]
pub async fn list_imap_providers(state: State<'_, RuntimeLink>) -> Result<Value, String> {
    state.get_json("/api/v1/connectors/imap/providers").await
}

/// What the add form sends. Deliberately without `Debug`: it carries the
/// password.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxInput {
    /// Address of the mailbox.
    pub email: String,
    /// IMAP server; the runtime fills it for a known provider when absent.
    #[serde(default)]
    pub host: Option<String>,
    /// IMAP port, implicit TLS.
    #[serde(default)]
    pub port: Option<u16>,
    /// Application password, sent to the runtime once and never kept here.
    pub password: String,
}

/// The body of the addition route for `input`.
fn mailbox_body(input: &MailboxInput) -> Value {
    json!({
        "email": input.email.trim(),
        "host": input
            .host
            .as_deref()
            .map(str::trim)
            .filter(|host| !host.is_empty()),
        "port": input.port,
        "password": input.password,
    })
}

/// Add a mailbox: the runtime logs in and opens the inbox read-only, then
/// stores the connection and keeps the password in the secret store. A
/// failed login stores nothing and answers why.
#[tauri::command]
pub async fn add_imap_mailbox(
    state: State<'_, RuntimeLink>,
    mailbox: MailboxInput,
) -> Result<Value, String> {
    state
        .post_json_within(
            "/api/v1/connectors/imap/accounts",
            &mailbox_body(&mailbox),
            MAILBOX_ADD_DEADLINE,
        )
        .await
}

/// Add the folder at `path`, chosen with the system picker, as a connection.
#[tauri::command]
pub async fn add_folder_connection(
    state: State<'_, RuntimeLink>,
    path: String,
) -> Result<Connection, String> {
    let json = state
        .post_json("/api/v1/connections/folders", &add_folder_body(&path))
        .await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse the folder connection: {e}"))
}

fn add_folder_body(path: &str) -> serde_json::Value {
    serde_json::json!({ "path": path })
}

/// Point the folder connection `connection_id` at the folder at `path`, chosen
/// again with the system picker; the connection keeps its identifier, so its
/// agents keep their accesses.
#[tauri::command]
pub async fn repoint_folder_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
    path: String,
) -> Result<Connection, String> {
    let json = state
        .put_json(
            &repoint_folder_path(&connection_id),
            &add_folder_body(&path),
        )
        .await?;
    parse_connection(json)
}

/// The route of the stored folder behind the connection `connection_id`.
fn repoint_folder_path(connection_id: &str) -> String {
    let stored = connection_id
        .strip_prefix("folder:")
        .unwrap_or(connection_id);
    format!(
        "/api/v1/connections/folders/{}",
        crate::attached::segment(stored)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_folder_chosen_again_reaches_the_route_of_its_stored_folder() {
        // GIVEN the identifier of a folder connection as the list names it
        let connection_id = "folder:6f1c2a/b";

        // WHEN the route that points it at another folder is built
        let path = repoint_folder_path(connection_id);

        // THEN it names the stored folder, one encoded segment, without the prefix
        assert_eq!(path, "/api/v1/connections/folders/6f1c2a%2Fb");
    }

    #[test]
    fn test_mailbox_body_trims_and_leaves_an_empty_server_to_the_runtime() {
        // GIVEN an address typed with spaces and an Advanced server left blank
        let input = MailboxInput {
            email: "  ana@gmail.com ".into(),
            host: Some("   ".into()),
            port: Some(993),
            password: "abcd efgh".into(),
        };

        // WHEN the route body is built
        let body = mailbox_body(&input);

        // THEN the address is trimmed, the blank server is absent so the
        // runtime fills it from the address, and the password is sent as typed
        assert_eq!(body["email"], "ana@gmail.com");
        assert!(body["host"].is_null());
        assert_eq!(body["port"], 993);
        assert_eq!(body["password"], "abcd efgh");
    }

    #[test]
    fn test_mailbox_body_keeps_a_server_given_under_advanced() {
        // GIVEN a server and a port typed under Advanced
        let input = MailboxInput {
            email: "ana@example.fr".into(),
            host: Some(" imap.example.fr ".into()),
            port: Some(1993),
            password: "secret".into(),
        };

        // WHEN the route body is built
        let body = mailbox_body(&input);

        // THEN both reach the runtime, the server trimmed
        assert_eq!(body["host"], "imap.example.fr");
        assert_eq!(body["port"], 1993);
    }

    #[test]
    fn test_each_gesture_names_its_connection_as_one_segment() {
        // GIVEN the identifier of a native account, which carries colons and an @
        let id = "native:google:ada@example.com";
        // WHEN the routes of a check, a pause and a resume are built
        let check = connection_path(id, "check");
        let pause = connection_path(id, "pause");
        let resume = connection_path(id, "resume");
        // THEN the identifier is one encoded segment before the action
        assert_eq!(
            check,
            "/api/v1/connections/native%3Agoogle%3Aada%40example.com/check"
        );
        assert!(pause.ends_with("%40example.com/pause"));
        assert!(resume.ends_with("%40example.com/resume"));
    }

    #[test]
    fn test_a_dry_run_asks_what_the_removal_would_erase() {
        // GIVEN a folder connection
        // WHEN the removal route is built for a dry run, then for the removal
        let dry = remove_path("folder:4f1c", true);
        let real = remove_path("folder:4f1c", false);
        // THEN the dry run carries the query the runtime reads, the removal none
        assert_eq!(dry, "/api/v1/connections/folder%3A4f1c?dry_run=1");
        assert_eq!(real, "/api/v1/connections/folder%3A4f1c");
    }

    #[test]
    fn test_a_check_sends_the_probe_of_a_server_and_nothing_else() {
        // GIVEN the probe a connector declares, and no probe
        let probe = apollia_mcp::manager::ProbeSpec {
            tool: "list_users".into(),
            args: Some(serde_json::json!({ "limit": 1 })),
        };
        // WHEN the body of a check is built for each
        let with = check_body(Some(probe));
        let without = check_body(None);
        // THEN the server's probe is sent, another kind sends an empty body
        assert_eq!(with["probe"]["tool"], "list_users");
        assert_eq!(without, serde_json::json!({}));
    }

    #[test]
    fn test_a_grant_names_its_connection_as_one_segment() {
        // GIVEN the identifier of a native account, which carries colons and an @
        // WHEN the grant route is built
        let path = grant_path("native:google:ada@example.com");
        // THEN the identifier is one segment of the route, encoded
        assert_eq!(
            path,
            "/api/v1/connections/native%3Agoogle%3Aada%40example.com/grants"
        );
    }

    #[test]
    fn test_the_added_folder_is_sent_by_its_path() {
        // GIVEN a path the picker answered
        // WHEN the body is built
        let body = add_folder_body("/Users/ada/Clients");
        // THEN it carries the path as is
        assert_eq!(body, serde_json::json!({ "path": "/Users/ada/Clients" }));
    }
}
