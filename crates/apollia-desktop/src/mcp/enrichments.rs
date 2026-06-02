use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Figma: remote MCP awaiting Catalog approval ───────────────────────────
//
// Figma's `https://mcp.figma.com/mcp` endpoint is gated to a private MCP
// Catalog allowlist (Claude.ai, Cursor, Windsurf, ...). Apollia has applied
// for inclusion via the waitlist at https://figma.com/mcp-catalog. Until
// approval, the curated `com.figma/mcp` entry points to the LOCAL Dev Mode
// MCP server (127.0.0.1:3845/mcp, no auth; identity provided by the
// running Figma Desktop session).
//
// Once Figma allowlists Apollia, restore the remote enrichment by replacing
// the local block in `enrichments.json` with the snippet below (and re-add
// `oauth_pre_registered_client_id_env` in `ConnectorEnrichment` consumers
// (already present in the struct, no schema change needed):
//
// ```json
// {
//   "package_identifier": "com.figma/mcp",
//   "registry_names": ["com.figma/mcp"],
//   "operator_label": { "en": "Figma", "fr": "Figma" },
//   "description": { "en": "Figma cloud MCP (OAuth)", "fr": "MCP cloud Figma (OAuth)" },
//   "category": "design",
//   "icon_name": "figma",
//   "trust_level": "verified_official",
//   "auth_help_url": "https://www.figma.com/developers/apps",
//   "auth_help_i18n_key": "integrations.connectors.figma.auth_help_remote",
//   "default_requires_approval": false,
//   "remote_url": "https://mcp.figma.com/mcp",
//   "remote_transport": "streamable-http",
//   "oauth_pre_registered_client_id_env": "APOLLIA_FIGMA_CLIENT_ID",
//   "cost_model": { "kind": "free" }
// }
// ```

/// UI-friendly metadata for a popular MCP server.
///
/// Complements the MCP Registry response with translated labels, category,
/// icon, trust level and auth guidance, all embedded at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorEnrichment {
    /// Package identifier matching the registry (e.g. `@notionhq/notion-mcp-server`).
    pub package_identifier: String,
    /// Server names in the MCP Registry that map to this connector (e.g. `["com.notion/mcp"]`).
    #[serde(default)]
    pub registry_names: Vec<String>,
    /// Human-readable labels keyed by locale (`"en"`, `"fr"`).
    pub operator_label: HashMap<String, String>,
    /// Category for grouping in the catalogue (e.g. `"productivity"`, `"data"`).
    pub category: String,
    /// Lucide icon name (e.g. `"database"`, `"search"`, `"globe"`).
    pub icon_name: String,
    /// Trust level badge shown in the UI.
    pub trust_level: TrustLevel,
    /// URL where the user can create or retrieve their API key.
    pub auth_help_url: Option<String>,
    /// Per-locale guidance shown on the auth step of the wizard.
    pub auth_help_text: Option<HashMap<String, String>>,
    /// i18n key to render on the auth step instead of `auth_help_text`.
    /// Resolved by the wizard via `$t(key)` against the bundled FR/EN catalogs,
    /// which lets us localise long explanatory blocks without bloating the
    /// embedded enrichments JSON. Falls back to `auth_help_text`, then to a
    /// generic default when both are absent.
    #[serde(default)]
    pub auth_help_i18n_key: Option<String>,
    /// Default value for the `requires_approval` flag on the created server.
    pub default_requires_approval: bool,
    /// Short description shown in the catalogue for synthetic entries.
    #[serde(default)]
    pub description: Option<HashMap<String, String>>,
    /// Remote URL for synthetic entries (streamable-http or SSE endpoint).
    #[serde(default)]
    pub remote_url: Option<String>,
    /// Remote transport type (`"streamable-http"` or `"sse"`).
    #[serde(default)]
    pub remote_transport: Option<String>,
    /// Package registry type for synthetic package entries (`"npm"`, `"pypi"`).
    #[serde(default)]
    pub package_registry_type: Option<String>,
    /// Launcher hint for the package (`"npx"`, `"uvx"`, `"bunx"`).
    ///
    /// Mirrors the upstream MCP registry semantic where this field names the
    /// **executable** used to fetch and run the package non-interactively, not
    /// the underlying language runtime. The wizard maps this directly to the
    /// stdio `command` (with the conventional `-y` prefix for `npx`).
    #[serde(default)]
    pub package_runtime_hint: Option<String>,
    /// Name of the env var that holds a pre-registered OAuth client id for
    /// this connector, e.g. `"APOLLIA_FIGMA_CLIENT_ID"`.
    ///
    /// Required only for AS that support neither RFC 7591 DCR nor CIMD
    /// (Apollia must be manually registered at the provider's dev portal).
    /// When set, the wizard resolves the env var via the
    /// `mcp_oauth_resolve_client_id` IPC and injects the value into
    /// `mcp_oauth_login`, bypassing CIMD/DCR discovery for this provider.
    #[serde(default)]
    pub oauth_pre_registered_client_id_env: Option<String>,
    /// Environment variable names required by this package (for synthetic entries).
    #[serde(default)]
    pub package_env_vars: Vec<EnrichmentEnvVar>,
    /// Positional or named arguments forwarded to the package at launch
    /// (for synthetic entries). Used to declare runtime parameters such as
    /// `@modelcontextprotocol/server-filesystem`'s allowed-directory paths.
    #[serde(default)]
    pub package_arguments: Vec<EnrichmentPackageArg>,
    /// Fallback auth headers for remote connectors when the registry entry
    /// does not declare its own headers. Registry data takes precedence.
    #[serde(default)]
    pub remote_headers: Vec<EnrichmentRemoteHeader>,
    /// Cost classification surfaced as a badge in the catalogue.
    ///
    /// `None` means "unspecified" (legacy entries); the UI defaults to
    /// "free" for those. New entries should always set this.
    #[serde(default)]
    pub cost_model: Option<CostModel>,
    /// Optional read-only probe used by the "Test" action to verify operational
    /// access (scopes, grants) beyond the handshake. Pure data: declaring it for
    /// a new connector is one JSON entry, never code. `None` means the Test
    /// reports reachability only ("not yet verified by an operation").
    #[serde(default)]
    pub health_probe: Option<EnrichmentHealthProbe>,
}

