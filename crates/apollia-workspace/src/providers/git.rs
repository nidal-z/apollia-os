//! [`GitWorkspaceProvider`] — provider de contexte git pour Apollia OS.
//!
//! Collecte la branche courante, l'état des fichiers, les commits récents,
//! le contenu d'`APOLLIA.md` et l'arborescence du répertoire.
//! Inactif hors d'un dépôt git (`is_applicable` retourne `false`).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::context::{ContextProvider, ContextSection, ContextSnapshot};
use apollia_llm::LlmRouter;

use crate::apollia_md::ApolliamdFinder;
use crate::config::WorkspaceConfig;
use crate::git::GitContextCollector;
use crate::style::StyleDetector;
use crate::tree::DirectoryTreeBuilder;

/// Fournit le contexte git, `APOLLIA.md` et l'arborescence du répertoire.
///
/// Inactif hors d'un dépôt git : [`is_applicable`](GitWorkspaceProvider::is_applicable)
/// remonte les répertoires parents à la recherche d'un `.git` et retourne `false` si
/// aucun n'est trouvé. Aucun overhead, aucune erreur si non applicable.
pub struct GitWorkspaceProvider {
    config: WorkspaceConfig,
    llm_router: Option<Arc<LlmRouter>>,
}

impl GitWorkspaceProvider {
    /// Construit un provider git avec la configuration fournie.
    ///
    /// La détection de style est désactivée — utiliser
    /// [`with_llm_router`](Self::with_llm_router) pour l'activer.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            config,
            llm_router: None,
        }
    }

    /// Active la détection de style en fournissant un backend LLM.
    pub fn with_llm_router(mut self, router: Arc<LlmRouter>) -> Self {
        self.llm_router = Some(router);
        self
    }
}

#[async_trait::async_trait]
impl ContextProvider for GitWorkspaceProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Branche git, status, commits récents et APOLLIA.md"
    }

    fn priority(&self) -> u8 {
        10
    }

    /// Retourne `false` si `cwd` n'est pas dans un dépôt git.
    ///
    /// Remonte les répertoires parents jusqu'à trouver un `.git`.
    /// Un provider inactif n'est jamais appelé par l'assembleur.
    fn is_applicable(&self, cwd: &Path) -> bool {
        std::iter::successors(Some(cwd), |p| p.parent()).any(|p| p.join(".git").exists())
    }

    /// Collecte le contexte git, `APOLLIA.md`, l'arborescence et le style de code.
    ///
    /// La collecte git, la recherche d'`APOLLIA.md` et la construction de
    /// l'arborescence sont lancées en parallèle via `tokio::join!`.
    /// La détection de style est optionnelle (nécessite un backend LLM configuré).
    async fn collect(&self, cwd: &Path) -> ContextSnapshot {
        // Lancer la détection de style en parallèle si un LLM est disponible.
        let style_handle = self.llm_router.as_ref().map(|llm| {
            let cwd_owned = cwd.to_path_buf();
            let llm_clone = llm.clone();
            let config = self.config.clone();
            tokio::spawn(
                async move { StyleDetector::detect(&cwd_owned, &llm_clone, &config).await },
            )
        });

        let (git_result, apollia_md_result, tree) = tokio::join!(
            GitContextCollector::collect(cwd, self.config.git_status_max_lines),
            ApolliamdFinder::find(
                cwd,
                self.config.apollia_md_max_bytes,
                self.config.apollia_md_search_depth,
            ),
            DirectoryTreeBuilder::build(cwd, 100),
        );

        // Attendre le résultat de la détection de style avec un timeout dédié.
        let code_style = if let Some(handle) = style_handle {
            tokio::time::timeout(
                Duration::from_millis(self.config.style_detection_timeout_ms),
                handle,
            )
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
        } else {
            None
        };

        // Construire les sections du snapshot.
        let source = "git";
        let mut sections: Vec<ContextSection> = Vec::new();

        if let Some(branch) = &git_result.branch {
            let clean_marker = if git_result.is_clean { " (clean)" } else { "" };
            let mut git_content = format!("Branche : {}{}", branch, clean_marker);
            if let Some(status) = &git_result.status {
                git_content.push_str(&format!("\n\nStatus :\n{}", status));
            }
            if !git_result.recent_commits.is_empty() {
                git_content.push_str(&format!(
                    "\n\nCommits récents :\n{}",
                    git_result.recent_commits.join("\n")
                ));
            }
            sections.push(ContextSection {
                title: "Git".to_owned(),
                content: git_content,
                source: source.to_owned(),
            });
        }

        if let Some((_, content)) = apollia_md_result {
            sections.push(ContextSection {
                title: "Règles du projet".to_owned(),
                content,
                source: source.to_owned(),
            });
        }

        if !tree.is_empty() {
            sections.push(ContextSection {
                title: "Arborescence".to_owned(),
                content: tree,
                source: source.to_owned(),
            });
        }

        if let Some(style) = code_style {
            sections.push(ContextSection {
                title: "Style de code".to_owned(),
                content: style,
                source: source.to_owned(),
            });
        }

        ContextSnapshot {
            source: source.to_owned(),
            sections,
            errors: vec![],
            collected_at: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn git_provider_inactive_outside_repo() {
        // GIVEN un répertoire sans .git
        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = GitWorkspaceProvider::new(WorkspaceConfig::default());
        // WHEN / THEN
        assert!(!provider.is_applicable(tmp.path()));
    }

    #[tokio::test]
    async fn git_provider_active_in_repo() {
        // GIVEN un répertoire avec .git
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tmp.path().join(".git")).expect("create .git");
        let provider = GitWorkspaceProvider::new(WorkspaceConfig::default());
        // WHEN / THEN
        assert!(provider.is_applicable(tmp.path()));
    }
}
