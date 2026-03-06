# [Sprint 4][apollia-aip] Validation AIP duck typing (manifest + run async)

**ID :** STORY-025
**Sprint :** 4
**Crate cible :** `apollia-aip`
**Fichier(s) cible(s) :** `crates/apollia-aip/src/validator.rs`, `crates/apollia-aip/src/lib.rs`
**Taille :** M (3h)
**Depend de :** STORY-024 (chargement module Python)
**Statut :** 🔲 A faire

---

## User Story

En tant que runtime, je veux valider qu'un objet Python est AIP-compatible (duck typing : `manifest()` + `run()` async), afin de rejeter les agents non conformes au demarrage (Principe #4 fail fast).

## Contexte technique

Le contrat AIP est base sur le duck typing Python : aucune classe de base n'est requise. Un agent est considere AIP-compatible s'il possede :

1. **`manifest()`** : methode synchrone retournant un `dict` deserialisable en `AgentManifest`
2. **`async run(task, ctx)`** : methode asynchrone (coroutine function) executant une tache

Callbacks optionnels detectes mais non requis :
- `on_start()` : appele au demarrage de l'agent
- `on_stop()` : appele a l'arret de l'agent
- `health_check()` : appele pour verifier l'etat de sante

La validation utilise `hasattr()` pour verifier la presence des methodes et `inspect.iscoroutinefunction()` pour verifier que `run` est bien async. Le `manifest()` est appele immediatement pour valider son contenu et deserialiser en `AgentManifest` via `serde_json`.

## Criteres d'Acceptation

### AC-1 : Agent valide avec manifest() + async run()
- Un objet Python possedant `manifest()` retournant un dict valide et `async run(task, ctx)`
- Retourne `ValidatedAgent` avec le manifest deserialise et les flags de callbacks

### AC-2 : Agent sans manifest()
- Un objet Python ne possedant pas de methode `manifest()`
- Retourne `AIPValidationError::MissingManifest`

### AC-3 : Agent sans run()
- Un objet Python possedant `manifest()` mais pas `run()`
- Retourne `AIPValidationError::MissingRun`

### AC-4 : Agent avec run() synchrone
- Un objet Python dont `run()` n'est pas une coroutine function
- Retourne `AIPValidationError::RunNotAsync`

### AC-5 : manifest() retourne des donnees invalides
- `manifest()` retourne un dict incomplet ou avec des types incorrects
- Retourne `AIPValidationError::InvalidManifest` avec le detail de l'erreur de deserialisation

### AC-6 : Detection des callbacks optionnels
- Detecter la presence de `on_start()`, `on_stop()`, `health_check()` sur l'objet
- Reporter leur presence dans les champs booleens de `ValidatedAgent`

## Specification technique

### Types

```rust
// crates/apollia-aip/src/validator.rs

use pyo3::prelude::*;
use apollia_core::AgentManifest;

/// Resultat de la validation AIP d'un agent Python.
///
/// Contient l'objet Python valide, le manifest deserialise,
/// et les flags indiquant la presence des callbacks optionnels.
#[derive(Debug)]
pub struct ValidatedAgent {
    /// L'objet Python agent valide.
    pub object: Py<PyAny>,
    /// Le manifest deserialise depuis le dict Python.
    pub manifest: AgentManifest,
    /// `true` si l'agent implemente `on_start()`.
    pub has_on_start: bool,
    /// `true` si l'agent implemente `on_stop()`.
    pub has_on_stop: bool,
    /// `true` si l'agent implemente `health_check()`.
    pub has_health_check: bool,
}

/// Erreurs possibles lors de la validation AIP d'un agent Python.
#[derive(Debug, thiserror::Error)]
pub enum AIPValidationError {
    /// L'agent ne possede pas de methode `manifest()`.
    #[error("agent missing required method 'manifest()'")]
    MissingManifest,

    /// L'agent ne possede pas de methode `run()`.
    #[error("agent missing required method 'run()'")]
    MissingRun,

    /// La methode `run()` n'est pas async (coroutine function).
    #[error("agent method 'run()' must be async (coroutine function)")]
    RunNotAsync,

    /// `manifest()` a retourne des donnees invalides pour `AgentManifest`.
    #[error("manifest() returned invalid data: {0}")]
    InvalidManifest(String),

    /// Erreur Python generique durant la validation.
    #[error("Python error during validation: {0}")]
    PythonError(String),
}
```

### Fonctions publiques

