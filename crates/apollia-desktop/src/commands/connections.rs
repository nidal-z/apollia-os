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
//! - `add_folder_connection`, `remove_folder_connection`:
//!   `/api/v1/connections/folders`. A folder is admitted, stored and journaled
//!   by the runtime; a refusal comes back as its reason.
//! - `list_imap_providers`: `GET /api/v1/connectors/imap/providers`, the
//!   providers whose server and port the add form fills from the address.
//! - `add_imap_mailbox`: `POST /api/v1/connectors/imap/accounts`, which logs in
//!   for real before it stores anything.

use std::time::Duration;

use crate::attached::RuntimeLink;
use apollia_runtime::connections::{folder_id_of, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

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

/// Remove the folder connection `connection_id` (`folder:<id>`).
///
/// Sources may be made on the folder. Without `confirm` the runtime removes
/// nothing while any depends on it and answers why; with it, those sources
/// and their indexes go with the connection.
#[tauri::command]
pub async fn remove_folder_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
    confirm: bool,
) -> Result<Connection, String> {
    let json = state
        .delete_json(&remove_folder_path(&connection_id, confirm)?)
        .await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse the folder connection: {e}"))
}

fn add_folder_body(path: &str) -> serde_json::Value {
    serde_json::json!({ "path": path })
}

/// The route that removes the folder `connection_id` names.
fn remove_folder_path(connection_id: &str, confirm: bool) -> Result<String, String> {
    let id = folder_id_of(connection_id)
        .ok_or_else(|| format!("'{connection_id}' is not a folder connection"))?;
    let confirmation = if confirm { "?confirm=true" } else { "" };
    Ok(format!(
        "/api/v1/connections/folders/{}{confirmation}",
        crate::attached::segment(id)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_a_folder_connection_is_removed_by_its_stored_identifier() {
        // GIVEN the identifier of a folder connection, and of an MCP server
        // WHEN the removal route is built for each
        let folder = remove_folder_path("folder:4f1c", false);
        let server = remove_folder_path("mcp:notion", false);
        // THEN the folder's names its stored identifier, the server is refused
        assert_eq!(folder.unwrap(), "/api/v1/connections/folders/4f1c");
        assert!(server.is_err());
    }

    #[test]
    fn test_a_confirmed_removal_tells_the_runtime_to_take_the_sources_with_the_folder() {
        // GIVEN a folder connection sources depend on, and the user's confirmation
        // WHEN the removal route is built
        let confirmed = remove_folder_path("folder:4f1c", true);
        // THEN it carries the confirmation the runtime waits for
        assert_eq!(
            confirmed.unwrap(),
            "/api/v1/connections/folders/4f1c?confirm=true"
        );
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
