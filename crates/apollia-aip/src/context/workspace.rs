//! The `WorkspaceContext` pyclass.
//!
//! Split out of `context.rs`: the runtime context stays in the parent, the
//! project view an agent reads through `ctx.workspace` lives here.

use pyo3::prelude::*;

/// Workspace context exposed to the Python agent via `ctx.workspace`.
///
/// Aggregates the sections produced by all active [`WorkspaceProvider`]s.
/// Readable from Python via `#[pymethods]`.
///
/// ## Python API
/// ```python
/// ctx.workspace.rules           # project rules content (APOLLIA.md)
/// ctx.workspace.apollia_md      # alias for rules (compatibility)
/// ctx.workspace.get("Git")      # content of a section by title
/// ctx.workspace.sections        # list of dicts {"title": ..., "content": ...}
/// ```
#[pyclass(name = "WorkspaceContext")]
pub struct WorkspaceContextPy {
    /// Flattened sections from all providers: (title, content).
    pub sections: Vec<(String, String)>,
}
impl WorkspaceContextPy {
    /// Builds from a [`WorkspaceSnapshot`] collected by `ProjectRuntime`.
    pub fn from_snapshot(snapshot: &apollia_workspace::WorkspaceSnapshot) -> Self {
        let sections = snapshot
            .slices
            .iter()
            .flat_map(|s| &s.sections)
            .map(|s| (s.title.clone(), s.content.clone()))
            .collect();
        Self { sections }
    }

    /// Builds an empty context (no sections).
    pub fn empty() -> Self {
        Self { sections: vec![] }
    }

    /// Inserts or replaces a section's content by its title.
    ///
    /// Used by the bridge to patch project rules after asynchronous collection.
    pub fn set_section(&mut self, title: &str, content: String) {
        if let Some(existing) = self.sections.iter_mut().find(|(t, _)| t == title) {
            existing.1 = content;
        } else {
            self.sections.push((title.to_owned(), content));
        }
    }

    /// Returns a section's content by title (Rust-side lookup).
    pub fn get_section_content(&self, title: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(t, _)| t == title)
            .map(|(_, c)| c.as_str())
    }
}
#[pymethods]
impl WorkspaceContextPy {
    /// Project rules content (section "Project rules"), or `None` if absent.
    #[getter]
    pub(crate) fn rules(&self) -> Option<&str> {
        self.get_section_content("Project rules")
    }

    /// Alias for `rules`, for compatibility with existing agents.
    #[getter]
    pub(crate) fn apollia_md(&self) -> Option<&str> {
        self.rules()
    }

    /// Returns a section's content by its title, or `None` if not found.
    ///
    /// ```python
    /// git_info = ctx.workspace.get("Git")
    /// jira_tickets = ctx.workspace.get("Jira")
    /// ```
    pub(crate) fn get(&self, title: &str) -> Option<&str> {
        self.get_section_content(title)
    }

    /// All sections as a list of dicts `{"title": ..., "content": ...}`.
    #[getter]
    pub(crate) fn sections<'py>(
        &self,
        py: pyo3::Python<'py>,
    ) -> Vec<pyo3::Bound<'py, pyo3::types::PyDict>> {
        use pyo3::types::PyDict;
        self.sections
            .iter()
            .map(|(title, content)| {
                let d = PyDict::new(py);
                let _ = d.set_item("title", title);
                let _ = d.set_item("content", content);
                d
            })
            .collect()
    }
}
