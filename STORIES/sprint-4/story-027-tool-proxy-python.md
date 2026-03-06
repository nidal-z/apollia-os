# [Sprint 4][apollia-aip] ToolProxy Python vers outils Rust

**ID :** STORY-027
**Sprint :** 4
**Crate cible :** `apollia-aip`
**Fichier(s) cible(s) :** `crates/apollia-aip/src/context.rs`
**Taille :** M (3h)
**Depend de :** STORY-026 (bridge async PyO3), apollia-tools Sprint 2 (STORY-010 a STORY-016 toutes livrees)
**Statut :** A faire

---

## User Story

En tant qu'agent Python, je veux acceder aux outils Rust via `ctx.tools.call(tool_name, input)`, afin d'executer des outils natifs (bash_executor, file_io, python_executor) depuis mon code Python sans avoir a reimplementer la logique d'invocation.

## Contexte technique

Le bridge PyO3 (`apollia-aip`) expose un `RuntimeContext` Python qui contient un attribut `tools` de type `ToolProxy`. Ce proxy encapsule un `ToolRegistryHandle` (lookup/invocation) et un `AuditTrailHandle` (journalisation). Chaque appel `call()` est asynchrone cote Python (`await ctx.tools.call(...)`) et se traduit par une lookup dans le registry, une verification des permissions agent, l'execution de l'outil, l'enregistrement dans l'audit trail, et l'incrementation du compteur `tool_calls` pour le StepBudget.

Le flux complet :
1. Python `await ctx.tools.call("file_io", {"action": "list", "path": "."})`
2. PyO3 convertit le dict Python en `serde_json::Value`
3. `ToolProxy` verifie que l'outil existe dans le registry
4. `ToolProxy` verifie que l'agent a le droit d'utiliser cet outil (present dans `tools_required` ou `tools_optional` du manifest)
5. L'outil est execute, le resultat (`serde_json::Value`) est converti en dict Python
6. L'appel est enregistre dans l'AuditTrail (tool_name, input_hash, success, duration)
7. Le compteur `tool_calls` est incremente (AtomicU32)

## Criteres d'Acceptation

### AC-1 : Appel outil nominal
`tools.call("file_io", {"action": "list", "path": "."})` depuis Python invoque l'outil file_io via le registry et retourne le resultat sous forme de dict Python.

### AC-2 : Outil inconnu
`tools.call("inexistant", {})` retourne une erreur `ToolProxyError::ToolNotFound` contenant le nom de l'outil.

### AC-3 : Outil non autorise
Si l'outil existe dans le registry mais n'est pas dans `tools_required` ni `tools_optional` du manifest de l'agent, `tools.call()` retourne `ToolProxyError::ToolNotAllowed`.

### AC-4 : Erreur d'execution
Si l'outil echoue pendant l'execution, `tools.call()` retourne `ToolProxyError::ExecutionFailed` avec le message d'erreur original.

### AC-5 : Audit trail
Chaque appel (succes ou echec) est enregistre dans l'AuditTrail avec : tool_name, input_hash (sha2), success (bool), duration (ms).

### AC-6 : Compteur tool_calls
Le compteur `tool_calls` (AtomicU32) est incremente de 1 a chaque appel a `call()`, quel que soit le resultat (succes ou echec). `tool_call_count()` retourne la valeur courante.

## Specification technique

### Types principaux

```rust
use pyo3::prelude::*;
use apollia_tools::{ToolRegistryHandle, AuditTrailHandle, compute_input_hash};
use std::sync::atomic::{AtomicU32, Ordering};

/// Proxy Python exposant les outils Rust a un agent.
/// Chaque agent recoit sa propre instance de ToolProxy
/// avec la liste des outils autorises par son manifest.
#[pyclass]
pub struct ToolProxy {
    registry: ToolRegistryHandle,
    audit: AuditTrailHandle,
    allowed_tools: Vec<String>,
    agent_id: String,
    task_id: String,
    tool_calls: AtomicU32,
}

/// Erreurs possibles lors de l'invocation d'un outil via le proxy.
#[derive(Debug, thiserror::Error)]
pub enum ToolProxyError {
    #[error("tool not found: '{0}'")]
    ToolNotFound(String),
    #[error("tool '{0}' not allowed for this agent")]
    ToolNotAllowed(String),
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
}
```

### Methodes #[pymethods]

```rust
#[pymethods]
impl ToolProxy {
    /// Appelle un outil par nom avec un dict Python en entree.
    /// Retourne un Future Python (awaitable) qui resolve en dict.
    fn call<'py>(
        &self,
        py: Python<'py>,
        tool_name: String,
        input: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Verifier tool_name dans allowed_tools
        // 2. Lookup dans registry.get(tool_name)
        // 3. Convertir PyObject -> serde_json::Value
        // 4. Executer l'outil
        // 5. Enregistrer dans audit trail
        // 6. Incrementer tool_calls
        // 7. Convertir serde_json::Value -> PyObject
        ...
    }

    /// Liste les outils disponibles pour cet agent.
    fn list_tools(&self) -> PyResult<Vec<String>> {
        // Retourne self.allowed_tools.clone()
        ...
    }

    /// Retourne le nombre d'appels outils effectues.
    fn tool_call_count(&self) -> u32 {
        self.tool_calls.load(Ordering::Relaxed)
    }
}
```

