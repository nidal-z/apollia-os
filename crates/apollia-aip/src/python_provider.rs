//! [`PythonProvider`] — provider de contexte basé sur un module Python.
//!
//! Contrat duck-typing Python : `manifest()` + `async collect(config)`.
//! Le module est chargé depuis un chemin de fichier configuré dans la base de données.

use std::path::{Path, PathBuf};
use std::time::Instant;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Provider de contexte qui délègue à un module Python.
///
/// Contrat minimal du module : `manifest() -> dict` + `collect(config: dict) -> list[tuple[str, str]]`.
/// La méthode `is_applicable(cwd: str) -> bool` est optionnelle (défaut `True`).
pub struct PythonProvider {
    /// Nom du provider, lu depuis `manifest()["name"]` à la construction.
    name: String,
    /// Description courte, lue depuis `manifest()["description"]`. Stockée pour introspection future.
    #[allow(dead_code)]
    description: String,
    /// Priorité d'affichage, lue depuis `manifest()["priority"]` (défaut 50).
    priority: u8,
    /// Chemin vers le fichier Python du provider.
    path: PathBuf,
}

impl PythonProvider {
    /// Charge le module Python depuis `path` et lit son `manifest()`.
    ///
    /// Retourne une erreur si le fichier n'existe pas, si le module ne peut
    /// pas être importé, ou si `manifest()` est absent ou malformé.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, PythonProviderError> {
        let path = path.into();
        let path_str = path
            .to_str()
            .ok_or_else(|| PythonProviderError::InvalidPath(path.display().to_string()))?
            .to_owned();

        let (name, description, priority) = Python::with_gil(|py| {
            let code = std::fs::read_to_string(&path).map_err(|e| {
                PythonProviderError::LoadFailed(format!("cannot read {}: {}", path_str, e))
            })?;

            let module = PyModule::from_code(
                py,
                &std::ffi::CString::new(code.as_str())
                    .map_err(|e| PythonProviderError::LoadFailed(e.to_string()))?,
                &std::ffi::CString::new(path_str.as_str())
                    .map_err(|e| PythonProviderError::LoadFailed(e.to_string()))?,
                c"apollia_context_provider",
            )
            .map_err(|e| PythonProviderError::LoadFailed(format!("import error: {e}")))?;

            let manifest = module
                .getattr("manifest")
                .map_err(|_| PythonProviderError::MissingManifest)?
                .call0()
                .map_err(|e| PythonProviderError::LoadFailed(format!("manifest() failed: {e}")))?;

            let manifest_dict = manifest.downcast::<PyDict>().map_err(|_| {
                PythonProviderError::LoadFailed("manifest() must return a dict".into())
            })?;

            let name = manifest_dict
                .get_item("name")
                .ok()
                .flatten()
                .and_then(|v| v.extract::<String>().ok())
                .ok_or(PythonProviderError::LoadFailed(
                    "manifest() missing 'name'".into(),
                ))?;

            let description = manifest_dict
                .get_item("description")
                .ok()
                .flatten()
                .and_then(|v| v.extract::<String>().ok())
                .unwrap_or_default();

            let priority = manifest_dict
                .get_item("priority")
                .ok()
                .flatten()
                .and_then(|v| v.extract::<u8>().ok())
                .unwrap_or(50);

            Ok::<_, PythonProviderError>((name, description, priority))
        })?;

        Ok(Self {
            name,
            description,
            priority,
            path,
        })
    }
}

/// Erreurs de chargement d'un [`PythonProvider`].
#[derive(Debug, thiserror::Error)]
pub enum PythonProviderError {
    /// Le chemin du fichier Python est invalide (caractères non-UTF-8).
    #[error("invalid provider path: {0}")]
    InvalidPath(String),
    /// Le chargement ou l'exécution du module Python a échoué.
    #[error("failed to load Python provider: {0}")]
    LoadFailed(String),
    /// Le module Python n'expose pas de fonction `manifest()`.
    #[error("Python provider is missing manifest() function")]
    MissingManifest,
}

