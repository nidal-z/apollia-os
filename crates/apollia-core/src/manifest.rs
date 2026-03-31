use serde::{Deserialize, Serialize};

use crate::budget::StepBudgetConfig;

/// Default value for `AgentManifest::max_concurrent_tasks`.
fn default_max_concurrent_tasks() -> u32 {
    1
}

/// Default value for `AgentManifest::execution_mode`.
fn default_execution_mode() -> String {
    "auto".to_string()
}

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
    #[serde(default)]
    pub tools_optional: Vec<String>,
    /// Indique si l'agent supporte le mode streaming (défaut: false).
    #[serde(default)]
    pub supports_streaming: bool,
    /// Indique si l'agent supporte le protocole Agent-to-Agent (défaut: false).
    #[serde(default)]
    pub supports_a2a: bool,
    /// Namespace mémoire privé de l'agent (None = pas de mémoire persistante).
    #[serde(default)]
    pub memory_namespace: Option<String>,
    /// Namespaces mémoire partagés accessibles en lecture.
    #[serde(default)]
    pub shared_memory_namespaces: Vec<String>,
    /// Nombre maximum de tâches concurrentes (défaut: 1).
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: u32,
    /// Override du budget d'étapes par défaut du runtime (None = utiliser le défaut).
    #[serde(default)]
    pub step_budget: Option<StepBudgetConfig>,
    /// Liste blanche réseau (None = pas d'accès réseau autorisé).
    #[serde(default)]
    pub network_allowlist: Option<Vec<String>>,
    /// Autorise explicitement l'utilisation d'outils marqués `dangerous=true`.
    /// `false` par défaut — les outils dangereux sont bloqués sauf opt-in explicite.
    #[serde(default)]
    pub dangerous_tools_allowed: bool,
    /// Tags libres pour le routage et la découverte.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Compétences déclaratives de l'agent (utilisées pour la carte A2A).
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
    /// Mode d'exécution ORIA : `"auto"` | `"direct"` | `"orchestrated"`.
    ///
    /// - `"auto"` (défaut) : l'heuristique `classify()` décide.
    /// - `"direct"` : force le Mode Direct quel que soit le manifest.
    /// - `"orchestrated"` : force le Mode Orchestré quel que soit le manifest.
    ///
    /// Toute valeur inconnue est traitée comme `"auto"` sans erreur.
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    /// Prompt système fourni à ORIA pour planifier l'exécution orchestrée.
    ///
    /// Requis si `execution_mode = "orchestrated"`.
    /// Ignoré pour les autres modes.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Outils nécessitant une approbation humaine avant exécution en Mode Orchestré.
    ///
    /// Déclarés par l'agent dans son `manifest()`. Le runtime refuse d'exécuter un step
    /// utilisant ces outils sans approbation explicite.
    /// Vide par défaut — aucun outil ne nécessite d'approbation.
    #[serde(default)]
    pub tools_requiring_approval: Vec<String>,
    /// Nom du backend LLM à utiliser pour cet agent.
    ///
    /// Doit correspondre au champ `name` d'une entrée dans `llm_backends` (SQLite system.db).
    /// Si absent ou `None`, le runtime utilise le backend marqué `is_default = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub llm_backend: Option<String>,
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
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
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
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
        };
        // THEN
        assert_eq!(manifest.max_concurrent_tasks, 1);
        assert!(!manifest.supports_streaming);
        assert!(manifest.memory_namespace.is_none());
    }

    #[test]
    fn test_ac2_tools_requiring_approval_default_empty() {
        // GIVEN un manifest JSON sans le champ tools_requiring_approval
        let json = r#"{"name":"test","version":"0.1.0","description":"desc","tools_required":[]}"#;
        // WHEN on désérialise
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN tools_requiring_approval est vide
        assert!(manifest.tools_requiring_approval.is_empty());
    }

    #[test]
    fn test_ac1_tools_requiring_approval_parsed() {
        // GIVEN un manifest JSON avec tools_requiring_approval
        let json = r#"{
            "name":"test","version":"0.1.0","description":"desc","tools_required":[],
            "tools_requiring_approval":["smtp","http_client"]
        }"#;
        // WHEN on désérialise
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN la liste est correctement parsée
        assert_eq!(
            manifest.tools_requiring_approval,
            vec!["smtp", "http_client"]
        );
    }

    // GIVEN un manifest JSON avec "llm_backend": "local-code"
    // WHEN on désérialise
    // THEN llm_backend == Some("local-code")
    #[test]
    fn test_llm_backend_present_extracts_some() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": "local-code"
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.llm_backend, Some("local-code".to_string()));
    }

    // GIVEN un manifest JSON sans la clé "llm_backend"
    // WHEN on désérialise
    // THEN llm_backend == None
    #[test]
    fn test_llm_backend_absent_is_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": []
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert!(manifest.llm_backend.is_none());
    }

    // GIVEN un manifest JSON avec "llm_backend": null
    // WHEN on désérialise
    // THEN llm_backend == None
    #[test]
    fn test_llm_backend_null_is_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": null
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert!(manifest.llm_backend.is_none());
    }

    // GIVEN un manifest avec llm_backend = Some(...)
    // WHEN on sérialise en JSON
    // THEN le champ apparaît dans le JSON
    #[test]
    fn test_llm_backend_serialized_when_some() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": "mistral-small"
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        let output = serde_json::to_string(&manifest).unwrap();
        assert!(output.contains("mistral-small"));
    }

    // GIVEN un manifest avec llm_backend = None
    // WHEN on sérialise en JSON
    // THEN le champ n'apparaît pas (skip_serializing_if)
    #[test]
    fn test_llm_backend_absent_from_json_when_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": []
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        let output = serde_json::to_string(&manifest).unwrap();
        assert!(!output.contains("llm_backend"));
    }

    #[test]
    fn test_ac3_tools_requiring_approval_roundtrip() {
        // GIVEN un AgentManifest avec tools_requiring_approval
        let manifest = AgentManifest {
            name: "roundtrip-agent".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".into(),
            system_prompt: None,
            tools_requiring_approval: vec!["smtp".into(), "bash_executor".into()],
            llm_backend: None,
        };
        // WHEN serde roundtrip JSON
        let json = serde_json::to_string(&manifest).expect("serialize must succeed");
        let restored: AgentManifest =
            serde_json::from_str(&json).expect("deserialize must succeed");
        // THEN pas de perte
        assert_eq!(
            restored.tools_requiring_approval,
            vec!["smtp", "bash_executor"]
        );
    }
}
