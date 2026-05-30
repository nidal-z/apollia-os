//! [`ProjectRuntime`] factory from the runtime configuration.
//!
//! Builds a [`ProjectRuntime`] with the enabled providers.
//! Python providers must be added via [`ProjectRuntime::with_provider`]
//! by the caller after construction (they require Python initialization).

use std::sync::Arc;

use apollia_llm::LlmRouter;
use apollia_workspace::{ProjectRuntime, ProviderEntry};

/// Build a [`ProjectRuntime`] from the list of configured providers.
///
/// Supported providers:
/// - `type = "git"`: [`GitProvider`]
/// - `type = "rules"`: [`RulesProvider`]
/// - `type = "tree"`: [`TreeProvider`]
/// - `type = "style"`: [`StyleProvider`] (requires an `LlmRouter`)
/// - `type = "script"`: [`ScriptProvider`]
///
/// Providers of type `"python"` must be added separately via
/// [`ProjectRuntime::with_provider`].
pub fn build_project_runtime(
    providers: &[ProviderEntry],
    llm_router: Option<Arc<LlmRouter>>,
) -> ProjectRuntime {
    ProjectRuntime::from_providers_config(providers, llm_router)
}
