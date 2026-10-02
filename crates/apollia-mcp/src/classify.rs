//! The action family of a tool published by an MCP server.
//!
//! A family is decided once, when the tool is registered, and never by the
//! model. Three sources exist, in order of precedence:
//!
//! 1. the classification the curated selection ships for the tool;
//! 2. the hints the server declares in the tool's `annotations`;
//! 3. nothing, and the tool is `Unclassified`, which every reader treats as
//!    sensitive.
//!
//! The hints come from an untrusted server: they classify a tool, they never
//! authorise one. They can say "reads" or "writes", never "sends outside":
//! no hint of the protocol carries that meaning, so `SendExternal` only ever
//! comes from the selection.

use apollia_tools::{ActionFamily, FamilySource};

use crate::protocol::McpToolAnnotations;

/// Decide the family of one MCP tool, and where the decision came from.
///
/// - `selection`: the family the curated selection gives this tool, if any.
///   An `Unclassified` entry counts as no entry.
/// - `hints`: the tool's declared annotations, if any.
/// - `has_account`: the connection carries an account (an OAuth token or a
///   stored secret), so what it reads is the user's personal data.
///
/// With no selection entry, `destructiveHint: true` or `readOnlyHint: false`
/// gives [`ActionFamily::WriteOrDelete`]; `readOnlyHint: true` gives
/// [`ActionFamily::Read`], or [`ActionFamily::ReadPersonal`] behind an
/// account. A destructive hint wins over a read-only one, the reading that
/// costs least if the server is wrong. Anything else is unclassified.
#[must_use]
pub fn classify_mcp_tool(
    selection: Option<ActionFamily>,
    hints: Option<&McpToolAnnotations>,
    has_account: bool,
) -> (ActionFamily, FamilySource) {
    if let Some(family) = selection.filter(|f| *f != ActionFamily::Unclassified) {
        return (family, FamilySource::Selection);
    }
    let Some(hints) = hints else {
        return (ActionFamily::Unclassified, FamilySource::Unsourced);
    };
    if hints.destructive_hint == Some(true) {
        return (ActionFamily::WriteOrDelete, FamilySource::Server);
    }
    match hints.read_only_hint {
        Some(true) if has_account => (ActionFamily::ReadPersonal, FamilySource::Server),
        Some(true) => (ActionFamily::Read, FamilySource::Server),
        Some(false) => (ActionFamily::WriteOrDelete, FamilySource::Server),
        None => (ActionFamily::Unclassified, FamilySource::Unsourced),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(read_only: Option<bool>, destructive: Option<bool>) -> McpToolAnnotations {
        McpToolAnnotations {
            read_only_hint: read_only,
            destructive_hint: destructive,
            ..McpToolAnnotations::default()
        }
    }

    #[test]
    fn selection_wins_over_the_server_hints() {
        // GIVEN a tool the selection classifies as sending, which the server
        //       declares read-only
        let declared = hints(Some(true), None);
        // WHEN it is classified
        let out = classify_mcp_tool(Some(ActionFamily::SendExternal), Some(&declared), true);
        // THEN the selection decides
        assert_eq!(out, (ActionFamily::SendExternal, FamilySource::Selection));
    }

    #[test]
    fn an_unclassified_selection_entry_counts_as_none() {
        // GIVEN a selection entry that classifies nothing, and a read-only hint
        let declared = hints(Some(true), None);
        // WHEN the tool is classified
        let out = classify_mcp_tool(Some(ActionFamily::Unclassified), Some(&declared), false);
        // THEN the hint decides
        assert_eq!(out, (ActionFamily::Read, FamilySource::Server));
    }

    #[test]
    fn read_only_hint_reads_without_an_account() {
        // GIVEN a read-only hint on a connection with no account
        let declared = hints(Some(true), Some(false));
        // WHEN classified
        let out = classify_mcp_tool(None, Some(&declared), false);
        // THEN the tool reads
        assert_eq!(out, (ActionFamily::Read, FamilySource::Server));
    }

    #[test]
    fn read_only_hint_reads_personal_data_behind_an_account() {
        // GIVEN a read-only hint on a connection that carries an account
        let declared = hints(Some(true), None);
        // WHEN classified
        let out = classify_mcp_tool(None, Some(&declared), true);
        // THEN the tool reads personal data
        assert_eq!(out, (ActionFamily::ReadPersonal, FamilySource::Server));
    }

    #[test]
    fn destructive_hint_writes() {
        // GIVEN a destructive hint, alone and contradicting a read-only one
        for declared in [hints(None, Some(true)), hints(Some(true), Some(true))] {
            // WHEN classified
            let out = classify_mcp_tool(None, Some(&declared), false);
            // THEN the tool writes or deletes
            assert_eq!(out, (ActionFamily::WriteOrDelete, FamilySource::Server));
        }
    }

    #[test]
    fn read_only_false_writes() {
        // GIVEN a tool declared not read-only and not destructive
        let declared = hints(Some(false), Some(false));
        // WHEN classified
        let out = classify_mcp_tool(None, Some(&declared), true);
        // THEN it writes or deletes
        assert_eq!(out, (ActionFamily::WriteOrDelete, FamilySource::Server));
    }

    #[test]
    fn no_hint_is_unclassified() {
        // GIVEN no annotations at all, and annotations that carry neither
        //       the read-only nor the destructive hint
        let silent = McpToolAnnotations {
            idempotent_hint: Some(true),
            open_world_hint: Some(true),
            ..McpToolAnnotations::default()
        };
        for declared in [None, Some(&silent)] {
            // WHEN classified
            let out = classify_mcp_tool(None, declared, true);
            // THEN the tool is unclassified, with no source
            assert_eq!(out, (ActionFamily::Unclassified, FamilySource::Unsourced));
        }
    }

    #[test]
    fn no_hint_combination_sends_outside() {
        // GIVEN every combination of the four hints
        let values = [None, Some(true), Some(false)];
        for read_only in values {
            for destructive in values {
                for idempotent in values {
                    for open_world in values {
                        let declared = McpToolAnnotations {
                            read_only_hint: read_only,
                            destructive_hint: destructive,
                            idempotent_hint: idempotent,
                            open_world_hint: open_world,
                        };
                        for account in [false, true] {
                            // WHEN classified without a selection entry
                            let (family, _) = classify_mcp_tool(None, Some(&declared), account);
                            // THEN the family is never a send
                            assert_ne!(family, ActionFamily::SendExternal);
                        }
                    }
                }
            }
        }
    }
}
