# [Sprint 2][apollia-tools] ToolResolver — validation des outils a INITIALIZING

**ID :** STORY-012
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/resolver.rs`
**Taille :** M
**Depend de :** STORY-011 (ToolRegistry), STORY-007 ✅ (AgentRegistry)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un ToolResolver qui valide la disponibilite de tous les outils declares
dans un AgentManifest au moment ou l'agent entre en etat INITIALIZING,
afin que toute absence d'outil bloquant soit detectee immediatement
et que l'agent passe en STOPPED avec un message d'erreur clair.
```

---

## Contexte technique

Le ToolResolver implemente le principe "Fail fast" (#4) pour l'outillage. Il distingue
`tools_required` (echec fatal → STOPPED) et `tools_optional` (degradation → DEGRADED).
Appele par le runtime lors de la transition CREATED → INITIALIZING.

**Principe(s) architecturaux concernes :**
- Principe #4 — Fail fast (verification complete avant ACTIVE)
- Principe #5 — Un acteur, une responsabilite (ToolResolver valide uniquement, n'execute pas)

**Position dans l'architecture :**
```
Runtime Core
  └── transition INITIALIZING
        └── ToolResolver::resolve(manifest, registry)  <- cette story
              ├── check tools_required  → Err(RequiredToolMissing)
              ├── check tools_optional  → Ok(resolutions avec warnings)
              └── ResolutionReport      → transmis a AgentRegistry
```

---

## Criteres d'Acceptation

### AC-1 — Tous les outils requis presents → succes

```
ETANT DONNE un ToolRegistry avec "bash_executor" et "file_io" enregistres
ET un AgentManifest avec tools_required = ["bash_executor", "file_io"]
QUAND on appelle ToolResolver::resolve(manifest, registry)
ALORS un ResolutionReport avec status = AllResolved et warnings vide est retourne
```

### AC-2 — Outil requis manquant → erreur fatale

```
ETANT DONNE un ToolRegistry vide
ET un AgentManifest avec tools_required = ["bash_executor"]
QUAND on appelle ToolResolver::resolve(manifest, registry)
ALORS Err(ToolResolutionError::RequiredToolMissing("bash_executor")) est retourne
```

### AC-3 — Outil optionnel manquant → warning, pas d'erreur

```
ETANT DONNE un ToolRegistry sans "mcp_erp"
ET un AgentManifest avec tools_optional = ["mcp_erp"] et tools_required = []
QUAND on appelle ToolResolver::resolve(manifest, registry)
ALORS Ok(ResolutionReport { status: Degraded, warnings: ["mcp_erp not found"] }) est retourne
```

### AC-4 — Conflit sandbox profile dangereux sans flag

```
ETANT DONNE un ToolRegistry avec un outil dangerous=true non-marque
ET un manifest qui requiert cet outil sans `dangerous_tools_allowed = true`
QUAND on appelle ToolResolver::resolve()
ALORS Err(ToolResolutionError::DangerousToolNotAllowed(name)) est retourne
```

### AC-5 — Manifest vide → succes immediat

```
ETANT DONNE un AgentManifest avec tools_required = [] et tools_optional = []
QUAND on appelle ToolResolver::resolve(manifest, registry)
ALORS Ok(ResolutionReport { status: AllResolved, warnings: [] }) est retourne
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/resolver.rs`

```rust
/// Rapport de resolution des outils d'un agent.
pub struct ResolutionReport {
    /// Status global de la resolution.
    pub status: ResolutionStatus,
    /// Outils requis resolus avec succes.
    pub resolved: Vec<String>,
    /// Avertissements (outils optionnels absents, etc.).
    pub warnings: Vec<String>,
}

/// Status global d'une resolution.
pub enum ResolutionStatus {
    /// Tous les outils requis disponibles.
    AllResolved,
    /// Certains outils optionnels absents — agent en DEGRADED.
    Degraded,
}

/// Erreurs de resolution bloquantes (provoquent STOPPED).
pub enum ToolResolutionError {
    /// Outil requis absent du catalogue.
    RequiredToolMissing(String),
    /// Outil dangerous=true mais agent n'a pas autorise les outils dangereux.
    DangerousToolNotAllowed(String),
    /// Erreur de communication avec le ToolRegistry.
    RegistryError(ToolRegistryError),
}

/// Valide la disponibilite des outils declares dans un manifest.
///
/// Appele exclusivement pendant la transition INITIALIZING.
/// Retourne Ok(ResolutionReport) ou Err(ToolResolutionError) si bloquant.
pub async fn resolve(
    manifest: &AgentManifest,
    registry: &ToolRegistryHandle,
) -> Result<ResolutionReport, ToolResolutionError> { ... }
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- `resolve()` est une fonction libre (pas un acteur), pure et asynchrone (queries vers registry).
- Les verifications se font dans l'ordre : required d'abord, optional ensuite.
- Si plusieurs outils requis sont manquants, seul le premier echec est retourne (fail fast).
- Un warning est emis via `tracing::warn!` pour chaque outil optionnel absent.
- Le `ResolutionReport` est transmis par le runtime a `AgentRegistry` pour transition vers ACTIVE ou DEGRADED.

### Ce que cette story N'implemente PAS

- La transition d'etat dans AgentRegistry (c'est le Runtime Core qui l'applique)
- L'installation des packages Python (hors scope — c'est python_executor STORY-014)
- La validation des schemas JSON des outils (hors scope sprint 2)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/resolver.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, SandboxProfile};

    fn minimal_manifest() -> AgentManifest {
        AgentManifest {
            name: "test-agent".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            tools_required: vec![],
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
        }
    }

    async fn registry_with_tools(names: &[&str]) -> ToolRegistryHandle {
        let registry = ToolRegistryHandle::start();
        for name in names {
            let descriptor = ToolDescriptor {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: format!("{} tool", name),
                kind: ToolKind::Native,
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: None,
                sandbox_profile: SandboxProfile::FileSystem,
                tags: vec![],
                dangerous: false,
            };
            registry.register(descriptor).await.unwrap();
        }
        registry
    }

    #[tokio::test]
    async fn test_ac1_all_required_tools_present_succeeds() {
        // GIVEN
        let registry = registry_with_tools(&["bash_executor", "file_io"]).await;
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["bash_executor".to_string(), "file_io".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("resolution should succeed");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert!(report.warnings.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac2_missing_required_tool_returns_error() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["bash_executor".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        assert!(matches!(result, Err(ToolResolutionError::RequiredToolMissing(_))));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac3_missing_optional_tool_is_warning_only() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec!["mcp_erp".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("resolution should succeed despite missing optional");
        assert!(matches!(report.status, ResolutionStatus::Degraded));
        assert!(!report.warnings.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_ac5_empty_manifest_resolves_immediately() {
        // GIVEN
        let registry = ToolRegistryHandle::start();
        let manifest = minimal_manifest();
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("resolution should succeed");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
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
- [ ] Principe #4 respecte : echec immediat sur premier outil requis manquant
- [ ] `tracing::warn!` emis pour chaque outil optionnel absent
- [ ] Pas de side-effects dans `resolve()` (lecture seule du registry)

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add ToolResolver for INITIALIZING validation`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-011 (ToolRegistry catalogue)
- Story suivante : aucune dans ce sprint (derniere story du sprint 2)
- ADR associe : aucun prevu
