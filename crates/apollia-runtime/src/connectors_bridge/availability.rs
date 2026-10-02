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
//! So a connector's tools are advertised only while their account is
//! connected. The check runs per turn, which is what lets an account connected
//! in the middle of a conversation be usable on the very next message, and an
//! account disconnected stop being offered on the next one.

use std::collections::HashMap;
use std::sync::OnceLock;

use apollia_auth::ConnectorProvider;

use super::connection_tools::parse_connection_tool;
use super::descriptors::{google_tool_descriptors, microsoft_tool_descriptors};
use crate::connections::NativeAccounts;

/// Operation name to the connector that serves it, built once from the
/// descriptors.
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

/// The connector an operation (`gmail.send`) belongs to, or `None` for any
/// other name.
#[must_use]
pub fn connector_of(operation: &str) -> Option<ConnectorProvider> {
    connector_index().get(operation).copied()
}

/// Whether `tool` may be advertised, given the accounts connected now.
///
/// The tool of an account is advertised while that account is connected. A
/// bare operation name names no account and is never advertised. Tools that
/// belong to no connector are always advertised.
#[must_use]
pub fn is_advertisable(tool: &str, accounts: &[NativeAccounts]) -> bool {
    if let Some(tool) = parse_connection_tool(tool) {
        return accounts.iter().any(|(provider, names)| {
            *provider == tool.provider && names.iter().any(|n| n == tool.account.as_str())
        });
    }
    connector_of(tool).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connector_tool_is_withheld_while_its_account_is_missing() {
        // GIVEN no connected account
        let accounts: &[NativeAccounts] = &[];

        // WHEN the tool of an account, a bare operation and a native tool are
        // checked
        // THEN only the native one is advertised
        assert!(!is_advertisable(
            "conn:google:ada@example.com/gdrive.find_by_name",
            accounts
        ));
        assert!(!is_advertisable("gdrive.find_by_name", accounts));
        assert!(is_advertisable("file_list", accounts));
    }

    #[test]
    fn a_connected_account_brings_its_own_tools_back() {
        // GIVEN one connected Google account and no Microsoft one
        let accounts = [(
            ConnectorProvider::Google,
            vec!["ada@example.com".to_owned()],
        )];

        // WHEN tools of that account, of another Google account, of a
        // Microsoft account and a bare operation are checked
        // THEN only the tool of the connected account is advertised
        assert!(is_advertisable(
            "conn:google:ada@example.com/gdrive.find_by_name",
            &accounts
        ));
        assert!(!is_advertisable(
            "conn:google:bob@example.com/gdrive.find_by_name",
            &accounts
        ));
        assert!(!is_advertisable(
            "conn:microsoft:ada@example.com/outlook.send",
            &accounts
        ));
        assert!(!is_advertisable("gdrive.find_by_name", &accounts));
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
