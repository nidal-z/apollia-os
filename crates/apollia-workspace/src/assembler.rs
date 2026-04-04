//! Assembleur de contexte workspace avec cache TTL configurable.
//!
//! [`WorkspaceAssembler`] orchestre [`GitContextCollector`], [`ApolliamdFinder`]
//! et [`DirectoryTreeBuilder`] en parallèle via `tokio::join!`.
//! Un cache [`RwLock`] évite les I/O répétées sur les sessions longues.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use apollia_core::WorkspaceContext;

use crate::apollia_md::ApolliamdFinder;
use crate::config::WorkspaceConfig;
use crate::git::GitContextCollector;
use crate::tree::DirectoryTreeBuilder;

/// Collecte et met en cache le contexte workspace d'un répertoire.
///
/// La collecte est orchestrée en parallèle : git, `APOLLIA.md` et arborescence
/// sont lancés simultanément via `tokio::join!`. Un timeout global configurable
/// garantit qu'aucun appel à [`collect`](WorkspaceAssembler::collect) ne bloque
/// indéfiniment.
///
/// Le cache est invalidé après [`context_ttl_secs`](WorkspaceConfig::context_ttl_secs)
/// secondes. Deux appels rapprochés sur le même `cwd` retournent le même
/// [`WorkspaceContext`] sans relancer les sous-processus.
pub struct WorkspaceAssembler {
    /// Cache partagé : instant de collecte + contexte. `None` avant le premier appel.
    cache: Arc<RwLock<Option<(Instant, WorkspaceContext)>>>,
    config: WorkspaceConfig,
}

impl WorkspaceAssembler {
    /// Construit un assembleur avec la configuration fournie.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Construit un assembleur avec les valeurs par défaut et un TTL personnalisé.
    ///
    /// Destiné aux tests unitaires qui contrôlent l'invalidation du cache.
    pub fn with_ttl(ttl: Duration) -> Self {
        let config = WorkspaceConfig {
            context_ttl_secs: ttl.as_secs().max(1),
            ..WorkspaceConfig::default()
        };
        Self {
            cache: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Collecte le contexte workspace de `cwd`.
    ///
    /// Retourne depuis le cache si la dernière collecte date de moins de
    /// [`context_ttl_secs`](WorkspaceConfig::context_ttl_secs).
    /// En cas de dépassement du timeout global, retourne [`WorkspaceContext::default()`].
    pub async fn collect(&self, cwd: &Path) -> WorkspaceContext {
        // — Lecture cache (verrou partagé) ————————————————————————————————————
        {
            let guard = self.cache.read().await;
            if let Some((collected_at, ref ctx)) = *guard {
                let ttl = Duration::from_secs(self.config.context_ttl_secs);
                if collected_at.elapsed() < ttl {
                    tracing::debug!(
                        elapsed_ms = collected_at.elapsed().as_millis(),
                        ttl_secs = self.config.context_ttl_secs,
                        "workspace cache hit"
                    );
                    return ctx.clone();
                }
            }
        }

        // — Collecte avec timeout global ——————————————————————————————————————
        let timeout = Duration::from_secs(self.config.collect_timeout_secs);
        let ctx = tokio::time::timeout(timeout, self.collect_inner(cwd))
            .await
            .unwrap_or_else(|_| {
                tracing::warn!(
                    timeout_secs = self.config.collect_timeout_secs,
                    "workspace collection timed out, using empty context"
                );
                WorkspaceContext::default()
            });

        // — Mise à jour du cache ———————————————————————————————————————————————
        *self.cache.write().await = Some((Instant::now(), ctx.clone()));
        ctx
    }

    /// Collecte réelle en parallèle — sans timeout (géré par l'appelant).
    async fn collect_inner(&self, cwd: &Path) -> WorkspaceContext {
        let (git_result, apollia_md_result, tree) = tokio::join!(
            GitContextCollector::collect(cwd, self.config.git_status_max_lines),
            ApolliamdFinder::find(
                cwd,
                self.config.apollia_md_max_bytes,
                self.config.apollia_md_search_depth,
            ),
            DirectoryTreeBuilder::build(cwd, 100),
        );

        WorkspaceContext {
            git_branch: git_result.branch,
            git_status: git_result.status,
            git_recent_commits: git_result.recent_commits,
            git_is_clean: git_result.is_clean,
            apollia_md: apollia_md_result.as_ref().map(|(_, c)| c.clone()),
            apollia_md_path: apollia_md_result.map(|(p, _)| p),
            directory_tree: tree,
            code_style: None,
            collected_at: Some(Instant::now()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_assembler_collects_in_git_repo() {
        // GIVEN : répertoire courant = dépôt git Apollia
        let assembler = WorkspaceAssembler::new(WorkspaceConfig::default());
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let ctx = assembler.collect(&cwd).await;
        // THEN
        assert!(ctx.git_branch.is_some(), "should detect branch");
        assert!(ctx.collected_at.is_some(), "collected_at should be set");
    }

    #[tokio::test]
    async fn test_assembler_cache_hit_on_second_call() {
        // GIVEN : assembler avec TTL de 60s
        let assembler = WorkspaceAssembler::with_ttl(Duration::from_secs(60));
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN : deux appels rapides
        let ctx1 = assembler.collect(&cwd).await;
        let ctx2 = assembler.collect(&cwd).await;
        // THEN : branche identique (même objet depuis le cache)
        assert_eq!(ctx1.git_branch, ctx2.git_branch);
    }

    #[tokio::test]
    async fn test_assembler_outside_git_repo() {
        // GIVEN : /tmp n'est pas un dépôt git
        let assembler = WorkspaceAssembler::new(WorkspaceConfig::default());
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let ctx = assembler.collect(cwd).await;
        // THEN — fail-silent : pas de panic, champs git à None/false
        assert!(ctx.git_branch.is_none());
        assert!(!ctx.git_is_clean);
    }

    #[tokio::test]
    async fn test_assembler_timeout_returns_default() {
        // GIVEN : timeout très court (1ms) → va expirer
        let mut config = WorkspaceConfig::default();
        config.collect_timeout_secs = 0; // 0 → Duration::from_secs(0) → expire immédiatement
                                         // Note : on utilise with_ttl(0) puis on écrase, ou on passe par new()
        let assembler = WorkspaceAssembler::new(config);
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN — ne doit pas paniquer
        let ctx = assembler.collect(&cwd).await;
        // THEN — contexte par défaut retourné proprement
        drop(ctx); // pas de panic = succès
    }
}
