//! Which connector tools are worth advertising to the model right now.
//!
//! Every tool of every connector used to be sent with every chat call, whether
//! or not an account was connected. Measured on 2026-09-22: the 51 Google and
//! Microsoft tools weigh about 9000 tokens of schema, out of the 13400 the first
//! call of a fresh session carried. On a 32768-token window that put a bare
//! "hello" at 41 % before the model had said a word, and on a small model it did
//! worse than waste the window: offered tools it could not use, it reached for
//! them, and spent its turns explaining that no Google account was connected
//! when it had been asked to list a local folder.
//!
//! So a connector's tools are advertised only while that connector has an
//! account. The check runs per turn, which is what lets an account connected in
//! the middle of a conversation be usable on the very next message.

use std::collections::HashMap;
use std::sync::OnceLock;

use apollia_auth::ConnectorProvider;

use super::descriptors::{google_tool_descriptors, microsoft_tool_descriptors};

/// Tool name to the connector that serves it, built once from the descriptors.
fn connector_index() -> &'static HashMap<String, ConnectorProvider> {
    static INDEX: OnceLock<HashMap<String, ConnectorProvider>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for d in google_tool_descriptors() {
            index.insert(d.name, ConnectorProvider::Google);
        }
        for d in microsoft_tool_descriptors() {
            index.insert(d.name, ConnectorProvider::Microsoft);
        }
        index
    })
}

/// The connector a tool belongs to, or `None` for any other tool.
#[must_use]
pub fn connector_of(tool: &str) -> Option<ConnectorProvider> {
    connector_index().get(tool).copied()
}

/// The connectors that currently have at least one account.
///
/// A storage that cannot be opened reads as "nothing connected": those tools
/// could not authenticate anyway, and advertising them is exactly the waste this
/// module exists to stop.
pub async fn connected_providers() -> Vec<ConnectorProvider> {
    let Ok(auth) = super::get_auth().await else {
        return Vec::new();
    };
    let mut connected = Vec::new();
    for provider in [ConnectorProvider::Google, ConnectorProvider::Microsoft] {
        if auth
            .list_accounts(provider)
            .await
            .is_ok_and(|accounts| !accounts.is_empty())
        {
            connected.push(provider);
        }
    }
    connected
}

/// Whether `tool` may be advertised, given the connectors that are connected.
///
/// Tools that belong to no connector are always advertised.
#[must_use]
pub fn is_advertisable(tool: &str, connected: &[ConnectorProvider]) -> bool {
    match connector_of(tool) {
        Some(provider) => connected.contains(&provider),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connector_tool_is_withheld_while_its_account_is_missing() {
        // GIVEN no connected account
        let connected: &[ConnectorProvider] = &[];

        // WHEN a Google tool and a native tool are checked
        // THEN only the native one is advertised
        assert!(!is_advertisable("gdrive.find_by_name", connected));
        assert!(is_advertisable("file_list", connected));
    }

    #[test]
    fn a_connected_account_brings_its_tools_back() {
        // GIVEN a connected Google account and no Microsoft one
        let connected = [ConnectorProvider::Google];

        // WHEN one tool of each connector is checked
        // THEN the Google tool is advertised and the Microsoft one is not
        assert!(is_advertisable("gdrive.find_by_name", &connected));
        assert!(!is_advertisable("outlook.send", &connected));
    }

    #[test]
    fn every_connector_tool_is_indexed() {
        // GIVEN the descriptors of both connectors
        let total = google_tool_descriptors().len() + microsoft_tool_descriptors().len();

        // WHEN the index is built
        // THEN it covers every one of them, so none slips through unfiltered
        assert_eq!(connector_index().len(), total);
        assert_eq!(
            connector_of("youtube.search"),
            Some(ConnectorProvider::Google)
        );
    }
}
