//! Default implementation of [`ProjectContextProvider`].
//!
//! Reads project instructions, document contents, and workspace provider
//! snapshots from [`ProjectRepository`] to build a context block injected
//! into chat system prompts when a session belongs to a project.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_tools::ProjectRepository;
use tracing::{debug, info, warn};

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
        let pid_for_log = pid.clone();

        let detail = match tokio::task::spawn_blocking(move || repo.get_project(&pid)).await {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                warn!(project_id = %pid_for_log, error = %e, "project context: get_project failed");
                return None;
            }
            Err(e) => {
                warn!(project_id = %pid_for_log, error = %e, "project context: join failure");
                return None;
            }
        };

        debug!(
            project_id = %pid_for_log,
            documents = detail.documents.len(),
            providers_total = detail.providers.len(),
            providers_enabled = detail.providers.iter().filter(|p| p.enabled).count(),
            workspace_path = ?detail.workspace_path,
            "project context: building"
        );

        let mut block = String::from(
            "## Project Context\n\
             Le snapshot suivant a été collecté juste avant ce tour par les \
             providers du projet (git, rules, tree, etc.). Privilégie ces \
             informations à un appel d'outil pour répondre aux questions \
             factuelles auxquelles elles répondent déjà.\n",
        );
        let mut has_content = false;

        // 1. Project instructions
        has_content |= append_instructions(&mut block, &detail);

        // 2. Attached document contents (read from disk, truncated)
        has_content |= append_documents(&mut block, &detail).await;

        // 3. Workspace snapshot from configured providers (git, rules, tree, etc.)
        has_content |= append_workspace_snapshot(&mut block, &detail).await;

        if has_content {
            info!(
                project_id = %pid_for_log,
                context_bytes = block.len(),
                "project context: injected into system prompt"
            );
            Some(block)
        } else {
            warn!(
                project_id = %pid_for_log,
                "project context: built block is empty — \
                 no instructions, documents, or applicable provider sections; \
                 check provider configuration and workspace_path"
            );
            None
        }
    }
}

/// Append the project instructions section. Returns `true` when content was added.
fn append_instructions(block: &mut String, detail: &apollia_tools::ProjectDetail) -> bool {
    if let Some(ref instructions) = detail.instructions {
        if !instructions.is_empty() {
            block.push_str("\n### Project Instructions\n");
            block.push_str(instructions);
            block.push('\n');
            return true;
        }
    }
    false
}

/// Append attached document contents (read from disk, truncated). Returns
/// `true` when at least one document was added.
async fn append_documents(block: &mut String, detail: &apollia_tools::ProjectDetail) -> bool {
    let mut added = false;
    for doc in &detail.documents {
        let Ok(content) = tokio::fs::read_to_string(&doc.file_path).await else {
            // File may have been moved or deleted, skip silently.
            continue;
        };
        let truncated = if content.len() > MAX_DOCUMENT_BYTES {
            let mut s = content[..MAX_DOCUMENT_BYTES].to_string();
            s.push_str("\n...[truncated]");
            s
        } else {
            content
        };
        block.push_str(&format!("\n### Document: {}\n{}\n", doc.name, truncated));
        added = true;
    }
    added
}

/// Append the workspace snapshot collected by the project's configured
/// providers (git, rules, tree, etc.). Returns `true` when content was added.
async fn append_workspace_snapshot(
    block: &mut String,
    detail: &apollia_tools::ProjectDetail,
) -> bool {
    let enabled_providers: Vec<_> = detail.providers.iter().filter(|p| p.enabled).collect();
    if enabled_providers.is_empty() {
        return false;
    }

    // Determine the workspace directory: use project's workspace_path,
    // or fall back to the process cwd.
    let cwd = detail
        .workspace_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp")));

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

    let runtime = apollia_workspace::ProjectRuntime::from_providers_config(&entries, None);
    let snapshot = runtime.collect(&cwd).await;

    let mut added = false;
    for slice in &snapshot.slices {
        for section in &slice.sections {
            if !section.content.is_empty() {
                block.push_str(&format!("\n### {}\n{}\n", section.title, section.content));
                added = true;
            }
        }
    }
    added
}
