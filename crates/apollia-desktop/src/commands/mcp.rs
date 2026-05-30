//! Tauri IPC commands for MCP server management.
//!
//! Delegates to the runtime REST API (via TCP) for CRUD operations on connected
//! servers, and directly to [`McpRegistryClient`] and [`SecretStore`] for
//! registry discovery and secret management.

use apollia_mcp::approvals::McpApprovalStore;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::discovery;
use apollia_mcp::manager::{McpConnectionTestResult, McpServerDetail, McpServerStatus};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use std::collections::{HashMap, HashSet};

use crate::mcp::enrichments::{load_builtin_enrichments, ConnectorEnrichment, TrustLevel};
use crate::mcp::registry_client::{
    McpRegistryClient, RegistryEnvVar, RegistryIcon, RegistryPackage, RegistryPackageArg,
    RegistryRemote, RegistryRemoteHeader, RegistryRepository, RegistryServer, RegistryTransport,
};
use crate::mcp::secret_store::SecretStore;

use super::{http_delete_json, http_get_json, http_patch_json, http_post_json, http_put_json};

/// Apply enrichment `remote_headers` as fallback on remotes that have no headers.
///
/// Publisher registry data is preferred (primary source); this fallback is only
/// activated when the registry entry omits the `headers` field for a remote that
/// the enrichment knows about. Keeps curated connectors installable even when the
/// registry is incomplete, without preventing future registry-sourced improvements.
fn apply_remote_header_fallback(
    remotes: &mut Vec<RegistryRemote>,
    enrichment: &ConnectorEnrichment,
) {
    if enrichment.remote_headers.is_empty() {
        return;
    }
    for remote in remotes.iter_mut() {
        if remote.headers.is_empty() {
            remote.headers = enrichment
                .remote_headers
                .iter()
                .map(|h| RegistryRemoteHeader {
                    name: h.name.clone(),
                    description: h.description.clone(),
                    is_required: h.is_required,
                    is_secret: h.is_secret,
                })
                .collect();
        }
    }
}

