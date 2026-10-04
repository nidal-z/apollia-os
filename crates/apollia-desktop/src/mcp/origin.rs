//! The origin the desktop records for an MCP connection.
//!
//! The desktop is the one caller that knows the curated selection, so it
//! decides whether a connection comes from it, and whether that entry is the
//! service's official one. Any other caller's claim to the selection is
//! dropped: only a configuration that matches an entry is recorded as one.

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::{ConnectionOrigin, McpRepoError, McpServerRepository, Provenance};

use crate::mcp::enrichments::{selection_entry_for, ConnectorEnrichment, TrustLevel};

/// Give `config` the origin it is added under.
///
/// - matches a selection entry: the selection, official when the entry is
///   marked verified official, naming the entry by its package identifier;
/// - otherwise the origin the interface sent (a registry entry, with the
///   official flag recorded for it), unless it claims the selection, and
///   never naming a selection entry;
/// - otherwise custom.
#[must_use]
pub fn with_origin(
    mut config: McpServerConfig,
    enrichments: &[ConnectorEnrichment],
) -> McpServerConfig {
    config.origin = Some(match selection_entry_for(&config, enrichments) {
        Some(entry) => selection_origin(entry),
        None => match config.origin {
            Some(origin) if origin.provenance != Provenance::Selection => ConnectionOrigin {
                entry: None,
                ..origin
            },
            _ => ConnectionOrigin {
                provenance: Provenance::Custom,
                official: false,
                entry: None,
            },
        },
    });
    config
}

fn selection_origin(entry: &ConnectorEnrichment) -> ConnectionOrigin {
    ConnectionOrigin {
        provenance: Provenance::Selection,
        official: matches!(entry.trust_level, TrustLevel::VerifiedOfficial),
        entry: Some(entry.package_identifier.clone()),
    }
}

/// Record an origin for every stored server that has none: the selection
/// when its launch configuration or its name matches an entry, custom
/// otherwise; and name the entry of every selection server recorded before
/// the entry was. Returns how many servers were written.
pub fn backfill_origins(
    repo: &McpServerRepository,
    enrichments: &[ConnectorEnrichment],
) -> Result<usize, McpRepoError> {
    let mut written = 0;
    for config in repo.list()? {
        let matched = selection_entry_for(&config, enrichments)
            .or_else(|| entry_named(&config.name, enrichments));
        let wrote = match &config.origin {
            None => {
                let origin = matched.map_or(
                    ConnectionOrigin {
                        provenance: Provenance::Custom,
                        official: false,
                        entry: None,
                    },
                    selection_origin,
                );
                repo.set_origin_if_missing(&config.name, origin)?
            }
            Some(origin)
                if origin.provenance == Provenance::Selection && origin.entry.is_none() =>
            {
                match matched {
                    Some(entry) => repo
                        .set_selection_entry_if_missing(&config.name, &entry.package_identifier)?,
                    None => false,
                }
            }
            Some(_) => false,
        };
        if wrote {
            written += 1;
        }
    }
    Ok(written)
}

/// The listed entry a server name was derived from: the name the catalogue
/// installs an entry under is its label, or its registry name, made safe.
fn entry_named<'a>(
    name: &str,
    enrichments: &'a [ConnectorEnrichment],
) -> Option<&'a ConnectorEnrichment> {
    enrichments.iter().filter(|e| e.is_listed()).find(|e| {
        e.operator_label
            .values()
            .chain(e.registry_names.iter())
            .any(|candidate| sanitize_server_name(candidate) == name)
    })
}

/// The server name the interface derives from a label: lower case, every run
/// of characters outside `[a-z0-9_-]` replaced by one `-`, trimmed of `-`.
fn sanitize_server_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.to_lowercase().chars() {
        if matches!(c, 'a'..='z' | '0'..='9' | '_') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests;
