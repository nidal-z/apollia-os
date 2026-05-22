//! ctx.datasources — runtime YAML datasources access.
//!
//! Charge les fichiers YAML déclarés dans `@agent(datasources=...)` depuis
//! `<agent_dir>/datasources/<name>.yaml` au démarrage de l'agent, met en
//! cache les valeurs parsées en `serde_json::Value`, et les expose à Python
//! via `ctx.datasources.get("name")` qui retourne directement un
//! `dict`/`list`/scalaire Python.
//!
//! Le gating se fait sur la liste `declared` propagée depuis le manifest :
//! une datasource non déclarée déclenche `FileNotFoundError` même si le
//! fichier existe sur disque (principe least-privilege).
//!
//! Le passage de structures Python utilise `json.loads()` pour éviter une
//! dépendance externe `pythonize` (ADR : on reste sur stdlib Python pour la
//! conversion). C'est suffisant car les datasources sont chargées une seule
//! fois au boot et lues rarement.

use std::collections::HashMap;
use std::path::Path;

use pyo3::exceptions::PyFileNotFoundError;
use pyo3::prelude::*;

/// Erreurs internes de la couche datasources.
#[derive(Debug, thiserror::Error)]
pub enum DatasourcesError {
    /// Le fichier YAML est introuvable malgré sa déclaration au manifest.
    #[error("datasource file not found: {0}")]
    FileNotFound(String),
    /// Le contenu YAML n'a pas pu être parsé.
    #[error("invalid YAML in datasource '{name}': {reason}")]
    InvalidYaml {
        /// Nom logique de la datasource (clé manifest).
        name: String,
        /// Message d'erreur du parser YAML.
        reason: String,
    },
    /// La conversion JSON ↔ Python a échoué côté `json.loads`.
    #[error("python conversion failed for '{name}': {reason}")]
    PythonConversion {
        /// Nom de la datasource concernée.
        name: String,
        /// Détail Python.
        reason: String,
    },
}

/// Interface lecture-seule exposée à l'agent via `ctx.datasources`.
///
/// Pendant le bootstrap, le runtime appelle [`Self::load_from_dir`] pour
/// remplir le cache `values`. L'agent ne voit que les datasources déclarées
/// — `declared` agit comme garde-fou même si le filesystem contient d'autres
/// fichiers.
#[pyclass(name = "DatasourcesInterface", module = "apollia._native")]
pub struct DatasourcesInterface {
    /// Valeurs YAML parsées, indexées par nom logique (clé manifest).
    ///
    /// `serde_json::Value` est utilisé comme représentation pivot pour
    /// permettre la conversion vers Python via `json.loads` sans dépendance
    /// supplémentaire.
    values: HashMap<String, serde_json::Value>,
    /// Liste des datasources autorisées — copie du manifest. Toute clé non
    /// présente ici déclenche `FileNotFoundError`, peu importe le disque.
    declared: Vec<String>,
}

#[pymethods]
impl DatasourcesInterface {
    /// Retourne le contenu parsé de la datasource `name` sous forme d'objet
    /// Python natif (dict, list, scalaire selon le YAML).
    ///
    /// # Erreurs Python
    /// - `FileNotFoundError` si `name` n'est pas dans la liste déclarée au
    ///   manifest.
    /// - `FileNotFoundError` si `name` est déclaré mais le fichier YAML est
    ///   absent ou n'a pas été chargé (parse error).
    /// - `RuntimeError` si la conversion JSON → Python via `json.loads`
    ///   échoue (ne devrait jamais arriver pour un YAML bien formé).
    fn get(&self, py: Python<'_>, name: &str) -> PyResult<PyObject> {
        if !self.declared.iter().any(|d| d == name) {
            return Err(PyFileNotFoundError::new_err(format!(
                "Datasource '{name}' not declared in @agent(datasources=...)"
            )));
        }
        let value = self.values.get(name).ok_or_else(|| {
            PyFileNotFoundError::new_err(format!(
                "Datasource '{name}' declared but not found on disk \
                 (expected: datasources/{name}.yaml)"
            ))
        })?;

        // Conversion serde_json::Value → PyObject via json.loads (évite la
        // dépendance pythonize).
        let json_str = serde_json::to_string(value).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "JSON serialization failed for '{name}': {e}"
            ))
        })?;
        let json_mod = py.import("json")?;
        let py_obj = json_mod.call_method1("loads", (json_str,))?.unbind();
        Ok(py_obj)
    }

    /// Liste les noms logiques des datasources déclarées au manifest.
    ///
    /// Toujours retourné dans l'ordre du manifest. Permet à l'agent de
    /// vérifier sa propre configuration au démarrage sans hardcoder les noms.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }

    /// `True` si la datasource est déclarée ET chargée avec succès.
    ///
    /// Utile pour les agents qui veulent dégrader gracieusement quand un
    /// fichier YAML est absent (`ctx.datasources.has("foo")` est plus
    /// idiomatique qu'un try/except `get`).
    fn has(&self, name: &str) -> bool {
        self.declared.iter().any(|d| d == name) && self.values.contains_key(name)
    }
}

