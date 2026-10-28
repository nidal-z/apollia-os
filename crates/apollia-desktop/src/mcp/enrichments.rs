use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use apollia_mcp::config::McpServerConfig;
use apollia_tools::ActionFamily;

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
    /// The brand's logo by its Simple Icons name (e.g. `"stripe"`), which the
    /// interface ships under `ui/src/lib/assets/brand-logos/`; a brand it
    /// carries no logo of gets a generic name (e.g. `"database"`) and shows
    /// the generic icon.
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
    /// Exact version of the package this entry was tested at (e.g.
    /// `"2026.8.18"`).
    ///
    /// A plain version string, never a range: no operator, no wildcard, no
    /// marker. For a `pypi` entry it is the version the hash lock below pins
    /// the package itself to, and the two must agree. `None` means the entry
    /// is not pinned, and the version is then resolved once, when the
    /// connection is added, and recorded beside it.
    #[serde(default)]
    pub package_version: Option<String>,
    /// Hash lock of everything the package installs: the package itself and
    /// its whole dependency closure, one [`LockedRequirement`] per
    /// distribution.
    ///
    /// The lock is closed: an installer reading it must refuse any
    /// distribution it does not list and any file whose digest is not one of
    /// the listed ones. For `pypi` it is rendered as a pip requirements file
    /// and installed with `pip install --require-hashes`, so a single altered
    /// digest refuses the whole installation. Empty means no lock, which is
    /// allowed only for an entry that is not part of the curated selection.
    ///
    /// Read from the embedded catalogue only: it never crosses to the webview,
    /// which has no use for a thousand digests.
    #[serde(default, skip_serializing)]
    pub package_lock: Vec<LockedRequirement>,
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
    /// The provider's authorization server offers neither DCR nor CIMD and
    /// Apollia holds no pre-registered client there, so no sign-in can
    /// succeed. The entry stays in the data, and still shadows the same
    /// server coming from the public registry, but it is never listed.
    /// Drop the flag once the client id ships in the release build.
    #[serde(default)]
    pub awaiting_registration: bool,
    /// The entry cannot be installed the way Apollia installs a selection
    /// server: no package is published under its name, or it only works once
    /// an install script has run, and none ever runs; or a native connection
    /// replaces it, as the folder connection replaces the files server. A
    /// server already installed from it stays installed. Like an entry awaiting
    /// registration, it stays in the data, still shadows the same server
    /// coming from the public registry, and is never listed.
    #[serde(default)]
    pub withdrawn: bool,
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
    /// Action family of each tool the server publishes, as this selection
    /// classifies it, keyed by the tool's name on the server.
    ///
    /// Serialized as an object from tool name to family:
    ///
    /// ```json
    /// "tool_families": {
    ///   "notion-search": "read_personal",
    ///   "notion-create-pages": "write_or_delete"
    /// }
    /// ```
    ///
    /// The family is one of `read`, `read_personal`, `write_or_delete`,
    /// `send_external` and `execute_code`; any other value fails the catalogue
    /// load. `send_external` can only come from here: no hint a server
    /// declares means "sends outside". Every listed entry classifies every
    /// tool its server publishes, checked against a public list of them; a
    /// tool absent from the map, published later say, falls back to the hints
    /// its server declares, and without any is unclassified. Copied into the
    /// connection's configuration when it is added from this entry.
    #[serde(default)]
    pub tool_families: BTreeMap<String, ActionFamily>,
    /// Questions the "Try" dialog offers once the connection is added, one per
    /// tool it exercises. Serialized as:
    ///
    /// ```json
    /// "examples": [
    ///   {
    ///     "tool": "notion-search",
    ///     "label": { "en": "search pages", "fr": "..." },
    ///     "question": { "en": "Which pages mention the budget?", "fr": "..." }
    ///   }
    /// ]
    /// ```
    ///
    /// `tool` is a tool the entry classifies in `tool_families`. An entry
    /// without examples is tried with a general question about what it allows.
    #[serde(default)]
    pub examples: Vec<TryExample>,
}

/// One question offered to try a connection: the tool it exercises, what that
/// tool does and the question itself, each text per locale (`"en"`, `"fr"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TryExample {
    /// Name of the tool on the server, as `tool_families` keys it.
    pub tool: String,
    /// What the tool does, in a few words.
    pub label: HashMap<String, String>,
    /// The question a user would ask.
    pub question: HashMap<String, String>,
}

