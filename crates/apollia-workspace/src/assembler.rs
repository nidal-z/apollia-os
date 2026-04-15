//! [`ProjectRuntime`] — orchestrateur multi-provider avec cache TTL configurable.
//!
//! Orchestre un ensemble de [`WorkspaceProvider`] en parallèle via
//! `futures::future::join_all`. Un cache par répertoire évite les I/O répétées
//! sur les sessions longues.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSlice, WorkspaceSnapshot};
use apollia_llm::LlmRouter;

use crate::config::{GitProviderConfig, RulesProviderConfig, RuntimeConfig, StyleProviderConfig};
use crate::providers::{GitProvider, RulesProvider, ScriptProvider, StyleProvider, TreeProvider};

/// Cache partagé par répertoire : instant de collecte + snapshot produit.
type SnapshotCache = Arc<RwLock<HashMap<PathBuf, (Instant, WorkspaceSnapshot)>>>;

/// Orchestre plusieurs [`WorkspaceProvider`] et assemble un [`WorkspaceSnapshot`].
///
/// La collecte est parallèle : chaque provider applicable est lancé simultanément.
/// Un timeout par provider garantit le fail-silent.
/// Un cache TTL par répertoire évite les I/O répétées entre deux steps rapprochés.
pub struct ProjectRuntime {
    /// Providers triés par priorité croissante.
    providers: Vec<Box<dyn WorkspaceProvider>>,
    config: RuntimeConfig,
    /// Cache par répertoire.
    cache: SnapshotCache,
}

impl ProjectRuntime {
    /// Construit un runtime avec les providers fournis.
    pub fn new(providers: Vec<Box<dyn WorkspaceProvider>>, config: RuntimeConfig) -> Self {
        let mut providers = providers;
        providers.sort_by_key(|p| p.priority());
        Self {
            providers,
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Construit un runtime avec le projet par défaut (git + rules + tree).
    pub fn default_project() -> Self {
        let providers: Vec<Box<dyn WorkspaceProvider>> = vec![
            Box::new(GitProvider::default()),
            Box::new(RulesProvider::default()),
            Box::new(TreeProvider::default()),
        ];
        Self::new(providers, RuntimeConfig::default())
    }

    /// Active la détection de style en ajoutant un [`StyleProvider`] avec LLM.
    pub fn with_style_detection(mut self, router: Arc<LlmRouter>) -> Self {
        let style = StyleProvider::new(StyleProviderConfig::default(), router);
        self.providers.push(Box::new(style));
        self.providers.sort_by_key(|p| p.priority());
        self
    }

    /// Ajoute un provider supplémentaire (par exemple un [`ScriptProvider`]).
    pub fn with_provider(mut self, provider: Box<dyn WorkspaceProvider>) -> Self {
        self.providers.push(provider);
        self.providers.sort_by_key(|p| p.priority());
        self
    }

    /// Construit un runtime depuis une configuration de providers JSON
    /// (provenant de la table `project_providers` en SQLite).
    ///
    /// Types supportés : `"git"`, `"rules"`, `"tree"`, `"script"`.
    /// Le type `"style"` nécessite un `LlmRouter` — utiliser `with_style_detection`.
    pub fn from_providers_config(
        providers_config: &[ProviderEntry],
        llm_router: Option<Arc<LlmRouter>>,
    ) -> Self {
        let mut providers: Vec<Box<dyn WorkspaceProvider>> = Vec::new();

        for entry in providers_config {
            if !entry.enabled {
                continue;
            }
            match entry.provider_type.as_str() {
                "git" => {
                    let config: GitProviderConfig = entry
                        .config_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok())
                        .unwrap_or_default();
                    providers.push(Box::new(GitProvider::new(config)));
                }
                "rules" => {
                    let config: RulesProviderConfig = entry
                        .config_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok())
                        .unwrap_or_default();
                    providers.push(Box::new(RulesProvider::new(config)));
                }
                "tree" => {
                    let max_lines: usize = entry
                        .config_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                        .and_then(|v| v["max_lines"].as_u64())
                        .map(|n| n as usize)
                        .unwrap_or(100);
                    providers.push(Box::new(TreeProvider::new(max_lines)));
                }
                "style" => {
                    if let Some(ref router) = llm_router {
                        let config: StyleProviderConfig = entry
                            .config_json
                            .as_deref()
                            .and_then(|j| serde_json::from_str(j).ok())
                            .unwrap_or_default();
                        providers.push(Box::new(StyleProvider::new(config, router.clone())));
                    }
                }
                "script" => {
                    let path = match &entry.path {
                        Some(p) => p.clone(),
                        None => {
                            tracing::warn!(name = %entry.name, "script provider missing path — ignored");
                            continue;
                        }
                    };
                    let timeout_ms = entry
                        .config_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                        .and_then(|v| v["timeout_ms"].as_u64())
                        .unwrap_or(500);
                    providers.push(Box::new(ScriptProvider::new(
                        entry.name.clone(),
                        path,
                        timeout_ms,
                        entry.priority,
                    )));
                }
                other => {
                    tracing::warn!(name = %entry.name, provider_type = %other, "unknown provider type — ignored");
                }
            }
        }

        providers.sort_by_key(|p| p.priority());
        Self::new(providers, RuntimeConfig::default())
    }