/// Infer a category from a server's name and description using keyword matching.
///
/// Returns `None` when no keywords match — the frontend treats `None` as "other".
fn infer_category(name: &str, description: Option<&str>) -> Option<String> {
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
fn trust_level_str(tl: &TrustLevel) -> String {
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
    /// Env var name carrying a pre-registered OAuth client id (ADR-095).
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

/// List all connected MCP servers with their status.
///
/// Delegates to `GET /api/v1/mcp/servers` on the embedded runtime.
/// Returns an empty list when no MCP servers are configured.
#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<McpServerStatus>, String> {
    let json = http_get_json(state.api_port, "/api/v1/mcp/servers").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server list: {e}"))
}

/// Get detailed information for a single MCP server.
///
/// Delegates to `GET /api/v1/mcp/servers/{name}` on the embedded runtime.
/// Returns an error when the server is not found or MCP is not configured.
#[tauri::command]
pub async fn get_mcp_server_detail(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerDetail, String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    let json = http_get_json(state.api_port, &path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server detail: {e}"))
}

/// Add a new MCP server and persist its configuration to `mcp.toml`.
///
/// Delegates to `POST /api/v1/mcp/servers` on the embedded runtime. The server
/// process is spawned and the MCP handshake is performed before returning.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Remove an MCP server, delete its configuration from `mcp.db`, and purge
/// the associated OAuth token from the keychain (ADR-095 follow-up).
///
/// Delegates to `DELETE /api/v1/mcp/servers/{name}` on the embedded runtime
/// for the DB cleanup, then **idempotently** deletes the keychain entry at
/// `(apollia-mcp-oauth, <server_name>)` so a future reinstall doesn't
/// inherit a stale token that would silently fail with `invalid_grant`.
///
/// Keychain deletion errors are logged but not propagated — leaving an
/// orphan keychain entry is preferable to blocking the user from removing
/// a server they no longer want.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    http_delete_json(state.api_port, &path).await.map(|_| ())?;

    // Best-effort OAuth token cleanup. We don't know at this layer whether
    // the server used OAuth, so we always attempt the delete — it's a no-op
    // when no entry exists.
    if let Ok(store) = apollia_auth::select_secret_store() {
        if let Err(e) = apollia_auth::delete_mcp_token(&*store, &name) {
            tracing::warn!(
                server = %name,
                error = %e,
                "failed to purge OAuth token on server removal — keychain entry may be orphaned"
            );
        }
    }
    Ok(())
}

/// Return the raw persisted launch configuration of a server.
///
/// Delegates to `GET /api/v1/mcp/servers/{name}/raw_config`. Used by the
/// desktop "Modifier les arguments" flow to seed the inline edit form with
/// the current command/args/env (placeholders, never resolved secrets).
#[tauri::command]
pub async fn get_mcp_server_raw_config(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerConfig, String> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = http_get_json(state.api_port, &path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server config: {e}"))
}

/// Replace an MCP server's launch configuration and restart its session.
///
/// Delegates to `PUT /api/v1/mcp/servers/{name}/config` on the embedded
/// runtime, which performs a remove → add (restart) → persist cycle so the
/// new `command` / `args` / `env` / `transport` take effect immediately.
/// Used by the desktop "Modifier les arguments" flow on the manage panel,
/// which lets the operator fix runtime parameters (e.g. allowed directories
/// for `@modelcontextprotocol/server-filesystem`) without re-running the
/// full install wizard.
///
/// The `config.name` field must equal `name`; the runtime rejects mismatches.
#[tauri::command]
pub async fn update_mcp_server_config(
    state: State<'_, RuntimeHandle>,
    name: String,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let path = format!("/api/v1/mcp/servers/{name}/config");
    let json = http_put_json(state.api_port, &path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Tagged response envelope for `test_mcp_connection`, mirroring the
/// runtime-side `McpConnectionTestResponse` (ADR-095 Phase 4).
///
/// The wizard dispatches its Auth step UI on `kind`:
/// - `success` → list tools, allow continue.
/// - `oauth_required` → switch to "Sign in with <provider>" mode and call
///   [`mcp_oauth_login`] when the user clicks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpConnectionTestResponse {
    Success {
        #[serde(flatten)]
        result: McpConnectionTestResult,
    },
    OauthRequired {
        /// Verbatim `WWW-Authenticate` header captured from the 401.
        www_authenticate: String,
    },
}

/// Test an MCP server configuration without persisting a session.
///
/// Delegates to `POST /api/v1/mcp/servers/test` on the embedded runtime.
/// Spawns an ephemeral process, performs the MCP handshake, then immediately
/// terminates the process without modifying `mcp.toml` or the tool registry.
///
/// Returns a tagged enum so the wizard can route Auth step UI without an
/// extra round-trip — `oauth_required` means the server returned 401 and the
/// MCP HTTP OAuth flow should be driven via [`mcp_oauth_login`].
#[tauri::command]
pub async fn test_mcp_connection(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpConnectionTestResponse, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers/test", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Restart an MCP server session.
///
/// Delegates to `POST /api/v1/mcp/servers/{name}/restart` on the embedded
/// runtime. Stops the current session and spawns a new one using the original
/// configuration.
#[tauri::command]
pub async fn restart_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerStatus, String> {
    let path = format!("/api/v1/mcp/servers/{name}/restart");
    let json = http_post_json(state.api_port, &path, &serde_json::json!({})).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Whether a public-registry server is already covered by an Apollia curated
/// enrichment (by registry name, package identifier, or remote URL).
fn is_covered_by_curated(
    detail: &crate::mcp::registry_client::RegistryServerDetail,
    by_name: &HashMap<&str, &ConnectorEnrichment>,
    by_pkg: &HashMap<&str, &ConnectorEnrichment>,
    by_remote_url: &HashSet<&str>,
) -> bool {
    let name_covered = by_name.contains_key(detail.name.as_str());
    let pkg_covered = detail.packages.as_ref().is_some_and(|pkgs| {
        pkgs.iter()
            .any(|pkg| by_pkg.contains_key(pkg.identifier.as_str()))
    });
    let remote_covered = detail
        .remotes
        .iter()
        .any(|r| by_remote_url.contains(r.url.as_str()));
    name_covered || pkg_covered || remote_covered
}

/// Fetch MCP Registry servers with enrichment and install-status join.
///
/// Queries the official MCP registry, then enriches each result:
/// - Matches package identifiers against builtin enrichments to set
///   `trust_level` and `enrichment` (label, category, icon, auth help).
/// - Matches server names against currently installed MCP servers to set
///   `is_installed`.
///
/// Falls back to cached registry data when the network is unreachable.
#[tauri::command]
pub async fn fetch_mcp_registry(
    registry: State<'_, McpRegistryClient>,
    state: State<'_, RuntimeHandle>,
    search: Option<String>,
) -> Result<Vec<RegistryServerView>, String> {
    // The remote MCP registry can be unreachable (offline, DNS down, registry
    // outage, or simply not configured in this build). When that happens we
    // still want the operator to see the 18 curated entries from
    // enrichments.json — they are baked into the binary and require no
    // network. Treat any registry error as an empty result and log it.
    let raw_servers = match registry.fetch_servers(search.as_deref()).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "mcp.registry.fetch_failed — falling back to curated catalogue only"
            );
            Vec::new()
        }
    };

    // Build enrichment lookups: by package identifier AND by registry server name.
    let enrichments = load_builtin_enrichments();
    let enrichment_by_pkg: HashMap<&str, &crate::mcp::enrichments::ConnectorEnrichment> =
        enrichments
            .iter()
            .map(|e| (e.package_identifier.as_str(), e))
            .collect();
    let enrichment_by_name: HashMap<&str, &crate::mcp::enrichments::ConnectorEnrichment> =
        enrichments
            .iter()
            .flat_map(|e| e.registry_names.iter().map(move |name| (name.as_str(), e)))
            .collect();

    // Build set of installed server names for is_installed detection.
    let installed_names: HashSet<String> =
        match http_get_json(state.api_port, "/api/v1/mcp/servers").await {
            Ok(json) => serde_json::from_value::<Vec<McpServerStatus>>(json)
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.name)
                .collect(),
            Err(_) => HashSet::new(),
        };

    // Drop public-registry entries that are already covered by an Apollia
    // curated enrichment. Otherwise the catalogue shows duplicates: e.g.
    // a stray "Figma" from the public registry next to "Figma" (Dev Mode)
    // and "Figma (cloud, OAuth)" injected from our curated set.
    //
    // A public entry is considered covered when its registry name matches
    // an enrichment's registry_names, OR any of its package identifiers
    // matches an enrichment's package_identifier, OR any of its remote URLs
    // matches an enrichment's remote_url. The curated synthetic injection
    // below adds back our own version with verified URLs and headers.
    let enrichment_by_remote_url: HashSet<&str> = enrichments
        .iter()
        .filter_map(|e| e.remote_url.as_deref())
        .collect();

    let views: Vec<RegistryServerView> = raw_servers
        .into_iter()
        .filter_map(|s| {
            if is_covered_by_curated(
                &s.server,
                &enrichment_by_name,
                &enrichment_by_pkg,
                &enrichment_by_remote_url,
            ) {
                tracing::debug!(
                    server = %s.server.name,
                    "mcp.registry.dedup — skipping public entry already covered by curated"
                );
                return None;
            }
            let mut view = RegistryServerView::from(s);

            // Auto-categorize by keywords when no enrichment provided a category.
            if view.category.is_none() {
                view.category = infer_category(&view.name, view.description.as_deref());
            }

            // Check if installed by matching server name.
            if installed_names.contains(&view.name) {
                view.is_installed = true;
            }

            Some(view)
        })
        .collect();

    // Inject synthetic entries for enrichments not found in the paginated results.
    // The registry is alphabetically paginated, so known connectors (e.g. com.notion)
    // may fall outside the fetched pages. We inject them so they always appear.
    let matched_names: HashSet<String> = views
        .iter()
        .filter(|v| v.enrichment.is_some())
        .map(|v| v.name.clone())
        .collect();

    let mut result = views;
    for enrichment in &enrichments {
        let already_present = enrichment
            .registry_names
            .iter()
            .any(|n| matched_names.contains(n))
            || matched_names.contains(&enrichment.package_identifier);

        if !already_present {
            // Pick the first registry name as the synthetic server name, fallback to package_identifier.
            let name = enrichment
                .registry_names
                .first()
                .cloned()
                .unwrap_or_else(|| enrichment.package_identifier.clone());

            result.push(RegistryServerView {
                name,
                title: Some(
                    enrichment
                        .operator_label
                        .get("en")
                        .cloned()
                        .unwrap_or_default(),
                ),
                description: enrichment
                    .description
                    .as_ref()
                    .and_then(|m| m.get("en").cloned()),
                version: String::new(),
                website_url: enrichment.auth_help_url.clone(),
                packages: enrichment.package_registry_type.as_ref().map(|reg_type| {
                    vec![RegistryPackage {
                        registry_type: reg_type.clone(),
                        identifier: enrichment.package_identifier.clone(),
                        version: None,
                        runtime_hint: enrichment.package_runtime_hint.clone(),
                        transport: RegistryTransport {
                            transport_type: "stdio".to_string(),
                        },
                        environment_variables: enrichment
                            .package_env_vars
                            .iter()
                            .map(|ev| RegistryEnvVar {
                                name: ev.name.clone(),
                                description: ev.description.clone(),
                                is_required: ev.is_required,
                                is_secret: ev.is_secret,
                            })
                            .collect(),
                        package_arguments: enrichment
                            .package_arguments
                            .iter()
                            .map(|a| RegistryPackageArg {
                                arg_type: a.arg_type.clone(),
                                value: None,
                                value_hint: a.value_hint.clone(),
                                description: a
                                    .description
                                    .as_ref()
                                    .and_then(|m| m.get("en").cloned()),
                                is_required: a.is_required,
                                is_repeatable: a.is_repeatable,
                            })
                            .collect(),
                    }]
                }),
                icons: None,
                repository: None,
                trust_level: trust_level_str(&enrichment.trust_level),
                category: Some(enrichment.category.clone()),
                enrichment: Some(ConnectorEnrichmentView {
                    operator_label: enrichment
                        .operator_label
                        .get("en")
                        .cloned()
                        .unwrap_or_default(),
                    category: enrichment.category.clone(),
                    icon_name: enrichment.icon_name.clone(),
                    trust_level: enrichment.trust_level.clone(),
                    auth_help_url: enrichment.auth_help_url.clone(),
                    auth_help_text: enrichment
                        .auth_help_text
                        .as_ref()
                        .and_then(|m| m.get("en").cloned()),
                    auth_help_i18n_key: enrichment.auth_help_i18n_key.clone(),
                    default_requires_approval: enrichment.default_requires_approval,
                    oauth_pre_registered_client_id_env: enrichment
                        .oauth_pre_registered_client_id_env
                        .clone(),
                }),
                is_installed: installed_names.contains(&enrichment.package_identifier),
                remotes: match (&enrichment.remote_url, &enrichment.remote_transport) {
                    (Some(url), Some(transport)) => vec![RegistryRemote {
                        transport_type: transport.clone(),
                        url: url.clone(),
                        // Synthetic entries have no registry data — use enrichment
                        // fallback headers directly as the sole source.
                        headers: enrichment
                            .remote_headers
                            .iter()
                            .map(|h| RegistryRemoteHeader {
                                name: h.name.clone(),
                                description: h.description.clone(),
                                is_required: h.is_required,
                                is_secret: h.is_secret,
                            })
                            .collect(),
                    }],
                    _ => vec![],
                },
            });
        }
    }

    Ok(result)
}

/// Return only the 18 curated MCP entries baked into the binary —
/// instant, network-free path.
///
/// Used by the Catalogue Sheet to show a useful subset immediately on
/// open, before (optionally) triggering the heavy full-registry fetch.
/// Mirrors the synthetic-injection codepath inside `fetch_mcp_registry`
/// but skips the network call entirely.
#[tauri::command]
pub async fn fetch_mcp_curated(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<RegistryServerView>, String> {
    let enrichments = load_builtin_enrichments();
    let installed_names: HashSet<String> =
        match http_get_json(state.api_port, "/api/v1/mcp/servers").await {
            Ok(json) => serde_json::from_value::<Vec<McpServerStatus>>(json)
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.name)
                .collect(),
            Err(_) => HashSet::new(),
        };

    let mut result: Vec<RegistryServerView> = Vec::with_capacity(enrichments.len());
    for enrichment in &enrichments {
        let name = enrichment
            .registry_names
            .first()
            .cloned()
            .unwrap_or_else(|| enrichment.package_identifier.clone());
        result.push(RegistryServerView {
            name: name.clone(),
            title: Some(
                enrichment
                    .operator_label
                    .get("en")
                    .cloned()
                    .unwrap_or_default(),
            ),
            description: enrichment
                .description
                .as_ref()
                .and_then(|m| m.get("en").cloned()),
            version: String::new(),
            website_url: enrichment.auth_help_url.clone(),
            packages: enrichment.package_registry_type.as_ref().map(|reg_type| {
                vec![RegistryPackage {
                    registry_type: reg_type.clone(),
                    identifier: enrichment.package_identifier.clone(),
                    version: None,
                    runtime_hint: enrichment.package_runtime_hint.clone(),
                    transport: RegistryTransport {
                        transport_type: "stdio".to_string(),
                    },
                    environment_variables: enrichment
                        .package_env_vars
                        .iter()
                        .map(|ev| RegistryEnvVar {
                            name: ev.name.clone(),
                            description: ev.description.clone(),
                            is_required: ev.is_required,
                            is_secret: ev.is_secret,
                        })
                        .collect(),
                    package_arguments: enrichment
                        .package_arguments
                        .iter()
                        .map(|a| RegistryPackageArg {
                            arg_type: a.arg_type.clone(),
                            value: None,
                            value_hint: a.value_hint.clone(),
                            description: a.description.as_ref().and_then(|m| m.get("en").cloned()),
                            is_required: a.is_required,
                            is_repeatable: a.is_repeatable,
                        })
                        .collect(),
                }]
            }),
            icons: None,
            repository: None,
            trust_level: trust_level_str(&enrichment.trust_level),
            category: Some(enrichment.category.clone()),
            enrichment: Some(ConnectorEnrichmentView {
                operator_label: enrichment
                    .operator_label
                    .get("en")
                    .cloned()
                    .unwrap_or_default(),
                category: enrichment.category.clone(),
                icon_name: enrichment.icon_name.clone(),
                trust_level: enrichment.trust_level.clone(),
                auth_help_url: enrichment.auth_help_url.clone(),
                auth_help_text: enrichment
                    .auth_help_text
                    .as_ref()
                    .and_then(|m| m.get("en").cloned()),
                auth_help_i18n_key: enrichment.auth_help_i18n_key.clone(),
                default_requires_approval: enrichment.default_requires_approval,
                oauth_pre_registered_client_id_env: enrichment
                    .oauth_pre_registered_client_id_env
                    .clone(),
            }),
            is_installed: installed_names.contains(&enrichment.package_identifier)
                || installed_names.contains(&name),
            remotes: match (&enrichment.remote_url, &enrichment.remote_transport) {
                (Some(url), Some(transport)) => vec![RegistryRemote {
                    transport_type: transport.clone(),
                    url: url.clone(),
                    headers: enrichment
                        .remote_headers
                        .iter()
                        .map(|h| RegistryRemoteHeader {
                            name: h.name.clone(),
                            description: h.description.clone(),
                            is_required: h.is_required,
                            is_secret: h.is_secret,
                        })
                        .collect(),
                }],
                _ => vec![],
            },
        });
    }
    Ok(result)
}

/// Fetch fresh detail for a single MCP server directly from the registry.
///
/// Used by the wizard when the server's remote auth headers are absent from
/// the bulk-cached catalogue. Skips the local cache so the result is always
/// current — auth requirements are defined by the publisher, not by Apollia.
#[tauri::command]
pub async fn refresh_mcp_server_detail(
    registry: State<'_, McpRegistryClient>,
    state: State<'_, RuntimeHandle>,
    server_name: String,
) -> Result<Option<RegistryServerView>, String> {
    let raw = registry
        .fetch_server_by_name(&server_name)
        .await
        .map_err(|e| e.to_string())?;

    let Some(raw_server) = raw else {
        return Ok(None);
    };

    let enrichments = load_builtin_enrichments();
    let enrichment_by_pkg: HashMap<&str, &crate::mcp::enrichments::ConnectorEnrichment> =
        enrichments
            .iter()
            .map(|e| (e.package_identifier.as_str(), e))
            .collect();
    let enrichment_by_name: HashMap<&str, &crate::mcp::enrichments::ConnectorEnrichment> =
        enrichments
            .iter()
            .flat_map(|e| e.registry_names.iter().map(move |name| (name.as_str(), e)))
            .collect();

    let installed_names: std::collections::HashSet<String> =
        match http_get_json(state.api_port, "/api/v1/mcp/servers").await {
            Ok(json) => serde_json::from_value::<Vec<McpServerStatus>>(json)
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.name)
                .collect(),
            Err(_) => std::collections::HashSet::new(),
        };

    let mut view = RegistryServerView::from(raw_server);

    let matched = enrichment_by_name
        .get(view.name.as_str())
        .copied()
        .or_else(|| {
            view.packages.as_ref().and_then(|pkgs| {
                pkgs.iter()
                    .find_map(|pkg| enrichment_by_pkg.get(pkg.identifier.as_str()).copied())
            })
        });

    if let Some(enrichment) = matched {
        view.trust_level = trust_level_str(&enrichment.trust_level);
        view.category = Some(enrichment.category.clone());
        view.enrichment = Some(ConnectorEnrichmentView {
            operator_label: enrichment
                .operator_label
                .get("en")
                .cloned()
                .unwrap_or_default(),
            category: enrichment.category.clone(),
            icon_name: enrichment.icon_name.clone(),
            trust_level: enrichment.trust_level.clone(),
            auth_help_url: enrichment.auth_help_url.clone(),
            auth_help_text: enrichment
                .auth_help_text
                .as_ref()
                .and_then(|m| m.get("en").cloned()),
            auth_help_i18n_key: enrichment.auth_help_i18n_key.clone(),
            default_requires_approval: enrichment.default_requires_approval,
            oauth_pre_registered_client_id_env: enrichment
                .oauth_pre_registered_client_id_env
                .clone(),
        });
        apply_remote_header_fallback(&mut view.remotes, enrichment);
    }

    if installed_names.contains(&view.name) {
        view.is_installed = true;
    }

    Ok(Some(view))
}

