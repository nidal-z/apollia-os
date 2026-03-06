# ADR-015 — Trait ToolExecutor pour abstraire l'execution des outils

**Date :** 2026-03-06
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 4 (STORY-027)

---

## Contexte

STORY-027 introduit `ToolProxy`, un `#[pyclass]` qui permet aux agents Python d'invoquer les outils Rust via `ctx.tools.call(tool_name, input)`. Le flux complet est : verification des permissions, lookup dans le registry, execution de l'outil, audit, compteur.

Le probleme : `ToolRegistryHandle` (Sprint 2, STORY-011) est un catalogue pur. Il expose `register()`, `get()`, `list()` — aucune methode `execute()`. L'execution reelle est dispersee entre `FileIo`, `BashExecutor`, `PythonExecutor`, chacun avec sa propre API.

Le `ToolProxy` a besoin d'un point d'entree unifie pour executer n'importe quel outil par nom, indispensable pour les tests unitaires (mock) et pour le dispatch futur.

## Decision

Nous introduisons un trait `ToolExecutor` dans `apollia-aip::context` :

```rust
pub trait ToolExecutor: Send + Sync {
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
```

`ToolProxy` detient un `Arc<dyn ToolExecutor>` en plus des champs specifies dans STORY-027. Le constructeur `new()` accepte cet executor en parametre.

## Alternatives considerees

### Option A — Ajouter `execute()` a `ToolRegistryHandle` (rejetee)
**Pour :** Pas de nouveau trait, interface unifiee dans apollia-tools.
**Contre :** Couple le catalogue (responsabilite : stocker les descripteurs) avec l'execution (responsabilite : invoquer les outils). Modifie du code Sprint 2 stable. Le registry devrait connaitre tous les types d'outils concrets — violation du principe #5.

### Option B — Pas d'abstraction, execution hardcodee dans ToolProxy (rejetee)
**Pour :** Zero indirection.
**Contre :** Impossible a tester unitairement sans Python reel et sans outils fonctionnels. Chaque nouvel outil necessite une modification du ToolProxy.

### Option retenue — Trait `ToolExecutor` injectable
**Pour :** Testable via `MockExecutor`, respecte le principe de responsabilite unique, extensible sans modifier ToolProxy.
**Compromis acceptes :** Un champ supplementaire (`executor: Arc<dyn ToolExecutor>`) par rapport a la spec STORY-027 initiale. Indirection dynamique (vtable) negligeable.

## Consequences

**Positives :**
- Tests unitaires de ToolProxy sans Python, sans outils reels, sans base SQLite
- Le `NativeToolExecutor` concret (dispatch FileIo/BashExecutor/PythonExecutor) sera implemente dans une story ulterieure sans modifier ToolProxy

**Negatives / Compromis :**
- Le struct ToolProxy diverge legerement de la spec STORY-027 (champ `executor` ajoute)
- L'execution reelle n'est pas encore implementee — les tests utilisent un mock

**Neutres / A surveiller :**
- Le trait est synchrone (`fn execute`) pour simplicite. Si des outils necessitent une execution async, le trait devra evoluer vers `async fn execute` (ou un pattern `BoxFuture`)

## Principes architecturaux impactes

- Principe #5 — Un acteur, une responsabilite : Respecte. Le registry catalogue, l'executor execute, le proxy coordonne.

## Liens

- Story associee : STORY-027
- ToolRegistryHandle : `crates/apollia-tools/src/registry.rs`
- ADR precedent : ADR-014 (bridge async pattern)
