# [SPRINT-6][apollia-oria] ResilienceLayer circuit breaker par outil

**ID :** STORY-041
**Sprint :** 6
**Crate cible :** `apollia-oria`
**Fichier(s) cible(s) :** `crates/apollia-oria/src/resilience.rs`
**Taille :** L
**Depend de :** Sprint 4 (ORIA Engine, ToolProxy)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que runtime,
je veux un circuit breaker par outil qui coupe les appels vers un outil defaillant,
afin d'eviter les cascades d'erreurs et de permettre une recuperation automatique apres cooldown.
```

---

## Contexte technique

La ResilienceLayer est le composant de fiabilite production-grade d'ORIA. Chaque outil enregistre dispose de son propre circuit breaker avec trois etats (Closed/Open/HalfOpen). Seules les erreurs transitoires (timeout, rate limit) incrementent le compteur de failures. Les erreurs permanentes (input invalide) ne declenchent pas le circuit breaker.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #7 — Garde-fous non-negociables (resilience appliquee par le runtime)
- Principe #5 — Un acteur, une responsabilite (circuit breaker isole par outil)

**Position dans l'architecture :**
```
ORIA Engine
  └── ResilienceLayer  <-- cette story
        ├── CircuitBreaker[bash_executor]
        ├── CircuitBreaker[file_io]
        └── CircuitBreaker[python_executor]
```

---

## Criteres d'Acceptation

### AC-1 — Circuit breaker en etat Closed laisse passer les appels

```
ETANT DONNE un CircuitBreaker en etat Closed pour l'outil "file_io"
QUAND un appel outil est soumis via ResilienceLayer
ALORS l'appel est execute normalement
ET le resultat (succes ou erreur) est retourne au caller
```

### AC-2 — Erreurs transitoires incrementent le compteur et ouvrent le circuit

```
ETANT DONNE un CircuitBreaker avec failure_threshold = 5
QUAND 5 erreurs Transient consecutives surviennent
ALORS le CircuitBreaker passe en etat Open
ET les appels suivants retournent ResilienceError::CircuitOpen immediatement (sans appeler l'outil)
```

### AC-3 — Cooldown declenche le passage a HalfOpen

```
ETANT DONNE un CircuitBreaker en etat Open avec cooldown = 30s
QUAND le cooldown est ecoule
ALORS le prochain appel est autorise (sonde)
ET si la sonde reussit, le circuit repasse en Closed (compteur reset)
ET si la sonde echoue, le circuit repasse en Open (cooldown reinitialisé)
```

### AC-4 — Erreurs permanentes ne declenchent pas le circuit breaker

```
ETANT DONNE un CircuitBreaker en etat Closed
QUAND une erreur Permanent survient (ex: fichier non trouve)
ALORS l'erreur est retournee au caller
ET failure_count n'est PAS incremente
ET le circuit reste Closed
```

### AC-5 — Succes en etat Closed remet le compteur a zero

```
ETANT DONNE un CircuitBreaker avec failure_count = 3 (sous le threshold de 5)
QUAND un appel reussit
ALORS failure_count est remis a 0
```

### AC-6 — ResilienceLayer gere plusieurs outils independamment

```
ETANT DONNE une ResilienceLayer avec 3 outils enregistres
QUAND "file_io" est en circuit Open
ALORS "bash_executor" et "python_executor" restent en Closed
ET les appels vers ces outils fonctionnent normalement
```

---

## Specification technique

### Types a creer

```rust
/// Classification des erreurs pour determiner la strategie de retry/circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    /// Timeout reseau, rate limit LLM — retryable.
    Transient,
    /// Input invalide, fichier non trouve — ne jamais retenter.
    Permanent,
    /// StepBudget atteint — ne pas retenter.
    BudgetExceeded,
    /// Tentative de path traversal, acces reseau non autorise.
    SandboxViolation,
}

/// Etat du circuit breaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal — requetes passent.
    Closed,
    /// Circuit ouvert — requetes rejetees immediatement.
    Open,
    /// Test — une requete sonde autorisee.
    HalfOpen,
}

/// Circuit breaker par outil.
#[derive(Debug)]
pub struct CircuitBreaker {
    tool_name: String,
    state: CircuitState,
    failure_count: u32,
    failure_threshold: u32,
    cooldown: Duration,
    last_failure_at: Option<Instant>,
}

/// Erreurs de la couche de resilience.
#[derive(Debug, thiserror::Error)]
pub enum ResilienceError {
    #[error("circuit open for tool '{tool_name}': {failure_count} consecutive failures, retry after cooldown")]
    CircuitOpen { tool_name: String, failure_count: u32 },

    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
}

