# [Sprint 2][apollia-tools] ToolRegistry — catalogue en memoire

**ID :** STORY-011
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/registry.rs`
**Taille :** M
**Depend de :** STORY-010
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un ToolRegistry qui maintient un catalogue en memoire de tous les outils disponibles,
afin que le ToolResolver puisse verifier si un outil requis par un agent est disponible
a l'etat INITIALIZING.
```

---

## Contexte technique

Le ToolRegistry est le composant central de `apollia-tools`. Il suit le pattern acteur Tokio
(mpsc::channel + handle clonable) etabli par AgentRegistry (STORY-007). Les outils natifs
(bash_executor, python_executor, file_io) s'enregistrent automatiquement au demarrage du runtime.

**Principe(s) architecturaux concernes :**
- Principe #5 — Un acteur, une responsabilite (ToolRegistry = catalogue uniquement)
- Principe #4 — Fail fast (enregistrement avec validation via `ToolDescriptor::validate()`)

**Position dans l'architecture :**
```
apollia-tools
  ├── descriptor.rs  (STORY-010 ✅)
  └── registry.rs    <- cette story
        ├── ToolRegistry     (acteur interne)
        ├── ToolRegistryHandle (handle public clonable)
        └── ToolRegistryMessage (messages mpsc)
```

---

## Criteres d'Acceptation

### AC-1 — Enregistrement d'un outil valide

```
ETANT DONNE un ToolRegistry vide
QUAND on enregistre un ToolDescriptor valide (bash_executor)
ALORS get("bash_executor") retourne Some(ToolDescriptor) avec les bons champs
```

### AC-2 — Enregistrement invalide rejete

```
ETANT DONNE un ToolRegistry actif
QUAND on tente d'enregistrer un ToolDescriptor avec name = ""
ALORS register() retourne Err(ToolRegistryError::InvalidDescriptor(...))
et le catalogue n'est pas modifie
```

### AC-3 — Doublon rejete

```
ETANT DONNE un ToolRegistry avec "bash_executor" v1.0.0 enregistre
QUAND on tente d'enregistrer un second outil avec name = "bash_executor"
ALORS register() retourne Err(ToolRegistryError::AlreadyRegistered("bash_executor"))
```

### AC-4 — Liste des outils disponibles

```
ETANT DONNE un ToolRegistry avec 3 outils enregistres
QUAND on appelle list()
ALORS une Vec<ToolDescriptor> avec les 3 descripteurs est retournee
```

### AC-5 — Outil inconnu retourne None

```
ETANT DONNE un ToolRegistry vide
QUAND on appelle get("inexistant")
ALORS None est retourne (pas d'erreur)
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/registry.rs`

```rust
/// Messages internes de l'acteur ToolRegistry.
enum ToolRegistryMessage {
    Register {
        descriptor: ToolDescriptor,
        reply: oneshot::Sender<Result<(), ToolRegistryError>>,
    },
    Get {
        name: String,
        reply: oneshot::Sender<Option<ToolDescriptor>>,
    },
    List {
        reply: oneshot::Sender<Vec<ToolDescriptor>>,
    },
    Shutdown,
}

/// Acteur interne — jamais expose directement.
struct ToolRegistry {
    catalogue: HashMap<String, ToolDescriptor>,
    receiver: mpsc::Receiver<ToolRegistryMessage>,
}

/// Handle clonable et thread-safe vers le ToolRegistry.
///
/// Seul type expose en dehors de ce module.
pub struct ToolRegistryHandle {
    sender: mpsc::Sender<ToolRegistryMessage>,
}

/// Erreurs du ToolRegistry.
pub enum ToolRegistryError {
    AlreadyRegistered(String),
    InvalidDescriptor(ToolDescriptorError),
    ActorGone,
}

impl ToolRegistryHandle {
    /// Demarre l'acteur ToolRegistry et retourne son handle.
    pub fn start() -> Self { ... }

    /// Enregistre un outil dans le catalogue.
    pub async fn register(&self, descriptor: ToolDescriptor) -> Result<(), ToolRegistryError> { ... }

    /// Retourne le descripteur d'un outil par son nom, ou None s'il est absent.
    pub async fn get(&self, name: &str) -> Result<Option<ToolDescriptor>, ToolRegistryError> { ... }

    /// Retourne la liste de tous les outils enregistres.
    pub async fn list(&self) -> Result<Vec<ToolDescriptor>, ToolRegistryError> { ... }

    /// Arrete l'acteur proprement.
    pub async fn shutdown(self) { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance — tokio, thiserror, tracing sont deja declares
```

### Comportement attendu

- L'acteur est demarre via `ToolRegistryHandle::start()` qui spawn une task Tokio.
- Le catalogue interne est un `HashMap<String, ToolDescriptor>` (cle = `descriptor.name`).
- `register()` appelle `descriptor.validate()` avant insertion — si invalide, `InvalidDescriptor` est retourne.
- `ToolRegistryHandle` est `Clone + Send + Sync`.
- Taille de channel : 32 messages (meme convention qu'AgentRegistry).

### Ce que cette story N'implemente PAS

- La resolution des outils d'un agent (STORY-012)
- L'enregistrement automatique des outils natifs au demarrage du runtime (hors scope sprint 2)
- La persistence du catalogue (en memoire uniquement, reinitialisee au redemarrage)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/registry.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn bash_executor_descriptor() -> ToolDescriptor {
        use apollia_core::SandboxProfile;
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands in sandbox".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
        }
    }

    #[tokio::test]
    async fn test_ac1_register_and_get_tool() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let descriptor = bash_executor_descriptor();
        // WHEN
        registry.register(descriptor.clone()).await.expect("register failed");
        let result = registry.get("bash_executor").await.expect("get failed");
        // THEN
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "bash_executor");
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac2_invalid_descriptor_rejected() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let mut descriptor = bash_executor_descriptor();
        descriptor.name = "".to_string();
        // WHEN
        let result = registry.register(descriptor).await;
        // THEN
        assert!(matches!(result, Err(ToolRegistryError::InvalidDescriptor(_))));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac3_duplicate_rejected() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        registry.register(bash_executor_descriptor()).await.expect("first register failed");
        // WHEN
        let result = registry.register(bash_executor_descriptor()).await;
        // THEN
        assert!(matches!(result, Err(ToolRegistryError::AlreadyRegistered(_))));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac4_list_returns_all_tools() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        registry.register(bash_executor_descriptor()).await.unwrap();
        // WHEN
        let list = registry.list().await.expect("list failed");
        // THEN
        assert_eq!(list.len(), 1);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac5_get_unknown_tool_returns_none() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        // WHEN
        let result = registry.get("inexistant").await.expect("get failed");
        // THEN
        assert!(result.is_none());
        registry.shutdown().await;
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
- [ ] Pattern acteur Tokio respecte : `mpsc::channel` + handle clonable
- [ ] Jamais `Arc<Mutex<T>>` cross-acteurs
- [ ] `ToolRegistryHandle` est `Clone + Send + Sync`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add ToolRegistry actor with in-memory catalogue`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-010 (ToolDescriptor types)
- Story suivante : STORY-015 (file_io) ou STORY-013 (bash_executor)
- ADR associe : aucun prevu
