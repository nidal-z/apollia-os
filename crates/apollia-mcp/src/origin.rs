//! Where a connection to an MCP server comes from.
//!
//! The origin is written once, when the connection is added, by the caller
//! that knows it: the desktop for an entry of the curated selection or of the
//! public registry, the package installer for a server a package brings. A
//! configuration that carries none (the CLI, `mcp.toml`, an HTTP client) keeps
//! the origin already stored for its name, or reads as [`Provenance::Custom`].

use serde::{Deserialize, Serialize};

/// Where a connection was found before it was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// An entry of the curated selection embedded in the application.
    Selection,
    /// An entry of the public MCP registry.
    Registry,
    /// Typed by the user: a command or an address of their own.
    Custom,
    /// Brought by an installed package.
    Package,
}

impl Provenance {
    /// Stable column value in `mcp.db`, identical to the wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Registry => "registry",
            Self::Custom => "custom",
            Self::Package => "package",
        }
    }

    /// Parse a stored column value; an unknown one is `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "selection" => Some(Self::Selection),
            "registry" => Some(Self::Registry),
            "custom" => Some(Self::Custom),
            "package" => Some(Self::Package),
            _ => None,
        }
    }
}

/// The origin of a connection, persisted beside its configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionOrigin {
    /// Where the connection was found.
    pub provenance: Provenance,
    /// The publisher of the service is the one behind the server: a selection
    /// entry marked as verified official, or a registry entry recorded as such
    /// when it was added. `false` unless written.
    #[serde(default)]
    pub official: bool,
    /// The curated selection entry the connection was made from, named by its
    /// package identifier (`@notionhq/notion-mcp-server`). Written by the
    /// desktop for a selection connection only; `None` otherwise. An
    /// organisation policy names such a connection `selection:<entry>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_provenance_round_trips_through_its_column_value() {
        // GIVEN every provenance
        for p in [
            Provenance::Selection,
            Provenance::Registry,
            Provenance::Custom,
            Provenance::Package,
        ] {
            // WHEN written as a column value and read back
            let back = Provenance::parse(p.as_str());

            // THEN it is the same, and the wire value agrees with the column
            assert_eq!(back, Some(p));
            assert_eq!(
                serde_json::to_value(p).unwrap(),
                serde_json::Value::String(p.as_str().to_owned())
            );
        }
    }

    #[test]
    fn test_an_origin_without_official_reads_as_not_official() {
        // GIVEN an origin sent without the official flag
        let json = serde_json::json!({ "provenance": "registry" });

        // WHEN deserialized
        let origin: ConnectionOrigin = serde_json::from_value(json).unwrap();

        // THEN it is not official
        assert!(!origin.official);
        assert_eq!(origin.provenance, Provenance::Registry);
    }
}
