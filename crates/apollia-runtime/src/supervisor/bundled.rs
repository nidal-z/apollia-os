use super::*;

/// Install the bundled agents on first boot.
///
/// Reads `<bundled_agents_path>/manifest.json` and, for each entry marked
/// `auto_install: true` that is absent from the database, loads the manifest
/// via `loader` and persists an [`apollia_tools::InstalledAgent`] in `repo`.
///
/// The later auto-load step (`list_enabled`) then loads and registers them in
/// `AgentRegistry`. Errors are always logged but never block the boot.
pub(in crate::supervisor) fn auto_load_bundled_agents(
    bundled_agents_path: Option<&std::path::Path>,
    repo: &AgentRepository,
    loader: &Arc<dyn AgentLoader>,
) {
    let bundled_dir = match bundled_agents_path {
        Some(d) => d,
        None => {
            tracing::debug!("no bundled agents path configured");
            return;
        }
    };

    let manifest_path = bundled_dir.join("manifest.json");
    if !manifest_path.exists() {
        tracing::debug!("no bundled agents manifest found");
        return;
    }

    let manifest_content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to read bundled agents manifest");
            return;
        }
    };

    let manifest: BundledManifest = match serde_json::from_str(&manifest_content) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to parse bundled agents manifest");
            return;
        }
    };

    for entry in &manifest.bundled_agents {
        if !entry.auto_install {
            continue;
        }

        match repo.get(&entry.name) {
            Ok(Some(_)) => {
                tracing::debug!(agent = %entry.name, "bundled agent already installed");
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(agent = %entry.name, error = %e, "failed to check bundled agent in repository");
                continue;
            }
        }

        let source_path = bundled_dir.join(&entry.file);
        info!(agent = %entry.name, "installing bundled agent");

        let agent_manifest = match loader.load_and_validate(&source_path) {
            Ok(m) => m,
            Err(e) => {
                warn!(agent = %entry.name, error = %e, "failed to load bundled agent manifest");
                continue;
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let installed = apollia_tools::InstalledAgent {
            name: agent_manifest.name.clone(),
            version: agent_manifest.version.clone(),
            install_path: source_path.clone(),
            source_path,
            manifest: agent_manifest,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
        };

        if let Err(e) = repo.save(&installed) {
            warn!(agent = %entry.name, error = %e, "failed to persist bundled agent to repository");
            continue;
        }

        info!(agent = %entry.name, "bundled agent registered for auto-load");
    }
}

/// Phase 3: register the native tool descriptors and the static connector
/// descriptors (Google Workspace, future Microsoft 365). Connector descriptors
/// make the LLM aware of the tools regardless of whether an account is
/// connected; the matching executors are wired in by the desktop dispatcher.
pub(in crate::supervisor) async fn register_builtin_tools(
    tool_registry_handle: &ToolRegistryHandle,
) {
    for descriptor in native_tool_descriptors() {
        if let Err(e) = tool_registry_handle.register(descriptor).await {
            warn!(error = %e, "failed to register native tool");
        }
    }
    let connector_descriptor_count = crate::connectors_bridge::all_connector_descriptors().len();
    for descriptor in crate::connectors_bridge::all_connector_descriptors() {
        let tool_name = descriptor.name.clone();
        if let Err(e) = tool_registry_handle.register(descriptor).await {
            warn!(
                error = %e,
                tool = %tool_name,
                "failed to register connector tool descriptor"
            );
        }
    }
    // MCP resource tools (agent-initiative path). Advertised regardless of
    // whether any MCP server is connected; the matching executors are wired
    // per agent when an MCP handle exists.
    for descriptor in apollia_mcp::mcp_resources::mcp_resource_descriptors() {
        let tool_name = descriptor.name.clone();
        if let Err(e) = tool_registry_handle.register(descriptor).await {
            warn!(
                error = %e,
                tool = %tool_name,
                "failed to register MCP resource tool descriptor"
            );
        }
    }
    info!(
        connector_tools = connector_descriptor_count,
        "Supervisor: ToolRegistry ready (native + connector + MCP resource tools registered)"
    );
}

/// Phase 10.6: validate installed package integrity.
///
/// Checks that each package's `root_path` still exists on disk.
/// Missing packages → log warning, disable all package agents.
/// Never panics, never blocks boot.
pub(in crate::supervisor) fn validate_installed_packages(
    pkg_repo: &apollia_tools::PackageRepository,
    agent_repo: &AgentRepository,
) {
    let packages = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Phase 10.6: failed to list packages");
            return;
        }
    };
    for pkg in &packages {
        if !pkg.root_path.exists() {
            warn!(
                package = %pkg.name,
                path = %pkg.root_path.display(),
                "Phase 10.6: package root_path missing - disabling agents"
            );
            let agent_names = pkg_repo
                .list_agents_for_package(&pkg.name)
                .unwrap_or_default();
            for agent_name in &agent_names {
                if let Err(e) = agent_repo.set_enabled(agent_name, false) {
                    warn!(agent = %agent_name, error = %e, "Phase 10.6: failed to disable agent");
                }
            }
        }
    }
}

/// Returns descriptors for native tools bundled with `apollia-tools`.
///
/// Registers all 12 active native tools in the order: existing tools first,
/// then new atomic tools grouped by category.
pub(in crate::supervisor) fn native_tool_descriptors() -> Vec<apollia_tools::ToolDescriptor> {
    // `mut` is used only when at least one web-* feature is active; allow the
    // warning explicitly for minimal-feature builds.
    #[allow(unused_mut)]
    let mut descriptors = vec![
        apollia_tools::tools::bash_executor::BashExecutor::descriptor(),
        apollia_tools::tools::python_executor::PythonExecutor::descriptor(),
        apollia_tools::tools::file_read::FileRead::descriptor(),
        apollia_tools::tools::file_write::FileWrite::descriptor(),
        apollia_tools::tools::file_edit::FileEdit::descriptor(),
        apollia_tools::tools::file_list::FileList::descriptor(),
        apollia_tools::tools::file_glob::FileGlob::descriptor(),
        apollia_tools::tools::file_grep::FileGrep::descriptor(),
        apollia_tools::tools::http_fetch::HttpFetch::descriptor(),
        apollia_tools::tools::memory_search::MemorySearchTool::descriptor(),
        apollia_tools::tools::notebook_read::NotebookRead::descriptor(),
        apollia_tools::tools::notebook_edit::NotebookEdit::descriptor(),
        apollia_tools::tools::ask_user::AskUser::descriptor(),
        // Agent-driven permission governance.
        apollia_tools::tools::permission_rules::PermissionRuleAdd::descriptor(),
        apollia_tools::tools::permission_rules::PermissionRuleRemove::descriptor(),
        apollia_tools::tools::permission_rules::PermissionRuleList::descriptor(),
    ];

    // Web tools are always advertised in the catalogue (so the UI and agent
    // manifests can reference them) but runtime availability still depends on
    // `[tools].web_search` / `[tools].web_read` in `apollia.toml`.
    #[cfg(feature = "web-search")]
    descriptors.push(apollia_tools::tools::web_search::WebSearch::descriptor());

    #[cfg(feature = "web-read")]
    descriptors.push(apollia_tools::tools::web_read::WebRead::descriptor());

    descriptors
}
