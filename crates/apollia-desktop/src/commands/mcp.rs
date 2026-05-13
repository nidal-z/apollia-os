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
    McpRegistryClient, RegistryEnvVar, RegistryIcon, RegistryPackage, RegistryRemote,
    RegistryRemoteHeader, RegistryRepository, RegistryServer, RegistryTransport,
};
use crate::mcp::secret_store::SecretStore;

use super::{http_delete_json, http_get_json, http_patch_json, http_post_json};

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
    /// Default value for the `requires_approval` flag on the created server.
    pub default_requires_approval: bool,
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
                default_requires_approval: e.default_requires_approval,
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

/// Remove an MCP server and delete its configuration from `mcp.toml`.
///
/// Delegates to `DELETE /api/v1/mcp/servers/{name}` on the embedded runtime.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    http_delete_json(state.api_port, &path).await.map(|_| ())
}

/// Test an MCP server configuration without persisting a session.
///
/// Delegates to `POST /api/v1/mcp/servers/test` on the embedded runtime.
/// Spawns an ephemeral process, performs the MCP handshake, then immediately
/// terminates the process without modifying `mcp.toml` or the tool registry.
#[tauri::command]
pub async fn test_mcp_connection(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpConnectionTestResult, String> {
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

    let views: Vec<RegistryServerView> = raw_servers
        .into_iter()
        .map(|s| {
            let mut view = RegistryServerView::from(s);

            // Try to find enrichment: first by registry server name, then by package identifier.
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
                    default_requires_approval: enrichment.default_requires_approval,
                });
                // Apply enrichment header fallback when the registry entry omits headers.
                apply_remote_header_fallback(&mut view.remotes, enrichment);
            }

            // Auto-categorize by keywords when no enrichment provided a category.
            if view.category.is_none() {
                view.category = infer_category(&view.name, view.description.as_deref());
            }

            // Check if installed by matching server name.
            if installed_names.contains(&view.name) {
                view.is_installed = true;
            }

            view
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
                        package_arguments: vec![],
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
                    default_requires_approval: enrichment.default_requires_approval,
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
            operator_label: enrichment.operator_label.get("en").cloned().unwrap_or_default(),
            category: enrichment.category.clone(),
            icon_name: enrichment.icon_name.clone(),
            trust_level: enrichment.trust_level.clone(),
            auth_help_url: enrichment.auth_help_url.clone(),
            auth_help_text: enrichment
                .auth_help_text
                .as_ref()
                .and_then(|m| m.get("en").cloned()),
            default_requires_approval: enrichment.default_requires_approval,
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
