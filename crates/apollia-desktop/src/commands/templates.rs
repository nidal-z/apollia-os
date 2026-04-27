//! Tauri IPC commands for the template gallery.
//!
//! The backend is intentionally thin: it reads `agents/templates/registry.toml`
//! (and the per-template body files) and returns structured data. "Instantiating"
//! a template means returning its body to the frontend, which then feeds the
//! existing creation wizards pre-filled. This matches the story requirement
//! that `templates_instantiate` never overwrites existing user artifacts.

use std::path::PathBuf;
use std::sync::OnceLock;

use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::templates::{Template, TemplateMeta, TemplateRegistry};
use serde::Serialize;
use tauri::State;

/// Return value of `templates_instantiate`.
///
/// The UI dispatches on `kind` to open the correct creation flow with the
/// body pre-loaded. `suggested_name` is what the caller should display as
/// the default name (suffixed with a timestamp when instantiating a copy).
#[derive(Debug, Serialize)]
pub struct TemplateInstantiation {
    /// Unique id of the source template (used for the "Based on…" badge).
    pub template_id: String,
    /// Artifact kind ("automation" | "agent" | "pipeline").
    pub kind: String,
    /// Name the UI should pre-fill (raw template name + `-copy-{timestamp}`).
    pub suggested_name: String,
    /// Raw TOML body — the UI parses it to drive the wizard fields.
    pub body: String,
}

/// Lazily-loaded registry. We keep the file-reading off the hot path of each
/// IPC call but avoid putting a `TemplateRegistry` field on `RuntimeHandle`
/// which would force a circular dependency between the desktop layer and
/// the embedded-runtime crate for a read-only cache.
fn registry() -> &'static TemplateRegistry {
    static REGISTRY: OnceLock<TemplateRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let root = locate_templates_root();
        TemplateRegistry::load_or_empty(root)
    })
}

/// Find `agents/templates/` relative to the running executable or the
/// current workspace. Dev builds sit in `target/*/`, packaged builds sit
/// inside the Tauri bundle — we probe the usual suspects.
fn locate_templates_root() -> PathBuf {
    let candidates = [
        // CWD/agents/templates — dev invocation from the workspace root.
        std::env::current_dir()
            .ok()
            .map(|d| d.join("agents").join("templates")),
        // Executable-adjacent — packaged app (`Contents/Resources/` on macOS,
        // `share/apollia-os/` on Linux, etc.).
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .map(|d| d.join("templates")),
        // Dev build — up from `target/{debug,release}/bundle/...` to the
        // workspace root.
        std::env::current_exe()
            .ok()
            .and_then(|p| p.ancestors().nth(3).map(|d| d.to_path_buf()))
            .map(|d| d.join("agents").join("templates")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.join("registry.toml").is_file() {
            return candidate;
        }
    }
    // Final fallback — returns a non-existent path, `load_or_empty` handles it.
    PathBuf::from("agents/templates")
}

/// List all templates declared in the registry. Cheap — only metadata,
/// no body loading.
#[tauri::command]
pub async fn templates_list(_state: State<'_, RuntimeHandle>) -> Result<Vec<TemplateMeta>, String> {
    Ok(registry().list().to_vec())
}

/// Fetch a single template with its raw body.
#[tauri::command]
pub async fn templates_get(
    _state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<Template, String> {
    registry().get(&id).map_err(|e| e.to_string())
}

/// Prepare the payload the UI needs to open the pre-filled creation flow.
///
/// This command is intentionally pure: it does NOT write to disk. The UI is
/// expected to feed `body` into the corresponding wizard (`CreateAutomation`,
/// agent creation wizard, or `CreatePipelineDialog`), which then persists
/// the new artifact through the regular create paths (with the copy-suffix
/// name, enforced here via `suggested_name`).
#[tauri::command]
pub async fn templates_instantiate(
    _state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<TemplateInstantiation, String> {
    let tmpl = registry().get(&id).map_err(|e| e.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let suggested_name = format!("{}-copy-{timestamp}", slugify(&tmpl.meta.title));
    let kind_str = match tmpl.meta.kind {
        apollia_runtime::templates::TemplateKind::Automation => "automation",
        apollia_runtime::templates::TemplateKind::Agent => "agent",
        apollia_runtime::templates::TemplateKind::Pipeline => "pipeline",
    }
    .to_string();
    Ok(TemplateInstantiation {
        template_id: tmpl.meta.id,
        kind: kind_str,
        suggested_name,
        body: tmpl.body,
    })
}

/// Minimal slugifier — ASCII lowercase, spaces and non-alphanumerics → `-`.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}
