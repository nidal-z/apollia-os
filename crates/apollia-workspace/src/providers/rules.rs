//! [`RulesProvider`] - fournit le fichier de règles projet.
//!
//! Cherche un fichier de règles (par défaut `APOLLIA.md`) en remontant la
//! hiérarchie de répertoires depuis le CWD. Le contenu est injecté tel quel
//! dans le system prompt sous la section "Règles du projet".

use std::path::Path;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};

use crate::apollia_md::ApolliamdFinder;
use crate::config::RulesProviderConfig;

/// Fournit le contenu du fichier de règles projet (APOLLIA.md ou personnalisé).
///
/// Le fichier est cherché depuis `cwd` vers les répertoires parents, jusqu'à
/// `config.search_depth` niveaux. Le contenu est tronqué à `config.max_bytes`
/// pour protéger la fenêtre de contexte LLM.
pub struct RulesProvider {
    config: RulesProviderConfig,
}

impl RulesProvider {
    /// Construit un provider de règles avec la configuration fournie.
    pub fn new(config: RulesProviderConfig) -> Self {
        Self { config }
    }
}

impl Default for RulesProvider {
    fn default() -> Self {
        Self::new(RulesProviderConfig::default())
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for RulesProvider {
    fn name(&self) -> &str {
        "rules"
    }

    fn description(&self) -> &str {
        "Fichier de règles projet (APOLLIA.md ou personnalisé)"
    }

    fn priority(&self) -> u8 {
        20
    }

    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        // Utilise ApolliamdFinder mais avec le nom de fichier configurable.
        // Pour l'instant, si le fichier configuré n'est pas "APOLLIA.md", on cherche
        // directement dans le CWD et les parents avec le nom fourni.
        let result = if self.config.file_name == "APOLLIA.md" {
            ApolliamdFinder::find(cwd, self.config.max_bytes, self.config.search_depth).await
        } else {
            find_rules_file(
                cwd,
                &self.config.file_name,
                self.config.max_bytes,
                self.config.search_depth,
            )
            .await
        };

        match result {
            Some((_, content)) => WorkspaceSlice {
                source: "rules".to_owned(),
                sections: vec![WorkspaceSection {
                    title: "Règles du projet".to_owned(),
                    content,
                    source: "rules".to_owned(),
                }],
                errors: vec![],
                collected_at: std::time::Instant::now(),
            },
            None => WorkspaceSlice::empty("rules"),
        }
    }
}

/// Cherche un fichier de règles par nom depuis `cwd` vers les parents.
async fn find_rules_file(
    cwd: &Path,
    file_name: &str,
    max_bytes: usize,
    search_depth: usize,
) -> Option<(std::path::PathBuf, String)> {
    use apollia_core::truncate_middle;
    let mut current = cwd.to_owned();
    for _ in 0..=search_depth {
        let candidate = current.join(file_name);
        if candidate.exists() {
            let content = tokio::fs::read_to_string(&candidate).await.ok()?;
            let (truncated, _) = truncate_middle(&content, max_bytes);
            return Some((candidate, truncated));
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rules_provider_finds_default_file() {
        // GIVEN un répertoire avec APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(
            dir.path().join("APOLLIA.md"),
            "# Règles\nFaire du bon code.",
        )
        .await
        .expect("write");
        let provider = RulesProvider::default();
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        assert!(!slice.is_empty(), "should find APOLLIA.md");
        assert!(slice.sections[0].content.contains("Règles"));
    }

    #[tokio::test]
    async fn rules_provider_empty_when_no_file() {
        // GIVEN un répertoire sans fichier de règles
        let dir = tempfile::tempdir().expect("tempdir");
        let provider = RulesProvider::default();
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        assert!(slice.is_empty(), "no rules file → empty slice");
    }

    #[tokio::test]
    async fn rules_provider_custom_file_name() {
        // GIVEN un fichier de règles avec un nom custom
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("PROJECT.md"), "Custom rules here.")
            .await
            .expect("write");
        let provider = RulesProvider::new(RulesProviderConfig {
            file_name: "PROJECT.md".to_owned(),
            ..Default::default()
        });
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        assert!(!slice.is_empty());
        assert!(slice.sections[0].content.contains("Custom rules"));
    }
}
