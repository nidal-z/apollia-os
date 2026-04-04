//! Câblage du [`WorkspaceAssembler`] depuis la configuration runtime.
//!
//! Fournit une factory qui construit un [`WorkspaceAssembler`] configuré
//! depuis la section `[workspace]` et `[[workspace.providers]]` de `apollia.toml`.

use std::sync::Arc;

use apollia_llm::LlmRouter;
use apollia_workspace::{
    config::{ProviderConfig, WorkspaceConfig},
    WorkspaceAssembler,
};

/// Construit un [`WorkspaceAssembler`] depuis la configuration `[workspace]`.
///
/// Providers supportés depuis la configuration :
/// - `type = "builtin"`, `name = "git"` → [`GitWorkspaceProvider`]
/// - `type = "script"` → [`ScriptContextProvider`]
/// - `type = "python"` → doit être ajouté via [`WorkspaceAssembler::with_provider`]
///   par l'appelant (nécessite une initialisation Python).
///
/// Les providers désactivés (`enabled = false`) sont ignorés silencieusement.
pub fn build_assembler(
    config: &WorkspaceConfig,
    providers_config: &[ProviderConfig],
    llm_router: Option<Arc<LlmRouter>>,
) -> WorkspaceAssembler {
    WorkspaceAssembler::from_config(config, providers_config, llm_router)
}
