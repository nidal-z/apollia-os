//! Niveau de routing LLM — distingue raisonnement profond et extraction légère.
//!
//! [`LlmRoutingLevel`] encode le tradeoff coût/latence/qualité en deux catégories
//! directement déductibles des scaling laws (Kaplan et al., 2020).
//! Utilisé par [`crate::router::LlmRouter::route_precise`] et
//! [`crate::router::LlmRouter::route_fast`] pour sélectionner le backend approprié.

/// Niveau de routing LLM par type de tâche.
///
/// Issu du tradeoff coût/latence/qualité documenté dans les scaling laws
/// (Kaplan et al., 2020, "Scaling Laws for Neural Language Models").
/// Les deux niveaux correspondent aux deux axes naturels d'utilisation des LLMs :
/// raisonnement profond vs extraction déterministe.
///
/// Utilisé pour sélectionner le backend via
/// [`LlmRouter::route_precise`](crate::router::LlmRouter::route_precise) et
/// [`LlmRouter::route_fast`](crate::router::LlmRouter::route_fast).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmRoutingLevel {
    /// Tâches de raisonnement : planification, analyse complexe, jugement.
    ///
    /// Privilégie la qualité au détriment du coût et de la latence.
    /// Configurable via `[llm.routing] precise` dans `apollia.toml`.
    Precise,

    /// Tâches d'extraction : métadonnées, résumés courts, classification, parsing.
    ///
    /// Privilégie la vitesse et le coût au détriment de la nuance.
    /// Configurable via `[llm.routing] fast` dans `apollia.toml`.
    Fast,
}