```rust
/// Valide qu'un objet Python est AIP-compatible.
///
/// Verifie la presence de `manifest()` (synchrone) et `run()` (async),
/// appelle `manifest()` pour deserialiser en `AgentManifest`,
/// et detecte les callbacks optionnels.
///
/// # Errors
///
/// - `MissingManifest` si l'objet n'a pas de methode `manifest()`
/// - `MissingRun` si l'objet n'a pas de methode `run()`
/// - `RunNotAsync` si `run()` n'est pas une coroutine function
/// - `InvalidManifest` si le dict retourne par `manifest()` n'est pas valide
/// - `PythonError` pour toute autre erreur Python
pub fn validate_agent(agent: &Py<PyAny>) -> Result<ValidatedAgent, AIPValidationError> {
    // 1. Python::with_gil
    // 2. Verifier hasattr(agent, "manifest")
    // 3. Verifier hasattr(agent, "run")
    // 4. Verifier inspect.iscoroutinefunction(agent.run)
    // 5. Appeler agent.manifest() → dict Python
    // 6. Convertir dict → JSON string → serde_json::from_str::<AgentManifest>
    // 7. Detecter on_start, on_stop, health_check
    // 8. Construire et retourner ValidatedAgent
    todo!()
}
```

### Algorithme detaille de `validate_agent`

```
1. Python::with_gil(|py| {
     let agent_ref = agent.bind(py);

     // Verifier manifest()
     si !agent_ref.hasattr("manifest")? → Err(MissingManifest)

     // Verifier run()
     si !agent_ref.hasattr("run")? → Err(MissingRun)

     // Verifier que run est async
     let inspect = py.import_bound("inspect")?;
     let run_method = agent_ref.getattr("run")?;
     let is_coro: bool = inspect
       .call_method1("iscoroutinefunction", (&run_method,))?
       .extract()?;
     si !is_coro → Err(RunNotAsync)

     // Appeler manifest()
     let manifest_dict = agent_ref.call_method0("manifest")?;

     // Convertir dict Python → JSON → AgentManifest
     let json_mod = py.import_bound("json")?;
     let json_str: String = json_mod
       .call_method1("dumps", (&manifest_dict,))?
       .extract()?;
     let manifest: AgentManifest = serde_json::from_str(&json_str)
       .map_err(|e| InvalidManifest(e.to_string()))?;

     // Detecter callbacks optionnels
     let has_on_start = agent_ref.hasattr("on_start")?;
     let has_on_stop = agent_ref.hasattr("on_stop")?;
     let has_health_check = agent_ref.hasattr("health_check")?;

     Ok(ValidatedAgent {
       object: agent.clone(),
       manifest,
       has_on_start,
       has_on_stop,
       has_health_check,
     })
   })
```

### Conversion dict Python → AgentManifest

Le chemin de conversion est : `dict Python` → `json.dumps()` → `String` → `serde_json::from_str()` → `AgentManifest`. Cette approche evite d'ecrire manuellement l'extraction champ par champ et reutilise les derives `serde::Deserialize` existantes sur `AgentManifest`.

Les champs optionnels de `AgentManifest` qui ont `#[serde(default)]` sont automatiquement geres (ex: `dangerous_tools_allowed`, `tags`, `skills`).

### Integration dans lib.rs

