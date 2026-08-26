//! The shapes the MCP catalogue speaks to the frontend in: the flattened view
//! of a registry entry, the curated enrichment that gives it a label, a
//! category and a trust level, and the keyword inference that guesses a
//! category when nothing else names one.

use serde::{Deserialize, Serialize};

use crate::mcp::enrichments::{load_builtin_enrichments, TrustLevel};
use crate::mcp::registry_client::{
    RegistryIcon, RegistryPackage, RegistryRemote, RegistryRepository, RegistryServer,
};

/// Infer a category from a server's name and description using keyword matching.
///
/// Returns `None` when no keywords match - the frontend treats `None` as "other".
pub(super) fn infer_category(name: &str, description: Option<&str>) -> Option<String> {
    let text = format!(
        "{} {}",
        name.to_lowercase(),
        description.unwrap_or("").to_lowercase()
    );

    // Order matters: more specific patterns first.
    static RULES: &[(&[&str], &str)] = &[
        (
            &[
                "database",
                "sql",
                "sqlite",
                "postgres",
                "mysql",
                "mongo",
                "redis",
                "supabase",
                "firebase",
                "dynamodb",
                "bigquery",
                "snowflake",
                "csv",
                "parquet",
                "warehouse",
            ],
            "data",
        ),
        (
            &[
                "search",
                "brave",
                "google",
                "bing",
                "duckduckgo",
                "serp",
                "web search",
            ],
            "search",
        ),
        (
            &[
                "slack",
                "discord",
                "email",
                "smtp",
                "telegram",
                "whatsapp",
                "teams",
                "chat",
                "messaging",
                "notification",
            ],
            "communication",
        ),
        (
            &[
                "git",
                "github",
                "gitlab",
                "jira",
                "linear",
                "ci/cd",
                "docker",
                "kubernetes",
                "terraform",
                "deploy",
                "devops",
                "debug",
                "lint",
                "test",
                "ide",
                "vscode",
                "compiler",
                "build",
            ],
            "development",
        ),
        (
            &[
                "file",
                "filesystem",
                "storage",
                "s3",
                "gcs",
                "blob",
                "drive",
                "dropbox",
                "ftp",
            ],
            "storage",
        ),
        (
            &[
                "notion",
                "confluence",
                "asana",
                "trello",
                "todoist",
                "calendar",
                "schedule",
                "project",
                "task",
                "productivity",
                "spreadsheet",
                "airtable",
            ],
            "productivity",
        ),
        (
            &[
                "llm",
                "openai",
                "anthropic",
                "claude",
                "gpt",
                "ai",
                "machine learning",
                "embedding",
                "vector",
                "rag",
                "agent",
            ],
            "ai",
        ),
        (
            &[
                "api",
                "rest",
                "graphql",
                "webhook",
                "http",
                "endpoint",
                "scrape",
                "crawl",
                "browser",
                "puppeteer",
                "playwright",
            ],
            "web",
        ),
        (
            &["image", "video", "audio", "media", "pdf", "document", "ocr"],
            "media",
        ),
        (
            &[
                "crypto",
                "blockchain",
                "wallet",
                "nft",
                "defi",
                "trading",
                "finance",
                "payment",
                "stripe",
                "invoice",
            ],
            "finance",
        ),
        (&["map", "location", "geo", "weather", "travel"], "geo"),
        (
            &[
                "security", "auth", "oauth", "identity", "encrypt", "vault", "secret",
            ],
            "security",
        ),
        (
            &[
                "analytics",
                "metrics",
                "monitor",
                "log",
                "observ",
                "grafana",
                "datadog",
                "sentry",
            ],
            "analytics",
        ),
    ];

    for (keywords, category) in RULES {
        if keywords.iter().any(|kw| text.contains(kw)) {
            return Some((*category).to_string());
        }
    }
    None
}

/// Convert a [`TrustLevel`] enum to the snake_case string expected by the frontend.
pub(super) fn trust_level_str(tl: &TrustLevel) -> String {
    match tl {
        TrustLevel::VerifiedOfficial => "verified_official",
        TrustLevel::CommunityVerified => "community_verified",
        TrustLevel::Community => "community",
        TrustLevel::Custom => "custom",
    }
    .to_string()
}

/// Flattened view of a registry server entry for the catalogue UI.
///
/// Removes the `server` / `_meta` nesting of [`RegistryServer`] and exposes
/// all relevant metadata at the top level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryServerView {
    /// Package identifier (e.g. `@notionhq/notion-mcp-server`).
    pub name: String,
    /// Human-readable display name.
    pub title: Option<String>,
    /// Short description of the server's capabilities.
    pub description: Option<String>,
    /// Semantic version of this registry entry.
    pub version: String,
    /// Documentation or product website URL.
    pub website_url: Option<String>,
    /// Installable packages (npm, pip, …).
    pub packages: Option<Vec<RegistryPackage>>,
    /// Icon assets for display in the catalogue.
    pub icons: Option<Vec<RegistryIcon>>,
    /// Source code repository reference.
    pub repository: Option<RegistryRepository>,
    /// Trust level resolved from builtin enrichments.
    pub trust_level: String,
    /// Category for grouping in the catalogue (e.g. `"productivity"`, `"data"`).
    pub category: Option<String>,
    /// Enrichment metadata if the server matches a builtin connector.
    pub enrichment: Option<ConnectorEnrichmentView>,
    /// Whether this server is already installed locally.
    pub is_installed: bool,
    /// Remote connection endpoints (streamable-http, SSE).
    ///
    /// Empty when the server is only distributed as an installable package.
    pub remotes: Vec<RegistryRemote>,
}

