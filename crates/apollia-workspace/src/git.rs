//! Collecteur de contexte git via sous-processus.
//!
//! Toutes les commandes git sont lancées via [`tokio::process::Command`].
//! Aucune dépendance `git2` — zéro dépendance C supplémentaire (Principe #2).
//! Toute erreur (git absent, répertoire hors dépôt) est absorbée silencieusement.

use std::path::Path;

/// Résultat brut de la collecte git pour un répertoire donné.
///
/// Tous les champs sont optionnels ou ont un défaut inerte — jamais de panic
/// si git est absent ou si le répertoire n'est pas un dépôt.
#[derive(Debug, Default)]
pub struct GitResult {
    /// Branche courante (`git branch --show-current`). `None` hors dépôt.
    pub branch: Option<String>,
    /// Output de `git status --short` tronqué, ou `None` si working tree propre / hors dépôt.
    pub status: Option<String>,
    /// 5 derniers commits (`git log --oneline -n5`).
    pub recent_commits: Vec<String>,
    /// `true` si `git status --short` produit un output vide.
    pub is_clean: bool,
}

/// Collecte le contexte git d'un répertoire via des sous-processus.
///
/// Toutes les méthodes sont fail-silent : une erreur git (commande absente,
/// répertoire hors dépôt, permissions) produit des valeurs `None` ou vides
/// sans jamais émettre de log `ERROR`.
pub struct GitContextCollector;

impl GitContextCollector {
    /// Collecte la branche, le statut et les commits récents du dépôt git en `cwd`.
    ///
    /// Lance trois sous-processus en séquence. Chacun est indépendant :
    /// l'échec de l'un n'empêche pas les autres.
    pub async fn collect(cwd: &Path, git_status_max_lines: usize) -> GitResult {
        let branch = Self::run_git(cwd, &["branch", "--show-current"])
            .await
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());

        let status_raw = Self::run_git(cwd, &["status", "--short"]).await;
        let is_clean = status_raw
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(false);
        let status = status_raw
            .map(|s| {
                s.lines()
                    .take(git_status_max_lines)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|s| !s.is_empty());

        let commits_raw = Self::run_git(cwd, &["log", "--oneline", "-n5"]).await;
        let recent_commits = commits_raw
            .map(|s| s.lines().map(|l| l.to_owned()).collect())
            .unwrap_or_default();

        GitResult {
            branch,
            status,
            recent_commits,
            is_clean,
        }
    }

    /// Lance une commande git avec les arguments donnés dans `cwd`.
    ///
    /// Retourne `Some(stdout)` si la commande réussit (exit code 0 et UTF-8 valide),
    /// `None` dans tous les autres cas.
    async fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_collector_in_repo() {
        // GIVEN : CWD = répertoire du repo Apollia lui-même
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let result = GitContextCollector::collect(&cwd, 200).await;
        // THEN
        assert!(
            result.branch.is_some(),
            "should detect branch in a git repo"
        );
    }

    #[tokio::test]
    async fn test_git_collector_outside_repo() {
        // GIVEN : /tmp n'est pas un dépôt git
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let result = GitContextCollector::collect(cwd, 200).await;
        // THEN
        assert!(result.branch.is_none(), "no branch outside a git repo");
        assert!(result.status.is_none(), "no status outside a git repo");
    }

    #[tokio::test]
    async fn test_git_status_max_lines_respected() {
        // GIVEN : repo avec au moins un fichier modifié (ou pas — on teste la limite)
        let cwd = std::env::current_dir().expect("current_dir");
        let max_lines = 2;
        // WHEN
        let result = GitContextCollector::collect(&cwd, max_lines).await;
        // THEN — si du statut existe, il ne dépasse pas max_lines lignes
        if let Some(status) = &result.status {
            assert!(
                status.lines().count() <= max_lines,
                "status truncated to {max_lines}: got {}",
                status.lines().count()
            );
        }
    }
}
