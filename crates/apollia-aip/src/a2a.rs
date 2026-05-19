//! ctx.a2a — agent-to-agent invocation surface (LOT 4/7 — ADR-102).
//!
//! Façade nestée consolidant les 6 méthodes A2A historiquement aplaties sur
//! `RuntimeContext` (`a2a_invoke`, `a2a_discover`, `a2a_list_skills`,
//! `send`, `receive`, `delegate`). Pour LOT 4, on expose les 3 méthodes
//! "haut niveau" qui pilotent l'[`A2AInvoker`] :
//!
//! - [`A2AInterface::invoke`] — appel synchrone d'un skill avec retour
//!   typé `dict` (équivalent `a2a_invoke`).
//! - [`A2AInterface::discover`] — résolution agent/skill (équivalent
//!   `a2a_discover`).
//! - [`A2AInterface::list_skills`] — inventaire complet du runtime
//!   (équivalent `a2a_list_skills`).
//! - [`A2AInterface::skill_as_tool`] — **nouveau** : produit un descriptor
//!   tool consommable par `ctx.react` (LOT 7). Format minimal pour LOT 4 ;
//!   l'enrichissement (schémas IO depuis la carte de découverte) viendra en
//!   LOT 7.
//!
//! Les méthodes mailbox (`send`/`receive`) et la délégation Director→Worker
//! (`delegate`) restent sur `RuntimeContext` flat pour LOT 4 — elles
//! seront migrées en LOT 7 sans changement de sémantique.
//!
//! L'interface partage le même `Arc<A2AInvoker>` que `RuntimeContext` —
//! pas de duplication d'état ni de second canal d'événements. Le compteur
//! de profondeur (`a2a_depth`) et le `chain_deadline` sont copiés au moment
//! de la construction pour respecter l'immuabilité côté Python.

use std::sync::Arc;
use std::time::Instant;

use apollia_runtime::a2a::A2AInvoker;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Façade typée exposée à l'agent Python via `ctx.a2a`.
///
/// Construite par [`crate::context::RuntimeContext::new_with_llm`] (ou via
/// les builders `with_*`) lorsque le runtime fournit un `A2AInvoker`.
/// L'agent n'a jamais à instancier cette structure directement.
#[pyclass(name = "A2AInterface", module = "apollia._native")]
pub struct A2AInterface {
    /// Orchestrateur A2A partagé avec `RuntimeContext`. `None` = runtime
    /// minimal sans support A2A (tests, CLI dry-run).
    invoker: Option<Arc<A2AInvoker>>,
    /// Identifiant de l'agent caller (utilisé pour la chaîne A2A).
    caller_agent_name: String,
    /// Profondeur actuelle dans la chaîne (0 = invocation racine).
    a2a_depth: u32,
    /// Deadline cumulé de la chaîne, propagé par l'invoker. `None` avant la
    /// première invocation depuis cet agent.
    chain_deadline: Option<Instant>,
}

