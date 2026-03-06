# [Sprint 4][apollia-aip] Bridge Tokio - asyncio via pyo3-async-runtimes

**ID :** STORY-026
**Sprint :** 4
**Crate cible :** `apollia-aip`
**Fichier(s) cible(s) :** `crates/apollia-aip/src/bridge.rs`, `crates/apollia-aip/src/lib.rs`
**Taille :** L (6h)
**Depend de :** STORY-024 (chargement module Python), STORY-025 (validation AIP)
**Statut :** ✅ Terminé

---

## User Story

En tant que runtime, je veux appeler les methodes async Python (`run`, `on_start`, `on_stop`) depuis Rust async/Tokio, afin d'executer l'agent dans la boucle evenementielle du runtime.

## Contexte technique

Le bridge est la piece centrale du systeme AIP : il connecte le monde async Rust (Tokio) au monde async Python (asyncio). La crate `pyo3-async-runtimes` fournit les primitives necessaires :

- `pyo3_async_runtimes::tokio::future_into_py` : convertit un `Future` Rust en awaitable Python
- `pyo3_async_runtimes::tokio::into_future` : convertit un awaitable Python en `Future` Rust

Le flux d'execution pour `agent.run(task, ctx)` :
1. Rust : serialiser `AIPTask` en dict Python via `serde_json` + `json.loads`
2. Rust : acquerir le GIL, appeler `agent.run(task_dict, ctx)`
3. Python : retourne un coroutine (awaitable)
4. Rust : convertir l'awaitable en `Future` via `into_future`
5. Rust : relacher le GIL
6. Rust : `.await` le Future (non-bloquant pour Tokio)
7. Rust : re-acquerir le GIL, deserialiser le resultat Python en `AIPResult`

Point critique : le GIL ne doit PAS etre tenu pendant l'attente du Future Python. Le pattern est :
```
with_gil → call method → get awaitable → into_future → drop GIL
await future (GIL-free)
with_gil → extract result
```

## Criteres d'Acceptation

### AC-1 : Appel de run() async depuis Rust async
- Appeler `agent.run(task, ctx)` depuis un contexte `async fn` Rust
- Recevoir le resultat deserialise en `AIPResult`

### AC-2 : Exception Python pendant l'appel async
- Si le code Python leve une exception durant `run()`
- Retourner `AIPBridgeError::PythonException` avec la traceback

