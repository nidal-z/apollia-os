## [SPRINT-0][CORE] Implémenter les types fondamentaux de apollia-core

**ID :** STORY-002
**Sprint :** 0
**Crate cible :** `apollia-core`
**Fichier(s) cible(s) :**
- `crates/apollia-core/src/lib.rs`
- `crates/apollia-core/src/manifest.rs`
- `crates/apollia-core/src/task.rs`
- `crates/apollia-core/src/result.rs`

**Taille :** M
**Dépend de :** STORY-001
**Statut :** 🔲 À faire

---

## User Story

```
En tant que runtime Apollia OS,
je veux des types Rust sérialisables pour AgentManifest, AIPTask et AIPResult,
afin de pouvoir échanger des données structurées entre le runtime et les agents Python via le bridge PyO3.
```

---

## Contexte technique

`apollia-core` est la fondation partagée de tout le workspace. Les types définis ici sont utilisés par toutes les autres crates. Ils doivent être `#[derive(Debug, Clone, Serialize, Deserialize)]` pour la sérialisation JSON, et ne doivent dépendre d'aucune autre crate du workspace.

Cette story couvre les types du "contrat AIP" : la déclaration d'identité d'un agent (`AgentManifest`), la tâche envoyée par le runtime à l'agent (`AIPTask`), et le résultat retourné par l'agent (`AIPResult`). Les types d'état (`ProcessState`, `TaskStatus`) et les budgets (`StepBudgetConfig`) sont dans STORY-003 et STORY-004.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #3 — Contrat minimal : AgentManifest + AIPTask + AIPResult = l'interface complète

**Position dans l'architecture :**
```
apollia-core/src/
  ├── lib.rs         ← re-exports publics
  ├── manifest.rs    ← AgentManifest, AgentSkill   ← cette story
  ├── task.rs        ← AIPTask, AIPInput, AIPPart  ← cette story
  └── result.rs      ← AIPResult, AIPError          ← cette story
```

---

## Critères d'Acceptation

### AC-1 — AgentManifest est constructible et sérialisable

```
ÉTANT DONNÉ un AgentManifest avec name="devis-agent", version="1.0.0",
  tools_required=["file_io", "bash_executor"]
QUAND on le sérialise en JSON avec serde_json::to_string()
ALORS le JSON contient les champs attendus sans erreur de sérialisation
```

### AC-2 — AIPTask avec AIPInput multi-part est constructible

```
ÉTANT DONNÉ un AIPTask avec task_id UUID v4, un AIPInput contenant
  un TextPart("Génère un devis") et un DataPart({ "client": "Dupont" })
QUAND on clone et inspecte les champs
ALORS task.input.parts.len() == 2,
  ET task.task_id est un UUID valide non vide
```

### AC-3 — AIPResult avec status Failed porte une AIPError

```
ÉTANT DONNÉ un AIPResult avec status=TaskStatus::Failed
  ET error=Some(AIPError { code: "TIMEOUT", message: "..." })
QUAND on sérialise puis désérialise le résultat (round-trip JSON)
ALORS result.status == TaskStatus::Failed,
  ET result.error.is_some() == true,
  ET result.error.unwrap().code == "TIMEOUT"
```

### AC-4 — Les champs optionnels d'AgentManifest ont des valeurs par défaut

```
ÉTANT DONNÉ un AgentManifest::new("agent", "1.0.0", "desc", vec!["file_io"])
QUAND on inspecte les champs optionnels
ALORS tools_optional == [],
  ET supports_streaming == false,
  ET supports_a2a == false,
  ET max_concurrent_tasks == 1,
  ET memory_namespace == None
```

---

## Spécification technique

### Types à créer

