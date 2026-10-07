use super::*;

/// Phase 3: register the native tool descriptors, the tools of every native
/// connector account in `accounts` (one set per account, see
/// [`crate::connectors_bridge::connection_tools`]) and, when `mailbox_tools`
/// says so, the mailbox tools over IMAP. A provider with no connected account
/// publishes no tool; an account connected later registers its own when it
/// connects.
pub(crate) async fn register_builtin_tools(
    tool_registry_handle: &ToolRegistryHandle,
    accounts: &[crate::connections::NativeAccounts],
    mailbox_tools: bool,
) {
    for descriptor in native_tool_descriptors() {
        if let Err(e) = tool_registry_handle.register(descriptor).await {
            warn!(error = %e, "tool.native.register.failed");
        }
    }
    let mut connector_descriptors = crate::connectors_bridge::connector_descriptors_for(accounts);
    if mailbox_tools {
        connector_descriptors.extend(crate::connectors_bridge::imap_tool_descriptors());
    }
    let connector_descriptor_count = connector_descriptors.len();
    for descriptor in connector_descriptors {
        let tool_name = descriptor.name.clone();
        if let Err(e) = tool_registry_handle.register(descriptor).await {
            warn!(
                error = %e,
                tool = %tool_name,
                "tool.connector.register.failed"
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
                "tool.mcp_resource.register.failed"
            );
        }
    }
    info!(
        connector_tools = connector_descriptor_count,
        "supervisor.tool_registry.ready"
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
            warn!(error = %e, "package.list.failed");
            return;
        }
    };
    for pkg in &packages {
        if !pkg.root_path.exists() {
            warn!(
                package = %pkg.name,
                path = %pkg.root_path.display(),
                detail = "disabling its agents", "package.root.missing"
            );
            let agent_names = pkg_repo
                .list_agents_for_package(&pkg.name)
                .unwrap_or_default();
            for agent_name in &agent_names {
                if let Err(e) = agent_repo.set_enabled(agent_name, false) {
                    warn!(agent = %agent_name, error = %e, "agent.disable.failed");
                }
            }
        }
    }
}

/// Returns descriptors for native tools bundled with `apollia-tools`.
///
/// Registers all 14 active native tools in the order: existing tools first,
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
        apollia_tools::tools::source_search::SourceSearchTool::descriptor(),
        apollia_tools::tools::notebook_read::NotebookRead::descriptor(),
        apollia_tools::tools::notebook_edit::NotebookEdit::descriptor(),
        apollia_tools::tools::ask_user::AskUser::descriptor(),
        apollia_tools::CodeContext::descriptor(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::{AgentAccess, AgentRepository, APOLLIA_AGENT_ID};

    // GIVEN a fresh install: the built-in tools registered, no MCP server
    //   connected, Apollia stored without any override
    // WHEN the access of Apollia is read, then turned into tools
    // THEN Apollia is offered exactly the reading family: reading the files
    //   and notebooks granted, the MCP resources, the web search and the page
    //   reading; nothing that writes, sends, runs code or reads an account
    #[tokio::test]
    async fn a_fresh_install_gives_apollia_the_reading_family_only() {
        let registry = ToolRegistryHandle::start();
        register_builtin_tools(&registry, &[], true).await;
        let dir = tempfile::tempdir().unwrap();
        let repo = AgentRepository::open(&dir.path().join("agents.db")).unwrap();
        let apollia = repo
            .resolved(
                APOLLIA_AGENT_ID,
                None,
                apollia_core::AutonomyLevel::Assisted,
            )
            .unwrap()
            .unwrap();

        #[allow(unused_mut)]
        let mut expected = vec![
            "file_glob",
            "file_grep",
            "file_list",
            "file_read",
            "mcp_resources_list",
            "mcp_resources_read",
            "notebook_read",
            "source_search",
        ];
        #[cfg(feature = "web-read")]
        expected.push("web_read");
        #[cfg(feature = "web-search")]
        expected.push("web_search");

        let tools = AgentAccess::of(&apollia)
            .tools(&registry, &apollia_tools::DefaultAccess::default())
            .await;

        assert_eq!(tools, expected);
        registry.shutdown().await;
    }

    #[test]
    fn every_native_tool_designates_its_target_argument_or_none() {
        // GIVEN the native tools, and the argument each one acts on, copied here
        // by hand: a path, a folder or an address, never a content
        #[allow(unused_mut)]
        let mut expected: Vec<(&str, Option<&str>)> = vec![
            ("bash_executor", None),
            ("python_executor", None),
            ("file_read", Some("path")),
            ("file_write", Some("path")),
            ("file_edit", Some("path")),
            ("file_list", Some("dir")),
            ("file_glob", Some("path")),
            ("file_grep", Some("path")),
            ("http_fetch", Some("url")),
            ("memory_search", None),
            ("source_search", None),
            ("notebook_read", Some("path")),
            ("notebook_edit", Some("path")),
            ("ask_user", None),
            ("code_context", Some("folder")),
        ];
        #[cfg(feature = "web-search")]
        expected.push(("web_search", None));
        #[cfg(feature = "web-read")]
        expected.push(("web_read", Some("url")));

        // WHEN the descriptors registered at boot are read
        let descriptors = native_tool_descriptors();
        let actual: Vec<(&str, Option<&str>)> = descriptors
            .iter()
            .map(|d| (d.name.as_str(), d.target_arg.as_deref()))
            .collect();

        // THEN each designates exactly the argument of the table, and that
        // argument is one its input schema declares
        assert_eq!(actual, expected);
        for d in &descriptors {
            if let Some(arg) = d.target_arg.as_deref() {
                assert!(
                    d.input_schema["properties"].get(arg).is_some(),
                    "{} designates `{arg}`, which its schema does not declare",
                    d.name
                );
            }
        }
    }
}