#[async_trait::async_trait]
impl WorkspaceProvider for PythonProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    /// Vérifie si le provider est applicable via `is_applicable(cwd)` Python.
    ///
    /// Retourne `true` par défaut si le module n'expose pas `is_applicable`.
    fn is_applicable(&self, cwd: &Path) -> bool {
        let cwd_str = match cwd.to_str() {
            Some(s) => s.to_owned(),
            None => return true,
        };
        let path = self.path.clone();

        Python::with_gil(|py| {
            let code = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => return true,
            };
            let code_c = match std::ffi::CString::new(code.as_str()) {
                Ok(c) => c,
                Err(_) => return true,
            };
            let path_str = path.to_string_lossy();
            let path_c = match std::ffi::CString::new(path_str.as_ref()) {
                Ok(c) => c,
                Err(_) => return true,
            };
            let module = match PyModule::from_code(py, &code_c, &path_c, c"apollia_ctx_prov") {
                Ok(m) => m,
                Err(_) => return true,
            };
            match module.getattr("is_applicable") {
                Ok(func) => func
                    .call1((cwd_str,))
                    .ok()
                    .and_then(|r| r.extract::<bool>().ok())
                    .unwrap_or(true),
                Err(_) => true,
            }
        })
    }

    /// Appelle `collect(config)` sur le module Python et convertit le résultat.
    ///
    /// Le module doit retourner une liste de tuples `(titre, contenu)` ou
    /// une liste de dicts `{"title": "...", "content": "..."}`.
    /// Retourne [`WorkspaceSlice::with_error`] si l'appel échoue.
    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let source = self.name.clone();
        let path = self.path.clone();
        let cwd_str = match cwd.to_str() {
            Some(s) => s.to_owned(),
            None => {
                return WorkspaceSlice::with_error(&source, "cwd path is not valid UTF-8".into())
            }
        };

        let result = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<Vec<(String, String)>, String> {
                let code = std::fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read provider file: {e}"))?;
                let code_c = std::ffi::CString::new(code.as_str())
                    .map_err(|e| format!("NUL byte in provider source: {e}"))?;
                let path_str = path.to_string_lossy();
                let path_c = std::ffi::CString::new(path_str.as_ref())
                    .map_err(|e| format!("NUL byte in path: {e}"))?;

                let module = PyModule::from_code(py, &code_c, &path_c, c"apollia_ctx_prov_collect")
                    .map_err(|e| format!("import error: {e}"))?;

                let collect_fn = module
                    .getattr("collect")
                    .map_err(|_| "module is missing collect() function".to_owned())?;

                let config_dict = PyDict::new(py);
                config_dict
                    .set_item("cwd", &cwd_str)
                    .map_err(|e| format!("dict error: {e}"))?;

                // collect() may be a coroutine — run via asyncio.run()
                let asyncio = py.import("asyncio").map_err(|e| format!("asyncio: {e}"))?;
                let coro = collect_fn
                    .call1((config_dict,))
                    .map_err(|e| format!("collect() call failed: {e}"))?;

                let is_coroutine = asyncio
                    .call_method1("iscoroutine", (&coro,))
                    .and_then(|v| v.extract::<bool>())
                    .unwrap_or(false);
                let result = if is_coroutine {
                    asyncio
                        .call_method1("run", (coro,))
                        .map_err(|e| format!("asyncio.run() failed: {e}"))?
                } else {
                    coro
                };

                let list = result
                    .downcast::<PyList>()
                    .map_err(|_| "collect() must return a list".to_owned())?;

                let mut sections = Vec::new();
                for item in list.iter() {
                    // Accept both tuple (str, str) and dict {"title": str, "content": str}
                    if let Ok(tuple) = item.extract::<(String, String)>() {
                        sections.push(tuple);
                    } else if let Ok(d) = item.downcast::<PyDict>() {
                        let title = d
                            .get_item("title")
                            .ok()
                            .flatten()
                            .and_then(|v| v.extract::<String>().ok())
                            .unwrap_or_default();
                        let content = d
                            .get_item("content")
                            .ok()
                            .flatten()
                            .and_then(|v| v.extract::<String>().ok())
                            .unwrap_or_default();
                        sections.push((title, content));
                    }
                }
                Ok(sections)
            })
        })
        .await;

        match result {
            Ok(Ok(pairs)) => {
                let sections = pairs
                    .into_iter()
                    .map(|(title, content)| WorkspaceSection {
                        title,
                        content,
                        source: source.clone(),
                    })
                    .collect();
                WorkspaceSlice {
                    source: source.clone(),
                    sections,
                    errors: vec![],
                    collected_at: Instant::now(),
                }
            }
            Ok(Err(e)) => WorkspaceSlice::with_error(&source, e),
            Err(e) => WorkspaceSlice::with_error(&source, format!("spawn_blocking panic: {e}")),
        }
    }
}
