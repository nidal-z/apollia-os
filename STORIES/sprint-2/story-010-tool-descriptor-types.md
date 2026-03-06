# [Sprint 2][apollia-tools] ToolDescriptor et ToolKind — types fondamentaux du catalogue

**ID :** STORY-010
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/descriptor.rs`
**Taille :** S
**Depend de :** STORY-004 ✅ (`SandboxProfile` defini dans `apollia-core`)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un type ToolDescriptor qui decrit completement un outil disponible,
afin que le ToolRegistry puisse cataloguer les outils et que le ToolResolver
puisse valider leur disponibilite a INITIALIZING.
```

---

## Contexte technique

Premiere story de la crate `apollia-tools`. Etablit les types de base sur lesquels toutes les autres
stories du sprint (STORY-011 a 016) s'appuient. `SandboxProfile` existe deja dans `apollia-core`
et est importe ici — pas de redefinition.

**Principe(s) architecturaux concernes :**
- Principe #2 — Zero dependance externe (aucune nouvelle dep, uniquement types internes)
- Principe #4 — Fail fast (`dangerous=true` requis pour `SandboxProfile::Full`)

**Position dans l'architecture :**
```
apollia-tools
  └── descriptor.rs   <- cette story
        ├── ToolDescriptor  (struct)
        ├── ToolKind        (enum)
        └── McpTransport    (enum, sous-type de ToolKind::McpServer)
  [importe] apollia-core::SandboxProfile
```

---

## Criteres d'Acceptation

### AC-1 — ToolDescriptor serialisable et complete

```
ETANT DONNE un outil natif bash_executor
QUAND on construit un ToolDescriptor avec tous les champs
ALORS il se serialise en JSON sans erreur et se deserialise de maniere identique
```

### AC-2 — ToolKind::McpServer avec transport

```
ETANT DONNE un outil de type McpServer
QUAND on construit un ToolKind::McpServer { server_url, transport, tool_name }
ALORS le champ transport accepte les variantes Stdio, Http, et WebSocket
```

### AC-3 — Validation dangerous/SandboxProfile

```
ETANT DONNE un ToolDescriptor avec sandbox_profile = SandboxProfile::Full et dangerous = false
QUAND on appelle descriptor.validate()
ALORS une erreur ToolDescriptorError::FullProfileRequiresDangerous est retournee
```

### AC-4 — Validation name non-vide

```
ETANT DONNE un ToolDescriptor avec name = ""
QUAND on appelle descriptor.validate()
ALORS une erreur ToolDescriptorError::EmptyName est retournee
```

### AC-5 — input_schema JSON Schema valide (non-vide)

```
ETANT DONNE un ToolDescriptor avec input_schema = serde_json::Value::Null
QUAND on appelle descriptor.validate()
ALORS une erreur ToolDescriptorError::InvalidInputSchema est retournee
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/descriptor.rs`

```rust
/// Describe un outil disponible dans le catalogue d'Apollia OS.
///
/// Aligne sur le schema MCP pour interoperabilite native avec l'ecosysteme
/// (16K+ serveurs MCP). Cf. doc/Briques-Tool-Registry.md section 2.
pub struct ToolDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: ToolKind,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub sandbox_profile: SandboxProfile,  // importe depuis apollia-core
    pub tags: Vec<String>,
    pub dangerous: bool,
}

/// Type d'outil : natif Rust, serveur MCP externe, ou outil Python custom.
pub enum ToolKind {
    /// Implemente en Rust dans apollia-tools.
    Native,
    /// Serveur MCP externe (stdio, HTTP, ou WebSocket).
    McpServer {
        server_url: String,
        transport: McpTransport,
        tool_name: String,
    },
    /// Outil Python enregistre par l'utilisateur.
    Custom {
        module_path: String,
        class_name: String,
    },
}

/// Protocole de transport d'un serveur MCP.
pub enum McpTransport {
    Stdio,
    Http,
    WebSocket,
}

/// Erreurs de validation d'un ToolDescriptor.
pub enum ToolDescriptorError {
    EmptyName,
    InvalidVersion(String),
    FullProfileRequiresDangerous,
    InvalidInputSchema,
}

impl ToolDescriptor {
    /// Valide la coherence du descripteur.
    /// Appele a l'enregistrement dans ToolRegistry.
    pub fn validate(&self) -> Result<(), ToolDescriptorError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance — tout est deja dans apollia-tools/Cargo.toml
# apollia-core, serde, serde_json, thiserror, tracing sont deja declares
```

### Comportement attendu

- `ToolDescriptor::validate()` est synchrone et pure — pas d'IO.
- Les derives `Debug, Clone, Serialize, Deserialize` sur tous les types publics.
- `ToolDescriptorError` implemente `thiserror::Error` (jamais `anyhow`).
- `ToolKind::Native` est utilise par tous les outils natifs du sprint (STORY-013, 014, 015).

### Ce que cette story N'implemente PAS

- Le ToolRegistry lui-meme (STORY-011)
- L'execution effective des outils (STORY-013, 014, 015)
- La persistance de descripteurs (les descripteurs sont enregistres en memoire via ToolRegistry)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/descriptor.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SandboxProfile;
    use serde_json::json;

    fn make_valid_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["shell".to_string()],
            dangerous: false,
        }
    }

    #[test]
    fn test_ac1_tool_descriptor_serialization_roundtrip() {
        // GIVEN
        let descriptor = make_valid_descriptor();
        // WHEN
        let json = serde_json::to_string(&descriptor).expect("serialization failed");
        let roundtrip: ToolDescriptor = serde_json::from_str(&json).expect("deserialization failed");
        // THEN
        assert_eq!(descriptor.name, roundtrip.name);
        assert_eq!(descriptor.version, roundtrip.version);
    }

    #[test]
    fn test_ac3_full_profile_without_dangerous_flag_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.dangerous = false;
        // WHEN
        let result = descriptor.validate();
        // THEN
        assert!(matches!(result, Err(ToolDescriptorError::FullProfileRequiresDangerous)));
    }

    #[test]
    fn test_ac4_empty_name_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.name = "".to_string();
        // WHEN / THEN
        assert!(matches!(descriptor.validate(), Err(ToolDescriptorError::EmptyName)));
    }

    #[test]
    fn test_ac5_null_input_schema_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.input_schema = serde_json::Value::Null;
        // WHEN / THEN
        assert!(matches!(descriptor.validate(), Err(ToolDescriptorError::InvalidInputSchema)));
    }

    #[test]
    fn test_valid_descriptor_passes_validation() {
        // GIVEN
        let descriptor = make_valid_descriptor();
        // WHEN / THEN
        assert!(descriptor.validate().is_ok());
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-tools` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-tools -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] `SandboxProfile` importe depuis `apollia-core`, non reddefini
- [ ] `thiserror` utilise pour `ToolDescriptorError`, jamais `anyhow`
- [ ] Principe #4 (Fail fast) respecte via `validate()`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add ToolDescriptor and ToolKind types`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-009 (Test integration EventBus ↔ Registry)
- Story suivante : STORY-011 (ToolRegistry catalogue)
- ADR associe : aucun prevu