/// Couche de resilience avec un circuit breaker par outil.
pub struct ResilienceLayer {
    circuit_breakers: HashMap<String, CircuitBreaker>,
    default_failure_threshold: u32,
    default_cooldown: Duration,
}
```

### Dependances Cargo

Aucune nouvelle dependance — utilise `std::time::Instant`, `std::time::Duration`, `std::collections::HashMap`.

### Comportement attendu

1. `ResilienceLayer::new(threshold, cooldown)` cree la couche avec des defaults
2. `ResilienceLayer::register_tool(name)` ajoute un CircuitBreaker pour un outil
3. `ResilienceLayer::pre_check(tool_name) -> Result<(), ResilienceError>` verifie si l'appel est autorise :
   - Closed : autorise
   - Open : verifie cooldown. Si ecoule → passe en HalfOpen et autorise. Sinon → CircuitOpen error
   - HalfOpen : autorise (une seule sonde)
4. `ResilienceLayer::record_success(tool_name)` : reset failure_count, passe en Closed si HalfOpen
5. `ResilienceLayer::record_failure(tool_name, error_class)` :
   - Si Transient : incremente failure_count. Si >= threshold → passe en Open + enregistre last_failure_at
   - Si Permanent/BudgetExceeded/SandboxViolation : ne modifie pas le circuit
6. `CircuitBreaker::state()` retourne l'etat courant (pour affichage CLI)
7. `CircuitBreaker::reset()` remet en Closed manuellement (pour `tools reset-circuit`)

### Ce que cette story N'implemente PAS

- La retry policy (backoff, jitter) — STORY-042
- L'integration dans `ToolProxy` Python — sera fait quand la chaine complete est exercee (STORY-044)
- Les events EventBus (ToolCircuitBroken/Restored) — ajoutes dans cette story si les variants existent dans RuntimeEvent, sinon ajoutes dans apollia-core
- Le mode Orchestre — STORY-043

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_allows_call() {
        // GIVEN un CircuitBreaker en Closed
        // WHEN pre_check()
        // THEN Ok(())
    }

    #[test]
    fn test_transient_errors_open_circuit() {
        // GIVEN failure_threshold = 3
        // WHEN 3 record_failure(Transient)
        // THEN state() == Open
    }

    #[test]
    fn test_open_rejects_immediately() {
        // GIVEN circuit en Open (cooldown pas ecoule)
        // WHEN pre_check()
        // THEN Err(CircuitOpen)
    }

    #[test]
    fn test_cooldown_transitions_to_half_open() {
        // GIVEN circuit en Open avec cooldown ecoule
        // WHEN pre_check()
        // THEN Ok(()) et state == HalfOpen
    }

    #[test]
    fn test_half_open_success_closes_circuit() {
        // GIVEN circuit en HalfOpen
        // WHEN record_success()
        // THEN state == Closed et failure_count == 0
    }

    #[test]
    fn test_half_open_failure_reopens_circuit() {
        // GIVEN circuit en HalfOpen
        // WHEN record_failure(Transient)
        // THEN state == Open
    }

    #[test]
    fn test_permanent_error_does_not_increment() {
        // GIVEN circuit en Closed avec failure_count == 0
        // WHEN record_failure(Permanent)
        // THEN failure_count == 0 et state == Closed
    }

    #[test]
    fn test_success_resets_failure_count() {
        // GIVEN failure_count == 3 (sous threshold de 5)
        // WHEN record_success()
        // THEN failure_count == 0
    }

    #[test]
    fn test_independent_circuit_breakers() {
        // GIVEN ResilienceLayer avec 2 outils
        // WHEN "tool_a" passe en Open
        // THEN "tool_b" reste en Closed
    }

    #[test]
    fn test_manual_reset() {
        // GIVEN circuit en Open
        // WHEN reset()
        // THEN state == Closed et failure_count == 0
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-oria` passe
- [ ] `cargo clippy -p apollia-oria -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` en production
- [ ] Docstring `///` sur chaque type/fn publique

**Architectural :**
- [ ] Circuit breaker isole par outil (pas d'etat partage)
- [ ] ErrorClass determine le comportement (seul Transient ouvre le circuit)
- [ ] Machine d'etat Closed → Open → HalfOpen → Closed correcte

**Commit :**
- [ ] `feat(apollia-oria): add ResilienceLayer with per-tool circuit breaker`

---

## Liens

- Spec : `docs/Briques-ORIA-Engine.md` (section 6 — ResilienceLayer)
- Story suivante : STORY-042 (Retry policy)
- Events : RuntimeEvent::ToolCircuitBroken/Restored dans `apollia-core/src/events.rs`
