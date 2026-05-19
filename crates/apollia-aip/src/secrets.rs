//! ctx.secrets — read-only credentials access for agents (LOT 4/6 — ADR-104).
//!
//! Encapsule un [`ToolCredentialStore`](apollia_tools::ToolCredentialStore)
//! existant (AES-256-GCM) avec **gating manifest** : un agent ne voit que
//! les clés déclarées dans `@agent(secrets=...)`. Toute clé non déclarée est
//! invisible (retourne `None`), même si elle existe en base.
//!
//! Le store n'est pas exposé en écriture aux agents — les ops gèrent les
//! secrets via la CLI / le desktop. Cette interface est lecture-seule par
//! conception (ADR-104).
//!
//! **Note LOT 4 (transition)** : ce module pose la surface Python `ctx.secrets`
//! et la sémantique de gating. Le câblage effectif avec un
//! `ToolCredentialStore` partagé sera fait en LOT 6 — pour le moment, le
//! constructeur accepte le store mais l'interface fonctionne aussi en mode
//! dégradé (`store = None`) avec gating fonctionnel et toutes les valeurs à
//! `None`. Le namespace tool utilisé pour les agents est `"agent"` par
//! convention (LOT 6 pourra le rendre paramétrable par agent).

use std::sync::{Arc, Mutex};

use apollia_tools::ToolCredentialStore;
use pyo3::prelude::*;

/// Namespace `tool_name` utilisé dans `ToolCredentialStore` pour les
/// credentials propres à un agent (par opposition aux secrets propres à un
/// outil natif comme `web_search`). Convention temporaire LOT 4/6.
const AGENT_SECRETS_NAMESPACE: &str = "agent";

/// Interface lecture-seule exposée à l'agent via `ctx.secrets`.
///
/// Construit avec :
/// - `store` — optionnel ; `None` pour les tests / runtimes sans store.
/// - `declared` — copie de `manifest.secrets`, agit comme allowlist stricte.
#[pyclass(name = "SecretsInterface", module = "apollia._native")]
pub struct SecretsInterface {
    /// Store partagé (lecture seule depuis l'agent). `None` désactive la
    /// résolution effective mais conserve la sémantique de gating.
    ///
    /// Wrappé dans `Mutex` parce que [`ToolCredentialStore`] contient une
    /// [`rusqlite::Connection`] qui n'est pas `Sync` (à cause du
    /// `RefCell<StatementCache>`). Le `Mutex` n'introduit pas de contention
    /// dans la pratique : les agents sont single-threaded côté Python et le
    /// store est lu rarement (au plus quelques fois par tâche).
    store: Option<Arc<Mutex<ToolCredentialStore>>>,
    /// Liste des clés autorisées au manifest. Lookup linéaire car cette
    /// liste reste petite (≤ 10 typiquement).
    declared: Vec<String>,
    /// Namespace `tool_name` à utiliser pour chercher les credentials de cet
    /// agent dans le store. Par défaut [`AGENT_SECRETS_NAMESPACE`] ;
    /// surchargeable via [`Self::with_namespace`] pour les tests / les
    /// futurs scenarios par-agent (LOT 6).
    namespace: String,
}

#[pymethods]
impl SecretsInterface {
    /// Retourne la valeur du secret `key` ou `None` si inconnu / non
    /// configuré.
    ///
    /// **Pas d'exception** : un secret manquant est légitime (agent qui
    /// dégrade gracieusement). Pour faire échouer fail-fast, l'agent
    /// vérifie lui-même la présence et lève l'erreur métier appropriée.
    ///
    /// **Gating** : si `key` n'est pas dans la liste déclarée au manifest,
    /// le retour est `None` même si la valeur existe en base. Ce comportement
    /// est silencieux — la déclaration manifest est la **seule** source
    /// d'autorité (Principe #1, ADR-104).
    fn get(&self, key: &str) -> PyResult<Option<String>> {
        if !self.declared.iter().any(|d| d == key) {
            return Ok(None);
        }
        match &self.store {
            Some(store) => {
                let guard = match store.lock() {
                    Ok(g) => g,
                    Err(p) => {
                        tracing::error!(
                            target: "apollia.aip.secrets",
                            "secret store mutex poisoned for '{key}': {p}"
                        );
                        return Ok(None);
                    }
                };
                match guard.get(&self.namespace, key) {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        tracing::warn!(
                            target: "apollia.aip.secrets",
                            "secret read error for '{key}': {e}"
                        );
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// `True` si la clé est déclarée ET configurée en base.
    ///
    /// Strictement équivalent à `ctx.secrets.get(key) is not None` côté
    /// agent. Forme idiomatique pour la branchement précoce.
    fn has(&self, key: &str) -> PyResult<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Liste les clés déclarées au manifest (jamais les valeurs).
    ///
    /// Permet à un agent d'auto-introspecter sa configuration au démarrage
    /// sans hardcoder les noms. Aucune fuite : on ne révèle que les noms
    /// que l'agent a lui-même déclarés.
    fn list_names(&self) -> Vec<String> {
        self.declared.clone()
    }
}

impl SecretsInterface {
    /// Construit l'interface secrets avec le store partagé et la liste
    /// déclarée. Utilise le namespace par défaut
    /// ([`AGENT_SECRETS_NAMESPACE`]).
    pub fn new(
        store: Option<Arc<Mutex<ToolCredentialStore>>>,
        declared: Vec<String>,
    ) -> Self {
        Self {
            store,
            declared,
            namespace: AGENT_SECRETS_NAMESPACE.to_string(),
        }
    }

    /// Surcharge le namespace `tool_name` utilisé pour la résolution. Permet
    /// d'isoler les secrets par agent en LOT 6 (ex. `"agent::veille-ia"`).
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_without_store_returns_none() {
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        let v = s.get("API_KEY").expect("get should not raise");
        assert!(v.is_none(), "no store -> None");
    }

    #[test]
    fn test_undeclared_key_returns_none() {
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        // Un secret existant en base mais non déclaré : toujours None.
        let v = s.get("OTHER_KEY").expect("get");
        assert!(v.is_none(), "undeclared keys are invisible");
    }

    #[test]
    fn test_has_mirrors_get() {
        let s = SecretsInterface::new(None, vec!["API_KEY".to_string()]);
        assert!(!s.has("API_KEY").expect("has"));
        assert!(!s.has("UNKNOWN").expect("has"));
    }

    #[test]
    fn test_list_names_returns_declared() {
        let s = SecretsInterface::new(None, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(s.list_names(), vec!["A", "B"]);
    }
}