/// One distribution of a package's hash lock.
///
/// Serialized as:
///
/// ```json
/// {
///   "name": "pydantic-core",
///   "version": "2.46.5",
///   "marker": "sys_platform != \"darwin\" or platform_machine != \"x86_64\"",
///   "hashes": ["sha256:013d6f34...", "sha256:03b96..."]
/// }
/// ```
///
/// - `name`: the distribution name as the package index spells it, letters,
///   digits, `-`, `_` and `.` only.
/// - `version`: one exact version, never a range.
/// - `marker`: optional environment marker, PEP 508 vocabulary only, for a
///   distribution needed on some platforms and not others (a dependency that
///   ships no wheel for one of them, say). Absent means every platform.
/// - `hashes`: every artifact the installer may accept for that exact version,
///   as `<algorithm>:<lowercase hex digest>`. For `pypi` the algorithm is
///   `sha256`, one digest per wheel or source archive Apollia's interpreter can
///   install; at least one is required.
///
/// The same shape carries an npm lock, where each hash is the registry's
/// `integrity` value for that exact version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedRequirement {
    /// Distribution name.
    pub name: String,
    /// Exact version.
    pub version: String,
    /// Environment marker restricting the platforms that install it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Accepted artifact digests, `<algorithm>:<hex>`.
    pub hashes: Vec<String>,
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

impl ConnectorEnrichment {
    /// Whether the entry is offered: neither awaiting a registration nor
    /// withdrawn.
    pub fn is_listed(&self) -> bool {
        !self.awaiting_registration && !self.withdrawn
    }
}

/// The catalogue in force: the selection compiled into the application, with
/// the signed online index this machine last accepted laid over it (see
/// [`crate::mcp::remote_catalogue`]). Without an accepted index it is the
/// compiled selection alone.
pub fn load_builtin_enrichments() -> Vec<ConnectorEnrichment> {
    let compiled = compiled_enrichments();
    match crate::mcp::remote_catalogue::kept_on_this_machine() {
        Some(remote) => crate::mcp::remote_catalogue::merge(compiled, &remote),
        None => compiled,
    }
}

/// The selection compiled into the application, from `enrichments.json`. If
/// the embedded JSON is somehow malformed the function returns an empty `Vec`
/// rather than panicking.
pub fn compiled_enrichments() -> Vec<ConnectorEnrichment> {
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

/// The curated entry a launch configuration was made from, if any.
///
/// A remote connection matches on its URL; a local one on the package it
/// launches, through `npx`/`bunx` (the package after the launcher's own
/// options), `uvx`, the file of an installed npm package, or the entry point
/// of an installed Python package. An entry that is not listed matches
/// nothing.
pub fn selection_entry_for<'a>(
    config: &McpServerConfig,
    enrichments: &'a [ConnectorEnrichment],
) -> Option<&'a ConnectorEnrichment> {
    let listed = enrichments.iter().filter(|e| e.is_listed());
    if config.transport != "stdio" {
        let url = config.url.as_deref()?;
        return listed
            .into_iter()
            .find(|e| e.remote_url.as_deref() == Some(url));
    }
    let launched = launched_package(config)?;
    listed.into_iter().find(|e| {
        e.package_registry_type.is_some()
            && crate::mcp::python_server::normalize_name(&e.package_identifier)
                == crate::mcp::python_server::normalize_name(&launched)
    })
}

/// The package a stdio configuration launches, without any version suffix.
fn launched_package(config: &McpServerConfig) -> Option<String> {
    let stem = Path::new(&config.command)
        .file_stem()
        .and_then(|s| s.to_str())?
        .to_ascii_lowercase();
    match stem.as_str() {
        "npx" | "bunx" => {
            let spec = config.args.iter().find(|a| !a.starts_with('-'))?;
            Some(strip_npm_version(spec).to_string())
        }
        "uvx" => crate::mcp::python_server::UvxInvocation::parse(&config.args)
            .ok()
            .map(|i| i.package),
        // An installed npm connection runs a file under `node_modules`.
        "node" => crate::mcp::npm_server::installed_package(config),
        // An installed Python connection runs its entry point by path.
        _ => Some(stem),
    }
}