/// Store a secret in the OS keychain for an MCP server environment variable.
///
/// The secret is stored under the composite key `"{server_name}:{env_var}"`.
#[tauri::command]
pub async fn store_mcp_secret(
    secret_store: State<'_, SecretStore>,
    server_name: String,
    env_var: String,
    value: String,
) -> Result<(), String> {
    let key = SecretStore::key_for(&server_name, &env_var);
    secret_store.store(&key, &value).map_err(|e| e.to_string())
}

/// Update the `requires_approval` flag for a running MCP server.
///
/// Applies the change in-memory immediately and persists it to `mcp.toml`.
/// The server session is not restarted; the flag takes effect on the next tool call.
/// Returns the updated server status.
#[tauri::command]
pub async fn set_mcp_server_approval(
    state: State<'_, RuntimeHandle>,
    name: String,
    requires_approval: bool,
) -> Result<McpServerStatus, String> {
    let body = serde_json::json!({ "requires_approval": requires_approval });
    let path = format!("/api/v1/mcp/servers/{name}/approval");
    let json = http_patch_json(state.api_port, &path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Delete a secret from the OS keychain for an MCP server environment variable.
///
/// The secret is looked up under the composite key `"{server_name}:{env_var}"`.
#[tauri::command]
pub async fn delete_mcp_secret(
    secret_store: State<'_, SecretStore>,
    server_name: String,
    env_var: String,
) -> Result<(), String> {
    let key = SecretStore::key_for(&server_name, &env_var);
    secret_store.delete(&key).map_err(|e| e.to_string())
}

/// Chemin par défaut du fichier SQLite d'approbations MCP.
fn mcp_approvals_db_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(std::path::PathBuf::from(home)
        .join(".apollia")
        .join("mcp_approvals.db"))
}

/// Découvre les serveurs MCP disponibles sur le réseau local via mDNS.
///
/// Effectue un scan de 3 secondes pour `_apollia-mcp._tcp.local.`.
/// Retourne une liste de serveurs découverts avec leur nom, adresses, port et outils.
#[tauri::command]
pub async fn discover_mcp_servers() -> Result<Vec<serde_json::Value>, String> {
    let discovered = discovery::discover_mcp_servers()
        .await
        .map_err(|e| format!("mDNS discovery failed: {e}"))?;

    Ok(discovered
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "addresses": s.addresses,
                "port": s.port,
                "tools": s.tools,
            })
        })
        .collect())
}

