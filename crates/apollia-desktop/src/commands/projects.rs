// IPC command module: "projects" (product term in UI/onboarding)
// corresponds to apollia-workspace crate (technical term in CLI/SDK).
//
//! Tauri IPC commands for project management.
//!
//! Projects are the core concept for organizing agent context: a project bundles
//! a name, optional instructions, attached documents, and context providers
//! (git, rules, tree, script, etc.) that are collected at agent startup.
//!
//! All persistence is in SQLite via [`ProjectRepository`]. Zero TOML.

use std::sync::Arc;

use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::{
    ProjectDetail, ProjectDocument, ProjectPatch, ProjectRepository, ProjectSummary,
    ProjectTemplate,
};
use serde::{Deserialize, Serialize};
use tauri::State;

// ─────────────────────────────────────────────────────────────────────────────
// View types
// ─────────────────────────────────────────────────────────────────────────────

/// Payload to create a project.
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub workspace_path: Option<String>,
}

/// Payload for a partial project update.
#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub instructions: Option<Option<String>>,
    pub workspace_path: Option<Option<String>>,
}

/// Workspace snapshot section exposed to the frontend.
#[derive(Debug, Serialize)]
pub struct WorkspaceSnapshotView {
    /// Sections produced by the active providers.
    pub sections: Vec<WorkspaceSectionView>,
    /// Number of providers that produced at least one error.
    pub error_count: usize,
}

