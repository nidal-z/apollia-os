# [Sprint 4][apollia-aip] Chargement module Python via PyO3

**ID :** STORY-024
**Sprint :** 4
**Crate cible :** `apollia-aip`
**Fichier(s) cible(s) :** `crates/apollia-aip/src/loader.rs`, `crates/apollia-aip/src/lib.rs`
**Taille :** L (6h)
**Depend de :** Sprint 3 (workspace complet)
**Statut :** ✅ Terminée

---

## User Story

En tant que runtime, je veux charger un module Python depuis un fichier `.py` via PyO3, afin d'obtenir un objet Python manipulable representant l'agent.

## Contexte technique

Le bridge AIP (Agent Integration Protocol) est la couche qui connecte le runtime Rust aux agents Python. La premiere etape consiste a charger un fichier `.py` contenant un agent et a en extraire l'objet Python correspondant.

Convention AIP pour les agents Python :
- Le fichier `.py` contient une variable module-level `agent` (ex: `agent = MonAgent()`)
- Alternative : le module exporte une classe, et le loader l'instancie
- Le loader privilegie la convention `agent` (variable prete a l'emploi)

PyO3 avec la feature `auto-initialize` initialise automatiquement l'interpreteur Python au premier appel `Python::with_gil`. Le loader utilise `PyModule::from_code_bound` pour importer le contenu du fichier comme module Python, puis extrait l'attribut `agent`.

Le chemin du fichier doit etre ajoute au `sys.path` Python pour que les imports relatifs du module fonctionnent.

## Criteres d'Acceptation

### AC-1 : Chargement d'un fichier .py valide
- Charger un fichier `.py` contenant `agent = MonAgent()`
- Extraire l'objet `agent` du module
- Retourner `Py<PyAny>` representant l'objet agent

### AC-2 : Fichier inexistant
- Si le fichier n'existe pas sur le disque
- Retourner `AIPLoaderError::FileNotFound` avec le chemin en contexte

### AC-3 : Erreur d'import Python
- Si le fichier contient une erreur de syntaxe Python ou un import manquant
- Retourner `AIPLoaderError::ImportFailed` avec le nom du module et la traceback Python

### AC-4 : Attribut `agent` absent
- Si le module se charge correctement mais ne contient pas d'attribut `agent`
- Retourner `AIPLoaderError::NoAgentFound` avec le nom du module

### AC-5 : Chemin invalide (non .py)
- Si le chemin pointe vers un fichier qui n'a pas l'extension `.py`
- Retourner `AIPLoaderError::InvalidPath` avec le chemin

## Specification technique

### Types

```rust
// crates/apollia-aip/src/loader.rs

use std::path::Path;
use pyo3::prelude::*;

/// Erreurs possibles lors du chargement d'un module Python agent.
#[derive(Debug, thiserror::Error)]
pub enum AIPLoaderError {
    /// Le fichier specifie n'existe pas sur le disque.
    #[error("file not found: {0}")]
    FileNotFound(String),

    /// Le chemin ne pointe pas vers un fichier .py.
    #[error("invalid path (expected .py file): {0}")]
    InvalidPath(String),

    /// L'import Python a echoue (syntaxe, import manquant, etc.).
    #[error("Python import failed for '{module}': {reason}")]
    ImportFailed { module: String, reason: String },

    /// Le module ne contient pas d'attribut 'agent'.
    #[error("no 'agent' attribute found in module '{0}'")]
    NoAgentFound(String),

    /// Erreur Python generique.
    #[error("Python error: {0}")]
    PythonError(String),
}
```

### Fonctions publiques

```rust
/// Charge un module Python agent depuis un fichier .py.
///
/// Le fichier doit contenir une variable module-level `agent`.
/// Le repertoire parent du fichier est ajoute a `sys.path` pour
/// permettre les imports relatifs.
///
/// # Errors
///
/// - `FileNotFound` si le fichier n'existe pas
/// - `InvalidPath` si l'extension n'est pas `.py`
/// - `ImportFailed` si l'execution du module echoue
/// - `NoAgentFound` si le module ne contient pas d'attribut `agent`
pub fn load_agent_module(path: &Path) -> Result<Py<PyAny>, AIPLoaderError> {
    // 1. Verifier que le chemin a l'extension .py
    // 2. Verifier que le fichier existe
    // 3. Lire le contenu du fichier
    // 4. Python::with_gil {
    //      a. Ajouter le repertoire parent a sys.path
    //      b. PyModule::from_code_bound(py, &code, file_name, module_name)
    //      c. Extraire l'attribut "agent" du module
    //    }
    // 5. Retourner Py<PyAny>
    todo!()
}
```

### Integration dans lib.rs

```rust
// crates/apollia-aip/src/lib.rs
pub mod loader;
```

### Algorithme detaille de `load_agent_module`

```
1. extension = path.extension()
   si extension != "py" → Err(InvalidPath)

2. si !path.exists() → Err(FileNotFound)

3. code = std::fs::read_to_string(path)
   si erreur I/O → Err(FileNotFound) avec contexte

4. module_name = path.file_stem() (sans extension)
   file_name = path.file_name()

5. Python::with_gil(|py| {
     // Ajouter le parent dir a sys.path
     let sys = py.import_bound("sys")?;
     let sys_path = sys.getattr("path")?;
     let parent = path.parent().unwrap_or(Path::new("."));
     sys_path.call_method1("insert", (0, parent.to_string_lossy()))?;

     // Charger le module
     let module = PyModule::from_code_bound(
       py, &code, file_name, module_name
     ).map_err(|e| ImportFailed { module, reason: e.to_string() })?;

     // Extraire l'agent
     let agent = module.getattr("agent")
       .map_err(|_| NoAgentFound(module_name))?;

     Ok(agent.into())
   })
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper : creer un fichier .py temporaire avec le contenu donne
    fn create_temp_py(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(".py")
            .tempfile()
            .expect("failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("failed to write temp file");
        file
    }

    #[test]
    fn test_load_valid_agent_module() {
        // GIVEN un fichier .py avec un objet agent valide
        let file = create_temp_py(
            "class MonAgent:\n    pass\nagent = MonAgent()\n"
        );

        // WHEN on charge le module
        let result = load_agent_module(file.path());

        // THEN on obtient un Py<PyAny> valide
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_file_not_found() {
        // GIVEN un chemin vers un fichier inexistant
        let path = Path::new("/tmp/apollia_test_inexistant_xyz.py");

        // WHEN on tente de charger
        let result = load_agent_module(path);

        // THEN on obtient FileNotFound
        assert!(matches!(result, Err(AIPLoaderError::FileNotFound(_))));
    }

    #[test]
    fn test_load_import_failed_syntax_error() {
        // GIVEN un fichier .py avec une erreur de syntaxe
        let file = create_temp_py("def broken(\n");

        // WHEN on tente de charger
        let result = load_agent_module(file.path());

        // THEN on obtient ImportFailed
        assert!(matches!(
            result,
            Err(AIPLoaderError::ImportFailed { .. })
        ));
    }

    #[test]
    fn test_load_no_agent_attribute() {
        // GIVEN un fichier .py valide mais sans attribut 'agent'
        let file = create_temp_py("x = 42\n");

        // WHEN on tente de charger
        let result = load_agent_module(file.path());

        // THEN on obtient NoAgentFound
        assert!(matches!(result, Err(AIPLoaderError::NoAgentFound(_))));
    }

    #[test]
    fn test_load_invalid_path_not_py() {
        // GIVEN un chemin vers un fichier sans extension .py
        let file = tempfile::Builder::new()
            .suffix(".txt")
            .tempfile()
            .expect("failed to create temp file");

        // WHEN on tente de charger
        let result = load_agent_module(file.path());

        // THEN on obtient InvalidPath
        assert!(matches!(result, Err(AIPLoaderError::InvalidPath(_))));
    }
}
```

## Ce que cette story n'implemente PAS

- Validation du contrat AIP (manifest/run) → STORY-025
- Appel des methodes async Python → STORY-026
- Gestion du rechargement a chaud (hot reload)
- Support des packages Python multi-fichiers (uniquement fichier unique)
- Isolation virtualenv (deja gere par PythonExecutor dans apollia-tools)

## Definition of Done

- [x] `AIPLoaderError` implemente avec `thiserror`, 5 variantes
- [x] `load_agent_module()` charge un fichier `.py` et retourne `Py<PyAny>`
- [x] Le repertoire parent du fichier est ajoute a `sys.path`
- [x] 5 tests unitaires passent (`cargo test -p apollia-aip`)
- [x] Zero `unwrap()` dans le code de production
- [x] Zero `todo!()` dans le code commite
- [x] Docstring `///` sur chaque type et fonction publique
- [x] `cargo clippy -p apollia-aip` passe sans warning
- [x] `lib.rs` exporte le module `loader`

## Notes d'implementation

- `tempfile` doit etre ajoute en `[dev-dependencies]` de `apollia-aip` pour les tests
- PyO3 `auto-initialize` demarre l'interpreteur Python automatiquement au premier `Python::with_gil`
- Attention : `PyModule::from_code_bound` execute le code Python immediatement (les effets de bord du module s'executent au chargement)
- Le nom du module derive du `file_stem` est utilise comme identifiant Python — eviter les caracteres speciaux
- Les tests PyO3 s'executent dans un seul thread par defaut (GIL global) — pas besoin de `#[serial]` mais attention aux effets de bord sur `sys.path`

## Notes d'implementation (post-dev)

- PyO3 0.22 utilise l'API `_bound` : `import_bound()`, `from_code_bound()`, `getattr()` au lieu de `import()`, `from_code()`.
- Sur macOS, `PYO3_PYTHON` doit pointer vers un Python Homebrew (3.12+) car le Python system (CommandLineTools 3.9) ne link pas correctement. Voir ADR-013.
- `tempfile = "3"` ajoute en `[dev-dependencies]` pour les tests.

## Liens

- Spec AIP Bridge : `docs/Briques-AIP-Bridge.md` (si disponible)
- PyO3 documentation : https://pyo3.rs
- Principe #4 (Fail fast) : `docs/Architecture-Principes.md`
- ADR associe : ADR-013