/// `@scope/name@1.2` and `name@1.2` without their version.
fn strip_npm_version(spec: &str) -> &str {
    let search_from = usize::from(spec.starts_with('@'));
    match spec[search_from..].find('@') {
        Some(at) => &spec[..search_from + at],
        None => spec,
    }
}

/// Give `config` the classification of the curated entry it was made from.
///
/// Replaces whatever classification the configuration carried: one that no
/// longer matches an entry (an edit that points it elsewhere) is dropped, so a
/// connection never keeps a classification its selection did not write for it.
#[must_use]
pub fn with_selection_classification(
    mut config: McpServerConfig,
    enrichments: &[ConnectorEnrichment],
) -> McpServerConfig {
    config.tool_families = selection_entry_for(&config, enrichments)
        .map(|e| e.tool_families.clone())
        .unwrap_or_default();
    config
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
        // WHEN their package identifiers are collected
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
    fn the_catalog_holds_its_56_entries_each_with_a_cost_model() {
        // GIVEN the compiled selection: the nineteen first entries and the
        //       thirty-seven services of their editors added after them
        // WHEN it is loaded
        let enrichments = compiled_enrichments();
        // THEN it holds fifty-six entries
        assert_eq!(
            enrichments.len(),
            56,
            "expected 56 curated entries in the catalog"
        );
        // AND every entry declares a cost model so the UI can show the badge
        for e in &enrichments {
            assert!(
                e.cost_model.is_some(),
                "entry {} is missing cost_model",
                e.package_identifier
            );
        }
    }

    #[test]
    fn every_listed_remote_entry_is_reachable_and_written_in_both_locales() {
        // GIVEN the listed entries of the selection reached at an address on
        //       the network
        let enrichments = compiled_enrichments();
        let remote: Vec<&ConnectorEnrichment> = enrichments
            .iter()
            .filter(|e| e.is_listed())
            .filter(|e| {
                e.remote_url
                    .as_deref()
                    .is_some_and(|u| u.starts_with("https://"))
            })
            .collect();
        assert!(remote.len() >= 40, "{} remote entries", remote.len());
        for entry in remote {
            let id = entry.package_identifier.as_str();
            // WHEN its transport, its help and its texts are read
            // THEN it speaks streamable HTTP, its help is a page of the
            //      editor, and its name, description and help exist in
            //      English and French
            assert_eq!(
                entry.remote_transport.as_deref(),
                Some("streamable-http"),
                "{id}"
            );
            assert!(
                entry
                    .auth_help_url
                    .as_deref()
                    .is_some_and(|u| u.starts_with("https://")),
                "{id}: no help page"
            );
            let description = entry.description.as_ref();
            let help = entry.auth_help_text.as_ref();
            for locale in ["en", "fr"] {
                for (field, text) in [
                    ("label", entry.operator_label.get(locale)),
                    ("description", description.and_then(|d| d.get(locale))),
                    ("help", help.and_then(|h| h.get(locale))),
                ] {
                    assert!(
                        text.is_some_and(|t| !t.trim().is_empty()),
                        "{id}: {field} missing in {locale}"
                    );
                }
            }
            assert!(entry.cost_model.is_some(), "{id}: no cost model");
        }
    }

    #[test]
    fn catalog_flags_exactly_the_connectors_without_a_usable_client_registration() {
        // GIVEN the shipped catalogue
        let enrichments = load_builtin_enrichments();
        // WHEN the entries awaiting a provider-side registration are collected
        let mut awaiting: Vec<&str> = enrichments
            .iter()
            .filter(|e| e.awaiting_registration)
            .map(|e| e.package_identifier.as_str())
            .collect();
        awaiting.sort_unstable();
        // THEN only Slack and GitHub are flagged: their authorization servers
        // offer neither DCR nor CIMD, and no client id is pre-registered
        assert_eq!(
            awaiting,
            ["com.slack/mcp", "io.github.github/github-mcp-server"]
        );
    }

    #[test]
    fn v1_catalog_includes_expected_official_saas_connectors() {
        // GIVEN the shipped catalogue
        // WHEN its package identifiers are collected
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
        // THEN a representative sample of the official SaaS entries is there
        for id in expected {
            assert!(
                ids.contains(&id),
                "expected official SaaS entry {id} to be present"
            );
        }
    }

    #[test]
    fn every_python_entry_is_pinned_and_locked_by_digest() {
        // GIVEN the shipped catalogue's Python entries
        let enrichments = load_builtin_enrichments();
        let python: Vec<&ConnectorEnrichment> = enrichments
            .iter()
            .filter(|e| e.package_registry_type.as_deref() == Some("pypi"))
            .collect();
        assert!(!python.is_empty(), "the catalogue carries Python entries");
        for entry in python {
            // WHEN the `uvx` command the wizard builds for it is planned
            let config: apollia_mcp::config::McpServerConfig =
                serde_json::from_value(serde_json::json!({
                    "name": "probe",
                    "command": "uvx",
                    "args": [entry.package_identifier],
                }))
                .expect("config");
            let plan = crate::mcp::python_server::plan(&config, &enrichments)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.package_identifier))
                .expect("a uvx command is planned");
            // THEN it installs one exact version from a lock pip can read with
            //      `--require-hashes`, every line carrying a sha256 digest
            assert_eq!(plan.version, entry.package_version);
            assert!(
                !plan.lock.is_empty(),
                "{} has no lock",
                entry.package_identifier
            );
            let rendered = crate::mcp::python_server::render_requirements(
                &entry.package_identifier,
                &plan.lock,
            )
            .unwrap_or_else(|e| panic!("{}: {e}", entry.package_identifier));
            assert!(rendered
                .lines()
                .all(|line| line.contains("==") && line.contains(" --hash=sha256:")));
        }
    }

    #[test]
    fn every_listed_npx_entry_is_pinned_and_locked_by_integrity() {
        // GIVEN the shipped catalogue's listed entries launched by `npx`
        let enrichments = load_builtin_enrichments();
        let npm: Vec<&ConnectorEnrichment> = enrichments
            .iter()
            .filter(|e| e.is_listed() && e.package_runtime_hint.as_deref() == Some("npx"))
            .collect();
        assert!(!npm.is_empty(), "the catalogue carries npm entries");
        for entry in npm {
            let package = entry.package_identifier.as_str();
            // WHEN its version and its embedded lock are read
            let version = entry
                .package_version
                .as_deref()
                .unwrap_or_else(|| panic!("{package} carries no version"));
            let raw = crate::mcp::npm_server::embedded_lock(package)
                .unwrap_or_else(|| panic!("{package} carries no lock"));
            // THEN the version is exact, the lock installs that version, and
            //      every package of the lock carries a sha512 integrity and
            //      comes from the public registry
            assert!(
                crate::mcp::npm_server::is_exact_version(version),
                "{package}: {version}"
            );
            crate::mcp::npm_server::NpmLock::check(package, version, raw)
                .unwrap_or_else(|e| panic!("{package}: {e}"));
            let lock: serde_json::Value = serde_json::from_str(raw).expect("lock");
            let packages = lock["packages"].as_object().expect("packages");
            for (path, p) in packages.iter().filter(|(path, _)| !path.is_empty()) {
                assert!(
                    p["resolved"]
                        .as_str()
                        .is_some_and(|r| r.starts_with("https://registry.npmjs.org/")),
                    "{package}: {path} comes from elsewhere"
                );
            }
        }
    }

    #[test]
    fn an_entry_no_one_can_install_is_withdrawn_from_the_listing() {
        // GIVEN the shipped catalogue
        let enrichments = load_builtin_enrichments();
        // WHEN the withdrawn entries are collected
        let mut withdrawn: Vec<&str> = enrichments
            .iter()
            .filter(|e| e.withdrawn)
            .map(|e| e.package_identifier.as_str())
            .collect();
        withdrawn.sort_unstable();
        // THEN they are the files server the folder connection replaces, the
        //      two names npm does not publish and the browser server that
        //      needs an install script, and none is listed
        assert_eq!(
            withdrawn,
            [
                "@modelcontextprotocol/server-filesystem",
                "@modelcontextprotocol/server-git",
                "@modelcontextprotocol/server-puppeteer",
                "@modelcontextprotocol/server-sqlite",
            ]
        );
        assert!(enrichments
            .iter()
            .filter(|e| e.withdrawn)
            .all(|e| !e.is_listed()));
    }

    #[test]
    fn test_the_files_server_leaves_the_selection_and_an_installed_one_keeps_its_entry() {
        // GIVEN the shipped catalogue
        let enrichments = load_builtin_enrichments();
        // WHEN the files server is looked up
        let files = find_enrichment(&enrichments, "@modelcontextprotocol/server-filesystem")
            .expect("files server entry");
        // THEN it is not listed, so neither the selection nor the public
        //      registry offers it; its entry and its lock stay, so a server
        //      already installed from it keeps its name and its pinned install
        assert!(!files.is_listed());
        assert_eq!(
            files.operator_label.get("en").map(String::as_str),
            Some("Local Files")
        );
        assert!(crate::mcp::npm_server::embedded_lock(&files.package_identifier).is_some());
    }

    #[test]
    fn brave_search_is_marked_freemium() {
        // GIVEN the shipped catalogue
        let enrichments = load_builtin_enrichments();
        // WHEN the Brave search entry is looked up
        let brave = enrichments
            .iter()
            .find(|e| e.package_identifier == "@modelcontextprotocol/server-brave-search")
            .expect("brave");
        let cost = brave.cost_model.as_ref().expect("cost_model");
        // THEN it is billed as freemium and its quota note exists in both locales
        assert_eq!(cost.kind, CostKind::Freemium);
        // Note must surface the quota in both locales
        let note = cost.note_i18n.as_ref().expect("note_i18n");
        assert!(note.contains_key("en"));
        assert!(note.contains_key("fr"));
    }

    fn stdio(command: &str, args: &[&str]) -> McpServerConfig {
        serde_json::from_value(serde_json::json!({
            "name": "probe",
            "command": command,
            "args": args,
        }))
        .expect("config")
    }

    #[test]
    fn every_listed_entry_classifies_its_tools_with_a_known_family() {
        // GIVEN the embedded catalogue, read strictly so that a family no
        //       build knows fails here instead of emptying the catalogue
        let parsed: Result<Vec<ConnectorEnrichment>, _> =
            serde_json::from_str(include_str!("enrichments.json"));
        let enrichments = parsed.expect("the embedded catalogue parses");
        // The entries that classified their tools before the selection grew:
        // each keeps classifying at least one, so an entry cannot lose its
        // families and its examples together unnoticed.
        const CLASSIFYING: &[&str] = &[
            "@notionhq/notion-mcp-server",
            "@modelcontextprotocol/server-brave-search",
            "@modelcontextprotocol/server-filesystem",
            "@modelcontextprotocol/server-memory",
            "@modelcontextprotocol/server-puppeteer",
            "@playwright/mcp",
            "app.linear/mcp",
            "com.atlassian/rovo-mcp",
            "com.stripe/mcp",
            "com.figma/mcp",
            "io.sentry/mcp",
            "com.cloudflare/mcp",
            "@modelcontextprotocol/server-postgres",
            "@modelcontextprotocol/server-sqlite",
            "@modelcontextprotocol/server-git",
            "mcp-server-time",
            "mcp-server-fetch",
        ];
        for id in CLASSIFYING {
            let entry = enrichments
                .iter()
                .find(|e| e.package_identifier == *id)
                .unwrap_or_else(|| panic!("{id} is no longer in the catalogue"));
            assert!(
                !entry.tool_families.is_empty(),
                "{id} no longer classifies its tools"
            );
        }
        // WHEN the listed entries are inspected
        for entry in enrichments.iter().filter(|e| e.is_listed()) {
            // THEN an entry that classifies tools (an entry whose editor
            //      publishes no list of them classifies none, and its tools
            //      are then asked for) never files one as unclassified nor
            //      exempt, under a tool name a configuration accepts
            assert!(
                entry
                    .tool_families
                    .values()
                    .all(|f| !matches!(f, ActionFamily::Unclassified | ActionFamily::Exempt)),
                "{} holds a family outside the five",
                entry.package_identifier
            );
            let mut config = stdio("npx", &["-y", "pkg"]);
            config.tool_families = entry.tool_families.clone();
            assert!(
                config.validate().is_ok(),
                "{} holds a malformed tool name",
                entry.package_identifier
            );
        }
    }

    #[test]
    fn an_unknown_family_fails_the_catalogue() {
        // GIVEN an entry naming a family no build knows
        let raw = r#"[{
            "package_identifier": "x",
            "operator_label": {"en": "X", "fr": "X"},
            "category": "data",
            "icon_name": "database",
            "trust_level": "custom",
            "auth_help_url": null,
            "auth_help_text": null,
            "tool_families": {"lookup": "teleport"}
        }]"#;
        // WHEN it is parsed
        let parsed: Result<Vec<ConnectorEnrichment>, _> = serde_json::from_str(raw);
        // THEN the parse fails
        assert!(parsed.is_err());
    }

    #[test]
    fn a_connection_takes_the_classification_of_the_entry_it_launches() {
        // GIVEN the catalogue and the launch configurations the wizard writes
        let enrichments = load_builtin_enrichments();
        let mut remote = stdio("", &[]);
        remote.transport = "streamable-http".to_string();
        remote.url = Some("https://mcp.notion.com/mcp".to_string());
        let npm = stdio(
            "npx",
            &["-y", "@modelcontextprotocol/server-memory@2026.8.31"],
        );
        let uvx = stdio("uvx", &["mcp-server-time"]);
        let installed = stdio("/data/connections/time/venv/bin/mcp-server-time", &[]);
        let installed_npm = stdio(
            "node",
            &["/data/connections/memory/node_modules/@modelcontextprotocol/server-memory/dist/index.js"],
        );
        // WHEN each is given the selection's classification
        // THEN each carries the family its entry gives
        let notion = with_selection_classification(remote, &enrichments);
        assert_eq!(
            notion.tool_families.get("notion-search"),
            Some(&ActionFamily::ReadPersonal)
        );
        for config in [npm, installed_npm] {
            let memory = with_selection_classification(config, &enrichments);
            assert_eq!(
                memory.tool_families.get("delete_entities"),
                Some(&ActionFamily::WriteOrDelete)
            );
        }
        for config in [uvx, installed] {
            let time = with_selection_classification(config, &enrichments);
            assert_eq!(
                time.tool_families.get("get_current_time"),
                Some(&ActionFamily::Read)
            );
        }
    }

    #[test]
    fn a_connection_outside_the_selection_keeps_no_classification() {
        // GIVEN a custom server that carries a classification it was not given
        //       by the selection, and a server only an unlisted entry describes
        let enrichments = load_builtin_enrichments();
        let mut custom = stdio("npx", &["-y", "@someone/other-server"]);
        custom
            .tool_families
            .insert("anything".to_string(), ActionFamily::Read);
        let mut slack = stdio("", &[]);
        slack.transport = "streamable-http".to_string();
        slack.url = Some("https://mcp.slack.com".to_string());
        // WHEN each is given the selection's classification
        let custom = with_selection_classification(custom, &enrichments);
        let slack = with_selection_classification(slack, &enrichments);
        // THEN neither carries one
        assert!(custom.tool_families.is_empty());
        assert!(slack.tool_families.is_empty());
    }

    #[test]
    fn npm_version_suffix_is_stripped() {
        // GIVEN scoped and bare specs, with and without a version
        // WHEN their version is stripped
        // THEN the package name remains
        assert_eq!(strip_npm_version("@scope/name@1.2.3"), "@scope/name");
        assert_eq!(strip_npm_version("@scope/name"), "@scope/name");
        assert_eq!(strip_npm_version("name@1.2"), "name");
        assert_eq!(strip_npm_version("name"), "name");
    }

    #[test]
    fn every_listed_entry_offers_examples_on_tools_it_classifies() {
        // GIVEN the embedded selection
        let enrichments = load_builtin_enrichments();
        // WHEN the examples of every listed entry are read
        for entry in enrichments.iter().filter(|e| e.is_listed()) {
            // THEN an entry that classifies tools offers at least one, and an
            //      entry that classifies none offers none: it is tried with
            //      the general question
            assert_eq!(
                entry.examples.is_empty(),
                entry.tool_families.is_empty(),
                "{} offers examples that do not match its classification",
                entry.package_identifier
            );
            for example in &entry.examples {
                // AND each exercises a tool the entry classifies
                assert!(
                    entry.tool_families.contains_key(&example.tool),
                    "{}: example on unclassified tool {}",
                    entry.package_identifier,
                    example.tool
                );
                // AND says what it does and asks its question in both locales
                for locale in ["en", "fr"] {
                    for (field, texts) in
                        [("label", &example.label), ("question", &example.question)]
                    {
                        assert!(
                            texts.get(locale).is_some_and(|t| !t.trim().is_empty()),
                            "{} {}: {field} missing in {locale}",
                            entry.package_identifier,
                            example.tool
                        );
                    }
                }
            }
        }
    }
}
