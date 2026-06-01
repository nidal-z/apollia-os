# ADR-016 - Trait AgentRunner pour decoupler ORIAEngine de AIPBridge

**Date :** 2026-03-06
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 4 (STORY-030)

---

## Contexte

STORY-030 introduit `ORIAEngine::execute_direct()`, qui supervise l'execution d'un agent en Mode Direct avec un `StepBudget`. La spec initiale indique `bridge: &AIPBridge` comme parametre.

Le probleme : `AIPBridge` (STORY-026) depend de PyO3 (`Py<PyAny>`) et ne peut etre instancie sans un interpreteur Python reel. Les tests unitaires de `ORIAEngine` dans `apollia-oria` seraient donc impossibles sans Python, alors que la logique testee (verification budget, `tokio::select!`, propagation d'erreurs) est 100% Rust.

Ce probleme est identique a celui resolu par ADR-015 pour `ToolProxy` avec le trait `ToolExecutor`.

## Decision

Nous introduisons un trait `AgentRunner` dans `apollia-oria::engine` :

```rust
pub trait AgentRunner: Send + Sync {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send + '_>>;
}
```

`execute_direct()` prend `runner: &dyn AgentRunner` au lieu de `bridge: &AIPBridge`. `AIPBridge` implementera ce trait dans une story ulterieure.

## Alternatives considerees

### Option A - Prendre `&AIPBridge` directement (rejetee)
**Pour :** Conforme a la spec STORY-030 initiale, zero indirection.
**Contre :** Tests unitaires impossibles sans Python reel. `apollia-oria` devrait dependre de `apollia-aip` (qui depend de PyO3), creant un couplage fort entre le moteur d'execution et le bridge Python.

### Option B - Fonction libre inner testable sans trait (rejetee)
**Pour :** Pas de trait supplementaire.
**Contre :** La logique de `tokio::select!` necessite un Future pour la branche execution. Extraire une `execute_direct_inner()` ne resout pas le probleme car le Future doit etre fourni par le caller.

### Option retenue - Trait `AgentRunner` injectable
**Pour :** Testable via `MockRunnerOk`/`MockRunnerErr`, pas de dependance PyO3 dans apollia-oria, extensible (le trait pourra servir pour des runners non-Python dans le futur).
**Compromis acceptes :** Signature `execute_direct()` diverge de la spec initiale STORY-030. Indirection dynamique (vtable) negligeable.

## Consequences

**Positives :**
- Tests unitaires de ORIAEngine sans Python, sans interpreteur, sans GIL
- `apollia-oria` ne depend pas de `apollia-aip` (pas de couplage cyclique)
- Pattern coherent avec ADR-015 (`ToolExecutor`)

**Negatives / Compromis :**
- La signature `execute_direct()` prend `&dyn AgentRunner` au lieu de `&AIPBridge`
- L'implementation concrete du trait pour `AIPBridge` sera dans une story ulterieure

**Neutres / A surveiller :**
- Le trait retourne un `Pin<Box<dyn Future>>` (async trait object). Quand Rust stabilisera `async fn` dans les traits, le trait pourra etre simplifie

## Principes architecturaux impactes

- Principe #5 - Un acteur, une responsabilite : Respecte. Le runner execute, le moteur supervise.
- Principe #7 - Garde-fous non-negociables : Respecte. Le StepBudget est applique par ORIAEngine independamment du runner concret.

## Liens

- Story associee : STORY-030
- ADR precedent sur le meme sujet : ADR-015 (ToolExecutor, meme pattern)
- AIPBridge : `crates/apollia-aip/src/bridge.rs`
