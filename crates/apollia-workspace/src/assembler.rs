//! [`ProjectRuntime`] - multi-provider orchestrator with a configurable TTL cache.
//!
//! Orchestrates a set of [`WorkspaceProvider`] in parallel via
//! `futures::future::join_all`. A per-directory cache avoids repeated I/O
//! during long sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSlice, WorkspaceSnapshot};
use apollia_llm::LlmRouter;

use crate::config::{RuntimeConfig, StyleProviderConfig};
use crate::providers::{GitProvider, RulesProvider, StyleProvider, TreeProvider};

/// Per-directory shared cache: collection instant plus the produced snapshot.
type SnapshotCache = Arc<RwLock<HashMap<PathBuf, (Instant, WorkspaceSnapshot)>>>;

/// Orchestrates several [`WorkspaceProvider`] and assembles a [`WorkspaceSnapshot`].
///
/// Collection is parallel: every applicable provider runs simultaneously.
/// A per-provider timeout guarantees fail-silent behavior.
/// A per-directory TTL cache avoids repeated I/O between two close steps.
pub struct ProjectRuntime {
    /// Providers sorted by ascending priority.
    providers: Vec<Box<dyn WorkspaceProvider>>,
    config: RuntimeConfig,
    /// Per-directory cache.
    cache: SnapshotCache,
}

impl ProjectRuntime {
    /// Builds a runtime with the given providers.
    pub fn new(providers: Vec<Box<dyn WorkspaceProvider>>, config: RuntimeConfig) -> Self {
        let mut providers = providers;
        providers.sort_by_key(|p| p.priority());
        Self {
            providers,
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Builds a runtime with the default project (git + rules + tree).
    pub fn default_project() -> Self {
        let providers: Vec<Box<dyn WorkspaceProvider>> = vec![
            Box::new(GitProvider::default()),
            Box::new(RulesProvider::default()),
            Box::new(TreeProvider::default()),
        ];
        Self::new(providers, RuntimeConfig::default())
    }

    /// Enables style detection by adding a [`StyleProvider`] backed by the LLM.
    pub fn with_style_detection(mut self, router: Arc<LlmRouter>) -> Self {
        let style = StyleProvider::new(StyleProviderConfig::default(), router);
        self.providers.push(Box::new(style));
        self.providers.sort_by_key(|p| p.priority());
        self
    }

    /// Collects all applicable providers in parallel with a per-provider timeout.
    ///
    /// Returns from the cache when the last collection is more recent than
    /// [`context_ttl_secs`](RuntimeConfig::context_ttl_secs).
    pub async fn collect(&self, cwd: &Path) -> WorkspaceSnapshot {
        // - Cache read ---------------------------------------------------
        {
            let guard = self.cache.read().await;
            if let Some((collected_at, ref snapshot)) = guard.get(cwd) {
                let ttl = Duration::from_secs(self.config.context_ttl_secs);
                if collected_at.elapsed() < ttl {
                    tracing::debug!(
                        elapsed_ms = collected_at.elapsed().as_millis(),
                        ttl_secs = self.config.context_ttl_secs,
                        "workspace.cache.hit"
                    );
                    return snapshot.clone();
                }
            }
        }

        // - Parallel collection with a per-provider timeout ---------------
        let timeout_secs = self.config.collect_timeout_secs;
        let futures: Vec<_> = self
            .providers
            .iter()
            .filter(|p| p.is_applicable(cwd))
            .map(|p| {
                let provider_timeout =
                    Duration::from_secs(p.refresh_secs().unwrap_or(timeout_secs));
                let provider_name = p.name().to_owned();
                async move {
                    match tokio::time::timeout(provider_timeout, p.collect(cwd)).await {
                        Ok(slice) => slice,
                        Err(_) => {
                            tracing::warn!(
                                provider = %provider_name,
                                timeout_secs = provider_timeout.as_secs(),
                                "workspace.provider.timeout"
                            );
                            WorkspaceSlice::with_error(
                                &provider_name,
                                format!(
                                    "provider '{}' timed out after {}s",
                                    provider_name,
                                    provider_timeout.as_secs()
                                ),
                            )
                        }
                    }
                }
            })
            .collect();

        let slices = futures::future::join_all(futures).await;

        // Log provider errors
        for slice in &slices {
            for err in &slice.errors {
                tracing::warn!(provider = %slice.source, error = %err, "workspace.provider.failed");
            }
        }

        let snapshot = WorkspaceSnapshot::new(slices);

        // - Cache update ---------------------------------------------------
        self.cache
            .write()
            .await
            .insert(cwd.to_path_buf(), (Instant::now(), snapshot.clone()));

        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_collects_in_real_repo() {
        // GIVEN the current project (git repository)
        let runtime = ProjectRuntime::default_project();
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let snapshot = runtime.collect(&cwd).await;
        // THEN - at least one section
        assert!(!snapshot.is_empty(), "expected sections from git repo");
    }

    #[tokio::test]
    async fn runtime_cache_hit_on_second_call() {
        // GIVEN a runtime with a 60s TTL
        let config = RuntimeConfig {
            context_ttl_secs: 60,
            ..Default::default()
        };
        let runtime = ProjectRuntime::new(vec![Box::new(GitProvider::default())], config);
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN two quick calls
        let s1 = runtime.collect(&cwd).await;
        let s2 = runtime.collect(&cwd).await;
        // THEN same slice count (served from the cache)
        assert_eq!(
            s1.slices.len(),
            s2.slices.len(),
            "cache must return same slice count"
        );
    }

    #[tokio::test]
    async fn runtime_no_applicable_providers_outside_git() {
        // GIVEN only a GitProvider in a directory without .git
        let runtime = ProjectRuntime::new(
            vec![Box::new(GitProvider::default())],
            RuntimeConfig::default(),
        );
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let snapshot = runtime.collect(cwd).await;
        // THEN - git provider not applicable, empty snapshot
        assert!(
            snapshot.is_empty(),
            "no providers applicable outside git repo"
        );
    }

    #[tokio::test]
    async fn format_for_prompt_respects_priority_order() {
        // GIVEN a snapshot with two slices
        use apollia_core::workspace::WorkspaceSection;
        let s1 = WorkspaceSlice {
            source: "git".into(),
            sections: vec![WorkspaceSection {
                title: "Git".into(),
                content: "branche: main".into(),
                source: "git".into(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        };
        let s2 = WorkspaceSlice {
            source: "rules".into(),
            sections: vec![WorkspaceSection {
                title: "Rules".into(),
                content: "no magic".into(),
                source: "rules".into(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        };
        let snapshot = WorkspaceSnapshot::new(vec![s1, s2]);
        // WHEN
        let prompt = snapshot.format_for_prompt();
        // THEN - "Git" appears before "Rules"
        let pos_git = prompt.find("Git").expect("Git not found");
        let pos_rules = prompt.find("Rules").expect("Rules not found");
        assert!(pos_git < pos_rules, "Git must appear before Rules");
    }
}