/// Declarative read-only probe for the connection Test.
///
/// Names a side-effect-free tool the Test can call to prove the credential
/// actually grants access (e.g. Notion `get-users` / `users.list`). Kept as
/// data here so no per-connector code is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentHealthProbe {
    /// Local tool name to invoke on the server (e.g. `"get-users"`).
    pub tool: String,
    /// Static arguments for the probe call. Omit for parameterless tools.
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

/// User-facing cost classification for a connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModel {
    /// Cost kind: `free`, `freemium`, or `paid`.
    pub kind: CostKind,
    /// Optional per-locale note (e.g. quota information).
    #[serde(default)]
    pub note_i18n: Option<HashMap<String, String>>,
}

/// Cost kind enum, displayed as a badge in the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostKind {
    /// No charge expected; only the SaaS account itself is needed.
    Free,
    /// Free tier exists but heavy usage may incur charges.
    Freemium,
    /// Paid service end-to-end.
    Paid,
}

/// An HTTP header required by a remote enrichment connector.
///
/// Used as a fallback when the MCP Registry entry does not declare its auth
/// headers. Publishers are the primary source; this field is only consulted
/// when the registry returns an empty `headers` array for a known connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentRemoteHeader {
    /// Header name (e.g. `Authorization`).
    pub name: String,
    /// Human-readable description shown in the auth wizard step.
    pub description: Option<String>,
    /// Whether the header must be provided before connecting.
    #[serde(default)]
    pub is_required: bool,
    /// Whether the value should be stored in the OS keychain.
    #[serde(default)]
    pub is_secret: bool,
}

/// A positional argument declared in an enrichment for synthetic package entries.
///
/// Used for connectors whose CLI takes runtime parameters that the operator
/// must supply (e.g. `npx @modelcontextprotocol/server-filesystem /path/to/dir`).
/// When `is_repeatable` is true, the wizard splits the user input on whitespace
/// and emits one argv entry per token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentPackageArg {
    /// Argument kind: `"positional"` or `"named"`. Defaults to positional.
    #[serde(default = "default_arg_type")]
    pub arg_type: String,
    /// Hint shown in the wizard when the user must supply the value
    /// (e.g. `"/path/to/allowed/directory"`).
    pub value_hint: Option<String>,
    /// Per-locale description shown next to the input.
    #[serde(default)]
    pub description: Option<HashMap<String, String>>,
    /// Whether the argument must be provided before the server can start.
    #[serde(default)]
    pub is_required: bool,
    /// Whether multiple values can be supplied (split on whitespace, one argv entry each).
    #[serde(default)]
    pub is_repeatable: bool,
}

fn default_arg_type() -> String {
    "positional".to_string()
}

/// An environment variable declared in an enrichment for synthetic package entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentEnvVar {
    /// Variable name (e.g. `BRAVE_API_KEY`).
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Whether the variable is required.
    #[serde(default)]
    pub is_required: bool,
    /// Whether the variable is a secret (stored in keychain).
    #[serde(default)]
    pub is_secret: bool,
}

/// Trust level displayed as a badge next to a connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Publisher is the service vendor (e.g. `@notionhq`).
    VerifiedOfficial,
    /// Open-source package listed in the official registry.
    CommunityVerified,
    /// Listed in the registry but not verified.
    Community,
    /// Added manually by the user, not from the registry.
    Custom,
}

/// Load the builtin enrichments embedded at compile time.
///
/// Returns all entries from `enrichments.json`. If the embedded JSON is somehow
/// malformed the function returns an empty `Vec` rather than panicking.
pub fn load_builtin_enrichments() -> Vec<ConnectorEnrichment> {
    let json = include_str!("enrichments.json");
    serde_json::from_str(json).unwrap_or_default()
}

