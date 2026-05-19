//! ctx.templates — runtime Jinja2 template rendering (LOT 4/5 — ADR-103).
//!
//! Utilise [`minijinja`] (Apache-2.0, ~70 KB) pour rendre les templates Jinja2
//! déclarés dans `@agent(templates=...)`. Les fichiers sont chargés depuis
//! `<agent_dir>/templates/<name>.j2` (avec fallback `.jinja2` / `.jinja`)
//! au démarrage de l'agent et compilés une seule fois.
//!
//! Le contexte est passé depuis Python sous forme de `dict` puis converti en
//! `serde_json::Value` via `json.dumps` (évite la dépendance pythonize).
//! Minijinja consomme directement `serde_json::Value` comme contexte.

use std::path::Path;

use minijinja::Environment;
use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Erreurs internes de la couche templates (exposées en `PyErr` via mapping).
#[derive(Debug, thiserror::Error)]
pub enum TemplatesError {
    /// Template non déclaré au manifest.
    #[error("template '{0}' not declared in @agent(templates=...)")]
    NotDeclared(String),
    /// Template déclaré mais introuvable / non compilé.
    #[error("template '{0}' not loaded (file missing)")]
    NotLoaded(String),
    /// Rendu Jinja a échoué.
    #[error("render failed for '{name}': {reason}")]
    RenderFailed {
        /// Nom logique du template.
        name: String,
        /// Détail minijinja.
        reason: String,
    },
}

/// Interface lecture-seule exposée à l'agent via `ctx.templates`.
///
/// Construit avec la liste déclarée. [`Self::load_from_dir`] compile et
/// stocke chaque template dans un [`Environment`] partagé. L'agent
/// appelle `ctx.templates.render("name", **context)` pour obtenir la chaîne
/// rendue.
#[pyclass(name = "TemplatesInterface", module = "apollia._native")]
pub struct TemplatesInterface {
    /// Environnement minijinja contenant tous les templates compilés.
    /// `'static` car les sources sont owned (`add_template_owned`).
    env: Environment<'static>,
    /// Liste des templates autorisés au manifest. Toute clé non présente
    /// déclenche `FileNotFoundError`.
    declared: Vec<String>,
}

#[pymethods]
impl TemplatesInterface {
    /// Rend le template `name` avec le contexte fourni en kwargs.
    ///
    /// Le contexte Python est sérialisé vers JSON puis désérialisé en
    /// `serde_json::Value` pour être consommé directement par minijinja.
    ///
    /// # Exemple Python
    /// ```python
    /// prompt = ctx.templates.render("system_prompt", role="analyst", tools=["search"])
    /// ```
    ///
    /// # Erreurs Python
    /// - `FileNotFoundError` si `name` n'est pas déclaré au manifest ou
    ///   pas chargé en mémoire.
    /// - `RuntimeError` en cas d'erreur de rendu Jinja (variable manquante,
    ///   syntax error, etc.).
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

        // Convertir le contexte Python → serde_json::Value via json.dumps.
        let ctx_value: serde_json::Value = match context {
            Some(d) if !d.is_empty() => {
                let json_mod = py.import("json")?;
                let json_str: String = json_mod
                    .call_method1("dumps", (d,))?
                    .extract()
                    .map_err(|e| {
                        PyRuntimeError::new_err(format!(
                            "context serialization failed: {e}"
                        ))
                    })?;
                serde_json::from_str(&json_str).map_err(|e| {
                    PyRuntimeError::new_err(format!("context parse failed: {e}"))
                })?
            }
            _ => serde_json::Value::Object(serde_json::Map::new()),
        };

        let tmpl = self.env.get_template(name).map_err(|e| {
            PyFileNotFoundError::new_err(format!(
                "Template '{name}' not loaded: {e}"
            ))
        })?;
        tmpl.render(&ctx_value).map_err(|e| {
            PyRuntimeError::new_err(format!("template '{name}' render failed: {e}"))
        })
    }

    /// Liste les noms logiques des templates déclarés au manifest.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }

    /// `True` si le template est déclaré ET chargé avec succès en mémoire.
    fn has(&self, name: &str) -> bool {
        self.declared.iter().any(|d| d == name)
            && self.env.get_template(name).is_ok()
    }
}

impl TemplatesInterface {
    /// Construit l'interface avec la liste déclarée du manifest.
    pub fn new(declared: Vec<String>) -> Self {
        Self {
            env: Environment::new(),
            declared,
        }
    }

    /// Charge et compile tous les templates déclarés depuis
    /// `<agent_dir>/templates/<name>.{j2,jinja2,jinja}`.
    ///
    /// Les erreurs de compilation sont logguées (`warn!`) mais non fatales —
    /// l'agent obtient `FileNotFoundError` au premier `render()` du template
    /// défaillant (Principe #4 : visibilité de l'erreur quand l'agent essaie
    /// vraiment de l'utiliser).
    ///
    /// Retourne le nombre de templates chargés avec succès.
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
                    "template '{name}' declared but no file found in {}",
                    dir.display()
                );
                continue;
            };
            match self.env.add_template_owned(name.clone(), content) {
                Ok(()) => loaded += 1,
                Err(e) => {
                    tracing::warn!(
                        target: "apollia.aip.templates",
                        "template '{name}' compile error: {e}"
                    );
                }
            }
        }
        loaded
    }

    /// Injecte directement une source de template (tests unitaires).
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
        let t = TemplatesInterface::new(vec![]);
        Python::with_gil(|py| {
            let res = t.render(py, "system_prompt", None);
            assert!(res.is_err(), "expected FileNotFoundError");
            assert!(
                format!("{}", res.expect_err("err")).contains("not declared"),
                "expected 'not declared' in error"
            );
        });
    }

    #[test]
    fn test_render_declared_but_not_loaded() {
        let t = TemplatesInterface::new(vec!["system_prompt".to_string()]);
        Python::with_gil(|py| {
            let res = t.render(py, "system_prompt", None);
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
            assert_eq!(out, "Hello, World!");
        });
    }

    #[test]
    fn test_render_without_context() {
        // GIVEN a template that doesn't need context
        let mut t = TemplatesInterface::new(vec![]);
        t.inject("static", "constant output");
        Python::with_gil(|py| {
            let out = t.render(py, "static", None).expect("render");
            assert_eq!(out, "constant output");
        });
    }

    #[test]
    fn test_has() {
        let mut t = TemplatesInterface::new(vec!["foo".to_string()]);
        assert!(!t.has("foo")); // declared but not loaded
        t.inject("foo", "hi");
        assert!(t.has("foo"));
        assert!(!t.has("bar"));
    }
}
