//! Trait [`WorkspaceProvider`] et types pour la collecte de contexte situationnel.
//!
//! Un **workspace** est l'ensemble des informations sur l'environnement courant
//! qu'un agent reçoit avant de commencer à travailler. Ces informations sont
//! collectées par des [`WorkspaceProvider`]s indépendants, agrégées en un
//! [`WorkspaceSnapshot`], puis injectées dans le system prompt LLM.
//!
//! ## Distinction fondamentale avec la mémoire agent
//! - **Contexte workspace** : snapshot de l'environnement collecté par le runtime.
//! - **Mémoire** : accumulée par l'agent au fil du temps, à son initiative.

use std::path::Path;
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Trait principal
// ─────────────────────────────────────────────────────────────────────────────

/// Fournit une tranche de contexte situationnel avant qu'un agent commence.
///
/// Chaque implémentation représente une source d'information indépendante :
/// état git, fichier de règles, arborescence, script custom, module Python, etc.
/// Les providers sont orchestrés en parallèle par [`WorkspaceSnapshot`].
///
/// ## Fail-silent
/// La méthode [`collect`](WorkspaceProvider::collect) ne doit jamais propager
/// d'erreur — retourner [`WorkspaceSlice::with_error`] à la place.
/// L'agent continue même si un provider échoue ou dépasse son timeout.
#[async_trait::async_trait]
pub trait WorkspaceProvider: Send + Sync {
    /// Identifiant unique du provider, utilisé comme clé de cache et source des sections.
    fn name(&self) -> &str;

    /// Collecte une tranche de contexte pour le répertoire `cwd`.
    ///
    /// Toujours fail-silent : retourner [`WorkspaceSlice::with_error`] plutôt
    /// que propager une erreur.
    async fn collect(&self, cwd: &Path) -> WorkspaceSlice;

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
    /// `None` signifie que le TTL global du runtime s'applique.
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

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceSlice — ce qu'un provider produit
// ─────────────────────────────────────────────────────────────────────────────

/// Tranche éphémère de contexte produite par un [`WorkspaceProvider`].
///
/// Injectée dans le system prompt de l'agent au démarrage de la session.
/// Non persistée — recollectée à chaque démarrage de tâche (modulo cache TTL).
#[derive(Clone)]
pub struct WorkspaceSlice {
    /// Identifiant du provider source.
    pub source: String,
    /// Sections de texte à injecter dans le prompt.
    pub sections: Vec<WorkspaceSection>,
    /// Erreurs non-fatales : timeout, source inaccessible, parse error partiel.
    pub errors: Vec<String>,
    /// Horodatage de collecte pour le cache TTL.
    pub collected_at: Instant,
}

/// Section de contexte injectée sous un tag `<context name="titre">`.
#[derive(Clone)]
pub struct WorkspaceSection {
    /// Titre affiché dans le tag `<context name="...">`.
    pub title: String,
    /// Contenu textuel de la section.
    pub content: String,
    /// Nom du provider source de cette section.
    pub source: String,
}

impl WorkspaceSlice {
    /// Construit une tranche avec une seule section de contenu.
    pub fn single(source: &str, title: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            source: source.to_owned(),
            sections: vec![WorkspaceSection {
                title: title.to_owned(),
                content,
                source: source.to_owned(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Construit une tranche vide — aucun contenu à injecter.
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

    /// Construit une tranche vide avec une erreur non-fatale.
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

    /// Retourne `true` si la tranche ne contient aucune section.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceSnapshot — agrégation de toutes les tranches
// ─────────────────────────────────────────────────────────────────────────────

/// Snapshot complet du workspace, agrégant toutes les tranches produites
/// par les [`WorkspaceProvider`]s actifs.
///
/// Créé par le `ProjectRuntime` après collecte parallèle. Injecté dans le
/// system prompt LLM et exposé à l'agent Python via `ctx.workspace`.
#[derive(Clone, Default)]
pub struct WorkspaceSnapshot {
    /// Tranches produites par chaque provider, dans l'ordre de priorité.
    pub slices: Vec<WorkspaceSlice>,
}

impl WorkspaceSnapshot {
    /// Construit un snapshot à partir d'une liste de tranches.
    pub fn new(slices: Vec<WorkspaceSlice>) -> Self {
        Self { slices }
    }

    /// Formate le snapshot pour injection dans un system prompt LLM.
    ///
    /// Chaque section est encapsulée dans un tag `<context name="titre">`.
    /// Les tranches vides (aucune section) sont omises.
    /// L'ordre respecte la priorité des providers (déjà triés à la collecte).
    pub fn format_for_prompt(&self) -> String {
        let blocks: Vec<String> = self
            .slices
            .iter()
            .flat_map(|s| &s.sections)
            .map(|s| format!("<context name=\"{}\">\n{}\n</context>", s.title, s.content))
            .collect();
        blocks.join("\n\n")
    }

    /// Retourne le contenu de la première section dont le titre correspond,
    /// ou `None` si aucune section ne correspond.
    pub fn get_section(&self, title: &str) -> Option<&str> {
        self.slices
            .iter()
            .flat_map(|s| &s.sections)
            .find(|s| s.title == title)
            .map(|s| s.content.as_str())
    }

    /// Retourne `true` si le snapshot ne contient aucune section.
    pub fn is_empty(&self) -> bool {
        self.slices.iter().all(|s| s.sections.is_empty())
    }
}