    /// Collecte tous les providers applicables en parallèle avec timeout par provider.
    ///
    /// Retourne depuis le cache si la dernière collecte date de moins de
    /// [`context_ttl_secs`](RuntimeConfig::context_ttl_secs).
    pub async fn collect(&self, cwd: &Path) -> WorkspaceSnapshot {
        // — Lecture cache ————————————————————————————————————————————————
        {
            let guard = self.cache.read().await;
            if let Some((collected_at, ref snapshot)) = guard.get(cwd) {
                let ttl = Duration::from_secs(self.config.context_ttl_secs);
                if collected_at.elapsed() < ttl {
                    tracing::debug!(
                        elapsed_ms = collected_at.elapsed().as_millis(),
                        ttl_secs = self.config.context_ttl_secs,
                        "workspace cache hit"
                    );
                    return snapshot.clone();
                }
            }
        }

        // — Collecte parallèle avec timeout par provider ——————————————————
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
                                "provider timed out"
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
                tracing::warn!(provider = %slice.source, error = %err, "provider error");
            }
        }

        let snapshot = WorkspaceSnapshot::new(slices);

        // — Mise à jour cache ——————————————————————————————————————————————
        self.cache
            .write()
            .await
            .insert(cwd.to_path_buf(), (Instant::now(), snapshot.clone()));

        snapshot
    }
}

/// Entrée de provider lue depuis la table `project_providers` en SQLite.
#[derive(Debug)]
pub struct ProviderEntry {
    pub provider_type: String,
    pub name: String,
    pub config_json: Option<String>,
    pub path: Option<String>,
    pub enabled: bool,
    pub priority: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_collects_in_real_repo() {
        // GIVEN le projet courant (dépôt git)
        let runtime = ProjectRuntime::default_project();
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let snapshot = runtime.collect(&cwd).await;
        // THEN — au moins une section
        assert!(!snapshot.is_empty(), "expected sections from git repo");
    }

    #[tokio::test]
    async fn runtime_cache_hit_on_second_call() {
        // GIVEN un runtime avec TTL de 60s
        let config = RuntimeConfig {
            context_ttl_secs: 60,
            ..Default::default()
        };
        let runtime = ProjectRuntime::new(vec![Box::new(GitProvider::default())], config);
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN deux appels rapides
        let s1 = runtime.collect(&cwd).await;
        let s2 = runtime.collect(&cwd).await;
        // THEN même nombre de slices (depuis le cache)
        assert_eq!(
            s1.slices.len(),
            s2.slices.len(),
            "cache must return same slice count"
        );
    }

    #[tokio::test]
    async fn runtime_no_applicable_providers_outside_git() {
        // GIVEN uniquement un GitProvider dans un répertoire sans .git
        let runtime = ProjectRuntime::new(
            vec![Box::new(GitProvider::default())],
            RuntimeConfig::default(),
        );
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let snapshot = runtime.collect(cwd).await;
        // THEN — git provider non applicable, snapshot vide
        assert!(
            snapshot.is_empty(),
            "no providers applicable outside git repo"
        );
    }

    #[tokio::test]
    async fn format_for_prompt_respects_priority_order() {
        // GIVEN un snapshot avec deux slices
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
                title: "Règles".into(),
                content: "no magic".into(),
                source: "rules".into(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        };
        let snapshot = WorkspaceSnapshot::new(vec![s1, s2]);
        // WHEN
        let prompt = snapshot.format_for_prompt();
        // THEN — Git apparaît avant Règles
        let pos_git = prompt.find("Git").expect("Git not found");
        let pos_rules = prompt.find("Règles").expect("Règles not found");
        assert!(pos_git < pos_rules, "Git must appear before Règles");
    }
}
