//! ctx.templates: runtime Jinja2 template rendering.
//!
//! Uses [`minijinja`] (Apache-2.0, ~70 KB) to render the Jinja2 templates
//! declared in `@agent(templates=...)`. Files are loaded from
//! `<agent_dir>/templates/<name>.j2` (with `.jinja2` / `.jinja` fallbacks) at
//! agent startup and compiled once.
//!
//! The context is passed from Python as a `dict`, then converted into a
//! `serde_json::Value` via `json.dumps` (avoiding the pythonize dependency).
//! Minijinja consumes `serde_json::Value` directly as context.

use std::path::Path;

use minijinja::Environment;
use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Read-only interface exposed to the agent via `ctx.templates`.
///
/// Built with the declared list. [`Self::load_from_dir`] compiles and stores
/// each template in a shared [`Environment`]. The agent calls
/// `ctx.templates.render("name", **context)` to get the rendered string.
#[pyclass(name = "TemplatesInterface", module = "apollia._native")]
pub struct TemplatesInterface {
    /// Minijinja environment holding all compiled templates.
    /// `'static` because the sources are owned (`add_template_owned`).
    env: Environment<'static>,
    /// List of templates allowed by the manifest. Any key not present
    /// triggers `FileNotFoundError`.
    declared: Vec<String>,
}

#[pymethods]
impl TemplatesInterface {
    /// Renders the template `name` with the context provided as kwargs.
    ///
    /// The Python context is serialized to JSON then deserialized into a
    /// `serde_json::Value` so minijinja can consume it directly.
    ///
    /// # Python example
    /// ```python
    /// prompt = ctx.templates.render("system_prompt", role="analyst", tools=["search"])
    /// ```
    ///
    /// # Python errors
    /// - `FileNotFoundError` if `name` is not declared in the manifest or
    ///   not loaded in memory.
    /// - `RuntimeError` on a Jinja render error (missing variable, syntax
    ///   error, etc.).
    #[pyo3(signature = (name, **context))]
    fn render(
        &self,
        py: Python<'_>,
        name: &str,
        context: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        if !self.declared.iter().any(|d| d == name) {
            return Err(PyFileNotFoundError::new_err(format!(
                "Template '{name}' not declared in @agent(templates=...)"
            )));
        }

        // Convert the Python context to serde_json::Value via json.dumps.
        let ctx_value: serde_json::Value = match context {
            Some(d) if !d.is_empty() => {
                let json_mod = py.import("json")?;
                let json_str: String =
                    json_mod
                        .call_method1("dumps", (d,))?
                        .extract()
                        .map_err(|e| {
                            PyRuntimeError::new_err(format!("context serialization failed: {e}"))
                        })?;
                serde_json::from_str(&json_str)
                    .map_err(|e| PyRuntimeError::new_err(format!("context parse failed: {e}")))?
            }
            _ => serde_json::Value::Object(serde_json::Map::new()),
        };

        let tmpl = self.env.get_template(name).map_err(|e| {
            PyFileNotFoundError::new_err(format!("Template '{name}' not loaded: {e}"))
        })?;
        tmpl.render(&ctx_value)
            .map_err(|e| PyRuntimeError::new_err(format!("template '{name}' render failed: {e}")))
    }

    /// Lists the logical names of the templates declared in the manifest.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }

    /// `True` if the template is declared AND successfully loaded in memory.
    fn has(&self, name: &str) -> bool {
        self.declared.iter().any(|d| d == name) && self.env.get_template(name).is_ok()
    }
}

impl TemplatesInterface {
    /// Builds the interface with the manifest's declared list.
    pub fn new(declared: Vec<String>) -> Self {
        Self {
            env: Environment::new(),
            declared,
        }
    }

    /// Loads and compiles all declared templates from
    /// `<agent_dir>/templates/<name>.{j2,jinja2,jinja}`.
    ///
    /// Compile errors are logged (`warn!`) but not fatal: the agent gets a
    /// `FileNotFoundError` on the first `render()` of the failing template
    /// (error visibility when the agent actually tries to use it).
    ///
    /// Returns the number of templates loaded successfully.
    pub fn load_from_dir(&mut self, agent_dir: &Path) -> usize {
        let dir = agent_dir.join("templates");
        let mut loaded = 0usize;
        let extensions = ["j2", "jinja2", "jinja"];
        for name in self.declared.clone() {
            let mut content_opt: Option<String> = None;
            for ext in &extensions {
                let path = dir.join(format!("{name}.{ext}"));
                if let Ok(c) = std::fs::read_to_string(&path) {
                    content_opt = Some(c);
                    break;
                }
            }
            let Some(content) = content_opt else {
                tracing::warn!(
                    target: "apollia.aip.templates",
                    name = %name,
                    dir = %dir.display(),
                    "aip.template.file.missing"
                );
                continue;
            };
            match self.env.add_template_owned(name.clone(), content) {
                Ok(()) => loaded += 1,
                Err(e) => {
                    tracing::warn!(
                        target: "apollia.aip.templates",
                        name = %name,
                        error = %e,
                        "aip.template.compile.failed"
                    );
                }
            }
        }
        loaded
    }

