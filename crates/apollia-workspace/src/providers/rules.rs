//! [`RulesProvider`] provides the project rules file.
//!
//! Searches for a rules file (`APOLLIA.md` by default) by walking up the
//! directory hierarchy from the CWD. The content is injected verbatim into
//! the system prompt under the "Project rules" section.

use std::path::Path;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};

use crate::apollia_md::ApolliamdFinder;
use crate::config::RulesProviderConfig;

/// Provides the content of the project rules file (APOLLIA.md or a custom name).
///
/// The file is searched from `cwd` upward through parent directories, up to
/// `config.search_depth` levels. The content is truncated to `config.max_bytes`
/// to protect the LLM context window.
pub struct RulesProvider {
    config: RulesProviderConfig,
}

impl RulesProvider {
    /// Builds a rules provider with the given configuration.
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
        // Reuse ApolliamdFinder, but allow a configurable file name. When the
        // configured name is not "APOLLIA.md", search the CWD and its parents
        // directly with the provided name.
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

/// Searches for a rules file by name from `cwd` upward through parents.
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
        // GIVEN a directory containing APOLLIA.md
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
        // GIVEN a directory with no rules file
        let dir = tempfile::tempdir().expect("tempdir");
        let provider = RulesProvider::default();
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        assert!(slice.is_empty(), "no rules file → empty slice");
    }

    #[tokio::test]
    async fn rules_provider_custom_file_name() {
        // GIVEN a rules file with a custom name
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
