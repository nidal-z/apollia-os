//! Default implementation of [`ProjectContextProvider`].
//!
//! Reads project instructions, document contents, and workspace provider
//! snapshots from [`ProjectRepository`] to build a context block injected
//! into chat system prompts when a session belongs to a project.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_tools::ProjectRepository;

use super::types::ProjectContextProvider;

/// Default project context provider backed by [`ProjectRepository`].
///
/// Loads project details, reads document contents, and runs configured
/// workspace providers to build a complete context block.
pub struct DefaultProjectContextProvider {
    repo: Arc<ProjectRepository>,
}

impl DefaultProjectContextProvider {
    /// Create a new provider backed by the given project repository.
    pub fn new(repo: Arc<ProjectRepository>) -> Self {
        Self { repo }
    }
}

/// Maximum bytes to read from a single attached document.
const MAX_DOCUMENT_BYTES: usize = 10_000;

#[async_trait::async_trait]
impl ProjectContextProvider for DefaultProjectContextProvider {
    async fn build_context(&self, project_id: &str) -> Option<String> {
        let repo = self.repo.clone();
        let pid = project_id.to_string();

        let detail = tokio::task::spawn_blocking(move || repo.get_project(&pid))
            .await
            .ok()?
            .ok()?;

        let mut block = String::from("## Project Context\n");
        let mut has_content = false;

        // 1. Project instructions
        if let Some(ref instructions) = detail.instructions {
            if !instructions.is_empty() {
                block.push_str("\n### Project Instructions\n");
                block.push_str(instructions);
                block.push('\n');
                has_content = true;
            }
        }

        // 2. Attached document contents (read from disk, truncated)
        for doc in &detail.documents {
            match tokio::fs::read_to_string(&doc.file_path).await {
                Ok(content) => {
                    let truncated = if content.len() > MAX_DOCUMENT_BYTES {
                        let mut s = content[..MAX_DOCUMENT_BYTES].to_string();
                        s.push_str("\n...[truncated]");
                        s
                    } else {
                        content
                    };
                    block.push_str(&format!("\n### Document: {}\n{}\n", doc.name, truncated));
                    has_content = true;
                }
                Err(_) => {
                    // File may have been moved or deleted — skip silently.
                }
            }
        }

        // 3. Workspace snapshot from configured providers (git, rules, tree, etc.)
        let enabled_providers: Vec<_> = detail
            .providers
            .iter()
            .filter(|p| p.enabled)
            .collect();

        if !enabled_providers.is_empty() {
            // Determine the workspace directory: use project's workspace_path,
            // or fall back to the process cwd.
            let cwd = detail
                .workspace_path
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"))
                });

            let entries: Vec<apollia_workspace::ProviderEntry> = enabled_providers
                .iter()
                .map(|p| apollia_workspace::ProviderEntry {
                    provider_type: p.provider_type.clone(),
                    name: p.name.clone(),
                    config_json: if p.config_json == "{}" {
                        None
                    } else {
                        Some(p.config_json.clone())
                    },
                    path: p.path.clone(),
                    enabled: p.enabled,
                    priority: p.priority,
                })
                .collect();

            let runtime =
                apollia_workspace::ProjectRuntime::from_providers_config(&entries, None);
            let snapshot = runtime.collect(&cwd).await;

            for slice in &snapshot.slices {
                for section in &slice.sections {
                    if !section.content.is_empty() {
                        block.push_str(&format!(
                            "\n### {}\n{}\n",
                            section.title, section.content
                        ));
                        has_content = true;
                    }
                }
            }
        }

        if has_content {
            Some(block)
        } else {
            None
        }
    }
}
