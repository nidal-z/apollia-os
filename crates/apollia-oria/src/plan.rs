//! Types de plan d'exécution pour le mode Orchestrated d'ORIA.
//!
//! Ces types sont produits par [`crate::reasoner::Reasoner`], validés par
//! [`crate::topo::topological_sort`], et consommés par l'`ActorLoop`.

/// Plan d'exécution multi-étapes généré par le Reasoner.
///
/// Sérialisable en JSON pour être transmis au LLM et parsé depuis sa réponse.
/// La validation (step IDs uniques, dépendances valides, absence de cycles) est
/// effectuée par [`crate::reasoner::Reasoner::parse_and_validate`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    /// Identifiant unique du plan (UUID v4), généré à la création.
    pub plan_id: String,
    /// Identifiant de la tâche associée à ce plan.
    pub task_id: String,
    /// Steps à exécuter dans l'ordre résolu par le tri topologique.
    pub steps: Vec<PlanStep>,
}

/// Un step individuel dans un [`ExecutionPlan`].
///
/// Chaque step représente une action atomique que l'`ActorLoop` doit exécuter.
/// L'ordre d'exécution est déterminé par les `depends_on` via
/// [`crate::topo::topological_sort`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    /// Identifiant unique dans le plan (ex: `"s1"`, `"s2"`).
    pub step_id: String,
    /// Description en langage naturel de l'action à réaliser.
    pub description: String,
    /// Outil suggéré par le LLM (`None` si l'étape ne nécessite pas d'outil natif).
    #[serde(default)]
    pub tool_hint: Option<String>,
    /// Identifiants des steps dont ce step dépend (DAG d'exécution).
    ///
    /// Tous les steps listés ici doivent être complétés avant que ce step
    /// puisse démarrer. Validé par `parse_and_validate()`.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Hint optionnel pour router ce step vers un backend LLM spécifique.
    ///
    /// Si `None`, le backend par défaut du `LlmRouter` est utilisé.
    /// La résolution effective du hint vers un backend concret est faite
    /// par l'`ActorLoop`.
    #[serde(default)]
    pub model_hint: Option<String>,
}