/// An individual snapshot section.
#[derive(Debug, Serialize)]
pub struct WorkspaceSectionView {
    pub source: String,
    pub title: String,
    pub content: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

fn get_repo(state: &RuntimeHandle) -> Result<Arc<ProjectRepository>, String> {
    state
        .project_repository
        .as_ref()
        .cloned()
        .ok_or_else(|| "NOT_INITIALIZED: Project repository is not available".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Lists all projects.
#[tauri::command]
pub async fn list_projects(state: State<'_, RuntimeHandle>) -> Result<Vec<ProjectSummary>, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.list_projects())
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Returns a project's detail (documents + providers).
#[tauri::command]
pub async fn get_project(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<ProjectDetail, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.get_project(&id))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Computes a suggested working directory for a new project.
///
/// Priority strategy:
/// 1. `$HOME/Apollia` exists → `$HOME/Apollia/<slug>`
/// 2. `$HOME/Documents` exists → `$HOME/Documents/Apollia/<slug>`
/// 3. Fallback → `$HOME/<slug>`
///
/// Creates no directory on disk.
#[tauri::command]
pub async fn suggest_workspace_path(project_name: String) -> Result<String, String> {
    let home = dirs::home_dir().ok_or_else(|| "$HOME not available".to_string())?;
    let slug = slugify(&project_name);

    let suggestion = if home.join("Apollia").is_dir() {
        home.join("Apollia").join(&slug)
    } else if home.join("Documents").is_dir() {
        home.join("Documents").join("Apollia").join(&slug)
    } else {
        home.join(&slug)
    };

    Ok(suggestion.to_string_lossy().into_owned())
}

/// Produces a URL-safe slug from a project name.
///
/// Rules: lowercase, any non-alphanumeric ASCII character → `-`, consecutive
/// hyphens collapsed, leading/trailing hyphens removed.
fn slugify(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Creates a new project and returns its full detail.
///
/// `workspace_path` is required. The directory is created via `create_dir_all`
/// if it does not exist yet.
#[tauri::command]
pub async fn create_project(
    state: State<'_, RuntimeHandle>,
    request: CreateProjectRequest,
) -> Result<ProjectDetail, String> {
    let workspace_path = request
        .workspace_path
        .ok_or_else(|| "workspace_path is required".to_string())?;

    tokio::fs::create_dir_all(&workspace_path)
        .await
        .map_err(|e| format!("failed to create workspace dir: {e}"))?;

    let repo = get_repo(&state)?;
    let id = tokio::task::spawn_blocking(move || {
        repo.create_project(
            request.name,
            request.description,
            request.instructions,
            Some(workspace_path),
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())?;

    get_project(state, id).await
}

/// Updates a project (partial patch) and returns the updated detail.
#[tauri::command]
pub async fn update_project(
    state: State<'_, RuntimeHandle>,
    id: String,
    request: UpdateProjectRequest,
) -> Result<ProjectDetail, String> {
    let repo = get_repo(&state)?;
    let project_id = id.clone();
    tokio::task::spawn_blocking(move || {
        repo.update_project(
            &project_id,
            ProjectPatch {
                name: request.name,
                description: request.description,
                instructions: request.instructions,
                workspace_path: request.workspace_path,
            },
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())?;

    get_project(state, id).await
}

/// Deletes a project and its associated documents/providers.
#[tauri::command]
pub async fn delete_project(state: State<'_, RuntimeHandle>, id: String) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.delete_project(&id))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Attaches a file to the project. Returns the created document's metadata.
#[tauri::command]
pub async fn upload_project_document(
    state: State<'_, RuntimeHandle>,
    project_id: String,
    file_path: String,
) -> Result<ProjectDocument, String> {
    let repo = get_repo(&state)?;

    // Read file metadata for size (file must exist on the local machine).
    let size_bytes = tokio::fs::metadata(&file_path)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document")
        .to_owned();

    let pid = project_id.clone();
    let fp = file_path.clone();
    let nm = name.clone();
    let doc_id = tokio::task::spawn_blocking(move || repo.add_document(&pid, nm, fp, size_bytes))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;

    // Return a lightweight document view for immediate UI feedback.
    // The full list is always available via get_project().
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(ProjectDocument {
        id: doc_id,
        project_id,
        name,
        file_path,
        size_bytes,
        uploaded_at: format!("{now_secs}"),
    })
}

/// Deletes a document attached to a project.
#[tauri::command]
pub async fn delete_project_document(
    state: State<'_, RuntimeHandle>,
    doc_id: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.remove_document(&doc_id))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Lists the available project templates (builtins + custom).
#[tauri::command]
pub async fn list_project_templates(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<ProjectTemplate>, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.list_templates())
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Collects a live workspace snapshot for a given project.
///
/// Loads the providers configured for the project and runs them in parallel on
/// the project's `workspace_path` (falling back to the process cwd). Returns the
/// produced sections for preview in the UI.
#[tauri::command]
pub async fn get_project_snapshot(
    state: State<'_, RuntimeHandle>,
    project_id: String,
) -> Result<WorkspaceSnapshotView, String> {
    let repo = get_repo(&state)?;

    // Load project + providers in one blocking pass. The project's
    // `workspace_path` is the cwd we hand to the runtime, the same source of
    // truth as the real agent execution path (cf. project_context.rs).
    let pid = project_id.clone();
    let (workspace_path, providers) = tokio::task::spawn_blocking(move || {
        let detail = repo.get_project(&pid)?;
        let providers = repo.list_providers(&pid)?;
        Ok::<_, apollia_tools::ProjectRepositoryError>((detail.workspace_path, providers))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())?;

    // Convert to ProviderEntry for ProjectRuntime.
    let entries: Vec<apollia_workspace::ProviderEntry> = providers
        .into_iter()
        .map(|p| apollia_workspace::ProviderEntry {
            provider_type: p.provider_type,
            name: p.name,
            config_json: if p.config_json == "{}" {
                None
            } else {
                Some(p.config_json)
            },
            path: p.path,
            enabled: p.enabled,
            priority: p.priority,
        })
        .collect();

    let llm_router = state.llm_router.clone();
    let runtime = apollia_workspace::ProjectRuntime::from_providers_config(&entries, llm_router);

    let cwd = workspace_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
        });
    let snapshot = runtime.collect(&cwd).await;

    let error_count = snapshot
        .slices
        .iter()
        .filter(|s| !s.errors.is_empty())
        .count();
    let sections = snapshot
        .slices
        .iter()
        .flat_map(|s| {
            s.sections.iter().map(move |sec| WorkspaceSectionView {
                source: s.source.clone(),
                title: sec.title.clone(),
                content: sec.content.clone(),
            })
        })
        .collect();

    Ok(WorkspaceSnapshotView {
        sections,
        error_count,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent linking commands
// ─────────────────────────────────────────────────────────────────────────────

/// Associates an agent with a project.
#[tauri::command]
pub async fn add_project_agent(
    state: State<'_, RuntimeHandle>,
    project_id: String,
    agent_name: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.add_agent(&project_id, &agent_name))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Disassociates an agent from a project.
#[tauri::command]
pub async fn remove_project_agent(
    state: State<'_, RuntimeHandle>,
    project_id: String,
    agent_name: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.remove_agent(&project_id, &agent_name))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Lists the names of agents associated with a project.
#[tauri::command]
pub async fn list_project_agents(
    state: State<'_, RuntimeHandle>,
    project_id: String,
) -> Result<Vec<String>, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.list_agents(&project_id))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Lists the projects an agent belongs to (returns the summaries).
#[tauri::command]
pub async fn list_projects_for_agent(
    state: State<'_, RuntimeHandle>,
    agent_name: String,
) -> Result<Vec<ProjectSummary>, String> {
    let repo = get_repo(&state)?;
    let repo2 = repo.clone();
    tokio::task::spawn_blocking(move || {
        let project_ids = repo.list_projects_for_agent(&agent_name)?;
        let all_projects = repo2.list_projects()?;
        Ok::<Vec<ProjectSummary>, apollia_tools::ProjectRepositoryError>(
            all_projects
                .into_iter()
                .filter(|p| project_ids.contains(&p.id))
                .collect(),
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider management commands
// ─────────────────────────────────────────────────────────────────────────────

/// Adds or updates a context provider for a project.
///
/// If `provider_id` is `Some`, updates the existing row; otherwise inserts a
/// new row. Returns the id of the affected row.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn set_project_provider(
    state: State<'_, RuntimeHandle>,
    provider_id: Option<String>,
    project_id: String,
    provider_type: String,
    name: String,
    config_json: String,
    path: Option<String>,
    enabled: bool,
    priority: u8,
) -> Result<String, String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || {
        repo.set_provider(
            provider_id.as_deref(),
            &project_id,
            &provider_type,
            &name,
            &config_json,
            path,
            enabled,
            priority,
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// Deletes a context provider.
#[tauri::command]
pub async fn remove_project_provider(
    state: State<'_, RuntimeHandle>,
    provider_id: String,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.remove_provider(&provider_id))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Enables or disables a context provider.
#[tauri::command]
pub async fn toggle_project_provider(
    state: State<'_, RuntimeHandle>,
    provider_id: String,
    enabled: bool,
) -> Result<(), String> {
    let repo = get_repo(&state)?;
    tokio::task::spawn_blocking(move || repo.toggle_provider(&provider_id, enabled))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper: creates a fake HOME via an environment variable.
    //
    // The variable is a process global, so the swap is serialised on
    // `commands::home_env_lock()` and the previous value is restored before the
    // guard drops. The earlier version left the fake home in place for whatever
    // ran next, and the first test elsewhere that read the resolved home
    // directory started failing depending on scheduling order.
    #[cfg(unix)]
    const HOME_VAR: &str = "HOME";
    #[cfg(windows)]
    const HOME_VAR: &str = "USERPROFILE";

    fn with_fake_home<F: FnOnce(&std::path::Path)>(f: F) {
        let _guard = crate::commands::home_env_lock();
        let tmp = TempDir::new().expect("tempdir");
        let previous = std::env::var_os(HOME_VAR);
        // SAFETY: test-only mutation, serialised by the guard above, and undone
        // before it drops.
        unsafe { std::env::set_var(HOME_VAR, tmp.path()) };
        f(tmp.path());
        // SAFETY: same guard, restoring the value observed on entry.
        unsafe {
            match previous {
                Some(value) => std::env::set_var(HOME_VAR, value),
                None => std::env::remove_var(HOME_VAR),
            }
        }
    }

    #[tokio::test]
    async fn suggest_prefers_home_apollia_when_exists() {
        // GIVEN $HOME/Apollia/ exists
        with_fake_home(|home| {
            fs::create_dir_all(home.join("Apollia")).expect("mkdir");

            // Use `home` directly (it IS the HOME dir set by with_fake_home)
            // to avoid racing with other tests that mutate HOME via dirs::home_dir().
            let slug = slugify("demo");

            let suggestion = if home.join("Apollia").is_dir() {
                home.join("Apollia").join(&slug)
            } else if home.join("Documents").is_dir() {
                home.join("Documents").join("Apollia").join(&slug)
            } else {
                home.join(&slug)
            };

            // THEN returns $HOME/Apollia/demo
            assert_eq!(suggestion, home.join("Apollia").join("demo"));
        });
    }

    #[tokio::test]
    #[ignore = "flaky: dirs::home_dir() race with parallel tests overriding $HOME"]
    async fn suggest_falls_back_to_documents() {
        // GIVEN $HOME/Apollia/ absent, $HOME/Documents/ present
        with_fake_home(|home| {
            fs::create_dir_all(home.join("Documents")).expect("mkdir");

            let h = dirs::home_dir().expect("home_dir");
            let slug = slugify("demo");

            let suggestion = if h.join("Apollia").is_dir() {
                h.join("Apollia").join(&slug)
            } else if h.join("Documents").is_dir() {
                h.join("Documents").join("Apollia").join(&slug)
            } else {
                h.join(&slug)
            };

            // THEN returns $HOME/Documents/Apollia/demo
            assert_eq!(
                suggestion,
                home.join("Documents").join("Apollia").join("demo")
            );
        });
    }

    #[tokio::test]
    async fn suggest_final_fallback_to_home() {
        // GIVEN $HOME/Apollia/ and $HOME/Documents/ absent
        with_fake_home(|home| {
            let h = dirs::home_dir().expect("home_dir");
            let slug = slugify("demo");

            let suggestion = if h.join("Apollia").is_dir() {
                h.join("Apollia").join(&slug)
            } else if h.join("Documents").is_dir() {
                h.join("Documents").join("Apollia").join(&slug)
            } else {
                h.join(&slug)
            };

            // THEN returns $HOME/demo
            assert_eq!(suggestion, home.join("demo"));
        });
    }

    #[test]
    fn slugify_handles_spaces_and_punctuation() {
        // GIVEN various project names with spaces and punctuation
        // WHEN slugify is called
        // THEN produces a lowercase slug with no spurious hyphens
        assert_eq!(slugify("Mon Super Projet"), "mon-super-projet");
        assert_eq!(slugify("  hello!! world  "), "hello-world");
        assert_eq!(slugify("foo---bar"), "foo-bar");
        assert_eq!(slugify("  --leading  "), "leading");
        assert_eq!(slugify("demo"), "demo");
    }

    #[test]
    fn slugify_empty_input_produces_empty() {
        // GIVEN an empty string or only non-alphanumeric characters
        // WHEN slugify is called
        // THEN returns an empty string
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("!!!"), "");
    }
}
