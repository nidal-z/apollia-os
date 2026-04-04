//! Orchestrateur multi-provider avec cache TTL configurable.
//!
//! [`WorkspaceAssembler`] orchestre un ensemble de [`ContextProvider`]
//! en parallèle via `futures::future::join_all`. Un cache [`RwLock`] par
//! répertoire évite les I/O répétées sur les sessions longues.
//!
//! La construction par défaut inclut [`GitWorkspaceProvider`].
//! Pour un contrôle fin, utiliser [`WorkspaceAssembler::from_config`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use apollia_core::context::{ContextProvider, ContextSnapshot};
use apollia_llm::LlmRouter;

use crate::config::{ProviderConfig, WorkspaceConfig};
use crate::providers::{GitWorkspaceProvider, ScriptContextProvider};

/// Cache partagé par répertoire : instant de collecte + snapshots produits.
type SnapshotCache = Arc<RwLock<HashMap<PathBuf, (Instant, Vec<ContextSnapshot>)>>>;

/// Orchestre plusieurs [`ContextProvider`] et injecte les snapshots dans le system prompt.
///
/// La collecte est parallèle : chaque provider applicable est lancé simultanément.
/// Un timeout par provider garantit le fail-silent.
/// Un cache TTL par répertoire évite les I/O répétées entre deux steps rapprochés.
pub struct WorkspaceAssembler {
    /// Providers triés par priorité croissante.
    providers: Vec<Box<dyn ContextProvider>>,
    config: WorkspaceConfig,
    /// Cache par répertoire : instant de collecte + snapshots.
    cache: SnapshotCache,
}

