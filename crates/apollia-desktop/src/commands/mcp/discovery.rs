//! The two catalogue fetches the wizard offers: the public MCP registry, and
//! the curated list Apollia ships. Both join their results against the servers
//! already installed, and both fall back to cache when the network is down.

use crate::attached::RuntimeLink;
use tauri::State;

use std::collections::{HashMap, HashSet};

use crate::mcp::enrichments::{load_builtin_enrichments, ConnectorEnrichment};
use crate::mcp::registry_client::{
    RegistryEnvVar, RegistryPackage, RegistryPackageArg, RegistryRemote, RegistryRemoteHeader,
    RegistryServer, RegistryTransport,
};

use apollia_mcp::manager::McpServerStatus;

use super::catalog::{
    infer_category, trust_level_str, ConnectorEnrichmentView, RegistryServerView,
};

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
/// Reads the official MCP registry through `GET /api/v1/mcp/registry` on the
/// runtime this application drives, then enriches each result:
/// - Matches package identifiers against builtin enrichments to set
///   `trust_level` and `enrichment` (label, category, icon, auth help).
/// - Matches server names against currently installed MCP servers to set
///   `is_installed`.
///
/// The runtime falls back to cached registry data when the network is
/// unreachable.
#[tauri::command]
pub async fn fetch_mcp_registry(
    state: State<'_, RuntimeLink>,
    search: Option<String>,
) -> Result<Vec<RegistryServerView>, String> {
    // The remote MCP registry can be unreachable (offline, DNS down, registry
    // outage, or simply not configured in this build) with no cache to fall
    // back on. When that happens we still want the operator to see the
    // curated entries from enrichments.json - they are baked into the binary
    // and require no network. Treat any registry error as an empty result and
    // log it.
    let raw_servers = match fetch_registry_servers(&state, search.as_deref()).await {
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
    let installed_names: HashSet<String> = match state.get_json("/api/v1/mcp/servers").await {
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
    for enrichment in enrichments.iter().filter(|e| e.is_listed()) {
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

            let installed = installed_names.contains(&enrichment.package_identifier);
            result.push(curated_view(enrichment, name, installed));
        }
    }

    Ok(result)
}

/// The servers the MCP registry lists, read through the runtime's route, which
/// shares the registry cache and its offline fallback.
pub(crate) async fn fetch_registry_servers(
    link: &RuntimeLink,
    search: Option<&str>,
) -> Result<Vec<RegistryServer>, String> {
    let path = match search.map(str::trim).filter(|s| !s.is_empty()) {
        Some(term) => format!("/api/v1/mcp/registry?search={}", urlencoding::encode(term)),
        None => "/api/v1/mcp/registry".to_owned(),
    };
    let json = link.get_json(&path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse the registry: {e}"))
}

/// Return the selection Apollia offers, without the public registry.
///
/// The selection is the one compiled into the application with the signed
/// online index this machine last accepted laid over it. Opening refreshes
/// that index first, waiting a few seconds at most, then falls back to the
/// copy already kept: the public registry is never asked on this path.
///
/// Used by the catalogue to show the selection as soon as it opens, before
/// the user asks for the full registry.
#[tauri::command]
pub async fn fetch_mcp_curated(
    state: State<'_, RuntimeLink>,
) -> Result<Vec<RegistryServerView>, String> {
    crate::mcp::remote_catalogue::refresh_on_open().await;
    let enrichments = load_builtin_enrichments();
    let installed_names: HashSet<String> = match state.get_json("/api/v1/mcp/servers").await {
        Ok(json) => serde_json::from_value::<Vec<McpServerStatus>>(json)
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.name)
            .collect(),
        Err(_) => HashSet::new(),
    };

    let mut result: Vec<RegistryServerView> = Vec::with_capacity(enrichments.len());
    for enrichment in enrichments.iter().filter(|e| e.is_listed()) {
        let name = enrichment
            .registry_names
            .first()
            .cloned()
            .unwrap_or_else(|| enrichment.package_identifier.clone());
        let installed = installed_names.contains(&enrichment.package_identifier)
            || installed_names.contains(&name);
        result.push(curated_view(enrichment, name, installed));
    }
    Ok(result)
}

/// The catalogue view of a selection entry, under `name`. Its texts travel in
/// every locale through the enrichment, so the view itself carries no title
/// nor description: the interface reads them from the enrichment.
fn curated_view(
    enrichment: &ConnectorEnrichment,
    name: String,
    is_installed: bool,
) -> RegistryServerView {
    RegistryServerView {
        name,
        title: None,
        description: None,
        version: String::new(),
        website_url: enrichment.auth_help_url.clone(),
        packages: enrichment.package_registry_type.as_ref().map(|reg_type| {
            vec![RegistryPackage {
                registry_type: reg_type.clone(),
                identifier: enrichment.package_identifier.clone(),
                version: enrichment.package_version.clone(),
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
                        description: None,
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
        enrichment: Some(ConnectorEnrichmentView::from(enrichment)),
        is_installed,
        remotes: match (&enrichment.remote_url, &enrichment.remote_transport) {
            (Some(url), Some(transport)) => vec![RegistryRemote {
                transport_type: transport.clone(),
                url: url.clone(),
                // A selection entry has no registry data: its own headers are
                // the sole source.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection_entry(package_identifier: &str) -> ConnectorEnrichment {
        load_builtin_enrichments()
            .into_iter()
            .find(|e| e.package_identifier == package_identifier)
            .unwrap()
    }

    #[test]
    fn test_curated_view_sends_every_text_in_both_locales() {
        // GIVEN the selection entry for local files, whose label, description,
        // authentication help and argument are written in English and French
        let entry = selection_entry("@modelcontextprotocol/server-filesystem");

        // WHEN it is turned into the view the catalogue receives
        let view = serde_json::to_value(curated_view(&entry, "files".to_owned(), false)).unwrap();

        // THEN each text reaches the interface in both locales, and no text is
        // fixed in one language on the view itself
        let enrichment = &view["enrichment"];
        assert_eq!(enrichment["operator_label"]["fr"], "Fichiers locaux");
        assert_eq!(enrichment["operator_label"]["en"], "Local Files");
        for text in [
            &enrichment["description"],
            &enrichment["auth_help_text"],
            &enrichment["argument_descriptions"][0],
        ] {
            assert!(text["fr"].is_string() && text["en"].is_string());
            assert_ne!(text["fr"], text["en"]);
        }
        assert!(view["title"].is_null());
        assert!(view["description"].is_null());
        assert!(view["packages"][0]["packageArguments"][0]["description"].is_null());
    }
}