/// Find the enrichment for `package_identifier` in a pre-loaded slice.
///
/// Returns `None` when no entry matches the given identifier.
pub fn find_enrichment<'a>(
    enrichments: &'a [ConnectorEnrichment],
    package_identifier: &str,
) -> Option<&'a ConnectorEnrichment> {
    enrichments
        .iter()
        .find(|e| e.package_identifier == package_identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_builtin_enrichments_returns_at_least_5() {
        // GIVEN the embedded JSON
        // WHEN enrichments are loaded
        let enrichments = load_builtin_enrichments();
        // THEN at least 5 entries are returned
        assert!(enrichments.len() >= 5);
    }

    #[test]
    fn enrichment_has_required_fields() {
        // GIVEN the loaded enrichments
        let enrichments = load_builtin_enrichments();
        // WHEN the first entry is inspected
        let first = &enrichments[0];
        // THEN all required fields are present and non-empty
        assert!(!first.package_identifier.is_empty());
        assert!(first.operator_label.contains_key("en"));
        assert!(first.operator_label.contains_key("fr"));
        assert!(!first.category.is_empty());
        assert!(!first.icon_name.is_empty());
    }

    #[test]
    fn find_enrichment_returns_notion() {
        // GIVEN the loaded enrichments
        let enrichments = load_builtin_enrichments();
        // WHEN looking up the Notion package
        let found = find_enrichment(&enrichments, "@notionhq/notion-mcp-server");
        // THEN the Notion enrichment is returned
        assert!(found.is_some());
        assert_eq!(found.unwrap().operator_label["en"], "Notion");
    }

    #[test]
    fn find_enrichment_unknown_returns_none() {
        // GIVEN the loaded enrichments
        let enrichments = load_builtin_enrichments();
        // WHEN looking up an unknown package
        let found = find_enrichment(&enrichments, "unknown-package");
        // THEN None is returned
        assert!(found.is_none());
    }

    #[test]
    fn all_5_connectors_present() {
        // GIVEN the loaded enrichments
        let enrichments = load_builtin_enrichments();
        let ids: Vec<&str> = enrichments
            .iter()
            .map(|e| e.package_identifier.as_str())
            .collect();
        // THEN the 5 baseline connectors are present under the current
        // canonical package identifiers (the @modelcontextprotocol namespace
        // is the real one published on npm; earlier builds referenced an
        // @anthropic prefix that never existed).
        assert!(ids.contains(&"@notionhq/notion-mcp-server"));
        assert!(ids.contains(&"@modelcontextprotocol/server-brave-search"));
        assert!(ids.contains(&"@modelcontextprotocol/server-filesystem"));
        assert!(ids.contains(&"@modelcontextprotocol/server-memory"));
        assert!(ids.contains(&"@modelcontextprotocol/server-puppeteer"));
    }

    #[test]
    fn v1_catalog_has_18_entries_with_cost_model() {
        // GIVEN the v0.1.0 catalog. The Figma cloud OAuth variant was
        // removed pending automatic OAuth handshake (v0.1.1); Figma
        // remains accessible via the local Dev Mode entry.
        let enrichments = load_builtin_enrichments();
        assert_eq!(
            enrichments.len(),
            18,
            "expected 18 curated entries in the v0.1.0 catalog"
        );
        // AND every entry declares a cost model so the UI can show the badge
        for e in &enrichments {
            assert!(
                e.cost_model.is_some(),
                "entry {} is missing cost_model - required since ADR-091",
                e.package_identifier
            );
        }
    }

    #[test]
    fn v1_catalog_includes_expected_official_saas_connectors() {
        let enrichments = load_builtin_enrichments();
        let ids: Vec<&str> = enrichments
            .iter()
            .map(|e| e.package_identifier.as_str())
            .collect();
        // Anchor the catalog: a representative sample of the official SaaS
        // entries added in v0.1.0. Full list rotates with the catalog file.
        let expected = [
            "io.github.github/github-mcp-server",
            "com.slack/mcp",
            "app.linear/mcp",
            "com.atlassian/rovo-mcp",
            "com.stripe/mcp",
            "com.figma/mcp",
            "io.sentry/mcp",
            "com.cloudflare/mcp",
        ];
        for id in expected {
            assert!(
                ids.contains(&id),
                "expected official SaaS entry {id} to be present"
            );
        }
    }

    #[test]
    fn brave_search_is_marked_freemium() {
        let enrichments = load_builtin_enrichments();
        let brave = enrichments
            .iter()
            .find(|e| e.package_identifier == "@modelcontextprotocol/server-brave-search")
            .expect("brave");
        let cost = brave.cost_model.as_ref().expect("cost_model");
        assert_eq!(cost.kind, CostKind::Freemium);
        // Note must surface the quota in both locales
        let note = cost.note_i18n.as_ref().expect("note_i18n");
        assert!(note.contains_key("en"));
        assert!(note.contains_key("fr"));
    }
}