### AC-3 : Appel de on_start() si disponible
- Si `ValidatedAgent.has_on_start == true`, appeler `on_start(ctx)`
- Si `has_on_start == false`, ne rien faire (pas d'erreur)

### AC-4 : Appel de on_stop() si disponible
- Si `ValidatedAgent.has_on_stop == true`, appeler `on_stop()`
- Si `has_on_stop == false`, ne rien faire (pas d'erreur)

### AC-5 : Serialisation AIPTask / Deserialisation AIPResult
- `AIPTask` serialise en dict Python via JSON intermediaire
- Le dict retourne par `run()` deserialise en `AIPResult`
- Erreur de serialisation/deserialisation → erreur typee

### AC-6 : GIL relache pendant l'execution Python async
- Le GIL n'est pas tenu pendant le `.await` du Future Python
- Les autres taches Tokio ne sont pas bloquees

## Specification technique

### Types

```rust
// crates/apollia-aip/src/bridge.rs

use pyo3::prelude::*;
use apollia_core::{AIPTask, AIPResult};
use crate::validator::ValidatedAgent;

/// Erreurs possibles lors de l'appel d'un agent Python via le bridge.
#[derive(Debug, thiserror::Error)]
pub enum AIPBridgeError {
    /// Une exception Python a ete levee durant l'appel async.
    #[error("Python exception: {0}")]
    PythonException(String),

    /// Echec de serialisation de `AIPTask` vers un dict Python.
    #[error("failed to serialize task to Python: {0}")]
    SerializationError(String),

    /// Echec de deserialisation du resultat Python vers `AIPResult`.
    #[error("failed to deserialize result from Python: {0}")]
    DeserializationError(String),

    /// Erreur interne du bridge.
    #[error("bridge error: {0}")]
    Internal(String),
}

/// Bridge entre le runtime Tokio et un agent Python asyncio.
///
/// Encapsule un agent Python valide et fournit des methodes async Rust
/// pour appeler `run()`, `on_start()`, et `on_stop()` de l'agent.
pub struct AIPBridge {
    /// L'objet Python agent.
    agent: Py<PyAny>,
    /// Indique si l'agent possede un callback `on_start`.
    has_on_start: bool,
    /// Indique si l'agent possede un callback `on_stop`.
    has_on_stop: bool,
}
```

### Methodes publiques

```rust
impl AIPBridge {
    /// Cree un nouveau bridge a partir d'un agent valide.
    pub fn new(validated: ValidatedAgent) -> Self {
        Self {
            agent: validated.object,
            has_on_start: validated.has_on_start,
            has_on_stop: validated.has_on_stop,
        }
    }

    /// Appelle `agent.run(task, ctx)` de maniere asynchrone.
    ///
    /// Serialise `AIPTask` en dict Python, appelle la coroutine `run`,
    /// attend le resultat, et le deserialise en `AIPResult`.
    ///
    /// Le GIL est relache pendant l'attente du Future Python.
    ///
    /// # Errors
    ///
    /// - `SerializationError` si `AIPTask` ne peut pas etre converti en dict
    /// - `PythonException` si le code Python leve une exception
    /// - `DeserializationError` si le resultat ne peut pas devenir `AIPResult`
    pub async fn call_run(
        &self,
        task: &AIPTask,
        ctx: PyObject,
    ) -> Result<AIPResult, AIPBridgeError> {
        todo!()
    }

    /// Appelle `agent.on_start(ctx)` si le callback existe.
    ///
    /// Ne fait rien si `has_on_start` est `false`.
    ///
    /// # Errors
    ///
    /// - `PythonException` si le callback leve une exception
    pub async fn call_on_start(
        &self,
        ctx: PyObject,
    ) -> Result<(), AIPBridgeError> {
        todo!()
    }

    /// Appelle `agent.on_stop()` si le callback existe.
    ///
    /// Ne fait rien si `has_on_stop` est `false`.
    ///
    /// # Errors
    ///
    /// - `PythonException` si le callback leve une exception
    pub async fn call_on_stop(&self) -> Result<(), AIPBridgeError> {
        todo!()
    }
}
```

### Fonctions internes

```rust
/// Serialise un `AIPTask` en objet Python (dict).
///
/// Chemin : AIPTask → serde_json::to_string → json.loads (Python) → PyObject
fn task_to_py_dict(py: Python<'_>, task: &AIPTask) -> Result<PyObject, AIPBridgeError> {
    let json_str = serde_json::to_string(task)
        .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;

    let json_mod = py.import_bound("json")
        .map_err(|e| AIPBridgeError::Internal(e.to_string()))?;

    let py_dict = json_mod
        .call_method1("loads", (json_str,))
        .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;

    Ok(py_dict.into())
}

/// Deserialise un objet Python (dict) en `AIPResult`.
///
/// Chemin : PyObject → json.dumps (Python) → String → serde_json::from_str → AIPResult
fn py_dict_to_result(py: Python<'_>, obj: &PyObject) -> Result<AIPResult, AIPBridgeError> {
    let json_mod = py.import_bound("json")
        .map_err(|e| AIPBridgeError::Internal(e.to_string()))?;

    let json_str: String = json_mod
        .call_method1("dumps", (obj.bind(py),))
        .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))?
        .extract()
        .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))?;

    serde_json::from_str(&json_str)
        .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))
}
```

### Algorithme detaille de `call_run`

```
pub async fn call_run(&self, task: &AIPTask, ctx: PyObject) -> Result<AIPResult, AIPBridgeError> {
    // Phase 1 : GIL acquis — preparer l'appel
    let future = Python::with_gil(|py| {
        // Serialiser AIPTask en dict Python
        let task_dict = task_to_py_dict(py, task)?;

        // Appeler agent.run(task_dict, ctx) → coroutine
        let coroutine = self.agent.bind(py)
            .call_method1("run", (task_dict, ctx))
            .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

        // Convertir la coroutine en Future Rust
        pyo3_async_runtimes::tokio::into_future(py, coroutine)
            .map_err(|e| AIPBridgeError::Internal(format!("into_future failed: {e}")))
    })?;

    // Phase 2 : GIL relache — attendre le Future
    let py_result = future.await
        .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

    // Phase 3 : GIL acquis — deserialiser le resultat
    Python::with_gil(|py| {
        py_dict_to_result(py, &py_result.into_py(py))
    })
}
```

### Algorithme de `call_on_start`

```
pub async fn call_on_start(&self, ctx: PyObject) -> Result<(), AIPBridgeError> {
    if !self.has_on_start {
        return Ok(());
    }

    let future = Python::with_gil(|py| {
        let coroutine = self.agent.bind(py)
            .call_method1("on_start", (ctx,))
            .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

        pyo3_async_runtimes::tokio::into_future(py, coroutine)
            .map_err(|e| AIPBridgeError::Internal(format!("into_future failed: {e}")))
    })?;

    future.await
        .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

    Ok(())
}
```

### Algorithme de `call_on_stop`

```
pub async fn call_on_stop(&self) -> Result<(), AIPBridgeError> {
    if !self.has_on_stop {
        return Ok(());
    }

    let future = Python::with_gil(|py| {
        let coroutine = self.agent.bind(py)
            .call_method0("on_stop")
            .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

        pyo3_async_runtimes::tokio::into_future(py, coroutine)
            .map_err(|e| AIPBridgeError::Internal(format!("into_future failed: {e}")))
    })?;

    future.await
        .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

    Ok(())
}
```

### Integration dans lib.rs

```rust
// crates/apollia-aip/src/lib.rs
pub mod loader;
pub mod validator;
pub mod bridge;
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::{ValidatedAgent, validate_agent};

    // Helper : creer un ValidatedAgent a partir de code Python inline
    fn create_validated_agent(code: &str) -> ValidatedAgent {
        let agent = Python::with_gil(|py| {
            let module = PyModule::from_code_bound(
                py, code, "test_bridge.py", "test_bridge"
            ).expect("failed to create test module");
            module.getattr("agent")
                .expect("failed to get agent")
                .into()
        });
        validate_agent(&agent).expect("agent validation failed")
    }

    // Helper : creer un PyObject ctx vide (dict)
    fn empty_ctx() -> PyObject {
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new_bound(py);
            dict.into()
        })
    }

    const VALID_AGENT_CODE: &str = r#"
import asyncio

class TestAgent:
    def manifest(self):
        return {
            "name": "bridge-test",
            "version": "1.0.0",
            "description": "Bridge test agent",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {
            "status": "completed",
            "output": [{"type": "text", "text": "hello from python"}],
        }

    async def on_start(self, ctx):
        pass

    async def on_stop(self):
        pass

agent = TestAgent()
"#;

    #[tokio::test]
    async fn test_call_run_success() {
        // GIVEN un bridge avec un agent valide
        let validated = create_validated_agent(VALID_AGENT_CODE);
        let bridge = AIPBridge::new(validated);
        let task = AIPTask::default(); // ou construction minimale
        let ctx = empty_ctx();

        // WHEN on appelle run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN on obtient un AIPResult valide
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_run_python_exception() {
        // GIVEN un agent dont run() leve une exception
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "err-agent", "version": "1.0.0",
            "description": "error agent", "tools_required": [],
        }
    async def run(self, task, ctx):
        raise ValueError("test error from python")
agent = A()
"#;
        let validated = create_validated_agent(code);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN on appelle run()
        let result = bridge.call_run(&AIPTask::default(), ctx).await;

        // THEN on obtient PythonException
        assert!(matches!(result, Err(AIPBridgeError::PythonException(_))));
    }

    #[tokio::test]
    async fn test_call_on_start_with_callback() {
        // GIVEN un bridge avec un agent qui a on_start
        let validated = create_validated_agent(VALID_AGENT_CODE);
        assert!(validated.has_on_start);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN on appelle on_start()
        let result = bridge.call_on_start(ctx).await;

        // THEN l'appel reussit
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_on_stop_with_callback() {
        // GIVEN un bridge avec un agent qui a on_stop
        let validated = create_validated_agent(VALID_AGENT_CODE);
        assert!(validated.has_on_stop);
        let bridge = AIPBridge::new(validated);

        // WHEN on appelle on_stop()
        let result = bridge.call_on_stop().await;

        // THEN l'appel reussit
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_on_start_without_callback() {
        // GIVEN un bridge avec un agent SANS on_start
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "no-cb", "version": "1.0.0",
            "description": "no callbacks", "tools_required": [],
        }
    async def run(self, task, ctx):
        return {"status": "completed", "output": []}
agent = A()
"#;
        let validated = create_validated_agent(code);
        assert!(!validated.has_on_start);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN on appelle on_start() sur un agent sans callback
        let result = bridge.call_on_start(ctx).await;

        // THEN l'appel reussit (no-op)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_task_serialization_roundtrip() {
        // GIVEN un agent qui retourne le task_id recu
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "echo", "version": "1.0.0",
            "description": "echo agent", "tools_required": [],
        }
    async def run(self, task, ctx):
        return {
            "status": "completed",
            "output": [{"type": "text", "text": task.get("id", "no-id")}],
        }
agent = A()
"#;
        let validated = create_validated_agent(code);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // Construction d'un AIPTask avec un id connu
        let task = AIPTask::default(); // adapter selon la structure reelle

        // WHEN on appelle run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN la serialisation/deserialisation fonctionne
        assert!(result.is_ok());
    }
}
```

## Ce que cette story n'implemente PAS

- Timeout sur l'execution de `run()` (sera gere par StepBudget dans le runtime)
- Annulation cooperative des taches Python (cancel)
- Pool de threads Python pour parallelisme multi-agent
- Streaming des resultats (yield partiel depuis Python)
- Gestion du contexte enrichi (tools, memory) dans `ctx` — sera une story ulterieure
- Metriques et tracing des appels Python

## Definition of Done

- [ ] `AIPBridgeError` implemente avec `thiserror`, 4 variantes
- [ ] `AIPBridge` struct avec `new()`, `call_run()`, `call_on_start()`, `call_on_stop()`
- [ ] `task_to_py_dict()` serialise `AIPTask` en dict Python via JSON
- [ ] `py_dict_to_result()` deserialise le resultat Python en `AIPResult`
- [ ] `pyo3_async_runtimes::tokio::into_future` utilise pour convertir awaitable → Future
- [ ] GIL relache pendant le `.await` du Future Python
- [ ] 6 tests async passent (`cargo test -p apollia-aip`)
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code commite
- [ ] Docstring `///` sur chaque type et fonction publique
- [ ] `cargo clippy -p apollia-aip` passe sans warning
- [ ] `lib.rs` exporte le module `bridge`

## Notes d'implementation

- `pyo3_async_runtimes::tokio::into_future` prend un `Bound<'_, PyAny>` representant un awaitable Python et retourne un `impl Future<Output = PyResult<PyObject>>`
- Le pattern GIL en 3 phases (acquis/relache/acquis) est critique pour ne pas bloquer le runtime Tokio
- `AIPTask` et `AIPResult` doivent implementer `serde::Serialize` et `serde::Deserialize` — verifier dans `apollia-core`
- Les tests async avec PyO3 necessitent `#[tokio::test]` et un interpreteur Python disponible
- `pyo3-async-runtimes` avec feature `tokio-runtime` est deja declare dans les dependances de `apollia-aip`
- Attention : `into_future` peut echouer si l'objet Python n'est pas un vrai awaitable — toujours mapper l'erreur
- Pour les tests, `AIPTask::default()` doit etre disponible — si ce n'est pas le cas, ajouter `#[derive(Default)]` sur `AIPTask` dans `apollia-core` ou construire manuellement

## Liens

- pyo3-async-runtimes : https://docs.rs/pyo3-async-runtimes
- PyO3 documentation : https://pyo3.rs
- Spec AIP Bridge : `docs/Briques-AIP-Bridge.md` (si disponible)
- AIPTask / AIPResult : `crates/apollia-core/src/aip.rs`
- Principe #6 (Memoire a initiative de l'agent) : `docs/Architecture-Principes.md`
- ADR associe : ADR-014 (spawn_blocking + asyncio.run() au lieu de into_future)