impl From<RegistryServer> for RegistryServerView {
    fn from(s: RegistryServer) -> Self {
        Self {
            name: s.server.name,
            title: s.server.title,
            description: s.server.description,
            version: s.server.version,
            website_url: s.server.website_url,
            packages: s.server.packages,
            icons: s.server.icons,
            repository: s.server.repository,
            trust_level: "community".to_string(),
            category: None,
            enrichment: None,
            is_installed: false,
            remotes: s.server.remotes,
        }
    }
}

/// UI-friendly enrichment view for a connector, keyed by package identifier.
///
/// Mirrors the TypeScript `ConnectorEnrichmentView` type. The `operator_label`
/// and `auth_help_text` are resolved to English; full locale support is handled
/// by the i18n layer on the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorEnrichmentView {
    /// Human-readable label shown in the operator UI (English).
    pub operator_label: String,
    /// Category used for grouping in the catalogue (e.g. `"productivity"`).
    pub category: String,
    /// Lucide icon name (e.g. `"database"`, `"globe"`).
    pub icon_name: String,
    /// Trust level badge serialised as a snake_case string.
    pub trust_level: TrustLevel,
    /// URL where the user can obtain an API key, if applicable.
    pub auth_help_url: Option<String>,
    /// Guidance shown on the authentication step (English).
    pub auth_help_text: Option<String>,
    /// Optional i18n key resolved by the wizard via `$t(key)`. When set, it
    /// takes precedence over `auth_help_text` and lets us ship long localised
    /// explanations in the bundled FR/EN catalogs instead of duplicating them
    /// in `enrichments.json`. Used today for Figma's catalog-gating notice.
    pub auth_help_i18n_key: Option<String>,
    /// Default value for the `requires_approval` flag on the created server.
    pub default_requires_approval: bool,
    /// Env var name carrying a pre-registered OAuth client id.
    /// Set for providers that require manual dev-portal registration (Figma).
    /// `None` for providers that support CIMD or anonymous DCR.
    pub oauth_pre_registered_client_id_env: Option<String>,
}

/// Pair of a package identifier and its enrichment view.
///
/// Returned by [`list_mcp_enrichments`] so the frontend can build a
/// lookup table keyed by `package_identifier`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentEntry {
    /// Package identifier matching the registry (e.g. `@notionhq/notion-mcp-server`).
    pub package_identifier: String,
    /// Enrichment data for this package.
    pub enrichment: ConnectorEnrichmentView,
}

/// List all builtin connector enrichments.
///
/// Returns every entry from the embedded `enrichments.json`. The frontend uses
/// the resulting list to build a lookup table keyed by `package_identifier`,
/// enabling the operator connection cards to show human-readable labels and
/// icons for known connectors.
#[tauri::command]
pub fn list_mcp_enrichments() -> Vec<EnrichmentEntry> {
    load_builtin_enrichments()
        .into_iter()
        .map(|e| EnrichmentEntry {
            package_identifier: e.package_identifier,
            enrichment: ConnectorEnrichmentView {
                operator_label: e.operator_label.get("en").cloned().unwrap_or_default(),
                category: e.category,
                icon_name: e.icon_name,
                trust_level: e.trust_level,
                auth_help_url: e.auth_help_url,
                auth_help_text: e.auth_help_text.and_then(|m| m.get("en").cloned()),
                auth_help_i18n_key: e.auth_help_i18n_key,
                default_requires_approval: e.default_requires_approval,
                oauth_pre_registered_client_id_env: e.oauth_pre_registered_client_id_env,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RegistryServerView flattens RegistryServer correctly ──────────

    #[test]
    fn test_registry_server_view_from_maps_all_fields() {
        // GIVEN a RegistryServer with nested server detail
        let raw = serde_json::json!({
            "server": {
                "name": "notion",
                "title": "Notion",
                "description": "Read and write Notion pages",
                "version": "1.0.0",
                "repository": null,
                "websiteUrl": "https://notion.so",
                "packages": null,
                "icons": null
            },
            "_meta": null
        });
        let server: RegistryServer = serde_json::from_value(raw).unwrap();

        // WHEN converted to a view
        let view = RegistryServerView::from(server);

        // THEN all fields are lifted to the top level and meta is dropped
        assert_eq!(view.name, "notion");
        assert_eq!(view.title.as_deref(), Some("Notion"));
        assert_eq!(view.version, "1.0.0");
        assert_eq!(view.website_url.as_deref(), Some("https://notion.so"));
        assert!(view.packages.is_none());
    }

    #[test]
    fn test_registry_server_view_from_sqlite() {
        // GIVEN a RegistryServer for a server without website or packages
        let raw = serde_json::json!({
            "server": {
                "name": "@modelcontextprotocol/server-sqlite",
                "title": "SQLite",
                "description": "Query local SQLite databases",
                "version": "0.3.0",
                "repository": null,
                "websiteUrl": null,
                "packages": null,
                "icons": null
            },
            "_meta": null
        });
        let server: RegistryServer = serde_json::from_value(raw).unwrap();

        // WHEN converted to a view
        let view = RegistryServerView::from(server);

        // THEN optional fields are None and required fields are present
        assert_eq!(view.name, "@modelcontextprotocol/server-sqlite");
        assert_eq!(view.version, "0.3.0");
        assert!(view.website_url.is_none());
        assert!(view.icons.is_none());
    }
}
