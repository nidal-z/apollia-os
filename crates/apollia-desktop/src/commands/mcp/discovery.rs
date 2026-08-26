//! The two catalogue fetches the wizard offers: the public MCP registry, and
//! the curated list Apollia ships. Both join their results against the servers
//! already installed, and both fall back to cache when the network is down.

use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

use std::collections::{HashMap, HashSet};

use crate::mcp::enrichments::{load_builtin_enrichments, ConnectorEnrichment};
use crate::mcp::registry_client::{
    McpRegistryClient, RegistryEnvVar, RegistryPackage, RegistryPackageArg, RegistryRemote,
    RegistryRemoteHeader, RegistryTransport,
};

use apollia_mcp::manager::McpServerStatus;

use super::catalog::{
    infer_category, trust_level_str, ConnectorEnrichmentView, RegistryServerView,
};
use crate::commands::http_get_json;

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
    // enrichments.json - they are baked into the binary and require no
    // network. Treat any registry error as an empty result and log it.
    let raw_servers = match registry.fetch_servers(search.as_deref()).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "falling back to the curated catalogue only",
                "mcp.registry.fetch.failed"
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
                    reason = "already covered by the curated catalogue",
                    "mcp.registry.entry.deduplicated"
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
                        // Synthetic entries have no registry data - use enrichment
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

/// Return only the 18 curated MCP entries baked into the binary -
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
