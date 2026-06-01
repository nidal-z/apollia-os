//! Types partagés pour le binary feedback RLHF - deux plans alternatifs.
//!
//! Défini dans `apollia-core` pour être utilisé par `apollia-oria` (génération),
//! `apollia-memory` (persistance du choix), `apollia-cli` (affichage terminal)
//! et `apollia-desktop` (composant Svelte) sans dépendance circulaire.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Step / Plan
// ─────────────────────────────────────────────

/// Un step individuel dans un [`TaskPlan`].
///
/// Représentation partagée utilisée dans le contexte du binary feedback -
/// distincte de `ExecutionPlan` / `PlanStep` interne à `apollia-oria`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlanStep {
    /// Identifiant unique dans le plan (ex: `"s1"`, `"s2"`).
    pub step_id: String,
    /// Description en langage naturel de l'action à réaliser.
    pub description: String,
    /// Outil suggéré par le LLM (optionnel).
    #[serde(default)]
    pub tool_hint: Option<String>,
    /// Identifiants des steps dont ce step dépend.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Hint optionnel pour router ce step vers un backend LLM spécifique.
    #[serde(default)]
    pub model_hint: Option<String>,
}

/// Plan d'exécution sérialisable partagé entre les crates.
///
/// Utilisé comme représentation canonique dans [`PlanAlternatives`]
/// et dans les événements [`RuntimeEvent::PlanAlternativesGenerated`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    /// Identifiant unique du plan (UUID v4).
    pub plan_id: String,
    /// Identifiant de la tâche associée.
    pub task_id: String,
    /// Steps à exécuter.
    pub steps: Vec<TaskPlanStep>,
}

// ─────────────────────────────────────────────
// PlanAlternatives
// ─────────────────────────────────────────────

/// Deux plans alternatifs générés en parallèle par le Reasoner.
///
/// Plan A est produit à basse température (déterministe, conservateur).
/// Plan B est produit à haute température (créatif, exploratoire).
/// Le `session_id` corrèle la génération avec le choix de l'opérateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAlternatives {
    /// Plan A : température basse - déterministe et conservateur.
    pub plan_a: TaskPlan,
    /// Plan B : température haute - créatif et exploratoire.
    pub plan_b: TaskPlan,
    /// Identifiant de session pour la corrélation avec [`PlanChoice`].
    pub session_id: String,
    /// Timestamp Unix de génération (secondes).
    pub generated_at: i64,
}

// ─────────────────────────────────────────────
// PlanChoice / ChosenPlan
// ─────────────────────────────────────────────

/// Choix de l'opérateur entre les deux plans alternatifs.
///
/// Persisté en SQLite par `PlanChoiceStore::log_plan_choice()` pour constituer
/// le signal RLHF. Local uniquement - jamais envoyé à l'extérieur (Principe #1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanChoice {
    /// Identifiant de session correspondant à [`PlanAlternatives::session_id`].
    pub session_id: String,
    /// Plan choisi par l'opérateur.
    pub chosen: ChosenPlan,
    /// Timestamp Unix du choix (secondes).
    pub chosen_at: i64,
}

/// Identifie lequel des deux plans alternatifs a été choisi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChosenPlan {
    /// Plan A : le plan conservateur à basse température.
    PlanA,
    /// Plan B : le plan exploratoire à haute température.
    PlanB,
}

impl ChosenPlan {
    /// Retourne la représentation textuelle utilisée dans la table `plan_choices`.
    pub fn as_db_str(&self) -> &str {
        match self {
            ChosenPlan::PlanA => "plan_a",
            ChosenPlan::PlanB => "plan_b",
        }
    }
}
