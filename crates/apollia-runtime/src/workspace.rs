//! Factory de [`ProjectRuntime`] depuis la configuration runtime.
//!
//! Construit un [`ProjectRuntime`] avec les providers activés.
//! Les providers Python doivent être ajoutés via [`ProjectRuntime::with_provider`]
//! par l'appelant après la construction (nécessitent une initialisation Python).

use std::sync::Arc;

use apollia_llm::LlmRouter;
use apollia_workspace::{ProjectRuntime, ProviderEntry};

/// Construit un [`ProjectRuntime`] depuis la liste de providers configurés.
///
/// Providers supportés :
/// - `type = "git"` → [`GitProvider`]
/// - `type = "rules"` → [`RulesProvider`]
/// - `type = "tree"` → [`TreeProvider`]
/// - `type = "style"` → [`StyleProvider`] (nécessite un `LlmRouter`)
/// - `type = "script"` → [`ScriptProvider`]
///
/// Les providers de type `"python"` doivent être ajoutés séparément via
/// [`ProjectRuntime::with_provider`].
pub fn build_project_runtime(
    providers: &[ProviderEntry],
    llm_router: Option<Arc<LlmRouter>>,
) -> ProjectRuntime {
    ProjectRuntime::from_providers_config(providers, llm_router)
}