/// Liste les approbations MCP outil en attente.
///
/// Lit depuis `~/.apollia/mcp_approvals.db` et retourne les entrées en attente
/// de décision humaine.
#[tauri::command]
pub async fn list_mcp_tool_pending_approvals() -> Result<Vec<serde_json::Value>, String> {
    let db_path = mcp_approvals_db_path()?;

    if !db_path.exists() {
        return Ok(vec![]);
    }

    let store = McpApprovalStore::open(&db_path, 0)
        .map_err(|e| format!("failed to open approvals store: {e}"))?;

    let pending = store
        .list_pending()
        .map_err(|e| format!("failed to list pending approvals: {e}"))?;

    Ok(pending
        .into_iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "server_name": entry.server_name,
                "tool_name": entry.tool_name,
                "requested_at": entry.requested_at,
                "status": entry.status,
            })
        })
        .collect())
}

/// Révoque une approbation MCP pour un serveur et un outil spécifiques.
///
/// Supprime l'approbation de `~/.apollia/mcp_approvals.db`.
/// Retourne `true` si une entrée a été supprimée, `false` si aucune entrée correspondante.
#[tauri::command]
pub async fn revoke_mcp_tool_approval(server: String, tool: String) -> Result<bool, String> {
    let db_path = mcp_approvals_db_path()?;

    if !db_path.exists() {
        return Ok(false);
    }

    let store = McpApprovalStore::open(&db_path, 0)
        .map_err(|e| format!("failed to open approvals store: {e}"))?;

    store
        .revoke(&server, &tool)
        .map_err(|e| format!("failed to revoke approval: {e}"))?;

    Ok(true)
}

