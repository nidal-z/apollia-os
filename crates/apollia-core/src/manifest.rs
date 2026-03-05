use serde::{Deserialize, Serialize};

use crate::budget::StepBudgetConfig;

/// Identité et capacités déclarées d'un agent.
///
/// Source unique de vérité pour la résolution des outils et la configuration
/// du runtime au démarrage de l'agent (état INITIALIZING).
/// Retournée par la méthode `manifest()` de chaque agent Python.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Nom unique de l'agent dans le runtime.
    pub name: String,
    /// Version semver (ex: "1.0.0").
    pub version: String,
    /// Description humaine de l'agent.
    pub description: String,
    /// Outils requis — résolution fail-fast à l'état INITIALIZING.
    pub tools_required: Vec<String>,
    /// Outils optionnels — absent → état DEGRADED, pas d'erreur fatale.
    pub tools_optional: Vec<String>,
    /// Indique si l'agent supporte le mode streaming (défaut: false).
    pub supports_streaming: bool,
    /// Indique si l'agent supporte le protocole Agent-to-Agent (défaut: false).
    pub supports_a2a: bool,
    /// Namespace mémoire privé de l'agent (None = pas de mémoire persistante).
    pub memory_namespace: Option<String>,
    /// Namespaces mémoire partagés accessibles en lecture.
    pub shared_memory_namespaces: Vec<String>,
    /// Nombre maximum de tâches concurrentes (défaut: 1).
    pub max_concurrent_tasks: u32,
    /// Override du budget d'étapes par défaut du runtime (None = utiliser le défaut).
    pub step_budget: Option<StepBudgetConfig>,
    /// Liste blanche réseau (None = pas d'accès réseau autorisé).
    pub network_allowlist: Option<Vec<String>>,
    /// Tags libres pour le routage et la découverte.
    pub tags: Vec<String>,
    /// Compétences déclaratives de l'agent (utilisées pour la carte A2A).
    pub skills: Vec<AgentSkill>,
}

/// Compétence déclarative d'un agent.
///
/// Utilisée pour construire la carte A2A si `supports_a2a = true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    /// Identifiant unique de la compétence.
    pub id: String,
    /// Nom humain de la compétence.
    pub name: String,
    /// Description de ce que fait la compétence.
    pub description: String,
    /// Modes d'entrée supportés (ex: ["text", "data"]).
    pub input_modes: Vec<String>,
    /// Modes de sortie supportés (ex: ["text", "file"]).
    pub output_modes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac1_agent_manifest_serialization() {
        // GIVEN
        let manifest = AgentManifest {
            name: "devis-agent".into(),
            version: "1.0.0".into(),
            description: "Génère des devis".into(),
            tools_required: vec!["file_io".into(), "bash_executor".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            tags: vec![],
            skills: vec![],
        };
        // WHEN
        let json = serde_json::to_string(&manifest).expect("serialization failed");
        // THEN
        assert!(json.contains("devis-agent"));
        assert!(json.contains("file_io"));
    }

    #[test]
    fn test_ac4_manifest_optional_defaults() {
        // GIVEN / WHEN
        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "desc".into(),
            tools_required: vec!["file_io".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            tags: vec![],
            skills: vec![],
        };
        // THEN
        assert_eq!(manifest.max_concurrent_tasks, 1);
        assert!(!manifest.supports_streaming);
        assert!(manifest.memory_namespace.is_none());
    }
}