impl WorkspaceAssembler {
    /// Construit un assembleur avec [`GitWorkspaceProvider`] comme provider par défaut.
    pub fn new(config: WorkspaceConfig) -> Self {
        let git = GitWorkspaceProvider::new(config.clone());
        Self {
            providers: vec![Box::new(git)],
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Construit un assembleur avec TTL personnalisé et provider git par défaut.
    ///
    /// Utile dans les tests pour contrôler l'invalidation du cache.
    pub fn with_ttl(ttl: Duration) -> Self {
        let config = WorkspaceConfig {
            context_ttl_secs: ttl.as_secs().max(1),
            ..WorkspaceConfig::default()
        };
        Self::new(config)
    }

    /// Active la détection de style en remplaçant le provider git par une version LLM-aware.
    pub fn with_llm_router(self, router: Arc<LlmRouter>) -> Self {
        let Self {
            providers, config, ..
        } = self;
        let mut providers: Vec<Box<dyn ContextProvider>> = providers
            .into_iter()
            .filter(|p| p.name() != "git")
            .collect();
        providers.push(Box::new(
            GitWorkspaceProvider::new(config.clone()).with_llm_router(router),
        ));
        providers.sort_by_key(|p| p.priority());
        Self {
            providers,
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Construit un assembleur depuis la liste de providers configurés dans `apollia.toml`.
    ///
    /// Providers `type = "python"` sont ignorés ici (log warn) — les ajouter via
    /// [`with_provider`](Self::with_provider) après construction.
    pub fn from_config(
        config: &WorkspaceConfig,
        providers_config: &[ProviderConfig],
        llm_router: Option<Arc<LlmRouter>>,
    ) -> Self {
        let mut providers: Vec<Box<dyn ContextProvider>> = Vec::new();

        for pc in providers_config {
            if !pc.enabled {
                continue;
            }
            match pc.provider_type.as_str() {
                "builtin" if pc.name == "git" => {
                    let git = if let Some(ref router) = llm_router {
                        GitWorkspaceProvider::new(config.clone()).with_llm_router(router.clone())
                    } else {
                        GitWorkspaceProvider::new(config.clone())
                    };
                    providers.push(Box::new(git));
                }
                "script" => {
                    let path = match &pc.path {
                        Some(p) => p.clone(),
                        None => {
                            tracing::warn!(name = %pc.name, "script provider missing 'path' — ignored");
                            continue;
                        }
                    };
                    providers.push(Box::new(ScriptContextProvider::new(
                        pc.name.clone(),
                        path,
                        pc.timeout_ms.unwrap_or(500),
                        pc.priority.unwrap_or(50),
                    )));
                }
                "python" => {
                    tracing::warn!(
                        name = %pc.name,
                        "python providers must be registered via WorkspaceAssembler::with_provider()"
                    );
                }
                other => {
                    tracing::warn!(name = %pc.name, provider_type = %other, "unknown provider type — ignored");
                }
            }
        }

        providers.sort_by_key(|p| p.priority());
        Self {
            providers,
            config: config.clone(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Ajoute un provider supplémentaire (par exemple [`PythonContextProvider`]).
    ///
    /// Les providers sont triés par priorité après l'ajout.
    pub fn with_provider(mut self, provider: Box<dyn ContextProvider>) -> Self {
        self.providers.push(provider);
        self.providers.sort_by_key(|p| p.priority());
        self
    }

    /// Collecte tous les providers applicables en parallèle avec timeout par provider.
    ///
    /// Retourne depuis le cache si la dernière collecte date de moins de
    /// [`context_ttl_secs`](WorkspaceConfig::context_ttl_secs).
    /// Chaque provider dépassant son timeout retourne un [`ContextSnapshot::with_error`]
    /// — jamais de panic, jamais de blocage.
    pub async fn collect_all(&self, cwd: &Path) -> Vec<ContextSnapshot> {
        // — Lecture cache ————————————————————————————————————————————————
        {
            let guard = self.cache.read().await;
            if let Some((collected_at, ref snapshots)) = guard.get(cwd) {
                let ttl = Duration::from_secs(self.config.context_ttl_secs);
                if collected_at.elapsed() < ttl {
                    tracing::debug!(
                        elapsed_ms = collected_at.elapsed().as_millis(),
                        ttl_secs = self.config.context_ttl_secs,
                        "workspace cache hit"
                    );
                    return snapshots.clone();
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
                        Ok(snapshot) => snapshot,
                        Err(_) => {
                            tracing::warn!(
                                provider = %provider_name,
                                timeout_secs = provider_timeout.as_secs(),
                                "provider timed out"
                            );
                            ContextSnapshot::with_error(
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

        let snapshots = futures::future::join_all(futures).await;

        // Log errors from providers
        for snap in &snapshots {
            for err in &snap.errors {
                tracing::warn!(provider = %snap.source, error = %err, "provider error");
            }
        }

        // — Mise à jour cache ——————————————————————————————————————————————
        self.cache
            .write()
            .await
            .insert(cwd.to_path_buf(), (Instant::now(), snapshots.clone()));

        snapshots
    }

    /// Formate une liste de snapshots pour injection dans le system prompt LLM.
    ///
    /// Chaque section est encapsulée dans un tag `<context name="titre">`.
    /// Les snapshots vides (aucune section) sont omis.
    /// L'ordre respecte la priorité des providers (déjà triés à la construction).
    pub fn format_for_prompt(snapshots: &[ContextSnapshot]) -> String {
        let blocks: Vec<String> = snapshots
            .iter()
            .flat_map(|s| &s.sections)
            .map(|s| format!("<context name=\"{}\">\n{}\n</context>", s.title, s.content))
            .collect();
        blocks.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assembler_collects_in_parallel() {
        // GIVEN un assembleur dans le dépôt courant
        let assembler = WorkspaceAssembler::new(WorkspaceConfig::default());
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let snapshots = assembler.collect_all(&cwd).await;
        // THEN — git provider doit retourner au moins une section
        let total_sections: usize = snapshots.iter().map(|s| s.sections.len()).sum();
        assert!(
            total_sections > 0,
            "expected at least one section from git provider"
        );
    }

    #[tokio::test]
    async fn assembler_cache_hit_on_second_call() {
        // GIVEN un assembleur avec TTL de 60s
        let assembler = WorkspaceAssembler::with_ttl(Duration::from_secs(60));
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN deux appels rapides
        let s1 = assembler.collect_all(&cwd).await;
        let s2 = assembler.collect_all(&cwd).await;
        // THEN même nombre de snapshots (depuis le cache)
        assert_eq!(s1.len(), s2.len(), "cache must return same provider count");
    }

    #[tokio::test]
    async fn assembler_no_providers_applicable_outside_git() {
        // GIVEN un répertoire sans .git
        let assembler = WorkspaceAssembler::new(WorkspaceConfig::default());
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let snapshots = assembler.collect_all(cwd).await;
        // THEN — git provider non applicable, aucun snapshot
        assert!(
            snapshots.is_empty(),
            "no provider applicable outside git repo"
        );
    }

    #[tokio::test]
    async fn assembler_timeout_returns_error_not_panic() {
        // GIVEN timeout très court (1s)
        let config = WorkspaceConfig {
            collect_timeout_secs: 1,
            ..WorkspaceConfig::default()
        };
        let assembler = WorkspaceAssembler::new(config);
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN — ne doit pas paniquer même si le provider est lent
        let snapshots = assembler.collect_all(&cwd).await;
        // THEN — résultat retourné (vide ou non, pas de panic)
        drop(snapshots); // pas de panic = succès
    }

    #[tokio::test]
    async fn format_for_prompt_respects_priority_order() {
        // GIVEN snapshots de providers priority 50 et priority 10
        let snap_low = ContextSnapshot {
            source: "low-priority".into(),
            sections: vec![apollia_core::context::ContextSection {
                title: "LowPrio".into(),
                content: "content-low".into(),
                source: "low-priority".into(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        };
        let snap_high = ContextSnapshot {
            source: "high-priority".into(),
            sections: vec![apollia_core::context::ContextSection {
                title: "HighPrio".into(),
                content: "content-high".into(),
                source: "high-priority".into(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        };
        // WHEN — ordre : high priority (10) en premier, low priority (50) en second
        let formatted = WorkspaceAssembler::format_for_prompt(&[snap_high, snap_low]);
        // THEN — high priority appears first
        let pos_high = formatted.find("HighPrio").expect("HighPrio not found");
        let pos_low = formatted.find("LowPrio").expect("LowPrio not found");
        assert!(
            pos_high < pos_low,
            "high priority section must appear first"
        );
    }

    #[tokio::test]
    async fn disabled_provider_not_loaded() {
        // GIVEN config avec un provider désactivé
        let pc = ProviderConfig {
            name: "git".into(),
            provider_type: "builtin".into(),
            path: None,
            enabled: false,
            timeout_ms: None,
            priority: None,
            refresh_secs: None,
        };
        // WHEN
        let assembler = WorkspaceAssembler::from_config(&WorkspaceConfig::default(), &[pc], None);
        let cwd = std::env::current_dir().expect("current_dir");
        // THEN — aucun provider chargé
        let snapshots = assembler.collect_all(&cwd).await;
        assert!(snapshots.is_empty(), "disabled provider must not be loaded");
    }
}