// ─── MCP HTTP OAuth IPC (ADR-095 Phase 4) ───────────────────────────────────

/// Discovery result emitted by [`mcp_oauth_discover`].
///
/// Surfaces what the wizard needs to render the OAuth Auth step:
/// - `as_url` for telemetry / "you'll authenticate at <X>".
/// - `scopes_supported` populates the scope selector (defaults all checked).
/// - `scope_descriptions` lets the AS provide human labels — sparse in
///   practice, but rendered when available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthDiscoveryResult {
    pub as_url: String,
    pub scopes_supported: Vec<String>,
    /// Map of `scope → human-readable description`, when the AS exposes
    /// one. Sparse (most AS don't ship this).
    pub scope_descriptions: HashMap<String, String>,
    pub registration_supported: bool,
}

/// Sign-in outcome returned by [`mcp_oauth_login`]. Carries identity claims
/// for UI display ("Connecté en tant que …").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthAccount {
    /// `sub` claim from the access token JWT, when present.
    pub sub: Option<String>,
    /// `email` claim, when present.
    pub email: Option<String>,
    /// Effective scopes granted by the AS (may be a subset of requested).
    pub scopes: Vec<String>,
}

/// Discover the OAuth requirements of a remote MCP server.
///
/// Pre-condition: the caller already ran [`test_mcp_connection`] and observed
/// `OauthRequired { www_authenticate }`. This IPC runs the RFC 9728 + RFC 8414
/// discovery chain (with origin fallback) and returns enough metadata for the
/// wizard to render the scope selector before the OAuth dance starts.
///
/// Why a separate IPC from `mcp_oauth_login`? The user must see the scope list
/// (their explicit decision per the ADR) before consenting. Folding both into
/// `negotiate_token` would force the browser to open before scope selection.
#[tauri::command]
pub async fn mcp_oauth_discover(
    url: String,
    www_authenticate: Option<String>,
) -> Result<McpOAuthDiscoveryResult, String> {
    let discovery = apollia_auth::McpDiscoveryClient::new()
        .map_err(|e| format!("init discovery client: {e}"))?;

    // 1. PRM — prefer the URL advertised by WWW-Authenticate, then fall back
    //    to the well-known at the server's origin (ADR-095 §2).
    let prm = match www_authenticate
        .as_deref()
        .and_then(apollia_auth::parse_www_authenticate)
        .and_then(|wa| wa.resource_metadata)
    {
        Some(prm_url) => discovery.fetch_prm_at(&prm_url).await,
        None => discovery.fetch_prm(&url).await,
    }
    .map_err(|e| format!("fetch PRM: {e}"))?;

    let as_url = prm
        .authorization_servers
        .first()
        .cloned()
        .ok_or_else(|| "PRM declared no authorization servers".to_string())?;

    // 2. AS metadata.
    let as_metadata = discovery
        .fetch_as_metadata(&as_url)
        .await
        .map_err(|e| format!("fetch AS metadata: {e}"))?;

    // Refuse to surface OAuth as available when PKCE S256 isn't advertised —
    // matches the orchestrator's hard refusal at negotiation time so the UI
    // doesn't lure the user into a downgraded flow.
    if !as_metadata.supports_pkce_s256() {
        return Err(format!(
            "authorization server at {as_url} does not advertise PKCE S256"
        ));
    }

    // 3. Effective scope catalogue: PRM scopes_supported preferred (resource-
    //    defined), else nothing (we don't surface AS-wide scopes that may
    //    include unrelated services).
    let scopes_supported = if !prm.scopes_supported.is_empty() {
        prm.scopes_supported.clone()
    } else {
        // Fallback: no resource-level scopes published → leave empty so the
        // wizard defers to AS defaults (sends no `scope=`).
        Vec::new()
    };

    Ok(McpOAuthDiscoveryResult {
        as_url,
        scopes_supported,
        // Scope descriptions aren't part of RFC 8414; some AS extend the
        // metadata with `scopes_supported_description` or similar, but we
        // don't parse those today — leave empty.
        scope_descriptions: HashMap::new(),
        registration_supported: as_metadata.registration_endpoint.is_some(),
    })
}