    /// Injects a template source directly (unit tests).
    #[cfg(test)]
    pub(crate) fn inject(&mut self, name: &str, source: &str) {
        if !self.declared.iter().any(|d| d == name) {
            self.declared.push(name.to_string());
        }
        self.env
            .add_template_owned(name.to_string(), source.to_string())
            .expect("test template should compile");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_undeclared_returns_file_not_found() {
        // GIVEN a templates interface that declares nothing
        let t = TemplatesInterface::new(vec![]);
        Python::with_gil(|py| {
            // WHEN an undeclared template is rendered
            let res = t.render(py, "system_prompt", None);
            // THEN the render raises, and the error says the template was not declared
            assert!(res.is_err(), "expected FileNotFoundError");
            assert!(
                format!("{}", res.expect_err("err")).contains("not declared"),
                "expected 'not declared' in error"
            );
        });
    }

    #[test]
    fn test_render_declared_but_not_loaded() {
        // GIVEN a declared template that was never loaded from disk
        let t = TemplatesInterface::new(vec!["system_prompt".to_string()]);
        Python::with_gil(|py| {
            // WHEN it is rendered
            let res = t.render(py, "system_prompt", None);
            // THEN the render raises rather than returning an empty string
            assert!(res.is_err(), "expected FileNotFoundError (not loaded)");
        });
    }

    #[test]
    fn test_render_with_context_dict() {
        // GIVEN a template with a {{ name }} variable
        let mut t = TemplatesInterface::new(vec![]);
        t.inject("greeting", "Hello, {{ name }}!");

        // WHEN we render with name=World
        Python::with_gil(|py| {
            let ctx = PyDict::new(py);
            ctx.set_item("name", "World").expect("set");
            let out = t
                .render(py, "greeting", Some(&ctx))
                .expect("render should succeed");
            // THEN the variable is substituted in the output
            assert_eq!(out, "Hello, World!");
        });
    }

    #[test]
    fn test_render_without_context() {
        // GIVEN a template that doesn't need context
        let mut t = TemplatesInterface::new(vec![]);
        t.inject("static", "constant output");
        Python::with_gil(|py| {
            // WHEN it is rendered with no context at all
            let out = t.render(py, "static", None).expect("render");
            // THEN the constant body comes back unchanged
            assert_eq!(out, "constant output");
        });
    }

    /// Checks the production path: `load_from_dir` reads a real `.j2` file
    /// from `<agent_dir>/templates/<name>.j2` and renders it correctly with a
    /// Python context.
    #[test]
    fn test_load_from_dir_compiles_real_jinja() {
        // GIVEN a temp agent_dir containing templates/report.j2
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir templates");
        std::fs::write(
            tpl_dir.join("report.j2"),
            "Total: {{ count }} ({{ status }})",
        )
        .expect("write template");

        // WHEN we load via the production path
        let mut iface = TemplatesInterface::new(vec!["report".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN the template was compiled
        assert_eq!(loaded, 1);
        assert!(iface.has("report"));

        // AND it renders with a Python context
        Python::with_gil(|py| {
            let ctx = PyDict::new(py);
            ctx.set_item("count", 42).expect("set count");
            ctx.set_item("status", "ok").expect("set status");
            let rendered = iface
                .render(py, "report", Some(&ctx))
                .expect("render should succeed after load_from_dir");
            assert_eq!(rendered, "Total: 42 (ok)");
        });
    }

    /// Checks the `.jinja2` and `.jinja` extension fallbacks.
    #[test]
    fn test_load_from_dir_extension_fallbacks() {
        // GIVEN a templates directory holding one `.jinja2` file and one `.jinja` file
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir");
        std::fs::write(tpl_dir.join("a.jinja2"), "A={{ v }}").expect("a");
        std::fs::write(tpl_dir.join("b.jinja"), "B={{ v }}").expect("b");

        let mut iface = TemplatesInterface::new(vec!["a".to_string(), "b".to_string()]);
        // WHEN the directory is loaded
        let loaded = iface.load_from_dir(tmp.path());

        // THEN both extensions are picked up
        assert_eq!(loaded, 2);
        assert!(iface.has("a"));
        assert!(iface.has("b"));
    }

    /// A missing template does not prevent the others from compiling.
    #[test]
    fn test_load_from_dir_missing_template_is_non_fatal() {
        // GIVEN two declared templates, only one of which exists on disk
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir");
        std::fs::write(tpl_dir.join("present.j2"), "ok").expect("write");

        let mut iface = TemplatesInterface::new(vec!["present".to_string(), "missing".to_string()]);
        // WHEN the directory is loaded
        let loaded = iface.load_from_dir(tmp.path());

        // THEN the present one loads, and the absent one raises on render instead of aborting the load
        assert_eq!(loaded, 1);
        assert!(iface.has("present"));
        assert!(!iface.has("missing"));

        Python::with_gil(|py| {
            let err = iface
                .render(py, "missing", None)
                .expect_err("missing template should raise");
            assert!(err.is_instance_of::<PyFileNotFoundError>(py));
        });
    }

    /// A syntactically invalid template is skipped (warn!), not a crash.
    #[test]
    fn test_load_from_dir_invalid_template_is_non_fatal() {
        // GIVEN a declared template whose body does not compile
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir");
        // `{% if` not closed => compile error
        std::fs::write(tpl_dir.join("broken.j2"), "{% if x").expect("write");

        let mut iface = TemplatesInterface::new(vec!["broken".to_string()]);
        // WHEN the directory is loaded
        let loaded = iface.load_from_dir(tmp.path());

        // THEN nothing loads and the compile error stays non-fatal
        assert_eq!(loaded, 0);
        assert!(!iface.has("broken"));
    }

    #[test]
    fn test_has() {
        // GIVEN a declared template whose body has not been injected yet
        let mut t = TemplatesInterface::new(vec!["foo".to_string()]);
        assert!(!t.has("foo"));
        // WHEN the body is injected
        t.inject("foo", "hi");
        // THEN has() flips to true for it, and stays false for anything undeclared
        assert!(t.has("foo"));
        assert!(!t.has("bar"));
    }
}