```rust
// crates/apollia-core/src/manifest.rs

/// Identité et capacités déclarées d'un agent.
/// Source unique de vérité pour la résolution des outils et la configuration
/// du runtime au démarrage de l'agent (état INITIALIZING).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    pub version: String,                    // semver (ex: "1.0.0")
    pub description: String,
    pub tools_required: Vec<String>,        // résolution fail-fast à INITIALIZING
    pub tools_optional: Vec<String>,        // absent → DEGRADED, pas d'erreur
    pub supports_streaming: bool,           // défaut: false
    pub supports_a2a: bool,                 // défaut: false
    pub memory_namespace: Option<String>,
    pub shared_memory_namespaces: Vec<String>,
    pub max_concurrent_tasks: u32,          // défaut: 1
    pub step_budget: Option<StepBudgetConfig>,  // override du défaut runtime
    pub network_allowlist: Option<Vec<String>>, // None = pas de réseau
    pub tags: Vec<String>,
    pub skills: Vec<AgentSkill>,
}

/// Compétence déclarative d'un agent (utilisée pour la carte A2A si supports_a2a=true).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
}

// crates/apollia-core/src/task.rs

/// Tâche soumise par le runtime à l'agent via le bridge AIP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPTask {
    pub task_id: String,        // UUID v4 généré par le runtime
    pub context_id: String,     // groupe de tâches liées (session)
    pub input: AIPInput,
    pub history: Vec<AIPMessage>,
    pub timeout_seconds: Option<u32>,
}

/// Entrée multi-modale d'une tâche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPInput {
    pub parts: Vec<AIPPart>,
}

/// Part multi-modal aligné A2A.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AIPPart {
    Text(TextPart),
    File(FilePart),
    Data(DataPart),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart { pub text: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    pub name: String,
    pub mime_type: String,
    pub data: Option<Vec<u8>>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPart { pub data: serde_json::Value }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPMessage {
    pub role: String,           // "user" | "agent"
    pub parts: Vec<AIPPart>,
}

// crates/apollia-core/src/result.rs

/// Résultat retourné par l'agent au runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: Vec<AIPPart>,
    pub error: Option<AIPError>,
    pub artifacts: Vec<AIPArtifact>,
}

/// Artefact produit par une tâche (fichier généré, rapport, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPArtifact {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Erreur structurée retournée par l'agent.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AIPError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}
```

### Dépendances Cargo

```toml
# Déjà dans [workspace.dependencies] — pas de nouvelles dépendances
serde      = { workspace = true }
serde_json = { workspace = true }
thiserror  = { workspace = true }
uuid       = { workspace = true }
```

### Ce que cette story N'implémente PAS

- `ProcessState`, `TaskStatus`, `AIPError` enum-based lifecycle → STORY-003
- `StepBudgetConfig` → STORY-004
- Logique de validation ou de traitement — uniquement les types

---

## Tests requis

### Tests unitaires

```rust
// crates/apollia-core/src/manifest.rs

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

// crates/apollia-core/src/result.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_result_failed_round_trip() {
        // GIVEN
        let result = AIPResult {
            task_id: "task-123".into(),
            status: TaskStatus::Failed,  // défini en STORY-003
            output: vec![],
            error: Some(AIPError {
                code: "TIMEOUT".into(),
                message: "Agent timed out".into(),
                details: None,
            }),
            artifacts: vec![],
        };
        // WHEN
        let json = serde_json::to_string(&result).expect("serialize failed");
        let restored: AIPResult = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert!(restored.error.is_some());
        assert_eq!(restored.error.unwrap().code, "TIMEOUT");
    }
}
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-core` passe (0 test ignoré)
- [ ] `cargo clippy -p apollia-core -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de production (autorisé dans les tests)
- [ ] Zéro `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] `apollia-core` ne dépend d'aucune autre crate du workspace
- [ ] Toutes les dépendances utilisent `{ workspace = true }`
- [ ] Pas de `anyhow` introduit

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-core): add AgentManifest, AIPTask and AIPResult types`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : Sprint 0 — Fondations
- Story précédente : STORY-001
- Story suivante : STORY-003
- ADR associé : ADR-003 (Duck typing AIP — les types ici reflètent le contrat Python)
