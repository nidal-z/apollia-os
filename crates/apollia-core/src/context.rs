//! Trait [`ContextProvider`] et types pour la collecte de contexte situationnel.
//!
//! Distinction fondamentale avec la mémoire agent :
//! - **Contexte** : snapshot de l'environnement courant collectée par le runtime.
//! - **Mémoire** : accumulée par l'agent au fil du temps, à son initiative.

use std::path::Path;
use std::time::Instant;

/// Fournit un snapshot de contexte situationnel avant qu'un agent commence.
///
/// La mémoire (Principe #6) est à l'initiative de l'agent ; le contexte est
/// collecté par le runtime avant le premier step, indépendamment de la mémoire.
#[async_trait::async_trait]
pub trait ContextProvider: Send + Sync {
    /// Identifiant unique du provider, utilisé comme clé de cache et source des sections.
    fn name(&self) -> &str;

    /// Collecte un snapshot de contexte pour le répertoire `cwd`.
    ///
    /// Toujours fail-silent : retourner [`ContextSnapshot::with_error`] plutôt
    /// que propager une erreur. L'agent continue même si ce provider échoue.
    async fn collect(&self, cwd: &Path) -> ContextSnapshot;

    /// Description courte pour les logs et la CLI.
    fn description(&self) -> &str {
        ""
    }

    /// Priorité d'affichage dans le system prompt (valeur basse = affiché en premier).
    fn priority(&self) -> u8 {
        50
    }

    /// Intervalle de rafraîchissement en secondes.
    ///
    /// `None` signifie que le TTL global de l'assembleur s'applique.
    fn refresh_secs(&self) -> Option<u64> {
        None
    }

    /// Retourne `true` si ce provider est applicable dans `cwd`.
    ///
    /// Un provider retournant `false` n'est pas appelé — aucun overhead, aucune erreur.
    fn is_applicable(&self, cwd: &Path) -> bool {
        let _ = cwd;
        true
    }
}

/// Snapshot éphémère produit par un [`ContextProvider`].
///
/// Injecté dans le system prompt de l'agent au démarrage de la session.
/// Non persisté — recollecté à chaque démarrage de tâche (modulo cache TTL).
#[derive(Clone)]
pub struct ContextSnapshot {
    /// Identifiant du provider source.
    pub source: String,
    /// Sections de texte à injecter dans le prompt.
    pub sections: Vec<ContextSection>,
    /// Erreurs non-fatales : timeout, source inaccessible, parse error partiel.
    pub errors: Vec<String>,
    /// Horodatage de collecte pour le cache TTL.
    pub collected_at: Instant,
}

/// Section de contexte injectée sous un tag `<context name="titre">`.
#[derive(Clone)]
pub struct ContextSection {
    /// Titre affiché dans le tag `<context name="...">`.
    pub title: String,
    /// Contenu textuel de la section.
    pub content: String,
    /// Nom du provider source de cette section.
    pub source: String,
}

impl ContextSnapshot {
    /// Construit un snapshot avec une seule section de contenu.
    pub fn single(source: &str, title: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            source: source.to_owned(),
            sections: vec![ContextSection {
                title: title.to_owned(),
                content,
                source: source.to_owned(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Construit un snapshot vide — aucun contenu à injecter.
    ///
    /// Retourné quand le provider n'est pas applicable ou que la source est vide.
    pub fn empty(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            sections: vec![],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Construit un snapshot vide avec une erreur non-fatale.
    ///
    /// L'erreur est tracée mais n'empêche pas l'exécution de l'agent.
    pub fn with_error(source: &str, error: String) -> Self {
        Self {
            source: source.to_owned(),
            sections: vec![],
            errors: vec![error],
            collected_at: Instant::now(),
        }
    }

    /// Retourne `true` si le snapshot ne contient aucune section.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}