#[pymethods]
impl A2AInterface {
    /// Invoque un skill A2A avec entrée typée et timeout optionnel.
    ///
    /// Retourne un Python awaitable qui résout en `dict` avec les clés
    /// `result`, `agent_name`, `skill_id`, `duration_ms` en cas de succès,
    /// ou un dict `AIPResult` d'échec si une erreur runtime survient (jamais
    /// d'exception Python — sémantique alignée sur l'API historique).
    #[pyo3(signature = (skill_id, input, timeout_secs=None))]
    fn invoke<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
        input: PyObject,
        timeout_secs: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        // Convertir input Python → serde_json::Value via json.dumps.
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let json_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        let caller = self.caller_agent_name.clone();
        let timeout = timeout_secs.map(std::time::Duration::from_secs);
        let a2a_depth = self.a2a_depth.saturating_add(1);
        let chain_deadline = self.chain_deadline;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let out_json = match invoker
                .invoke(
                    &skill_id,
                    input_value,
                    &caller,
                    a2a_depth,
                    timeout,
                    chain_deadline,
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r)
                    .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?,
                Err(e) => {
                    let failed =
                        apollia_core::AIPResult::failed("a2a_invoke_error", &e.to_string());
                    serde_json::to_string(&failed).map_err(|err| {
                        PyRuntimeError::new_err(format!("serialization error: {err}"))
                    })?
                }
            };

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (out_json,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }

    /// Découvre l'agent qui expose `skill_id` et retourne sa carte de
    /// découverte.
    ///
    /// Retourne un Python awaitable qui résout en `dict | None`.
    /// `None` si aucun agent disponible ne déclare le skill.
    fn discover<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let card_opt = invoker
                .discover(&skill_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            match card_opt {
                None => Ok(Python::with_gil(|py| py.None())),
                Some(card) => {
                    let json_str = serde_json::to_string(&card).map_err(|e| {
                        PyRuntimeError::new_err(format!("serialization error: {e}"))
                    })?;
                    Python::with_gil(|py| {
                        let json_mod = py
                            .import("json")
                            .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                        let py_obj: PyObject = json_mod
                            .call_method1("loads", (json_str,))
                            .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                            .unbind();
                        Ok(py_obj)
                    })
                }
            }
        })
    }

    /// Liste tous les skills A2A disponibles dans le runtime.
    ///
    /// Retourne un Python awaitable qui résout en `list[dict]`.
    /// Chaque dict a les clés `skill_id`, `agent_name`, `skill_name`,
    /// `description`.
    fn list_skills<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let skills = invoker
                .list_skills()
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            let json_str = serde_json::to_string(&skills)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (json_str,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }

    /// Construit un descripteur tool consommable par `ctx.react` (LOT 7).
    ///
    /// Format minimal LOT 4 — `parameters` est un schéma vide,
    /// `description` est une stub. LOT 7 enrichira ce descripteur à partir
    /// de la carte de découverte du skill (`description`, `input_schema`)
    /// via un appel à [`Self::discover`].
    ///
    /// **Synchrone** : pas d'I/O, juste une construction de dict.
    fn skill_as_tool(&self, py: Python<'_>, skill_id: String) -> PyResult<PyObject> {
        let descriptor = serde_json::json!({
            "name": format!("a2a:{skill_id}"),
            "description": format!("Invoke A2A skill '{skill_id}'"),
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": true
            }
        });
        let json_str = serde_json::to_string(&descriptor).map_err(|e| {
            PyRuntimeError::new_err(format!("descriptor serialization failed: {e}"))
        })?;
        let json_mod = py.import("json")?;
        Ok(json_mod.call_method1("loads", (json_str,))?.unbind())
    }
}

impl A2AInterface {
    /// Construit une nouvelle interface A2A liée au caller.
    ///
    /// `invoker = None` désactive complètement la surface (toutes les
    /// méthodes lèvent `RuntimeError("A2A invoker not available …")`), sauf
    /// `skill_as_tool` qui reste constructible (utile pour les tests
    /// unitaires builder).
    pub fn new(
        invoker: Option<Arc<A2AInvoker>>,
        caller_agent_name: String,
        a2a_depth: u32,
        chain_deadline: Option<Instant>,
    ) -> Self {
        Self {
            invoker,
            caller_agent_name,
            a2a_depth,
            chain_deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_as_tool_returns_descriptor() {
        // GIVEN an A2AInterface (no invoker needed for skill_as_tool)
        let a2a = A2AInterface::new(None, "tester".to_string(), 0, None);
        Python::with_gil(|py| {
            let obj = a2a
                .skill_as_tool(py, "summarize".to_string())
                .expect("skill_as_tool should succeed");
            // THEN the descriptor has the expected keys
            let name: String = obj
                .bind(py)
                .get_item("name")
                .expect("name key")
                .extract()
                .expect("string");
            assert_eq!(name, "a2a:summarize");
        });
    }

    #[test]
    fn test_invoke_without_invoker_raises() {
        let a2a = A2AInterface::new(None, "tester".to_string(), 0, None);
        Python::with_gil(|py| {
            let input = py.None();
            let res = a2a.invoke(py, "x".to_string(), input, None);
            assert!(res.is_err(), "expected RuntimeError without invoker");
        });
    }
}