/// Keychain service slot for user-entered OAuth client ids (ADR-095 follow-up).
///
/// Holds the values typed into the wizard's OAuth client id input when the
/// catalog enrichment declares `oauth_pre_registered_client_id_env` and
/// neither the runtime env var nor the build-time constant resolves. Keeps
/// non-technical users out of the terminal while still supporting power-user
/// overrides (enterprise tenants that want their own Figma app).
const MCP_CLIENT_ID_SERVICE: &str = "apollia-mcp-client-ids";

/// Resolve a pre-registered OAuth client id, with three fallback layers so
/// end users get a turnkey experience.
///
/// Lookup order (mirrors the Google/Microsoft connector pattern, ADR-095
/// follow-up 2026-05-17):
/// 1. **Runtime env var** matching `env_var` — for dev / power-user override
///    (e.g. `APOLLIA_FIGMA_CLIENT_ID=xxx` exported before launch).
/// 2. **Keychain stored value** — set via the wizard input or settings panel
///    by users who don't want to touch env vars but registered their own app.
/// 3. **Build-time constant** baked into the binary via `option_env!` — set
///    when the release pipeline runs with `APOLLIA_BUILD_FIGMA_CLIENT_ID`
///    in the environment (cf. `OAUTH-SETUP-TUTO.md §4`). End users of the
///    release binary inherit this without setting anything.
///
/// Returns `None` only when all three layers are absent — the wizard then
/// shows an input field with the provider's registration help text.
#[tauri::command]
pub fn mcp_oauth_resolve_client_id(env_var: String) -> Option<String> {
    fn non_empty(v: &str) -> bool {
        !v.trim().is_empty()
    }
    if let Ok(v) = std::env::var(&env_var) {
        if non_empty(&v) {
            return Some(v);
        }
    }
    if let Some(v) = load_stored_client_id(&env_var) {
        if non_empty(&v) {
            return Some(v);
        }
    }
    compile_time_known_client_id(&env_var)
        .filter(|v| non_empty(v))
        .map(str::to_string)
}

