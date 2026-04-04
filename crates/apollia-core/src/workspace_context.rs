//! Type partagé représentant le contexte du projet courant.
//!
//! [`WorkspaceContext`] est collecté par `apollia-workspace` et injecté dans
//! les system prompts LLM par `apollia-oria` et `apollia-aip`.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Contexte du workspace collecté au démarrage d'une tâche ORIA.
///
/// Agrège les informations locales du projet : branche git, fichiers modifiés,
/// contenu de `APOLLIA.md` et arborescence. Sérialisable pour le transport
/// REST ; le champ [`collected_at`](WorkspaceContext::collected_at) est skippé
/// à la sérialisation car [`Instant`] n'est pas sérialisable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceContext {
    /// Branche git courante (`git branch --show-current`).
    ///
    /// `None` si le répertoire n'est pas un dépôt git ou si git n'est pas installé.
    pub git_branch: Option<String>,

    /// Output de `git status --short`, tronqué à `git_status_max_lines` lignes.
    ///
    /// `None` si le répertoire n'est pas un dépôt git, ou si le working tree
    /// est propre et que l'output est vide.
    pub git_status: Option<String>,

    /// Les 5 derniers commits (`git log --oneline -n5`).
    ///
    /// Vecteur vide si le dépôt n'a pas encore de commits ou si git est absent.
    pub git_recent_commits: Vec<String>,

    /// `true` si le working tree ne contient aucune modification non committée.
    pub git_is_clean: bool,

    /// Contenu du fichier `APOLLIA.md` le plus proche (CWD puis parents),
    /// limité à `apollia_md_max_bytes` octets via middle-trim.
    ///
    /// `None` si aucun fichier `APOLLIA.md` n'est trouvé dans la hiérarchie.
    pub apollia_md: Option<String>,

    /// Chemin absolu du fichier `APOLLIA.md` trouvé.
    pub apollia_md_path: Option<PathBuf>,

    /// Arborescence textuelle du répertoire, limitée à 100 lignes.
    ///
    /// Répertoires ignorés : `.git`, `target`, `node_modules`, `__pycache__`,
    /// `.DS_Store`, `.next`, `dist`.
    pub directory_tree: String,

    /// Style de code détecté par `StyleDetector` (STORY-460).
    ///
    /// `None` tant que STORY-460 n'est pas implémenté.
    pub code_style: Option<String>,

    /// Instant de collecte, utilisé pour l'invalidation du cache TTL.
    ///
    /// Skippé à la sérialisation JSON — [`Instant`] n'est pas sérialisable.
    #[serde(skip)]
    pub collected_at: Option<Instant>,
}

impl WorkspaceContext {
    /// Formate le contexte pour injection dans un system prompt LLM.
    ///
    /// Produit un bloc texte structuré avec les sections pertinentes :
    /// état git, règles projet (`APOLLIA.md`) et arborescence.
    /// Les sections absentes (`None`) sont omises du résultat.
    pub fn format_for_prompt(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // — Section git ——————————————————————————————————————————————————————
        if let Some(branch) = &self.git_branch {
            let clean_marker = if self.git_is_clean { " (clean)" } else { "" };
            parts.push(format!("Git branch: {}{}", branch, clean_marker));
        }

        if let Some(status) = &self.git_status {
            parts.push(format!("Git status:\n{}", status));
        }

        if !self.git_recent_commits.is_empty() {
            parts.push(format!(
                "Recent commits:\n{}",
                self.git_recent_commits.join("\n")
            ));
        }

        // — Section APOLLIA.md ———————————————————————————————————————————————
        if let Some(content) = &self.apollia_md {
            parts.push(format!("Project rules (APOLLIA.md):\n{}", content));
        }

        // — Section arborescence —————————————————————————————————————————————
        if !self.directory_tree.is_empty() {
            parts.push(format!("Directory structure:\n{}", self.directory_tree));
        }

        parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_for_prompt_clean_branch() {
        // GIVEN
        let ctx = WorkspaceContext {
            git_branch: Some("main".to_owned()),
            git_is_clean: true,
            apollia_md: Some("# Rules".to_owned()),
            directory_tree: "src/\n  lib.rs".to_owned(),
            ..Default::default()
        };
        // WHEN
        let output = ctx.format_for_prompt();
        // THEN
        assert!(
            output.contains("Git branch: main (clean)"),
            "branch: {output}"
        );
        assert!(
            output.contains("Project rules (APOLLIA.md):"),
            "apollia_md: {output}"
        );
        assert!(output.contains("Directory structure:"), "tree: {output}");
    }

    #[test]
    fn test_format_for_prompt_dirty_branch() {
        // GIVEN
        let ctx = WorkspaceContext {
            git_branch: Some("feature/x".to_owned()),
            git_is_clean: false,
            git_status: Some("M src/lib.rs".to_owned()),
            ..Default::default()
        };
        // WHEN
        let output = ctx.format_for_prompt();
        // THEN — pas de "(clean)" quand dirty
        assert!(
            output.contains("Git branch: feature/x\n")
                || output.contains("Git branch: feature/x\r")
        );
        assert!(!output.contains("(clean)"), "should not be clean: {output}");
        assert!(output.contains("Git status:"), "status: {output}");
    }

    #[test]
    fn test_format_for_prompt_no_git() {
        // GIVEN — pas de repo git
        let ctx = WorkspaceContext::default();
        // WHEN
        let output = ctx.format_for_prompt();
        // THEN — vide (pas de section vide)
        assert!(
            output.is_empty(),
            "empty context must produce empty output: {output}"
        );
    }

    #[test]
    fn test_workspace_context_serde_roundtrip() {
        // GIVEN
        let ctx = WorkspaceContext {
            git_branch: Some("main".to_owned()),
            git_is_clean: true,
            collected_at: Some(Instant::now()),
            ..Default::default()
        };
        // WHEN — serde_json doit ignorer collected_at
        let json = serde_json::to_string(&ctx).expect("serialize");
        let restored: WorkspaceContext = serde_json::from_str(&json).expect("deserialize");
        // THEN
        assert_eq!(restored.git_branch, Some("main".to_owned()));
        assert!(
            restored.collected_at.is_none(),
            "collected_at must be skipped"
        );
    }
}