### Constructeur interne (non expose a Python)

```rust
impl ToolProxy {
    /// Cree un nouveau ToolProxy pour un agent donne.
    /// Appele par le RuntimeContext lors de l'initialisation.
    pub(crate) fn new(
        registry: ToolRegistryHandle,
        audit: AuditTrailHandle,
        allowed_tools: Vec<String>,
        agent_id: String,
        task_id: String,
    ) -> Self {
        Self {
            registry,
            audit,
            allowed_tools,
            agent_id,
            task_id,
            tool_calls: AtomicU32::new(0),
        }
    }
}
```

### Conversion PyObject <-> serde_json::Value

Utiliser `pythonize` / `depythonize` de la crate `pythonize` (compatible PyO3 0.22) pour la conversion bidirectionnelle entre dict Python et `serde_json::Value`.

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // AC-1
    #[tokio::test]
    async fn test_call_tool_nominal() {
        // GIVEN un ToolProxy avec "file_io" dans allowed_tools
        //   et un registry contenant un descripteur "file_io"
        // WHEN on appelle call("file_io", input_valide)
        // THEN le resultat est Ok et contient la sortie de l'outil
    }

    // AC-2
    #[tokio::test]
    async fn test_call_tool_not_found() {
        // GIVEN un ToolProxy avec un registry vide
        // WHEN on appelle call("inexistant", {})
        // THEN l'erreur est ToolProxyError::ToolNotFound("inexistant")
    }

    // AC-3
    #[tokio::test]
    async fn test_call_tool_not_allowed() {
        // GIVEN un ToolProxy avec allowed_tools = ["file_io"]
        //   et un registry contenant "bash_executor"
        // WHEN on appelle call("bash_executor", {})
        // THEN l'erreur est ToolProxyError::ToolNotAllowed("bash_executor")
    }

    // AC-4
    #[tokio::test]
    async fn test_call_tool_execution_failed() {
        // GIVEN un ToolProxy avec un outil qui echoue a l'execution
        // WHEN on appelle call("failing_tool", {})
        // THEN l'erreur est ToolProxyError::ExecutionFailed avec le message
    }

    // AC-5
    #[tokio::test]
    async fn test_call_records_audit_trail() {
        // GIVEN un ToolProxy avec AuditTrailHandle connecte
        // WHEN on appelle call("file_io", input)
        // THEN un ToolInvocationRecord est enregistre avec
        //   tool_name, input_hash, success=true, duration>0
    }

    // AC-6
    #[test]
    fn test_tool_call_count_increments() {
        // GIVEN un ToolProxy avec tool_calls = 0
        // WHEN on appelle call() 3 fois
        // THEN tool_call_count() retourne 3
    }
}
```

**Note :** Les tests PyO3 necessitent un interpreteur Python. Utiliser `pyo3::prepare_freethreaded_python()` en setup. Pour les tests unitaires purs (AC-2, AC-3, AC-6), tester la logique Rust sans passer par PyO3.

## Definition of Done

- [ ] `ToolProxy` expose via `#[pyclass]` avec `call()`, `list_tools()`, `tool_call_count()`
- [ ] `ToolProxyError` avec `thiserror` (3 variantes)
- [ ] Conversion PyObject <-> serde_json::Value fonctionnelle
- [ ] AuditTrail enregistre chaque appel
- [ ] 6 tests passent (`cargo test -p apollia-aip`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` avant commit
- [ ] Docstring `///` sur chaque struct, enum, fn publique
- [ ] `cargo clippy -p apollia-aip` sans warning

## Ce que cette story N'implemente PAS

- L'execution reelle des outils natifs (file_io, bash_executor) — c'est le role du ToolRegistry existant
- Le mecanisme de StepBudget complet (verification max_tool_calls) — story ulterieure
- Le streaming de resultats d'outils — hors scope Sprint 4
- La gestion des outils MCP distants — Sprint 6+

## Notes d'implementation

- `pythonize` 0.22 est compatible avec PyO3 0.22 — ajouter au `[workspace.dependencies]`
- Le compteur `AtomicU32` avec `Ordering::Relaxed` suffit car il n'y a pas de synchronisation cross-thread sur cette valeur (un seul agent par ToolProxy)
- La verification `allowed_tools` doit etre faite AVANT le lookup registry pour eviter de reveler l'existence d'outils non autorises
- `compute_input_hash` de `apollia-tools` est reutilise pour le hash d'audit

## Liens

- Spec bridge PyO3 : `docs/Briques-AIP-Bridge.md`
- Spec Tool Registry : `docs/Briques-Tool-Registry.md`
- ADR-012 : Sandbox BashExecutor macOS
- STORY-012 : ToolResolver (resolve permission tools)
- STORY-016 : AuditTrail SQLite