impl DatasourcesInterface {
    /// Construit l'interface avec la liste déclarée du manifest. Le cache
    /// `values` reste vide tant que [`Self::load_from_dir`] n'a pas été
    /// appelé.
    pub fn new(declared: Vec<String>) -> Self {
        Self {
            values: HashMap::new(),
            declared,
        }
    }

    /// Charge toutes les datasources déclarées depuis
    /// `<agent_dir>/datasources/<name>.yaml`.
    ///
    /// Erreurs de parsing → trace `warn!` mais pas d'échec global : un
    /// fichier corrompu rend la datasource invisible à Python
    /// (`FileNotFoundError` à l'appel `get()`). C'est conforme à Principe #4
    /// (fail-fast) car l'agent reçoit une erreur claire au premier accès,
    /// pas une donnée silencieusement vide.
    ///
    /// Retourne le nombre de datasources chargées avec succès.
    pub fn load_from_dir(&mut self, agent_dir: &Path) -> usize {
        let dir = agent_dir.join("datasources");
        let mut loaded = 0usize;
        for name in self.declared.clone() {
            let path = dir.join(format!("{name}.yaml"));
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    // .yml fallback ?
                    let alt = dir.join(format!("{name}.yml"));
                    match std::fs::read_to_string(&alt) {
                        Ok(c) => c,
                        Err(_) => {
                            tracing::warn!(
                                target: "apollia.aip.datasources",
                                "datasource '{name}' declared but file missing ({}): {e}",
                                path.display()
                            );
                            continue;
                        }
                    }
                }
            };
            match serde_yaml::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    self.values.insert(name.clone(), val);
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        target: "apollia.aip.datasources",
                        "datasource '{name}' parse error: {e}"
                    );
                }
            }
        }
        loaded
    }

    /// Injecte directement une valeur en cache (utile pour les tests
    /// unitaires sans filesystem).
    #[cfg(test)]
    pub(crate) fn inject(&mut self, name: &str, value: serde_json::Value) {
        if !self.declared.iter().any(|d| d == name) {
            self.declared.push(name.to_string());
        }
        self.values.insert(name.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undeclared_returns_file_not_found() {
        // GIVEN an interface with declared=[]
        let ds = DatasourcesInterface::new(vec![]);

        // WHEN we ask for an undeclared name from Python
        Python::with_gil(|py| {
            let res = ds.get(py, "competitors");
            // THEN FileNotFoundError is raised
            assert!(res.is_err(), "expected FileNotFoundError");
            let err = res.expect_err("error");
            let msg = format!("{err}");
            assert!(
                msg.contains("not declared"),
                "expected 'not declared' in: {msg}"
            );
        });
    }

    #[test]
    fn test_declared_missing_on_disk_returns_file_not_found() {
        // GIVEN a datasource declared but no value cached
        let ds = DatasourcesInterface::new(vec!["competitors".to_string()]);

        // WHEN we ask for it
        Python::with_gil(|py| {
            let res = ds.get(py, "competitors");
            assert!(res.is_err(), "expected FileNotFoundError");
            let msg = format!("{}", res.expect_err("err"));
            assert!(
                msg.contains("not found on disk"),
                "expected 'not found on disk' in: {msg}"
            );
        });
    }

    #[test]
    fn test_list_names_returns_declared_in_order() {
        // GIVEN three declared datasources
        let ds = DatasourcesInterface::new(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        // THEN list_names returns them in order
        assert_eq!(ds.list_names(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_has_reflects_load_state() {
        // GIVEN a datasource declared but not loaded
        let mut ds = DatasourcesInterface::new(vec!["competitors".to_string()]);
        assert!(!ds.has("competitors"));

        // WHEN we inject a value
        ds.inject("competitors", serde_json::json!([{"name": "OpenAI"}]));

        // THEN has() returns true
        assert!(ds.has("competitors"));
        assert!(!ds.has("unknown"));
    }

    /// Vérifie le chemin de production : `load_from_dir` lit un vrai
    /// fichier YAML depuis `<agent_dir>/datasources/<name>.yaml` et l'expose
    /// au runtime Python via `get()`.
    #[test]
    fn test_load_from_dir_parses_real_yaml() {
        // GIVEN a temp agent_dir containing datasources/competitors.yaml
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir datasources");
        std::fs::write(
            ds_dir.join("competitors.yaml"),
            "- name: OpenAI\n  rank: 1\n- name: Anthropic\n  rank: 2\n",
        )
        .expect("write yaml");

        // WHEN we load via the production path
        let mut iface = DatasourcesInterface::new(vec!["competitors".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN one datasource was loaded
        assert_eq!(loaded, 1);
        assert!(iface.has("competitors"));

        // AND Python sees a list of two dicts
        Python::with_gil(|py| {
            let obj = iface
                .get(py, "competitors")
                .expect("get should succeed after load_from_dir");
            let len: usize = obj
                .bind(py)
                .call_method0("__len__")
                .expect("len")
                .extract()
                .expect("usize");
            assert_eq!(len, 2);
        });
    }

    /// Vérifie le fallback `.yml` quand `.yaml` est absent.
    #[test]
    fn test_load_from_dir_yml_fallback() {
        // GIVEN datasources/config.yml (not .yaml)
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("config.yml"), "enabled: true\n").expect("write");

        // WHEN we load
        let mut iface = DatasourcesInterface::new(vec!["config".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN the .yml file was picked up
        assert_eq!(loaded, 1);
        assert!(iface.has("config"));
    }

    /// Vérifie qu'une datasource manquante n'empêche pas le chargement
    /// des autres (logging warn! mais pas d'échec global, conforme au
    /// commentaire de `load_from_dir`).
    #[test]
    fn test_load_from_dir_missing_file_is_non_fatal() {
        // GIVEN datasources/present.yaml exists, declared also includes "missing"
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("present.yaml"), "value: 42\n").expect("write");

        // WHEN we load both
        let mut iface =
            DatasourcesInterface::new(vec!["present".to_string(), "missing".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // THEN only present was loaded — no panic on missing
        assert_eq!(loaded, 1);
        assert!(iface.has("present"));
        assert!(!iface.has("missing"));
    }

    /// Un YAML malformé est ignoré (warn!), pas un crash de boot.
    #[test]
    fn test_load_from_dir_invalid_yaml_is_non_fatal() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        // Tabulation au début d'un mapping = YAML invalide.
        std::fs::write(ds_dir.join("broken.yaml"), "\t- not\n  valid").expect("write");

        let mut iface = DatasourcesInterface::new(vec!["broken".to_string()]);
        let loaded = iface.load_from_dir(tmp.path());

        // load_from_dir doesn't propagate the parse error, just logs it.
        assert_eq!(loaded, 0);
        assert!(!iface.has("broken"));
    }

    #[test]
    fn test_get_returns_python_object_via_json_loads() {
        // GIVEN a declared datasource with a parsed YAML value
        let mut ds = DatasourcesInterface::new(vec!["entries".to_string()]);
        ds.inject(
            "entries",
            serde_json::json!([
                {"name": "Apollia", "rank": 1},
                {"name": "Other", "rank": 2}
            ]),
        );

        // WHEN we read it from Python
        Python::with_gil(|py| {
            let obj = ds.get(py, "entries").expect("get should succeed");
            // THEN it's a Python list of dicts
            let len: usize = obj
                .bind(py)
                .call_method0("__len__")
                .expect("len")
                .extract()
                .expect("usize");
            assert_eq!(len, 2);
        });
    }
}
