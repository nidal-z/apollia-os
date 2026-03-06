# [SPRINT-6][apollia-oria] Retry policy avec backoff exponentiel + jitter

**ID :** STORY-042
**Sprint :** 6
**Crate cible :** `apollia-oria`
**Fichier(s) cible(s) :** `crates/apollia-oria/src/resilience.rs` (extension de STORY-041)
**Taille :** M
**Depend de :** STORY-041 (ResilienceLayer)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que runtime,
je veux une politique de retry avec backoff exponentiel et jitter sur les erreurs transitoires,
afin de recuperer automatiquement des defaillances temporaires sans surcharger les services.
```

---

## Contexte technique

La RetryPolicy complete la ResilienceLayer (STORY-041) en ajoutant le retry automatique des erreurs transitoires avant de comptabiliser une failure. Le backoff exponentiel evite les retry storms, et le jitter aleatoire desynchronise les retries de plusieurs agents concurrents.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #7 — Garde-fous non-negociables (retry borne, pas de boucle infinie)

**Position dans l'architecture :**
```
ORIA Engine
  └── ResilienceLayer
        ├── RetryPolicy  <-- cette story
        └── CircuitBreaker[*] (STORY-041)
```

---

## Criteres d'Acceptation

### AC-1 — Retry avec backoff exponentiel sur erreur Transient

```
ETANT DONNE une RetryPolicy avec max_attempts=3, base_delay=500ms
QUAND un appel echoue avec ErrorClass::Transient
ALORS l'appel est retente jusqu'a 3 fois
ET les delais sont : ~500ms, ~1000ms, ~2000ms (backoff exponentiel base 2)
```

### AC-2 — Jitter aleatoire ajoute au delai

```
ETANT DONNE une RetryPolicy avec jitter=true
QUAND le delai de retry est calcule
ALORS un jitter aleatoire de +/- 25% est ajoute au delai de base
ET le delai ne depasse jamais max_delay
```

### AC-3 — Pas de retry sur erreur Permanent

```
ETANT DONNE une RetryPolicy avec max_attempts=3
QUAND un appel echoue avec ErrorClass::Permanent
ALORS l'erreur est retournee immediatement sans retry
```

### AC-4 — Succes apres retry ne comptabilise pas de failure

```
ETANT DONNE une RetryPolicy avec max_attempts=3
QUAND un appel echoue 2 fois (Transient) puis reussit au 3eme essai
ALORS le resultat OK est retourne
ET record_success() est appele sur le CircuitBreaker
```

### AC-5 — Echec apres tous les retries comptabilise une failure

```
ETANT DONNE une RetryPolicy avec max_attempts=3
QUAND un appel echoue 3 fois (Transient)
ALORS l'erreur est retournee
ET record_failure(Transient) est appele sur le CircuitBreaker
```

### AC-6 — Max delay borne le backoff

```
ETANT DONNE une RetryPolicy avec base_delay=500ms, max_delay=5000ms
QUAND le backoff calcule depasse 5000ms (ex: attempt 5 → 16000ms)
ALORS le delai est plafonne a 5000ms
```

---

## Specification technique

### Types a creer / modifier

```rust
/// Politique de retry avec backoff exponentiel et jitter.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Nombre maximal de tentatives (incluant l'appel initial).
    pub max_attempts: u32,
    /// Delai de base pour le premier retry (en ms).
    pub base_delay_ms: u64,
    /// Delai maximal (cap du backoff exponentiel, en ms).
    pub max_delay_ms: u64,
    /// Ajouter un jitter aleatoire (+/- 25%) au delai.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            jitter: true,
        }
    }
}

// Ajout a ResilienceLayer (STORY-041)
impl ResilienceLayer {
    /// Execute un appel avec retry policy et circuit breaker.
    pub async fn execute<F, Fut, T>(
        &mut self,
        tool_name: &str,
        error_classifier: impl Fn(&str) -> ErrorClass,
        operation: F,
    ) -> Result<T, ResilienceError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    { /* ... */ }
}
```

### Dependances Cargo

```toml
# Dans crates/apollia-oria/Cargo.toml
[dependencies]
rand = { workspace = true }

# Dans Cargo.toml workspace
[workspace.dependencies]
rand = "0.8"
```

### Comportement attendu

1. `RetryPolicy::calculate_delay(attempt)` retourne le delai pour la tentative N :
   - `delay = min(base_delay * 2^(attempt-1), max_delay)`
   - Si jitter : `delay += random(-25%, +25%)`
2. `ResilienceLayer::execute(tool_name, classifier, operation)` :
   - Appelle `pre_check(tool_name)` (circuit breaker)
   - Execute `operation()`
   - Si succes : `record_success(tool_name)`, retourne Ok
   - Si erreur : classifie via `classifier`
     - Si Permanent/BudgetExceeded/SandboxViolation : retourne Err immediatement
     - Si Transient et attempts < max_attempts : attend le delai, recommence
     - Si Transient et attempts >= max_attempts : `record_failure(tool_name, Transient)`, retourne Err

### Ce que cette story N'implemente PAS

- L'integration dans ToolProxy Python — sera fait dans STORY-044
- Le jitter cryptographiquement securise — `rand::thread_rng()` suffit
- La configuration par outil (toutes les RetryPolicy sont identiques pour le MVP)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_delay_exponential() {
        // GIVEN RetryPolicy avec base_delay=500, jitter=false
        // WHEN calculate_delay pour attempts 1, 2, 3
        // THEN 500, 1000, 2000
    }

    #[test]
    fn test_calculate_delay_capped_at_max() {
        // GIVEN RetryPolicy avec base_delay=500, max_delay=2000
        // WHEN calculate_delay pour attempt 10
        // THEN <= 2000
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        // GIVEN RetryPolicy avec jitter=true
        // WHEN calculate_delay appele 10 fois
        // THEN les valeurs varient (pas toutes identiques)
    }

    #[tokio::test]
    async fn test_execute_success_no_retry() {
        // GIVEN une operation qui reussit
        // WHEN execute()
        // THEN Ok retourne, operation appelee 1 fois
    }

    #[tokio::test]
    async fn test_execute_transient_then_success() {
        // GIVEN une operation qui echoue 2 fois (Transient) puis reussit
        // WHEN execute()
        // THEN Ok retourne apres 3 appels
    }

    #[tokio::test]
    async fn test_execute_permanent_no_retry() {
        // GIVEN une operation qui echoue avec Permanent
        // WHEN execute()
        // THEN Err retourne apres 1 appel
    }

    #[tokio::test]
    async fn test_execute_all_retries_exhausted() {
        // GIVEN une operation qui echoue toujours (Transient), max_attempts=3
        // WHEN execute()
        // THEN Err retourne apres 3 appels
        // ET record_failure appele sur circuit breaker
    }

    #[test]
    fn test_default_retry_policy() {
        // GIVEN RetryPolicy::default()
        // THEN max_attempts=3, base_delay=500, max_delay=10000, jitter=true
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
- [ ] Backoff exponentiel borne (max_delay)
- [ ] Jitter desynchronise les retries
- [ ] Seules les erreurs Transient sont retryees

**Commit :**
- [ ] `feat(apollia-oria): add RetryPolicy with exponential backoff and jitter`

---

## Liens

- Story precedente : STORY-041 (ResilienceLayer)
- Story suivante : STORY-043 (ORIA Orchestre)
- Spec : `docs/Briques-ORIA-Engine.md` (section 6.1 — RetryPolicy)