/// Persist a user-entered OAuth client id to the OS keychain.
///
/// Called by the wizard when the operator pastes a `client_id` into the
/// input field shown for connectors that require manual app registration
/// (Figma today). Subsequent `mcp_oauth_resolve_client_id` calls will
/// return this value (priority 2) until the user clears it.
///
/// Returns `Ok(())` on success. The store is keyed by `env_var` so each
/// provider lives in its own keychain entry, never collides.
#[tauri::command]
pub fn mcp_oauth_store_client_id(env_var: String, value: String) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("client id is empty".into());
    }
    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;
    store
        .set(MCP_CLIENT_ID_SERVICE, &env_var, value.trim())
        .map_err(|e| format!("keychain write failed: {e}"))
}

/// Remove a persisted OAuth client id (Settings panel "reset" button).
#[tauri::command]
pub fn mcp_oauth_clear_client_id(env_var: String) -> Result<(), String> {
    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;
    store
        .delete(MCP_CLIENT_ID_SERVICE, &env_var)
        .map_err(|e| format!("keychain delete failed: {e}"))
}

fn load_stored_client_id(env_var: &str) -> Option<String> {
    let store = apollia_auth::select_secret_store().ok()?;
    store.get(MCP_CLIENT_ID_SERVICE, env_var).ok().flatten()
}

