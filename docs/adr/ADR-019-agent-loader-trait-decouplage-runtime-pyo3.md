# ADR-019 — Trait AgentLoader pour decoupler apollia-runtime de PyO3

**Date :** 2026-03-06
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 6 (STORY-044)

---

## Contexte

STORY-044 resout DT-031 : `manifest_from_path()` dans `routes_agents.rs` retourne un manifest placeholder au lieu de charger le module Python reel via `AIPLoader` + `validate_agent()`. Le handler `start_agent` doit charger le module Python, valider le duck typing AIP, et enregistrer le vrai manifest dans `AgentRegistry`.

Le probleme : `AIPLoader` et `validate_agent()` vivent dans `apollia-aip`, qui depend de PyO3/pyo3-async-runtimes/apollia-memory. Ajouter `apollia-aip` comme dependance de `apollia-runtime` signifierait :
- Le runtime Rust depend transitvement de PyO3 pour compiler
- Les tests unitaires de `apollia-runtime` (73 tests, Sprint 5) necessiteraient Python
- Le graphe de dependances change : runtime -> aip -> memory (couplage fort)

Ce probleme est identique a ceux resolus par ADR-015 (ToolExecutor) et ADR-016 (AgentRunner) : decoupler un composant Rust pur d'une dependance PyO3 via un trait injectable.

## Decision

Nous introduisons un trait `AgentLoader` dans `apollia-runtime` :

```rust
pub trait AgentLoader: Send + Sync {
    fn load_and_validate(
        &self,
        path: &std::path::Path,
    ) -> Result<apollia_core::AgentManifest, String>;
}
```

Le trait est ajoute a `AppState` via `Arc<dyn AgentLoader>`. Le handler `start_agent` appelle `state.agent_loader.load_and_validate()` au lieu de `manifest_from_path()`.

L'implementation concrete `AIPAgentLoader` vit dans `apollia-cli` (qui depend deja de toutes les crates) et utilise `apollia_aip::loader::load_agent_module()` + `apollia_aip::validator::validate_agent()`.

## Alternatives considerees

### Option A — apollia-runtime depend directement de apollia-aip (rejetee)
**Pour :** Simple, aucune abstraction necessaire.
**Contre :** Couple le runtime a PyO3 (toute compilation du runtime necessite Python link). Les 73 tests existants deviendraient dependants de Python. Viole le pattern etabli par ADR-015/016.

### Option B — Feature flag `python` sur apollia-runtime (rejetee)
**Pour :** Le runtime reste compilable sans Python, activation optionnelle.
**Contre :** Complexite conditionnelle (`#[cfg(feature = "python")]` partout), deux chemins de code a maintenir, les tests doivent couvrir les deux configurations.

### Option retenue — Trait `AgentLoader` injectable via `Arc<dyn AgentLoader>`
**Pour :** Tests unitaires sans Python (mock loader), runtime decouple de l'implementation Python, pattern coherent avec ADR-015/016, zero dependance PyO3 dans apollia-runtime.
**Compromis acceptes :** Un champ `agent_loader` ajoute a `AppState`. Les tests existants doivent fournir un mock loader (trivial).

## Consequences

**Positives :**
- Tests unitaires de routes_agents.rs restent executables sans Python
- `apollia-runtime` ne depend pas de `apollia-aip` ni de PyO3
- Pattern coherent avec les 3 traits precedents (ExecutionBackend, ToolExecutor, AgentRunner)

**Negatives / Compromis :**
- `AppState` a un champ supplementaire (`agent_loader: Arc<dyn AgentLoader>`)
- Les tests existants de routes_agents.rs doivent fournir un mock loader
- L'indirection dynamique (vtable) est negligeable

**Neutres / A surveiller :**
- Si d'autres types de loaders apparaissent (WASM agents, containers), le trait les accueillera naturellement

## Principes architecturaux impactes

- Principe #3 — Contrat minimal : Respecte. Le trait expose seulement `load_and_validate(path) -> Result<AgentManifest>`.
- Principe #5 — Un acteur, une responsabilite : Respecte. Le loader charge, le handler enregistre.

## Liens

- Story associee : STORY-044
- ADR precedents sur le meme pattern : ADR-015 (ToolExecutor), ADR-016 (AgentRunner)
- DT-031 : manifest_from_path MVP dans routes_agents.rs