```rust
// crates/apollia-aip/src/lib.rs
pub mod loader;
pub mod validator;
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper : creer un objet Python agent a partir de code inline
    fn create_py_agent(code: &str) -> Py<PyAny> {
        Python::with_gil(|py| {
            let module = PyModule::from_code_bound(
                py, code, "test_agent.py", "test_agent"
            ).expect("failed to create test module");
            module.getattr("agent")
                .expect("failed to get agent")
                .into()
        })
    }

    const VALID_AGENT: &str = r#"
import asyncio

class TestAgent:
    def manifest(self):
        return {
            "name": "test-agent",
            "version": "0.1.0",
            "description": "A test agent",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {"status": "completed", "output": "ok"}

agent = TestAgent()
"#;

    #[test]
    fn test_validate_valid_agent() {
        // GIVEN un agent Python valide avec manifest() + async run()
        let agent = create_py_agent(VALID_AGENT);

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN la validation reussit avec le manifest correct
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.manifest.name, "test-agent");
        assert_eq!(validated.manifest.version, "0.1.0");
        assert!(!validated.has_on_start);
        assert!(!validated.has_on_stop);
        assert!(!validated.has_health_check);
    }

    #[test]
    fn test_validate_missing_manifest() {
        // GIVEN un agent Python sans methode manifest()
        let agent = create_py_agent(
            "class A:\n    async def run(self, t, c): pass\nagent = A()\n"
        );

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN on obtient MissingManifest
        assert!(matches!(result, Err(AIPValidationError::MissingManifest)));
    }

    #[test]
    fn test_validate_missing_run() {
        // GIVEN un agent Python sans methode run()
        let agent = create_py_agent(
            "class A:\n    def manifest(self): return {}\nagent = A()\n"
        );

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN on obtient MissingRun
        assert!(matches!(result, Err(AIPValidationError::MissingRun)));
    }

    #[test]
    fn test_validate_run_not_async() {
        // GIVEN un agent Python avec run() synchrone
        let agent = create_py_agent(
            "class A:\n    def manifest(self): return {}\n    def run(self, t, c): pass\nagent = A()\n"
        );

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN on obtient RunNotAsync
        assert!(matches!(result, Err(AIPValidationError::RunNotAsync)));
    }

    #[test]
    fn test_validate_invalid_manifest_data() {
        // GIVEN un agent dont manifest() retourne un dict incomplet
        let agent = create_py_agent(r#"
class A:
    def manifest(self):
        return {"name": "x"}  # missing version, description, tools_required
    async def run(self, t, c): pass
agent = A()
"#);

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN on obtient InvalidManifest
        assert!(matches!(
            result,
            Err(AIPValidationError::InvalidManifest(_))
        ));
    }

    #[test]
    fn test_validate_detects_optional_callbacks() {
        // GIVEN un agent avec on_start, on_stop et health_check
        let agent = create_py_agent(r#"
class A:
    def manifest(self):
        return {
            "name": "cb-agent",
            "version": "1.0.0",
            "description": "Agent with callbacks",
            "tools_required": [],
        }
    async def run(self, t, c): pass
    async def on_start(self, ctx): pass
    async def on_stop(self): pass
    def health_check(self): return True
agent = A()
"#);

        // WHEN on valide l'agent
        let result = validate_agent(&agent);

        // THEN les callbacks sont detectes
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert!(validated.has_on_start);
        assert!(validated.has_on_stop);
        assert!(validated.has_health_check);
    }
}
```

## Ce que cette story n'implemente PAS

- Chargement du fichier Python (STORY-024)
- Appel effectif de `run()` ou des callbacks (STORY-026)
- Validation des signatures de methodes (nombre de parametres)
- Validation du type de retour de `run()` (verifie a l'execution)
- Mecanisme de re-validation apres rechargement a chaud
- Validation des permissions ou du sandbox profile

## Definition of Done

- [ ] `ValidatedAgent` struct avec 5 champs (object, manifest, 3 flags)
- [ ] `AIPValidationError` implemente avec `thiserror`, 5 variantes
- [ ] `validate_agent()` verifie `manifest()`, `async run()`, callbacks optionnels
- [ ] Conversion `dict Python` → `JSON` → `AgentManifest` via `serde_json`
- [ ] `inspect.iscoroutinefunction` utilise pour verifier que `run` est async
- [ ] 6 tests unitaires passent (`cargo test -p apollia-aip`)
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code commite
- [ ] Docstring `///` sur chaque type et fonction publique
- [ ] `cargo clippy -p apollia-aip` passe sans warning
- [ ] `lib.rs` exporte le module `validator`

## Notes d'implementation

- `serde_json` est deja dans les dependances workspace — l'utiliser pour la deserialisation
- La conversion dict → JSON via `json.dumps` Python est plus fiable que l'extraction manuelle champ par champ car elle gere les types imbriques (listes, dicts, None → null)
- `AgentManifest` utilise `#[serde(default)]` sur `dangerous_tools_allowed`, `tags`, `skills`, `supports_streaming`, `supports_a2a` — les champs absents du dict Python prendront leurs valeurs par defaut
- `hasattr` en PyO3 : `obj.hasattr("name")` retourne `PyResult<bool>`
- Attention a bien utiliser `agent.bind(py)` dans le bloc `with_gil` pour obtenir une reference liee au GIL

## Liens

- Spec AIP Bridge : `docs/Briques-AIP-Bridge.md` (si disponible)
- AgentManifest : `crates/apollia-core/src/manifest.rs`
- Principe #4 (Fail fast) : `docs/Architecture-Principes.md`