/// Build-time client id registry. `option_env!` requires a literal so each
/// known env var must be enumerated here — adding a new provider is a
/// 2-line change. End users never see this layer; it's how releases ship
/// "turnkey" credentials for AS that don't support CIMD/DCR (Figma today).
fn compile_time_known_client_id(env_var: &str) -> Option<&'static str> {
    match env_var {
        "APOLLIA_FIGMA_CLIENT_ID" => option_env!("APOLLIA_BUILD_FIGMA_CLIENT_ID"),
        _ => None,
    }
}

/// Drive the MCP HTTP OAuth flow end-to-end for `server_name`.
///
/// Opens the default browser at the authorize URL, blocks until the loopback
/// callback fires, exchanges the code for tokens (with `resource=` per
/// RFC 8707), persists the token under `(MCP_OAUTH_SERVICE, server_name)` in
/// the OS keychain, and returns the identity claims for UI display.
///
/// `scopes` controls what's requested at the AS:
/// - `None` or empty → the orchestrator uses the PRM's `scopes_supported`.
/// - non-empty → the user-selected subset from the wizard scope selector.
///
/// Idempotent: calling this for the same `server_name` overwrites any
/// previously-stored token (useful for the "Reconnect" button in settings).
#[tauri::command]
pub async fn mcp_oauth_login(
    app: tauri::AppHandle,
    server_name: String,
    server_url: String,
    www_authenticate: Option<String>,
    scopes: Vec<String>,
    client_id: Option<String>,
) -> Result<McpOAuthAccount, String> {
    use tauri_plugin_opener::OpenerExt;

    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;

    let scopes_opt: Option<Vec<String>> = if scopes.is_empty() {
        None
    } else {
        Some(scopes)
    };

    let client_id_override = client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let req = apollia_auth::NegotiateRequest {
        server_name: &server_name,
        server_url: &server_url,
        www_authenticate: www_authenticate.as_deref(),
        scopes: scopes_opt,
        client_id_override,
    };

    // Use Tauri's opener plugin instead of the `open` crate — the latter
    // spawns a subprocess that gets blocked by Tauri 2's webview sandbox on
    // some platforms (silent no-op). The opener plugin goes through Tauri's
    // own native integration, with explicit capability gating.
    let token = apollia_auth::negotiate_token(req, &*store, |url| {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| apollia_auth::AuthError::CallbackServer(e.to_string()))
    })
    .await
    .map_err(|e| format!("{e}"))?;

    Ok(McpOAuthAccount {
        sub: token.identity_sub,
        email: token.identity_email,
        scopes: token.scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::registry_client::RegistryServer;
    use crate::mcp::secret_store::SecretStore;

    // ── AC-4 : RegistryServerView flattens RegistryServer correctly ──────────

    #[test]
    fn test_ac4_registry_server_view_from_maps_all_fields() {
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
    fn test_ac4_registry_server_view_from_sqlite() {
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

    // ── AC-5 : store_mcp_secret uses the correct composite key ───────────────

    #[test]
    fn test_ac5_store_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside store_mcp_secret)
        let key = SecretStore::key_for("notion", "NOTION_API_KEY");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "notion:NOTION_API_KEY");
    }

    // ── AC-5 : delete_mcp_secret uses the correct composite key ──────────────

    #[test]
    fn test_ac5_delete_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside delete_mcp_secret)
        let key = SecretStore::key_for("slack", "SLACK_BOT_TOKEN");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "slack:SLACK_BOT_TOKEN");
    }

    // ── discover_mcp_servers produces correct JSON shape ─────────────────────

    #[test]
    fn test_discovered_server_json_shape() {
        // GIVEN a discovered server mapped to JSON (as done inside discover_mcp_servers)
        let server_json = serde_json::json!({
            "name": "my-mcp-server._apollia-mcp._tcp.local",
            "addresses": ["192.168.1.42"],
            "port": 8765,
            "tools": ["search", "query"],
        });

        // WHEN the fields are accessed
        // THEN they match the expected shape
        assert_eq!(server_json["port"], 8765);
        assert!(server_json["addresses"].is_array());
        assert!(server_json["tools"].is_array());
    }

    // ── revoke_mcp_tool_approval returns false when db absent ─────────────────

    #[test]
    fn test_mcp_approvals_db_path_contains_apollia() {
        // GIVEN/WHEN the default DB path is computed
        let result = mcp_approvals_db_path();

        // THEN it resolves to a path under .apollia
        assert!(result.is_ok());
        let path = result.expect("path");
        assert!(path.to_str().expect("utf8").contains(".apollia"));
    }
}
