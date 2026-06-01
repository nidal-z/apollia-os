//! [`StyleProvider`] - détection automatique du style de code via LLM.
//!
//! Provider optionnel : nécessite un [`LlmRouter`] configuré.
//! Si le LLM est indisponible ou dépasse le timeout, la section est omise
//! silencieusement sans bloquer la collecte des autres providers.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};
use apollia_llm::LlmRouter;

use crate::config::StyleProviderConfig;
use crate::style::StyleDetector;

/// Fournit les conventions de code détectées automatiquement via LLM léger.
///
/// Actif uniquement si un [`LlmRouter`] est fourni à la construction.
/// La détection est bornée par `config.timeout_ms` - jamais de blocage.
pub struct StyleProvider {
    config: StyleProviderConfig,
    llm_router: Option<Arc<LlmRouter>>,
}

impl StyleProvider {
    /// Construit un provider de style avec un backend LLM.
    pub fn new(config: StyleProviderConfig, llm_router: Arc<LlmRouter>) -> Self {
        Self {
            config,
            llm_router: Some(llm_router),
        }
    }

    /// Construit un provider de style sans LLM (retourne toujours une tranche vide).
    pub fn disabled() -> Self {
        Self {
            config: StyleProviderConfig::default(),
            llm_router: None,
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for StyleProvider {
    fn name(&self) -> &str {
        "style"
    }

    fn description(&self) -> &str {
        "Conventions de code détectées automatiquement via LLM"
    }

    fn priority(&self) -> u8 {
        40
    }

    fn is_applicable(&self, _cwd: &Path) -> bool {
        self.llm_router.is_some()
    }

    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let Some(router) = &self.llm_router else {
            return WorkspaceSlice::empty("style");
        };

        let style = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            StyleDetector::detect(cwd, router, &self.config),
        )
        .await
        .ok()
        .flatten();

        match style {
            Some(content) => WorkspaceSlice {
                source: "style".to_owned(),
                sections: vec![WorkspaceSection {
                    title: "Style de code".to_owned(),
                    content,
                    source: "style".to_owned(),
                }],
                errors: vec![],
                collected_at: std::time::Instant::now(),
            },
            None => WorkspaceSlice::empty("style"),
        }
    }
}
