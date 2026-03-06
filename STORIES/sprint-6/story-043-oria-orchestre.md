# [SPRINT-6][apollia-oria] ORIA Mode Orchestre + Reasoner LLM

**ID :** STORY-043
**Sprint :** 6
**Crate cible :** `apollia-oria`
**Fichier(s) cible(s) :** `crates/apollia-oria/src/reasoner.rs`, `crates/apollia-oria/src/actor.rs`
**Taille :** XL
**Depend de :** STORY-041 (ResilienceLayer), STORY-042 (RetryPolicy)
**Statut :** 🚫 Reportee Sprint 7 — non necessaire pour la demo MVP (l'agent tourne en Mode Direct)

---

## User Story

```
En tant que runtime,
je veux un mode d'execution Orchestre qui planifie les taches complexes via un Reasoner LLM,
afin d'executer des workflows multi-etapes avec replanification automatique en cas d'echec.
```

---

## Contexte technique

Le Mode Orchestre est le second mode d'execution d'ORIA, active automatiquement par `classify()` (STORY-029) pour les taches complexes (> 4 outils, > 15 steps, tag "multi-step"). Il decouple la planification (Reasoner) de l'execution (ActorLoop), permettant la replanification en cas d'echec d'un step.

Le Reasoner utilise un LLM pour generer un `ExecutionPlan` structure (JSON). Le trait `LLMProvider` abstrait le backend LLM (Ollama local par defaut, conforme au principe local-first).

**Principe(s) architectural(aux) concerne(s) :**
- Principe #1 — Local-first (Ollama comme backend LLM par defaut)
- Principe #7 — Garde-fous non-negociables (StepBudget applique, max 2 replans)
- Principe #5 — Un acteur, une responsabilite (Reasoner planifie, Actor execute)

**Position dans l'architecture :**
```
ORIA Engine
  ├── Observer (STORY-029)
  ├── Reasoner  <-- cette story
  │     └── LLMProvider trait
  ├── ActorLoop  <-- cette story
  │     └── ResilienceLayer (STORY-041)
  └── StepBudget (STORY-030)
```

---

## Criteres d'Acceptation

### AC-1 — Reasoner produit un ExecutionPlan valide

```
ETANT DONNE un ContextBundle avec execution_mode == Orchestrated
QUAND le Reasoner appelle le LLM avec le prompt systeme et le contexte
ALORS un ExecutionPlan est retourne avec des PlanSteps ordonnes
ET chaque PlanStep a un step_id unique, une description, et un status Pending
```

### AC-2 — ActorLoop execute les steps dans l'ordre des dependances

```
ETANT DONNE un ExecutionPlan avec steps s1, s2 (depends_on: [s1]), s3 (depends_on: [s2])
QUAND ActorLoop.execute() est appele
ALORS s1 est execute en premier, puis s2, puis s3
ET chaque step est passe a l'agent via call_run() avec le contexte enrichi
```

### AC-3 — Step echoue avec erreur retryable declenche la replanification

```
ETANT DONNE un step qui echoue avec une erreur retryable (apres retries epuises)
QUAND replan_count < 2
ALORS le Reasoner est appele pour generer un plan alternatif
ET l'execution reprend avec le nouveau plan
```

### AC-4 — Maximum 2 replanifications

```
ETANT DONNE replan_count == 2
QUAND un step echoue avec une erreur retryable
ALORS AIPResult::failed("MAX_REPLAN_EXCEEDED", ...) est retourne
ET aucun nouvel appel LLM n'est fait
```

### AC-5 — StepBudget applique pendant l'execution orchestree

```
ETANT DONNE un ExecutionPlan avec 10 steps et un StepBudget de max_steps=5
QUAND 5 steps sont executes
ALORS AIPResult::failed("STEP_BUDGET_EXCEEDED", ...) est retourne
ET les steps restants ne sont pas executes
```

### AC-6 — LLMProvider trait avec MockProvider pour les tests

```
ETANT DONNE un MockLLMProvider qui retourne un plan predetermine
QUAND le Reasoner est instancie avec ce mock
ALORS le plan retourne est celui du mock
ET aucun appel reseau n'est fait
```

### AC-7 — ORIAEngine dispatch vers execute_orchestrated si mode Orchestrated

```
ETANT DONNE une tache classifiee Orchestrated par classify()
QUAND ORIAEngine execute la tache
ALORS execute_orchestrated() est appele (et non execute_direct())
```

---

## Specification technique

### Types a creer

```rust
// --- reasoner.rs ---

/// Trait abstrayant le backend LLM pour le Reasoner.
pub trait LLMProvider: Send + Sync {
    /// Envoie un prompt au LLM et retourne la reponse texte brute.
    fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>>;
}

/// Plan d'execution structure genere par le Reasoner.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    pub plan_id: String,
    pub task_id: String,
    pub steps: Vec<PlanStep>,
}

/// Etape d'un plan d'execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    pub step_id: String,
    pub description: String,
    pub tool_hint: Option<String>,
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub status: StepStatus,
}

/// Statut d'une etape du plan.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Completed { output: String },
    Failed { error: String, retryable: bool },
    Skipped,
}

/// Erreurs du Reasoner.
#[derive(Debug, thiserror::Error)]
pub enum ReasonerError {
    #[error("LLM call failed: {0}")]
    LLMFailed(String),
    #[error("failed to parse execution plan: {0}")]
    PlanParseError(String),
    #[error("max replans exceeded ({count})")]
    MaxReplansExceeded { count: u32 },
}

/// Reasoner LLM pour le Mode Orchestre.
pub struct Reasoner<L: LLMProvider> {
    provider: L,
    max_replans: u32,
}

// --- actor.rs ---

/// Boucle d'execution du Mode Orchestre.
pub struct ActorLoop {
    plan: ExecutionPlan,
    replan_count: u32,
    max_replans: u32,
}

/// Erreurs de l'ActorLoop.
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("step {step_id} failed: {reason}")]
    StepFailed { step_id: String, reason: String },
    #[error("dependency {dep_id} not completed for step {step_id}")]
    DependencyNotMet { step_id: String, dep_id: String },
}
```

### Dependances Cargo

```toml
# Dans crates/apollia-oria/Cargo.toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
```

### Comportement attendu

**Reasoner :**
1. `Reasoner::new(provider, max_replans)` construit le Reasoner
2. `Reasoner::plan(context_bundle, available_tools) -> Result<ExecutionPlan, ReasonerError>` :
   - Construit le system prompt avec les outils disponibles et le budget
   - Construit le user prompt avec le task input et le contexte memoire
   - Appelle `provider.complete(system, user)`
   - Parse la reponse JSON en `ExecutionPlan`
   - Valide que les step_ids sont uniques et les depends_on referent des steps existants
3. `Reasoner::replan(current_plan, failed_step, error) -> Result<ExecutionPlan, ReasonerError>` :
   - Construit un prompt de replanification incluant le plan original et l'erreur
   - Genere un plan alternatif

**ActorLoop :**
1. `ActorLoop::new(plan, max_replans)` construit la boucle
2. `ActorLoop::execute(runner, budget, resilience) -> AIPResult` :
   - Itere les steps dans l'ordre topologique (depends_on)
   - Pour chaque step : verifie budget, execute via runner, enregistre le resultat
   - Si step echoue (retryable) et replan_count < max_replans : appelle Reasoner::replan
   - Si step echoue (non-retryable) : retourne AIPResult::failed
   - Si tous les steps completent : retourne AIPResult::completed

**ORIAEngine :**
1. `execute_orchestrated()` ajoute a ORIAEngine :
   - Appelle Reasoner::plan()
   - Cree ActorLoop
   - Execute avec supervision StepBudget

### Ce que cette story N'implemente PAS

- Le backend Ollama reel — MVP utilise MockLLMProvider dans les tests. L'impl Ollama peut etre ajoutee dans un follow-up
- L'execution parallele de steps independants — execution sequentielle en MVP
- Le streaming step-by-step (emission d'events par step) — hors scope MVP
- La persistance de l'ExecutionPlan dans SQLite — hors scope

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // --- Reasoner tests ---

    #[tokio::test]
    async fn test_reasoner_plan_valid_json() {
        // GIVEN un MockLLMProvider qui retourne un JSON valide
        // WHEN plan()
        // THEN ExecutionPlan avec les steps corrects
    }

    #[tokio::test]
    async fn test_reasoner_plan_invalid_json() {
        // GIVEN un MockLLMProvider qui retourne du texte invalide
        // WHEN plan()
        // THEN ReasonerError::PlanParseError
    }

    #[tokio::test]
    async fn test_reasoner_llm_failure() {
        // GIVEN un MockLLMProvider qui retourne Err
        // WHEN plan()
        // THEN ReasonerError::LLMFailed
    }

    #[tokio::test]
    async fn test_reasoner_replan_produces_new_plan() {
        // GIVEN un plan avec un step echoue
        // WHEN replan()
        // THEN nouveau plan retourne (plan_id different)
    }

    // --- ActorLoop tests ---

    #[tokio::test]
    async fn test_actor_loop_sequential_execution() {
        // GIVEN un plan avec 3 steps sans deps
        // WHEN execute()
        // THEN les 3 steps sont executes en sequence
    }

    #[tokio::test]
    async fn test_actor_loop_respects_dependencies() {
        // GIVEN step s2 depends_on [s1]
        // WHEN execute()
        // THEN s1 est execute avant s2
    }

    #[tokio::test]
    async fn test_actor_loop_budget_exceeded() {
        // GIVEN budget max_steps=2 et plan avec 5 steps
        // WHEN execute()
        // THEN STEP_BUDGET_EXCEEDED apres 2 steps
    }

    #[tokio::test]
    async fn test_actor_loop_replan_on_failure() {
        // GIVEN un step qui echoue (retryable), max_replans=2
        // WHEN execute()
        // THEN replan est appele, execution reprend
    }

    #[tokio::test]
    async fn test_actor_loop_max_replans_exceeded() {
        // GIVEN replan_count >= 2
        // WHEN un step echoue
        // THEN MAX_REPLAN_EXCEEDED
    }

    // --- ORIAEngine integration ---

    #[tokio::test]
    async fn test_oria_dispatches_to_orchestrated() {
        // GIVEN une tache classifiee Orchestrated
        // WHEN ORIAEngine execute
        // THEN execute_orchestrated() est appele
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
- [ ] LLMProvider trait pour testabilite (meme pattern ADR-015/016)
- [ ] Max 2 replans non contournable
- [ ] StepBudget applique pendant l'execution orchestree
- [ ] Execution sequentielle respectant depends_on

**Commit :**
- [ ] `feat(apollia-oria): add Reasoner LLM and ActorLoop for orchestrated mode`

---

## Liens

- Story precedente : STORY-042 (RetryPolicy)
- Story suivante : STORY-044 (Agent devis-generator)
- Spec : `docs/Briques-ORIA-Engine.md` (sections 3, 4, 5)
- ADR potentiel : choix du backend LLM (Ollama vs API externe)
