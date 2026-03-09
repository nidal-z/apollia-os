# ORIA Engine — Observer, Reasoner, Actor

> *Le moteur d'exécution intelligent d'Apollia OS : deux modes, des garde-fous production-grade, et une résilience fondée sur l'état de l'art.*

---

## 1. Rôle et philosophie

ORIA (Observer-Reasoner-Actor) est le moteur qui transforme une tâche soumise en résultat produit. Il est responsable de :

1. **Observer** : Enrichir le contexte de la tâche (mémoire pertinente, historique, état)
2. **Reasoner** : Planifier l'exécution (mode direct ou mode orchestré)
3. **Actor** : Exécuter les steps, invoquer les outils, observer les résultats
4. **Superviser** : Faire respecter les garde-fous (StepBudget), gérer les défaillances (ResilienceLayer)

### 1.1 Le challenge de RP-ReAct

La recherche 2025 valide l'architecture Reasoner-Planner-Executor (RP-ReAct) pour les tâches complexes multi-outils. Elle montre aussi ses limites : le Reasoner peut introduire des actions redondantes et des replans inutiles sur des tâches simples, dégradant la performance par rapport à un simple agent ReAct.

**Décision :** ORIA implémente **deux modes d'exécution**, sélectionnés automatiquement :

- **Mode Direct** : Boucle ReAct supervisée pour les tâches atomiques (≤ 10 steps, ≤ 4 outils)
- **Mode Orchestré** : Reasoner + Executor découplés pour les tâches composites multi-agents

80% des cas d'usage PME relèvent du Mode Direct. Le Mode Orchestré est disponible pour les cas complexes sans imposer son overhead aux cas simples.

---

## 2. Observer — Enrichissement du contexte

L'Observer est le premier composant activé à réception d'une `AIPTask`. Son rôle : construire un `ContextBundle` complet avant tout raisonnement.

```rust
pub struct ContextBundle {
    pub task: AIPTask,
    pub memory_snapshot: Option<MemorySnapshot>,
    pub agent_state: AgentState,
    pub recent_history: Vec<TaskSummary>,    // 5 dernières tâches du context_id
    pub runtime_metrics: RuntimeMetrics,
}

pub struct MemorySnapshot {
    pub episodic_recent: Vec<EpisodicEntry>,
    pub semantic_relevant: Vec<SemanticEntry>,
    pub procedures: Vec<ProceduralEntry>,
}
```

**Opérations de l'Observer :**
1. Recevoir `AIPTask` (état `submitted`)
2. Si `memory_namespace` → `memory.search(task.input, limit=5)`
3. Charger l'historique récent depuis l'audit log (`context_id`)
4. Snapshot de l'état runtime (outils disponibles, ProcessState)
5. **Classifier la complexité** → `ExecutionMode`

**Algorithme de classification :**

```rust
pub enum ExecutionMode {
    Direct,        // ReAct supervisé, tâche atomique
    Orchestrated,  // Reasoner + Actor découplés
}
```

Le champ `execution_mode` de l'`AgentManifest` permet un override explicite (`"direct"` | `"orchestrated"`). La valeur `"auto"` (défaut) déclenche l'heuristique :

```rust
fn classify(task: &AIPTask, manifest: &AgentManifest) -> ExecutionMode {
    // Override explicite depuis le manifest Python
    match manifest.execution_mode.as_str() {
        "direct" => return ExecutionMode::Direct,
        "orchestrated" => return ExecutionMode::Orchestrated,
        _ => {}  // "auto" ou valeur inconnue → heuristique
    }

    let is_complex =
        manifest.step_budget.as_ref()
            .map(|b| b.max_steps > 15)
            .unwrap_or(false)
        || task.input.parts.len() > 3
        || manifest.tags.contains(&"multi-step".to_string())
        || manifest.tools_required.len() > 4;

    if is_complex { ExecutionMode::Orchestrated }
    else { ExecutionMode::Direct }
}
```

**Configuration Python de l'agent :**

```python
from apollia_aip import AgentManifest

def manifest(self):
    return AgentManifest(
        name="analyse-contrat",
        execution_mode="orchestrated",   # Force le Mode Orchestré
        system_prompt="Tu es un expert juridique spécialisé dans l'analyse de contrats...",
        tools_required=["file_io"],
        # ...
    )
```

Le champ `system_prompt` est injecté par ORIA dans les prompts du Reasoner pour personnaliser la planification par domaine métier.

---

## 3. Reasoner — Planification

Le Reasoner opère différemment selon le mode.

### 3.1 Mode Direct — Raisonnement délégué à l'agent

En Mode Direct, il n'y a pas de Reasoner séparé. L'agent Python gère lui-même son raisonnement interne (typiquement via une boucle ReAct avec son LLM). ORIA joue le rôle de **superviseur de boucle** :

```
ContextBundle → agent.run(task, ctx)
    │
    └── [L'agent gère sa boucle ReAct en Python]
             Thought → Action (tool_call) → Observe → Thought → ...
    │
    └── AIPResult (completed | failed | input_required)
```

**Rôle ORIA en Mode Direct :** Injecter le `RuntimeContext`, surveiller le `StepBudget`, appliquer la `ResilienceLayer` sur chaque appel d'outil.

### 3.2 Mode Orchestré — Reasoner LLM explicite

En Mode Orchestré, ORIA instancie un `Reasoner` qui appelle un `Arc<dyn CompletionModel>` (depuis `apollia-llm`) pour produire un `ExecutionPlan` structuré JSON.

```rust
pub struct ExecutionPlan {
    pub plan_id: String,
    pub steps: Vec<PlanStep>,
}

pub struct PlanStep {
    pub step_id: String,
    pub description: String,        // "Récupérer les infos client Dupont SA"
    pub tool_hint: Option<String>,  // "file_io"
    pub depends_on: Vec<String>,    // Dépendances entre steps (par step_id)
}
```

Le `Reasoner` est injecté dans l'`ORIAEngine` via un builder :

```rust
// Dans le Supervisor — après LlmRouter démarré (position 5)
let model: Arc<dyn CompletionModel> = llm_router.get(None).unwrap();
let reasoner = Reasoner::new(model);
let engine = ORIAEngine::new().with_reasoner(reasoner);
```

**Robustesse JSON :** Si le LLM produit un JSON invalide, le Reasoner retente automatiquement jusqu'à **3 fois** avec un message de correction explicite. Au-delà, `ReasonerError::PlanParseError` est levé et la tâche échoue proprement.

**Prompt système Reasoner :**

```
Tu es un planificateur de tâches pour un agent IA autonome souverain.
À partir du contexte fourni, génère un plan d'exécution JSON.

Contraintes :
- Maximum {max_steps} étapes
- Outils disponibles : {tool_names}

Format JSON strict :
{
  "steps": [
    {
      "step_id": "s1",
      "description": "...",
      "tool_hint": "file_io",
      "depends_on": []
    }
  ]
}

Tâche : {task_input}
```

**Décision modèle Reasoner :** Le Reasoner utilise **le même LLM que l'agent** via `Arc<dyn CompletionModel>` injecté depuis le `LlmRouter`. Pas de second modèle en MVP — la complexité de configurer deux modèles différents n'est pas justifiable pour la cible PME (ADR-004). Si `ORIAEngine` n'a pas de `Reasoner` configuré (LLM absent), `execute_orchestrated()` retourne `ORIAError::NoLlmConfigured`.

---

## 4. Actor — Exécution et observation

L'Actor est le composant qui traduit les steps en appels concrets aux outils, observe les résultats, et remonte au Reasoner si nécessaire.

### 4.1 Boucle Actor en Mode Orchestré

L'`ActorLoop` exécute un `ExecutionPlan` en ordre topologique. En Mode Orchestré (Option B — ADR-022), `agent.run()` n'est **jamais** appelé pendant les steps : ORIA appelle les outils et le LLM directement via `ToolProxyTrait`.

```rust
/// Abstraction du ToolProxy pour l'ActorLoop — permet les tests sans PyO3.
#[async_trait::async_trait]
pub trait ToolProxyTrait: Send + Sync {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String>;
}

/// Erreur d'un step individuel.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("Tool call failed: {0}")]
    ToolCallFailed(String),
    #[error("LLM call failed: {0}")]
    LlmCallFailed(String),
    #[error("No LLM backend configured")]
    NoLlmBackend,
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
}

impl StepError {
    /// Retourne true si l'erreur peut déclencher une replanification.
    pub fn is_retryable(&self) -> bool {
        matches!(self, StepError::ToolCallFailed(_) | StepError::LlmCallFailed(_))
    }
}
```

Pipeline d'exécution de l'`ActorLoop` :

```
ActorLoop::execute()
  ├── topological_sort(plan.steps)          → ordre d'exécution garanti
  ├── Pour chaque step dans l'ordre :
  │   ├── StepBudget::is_exhausted()        → STEP_BUDGET_EXCEEDED si épuisé
  │   ├── db.start_step()                   → SQLite persistance
  │   ├── EventBus: StepStarted { step_id, step_num, total, desc }
  │   ├── execute_step()                    → outil via ToolProxyTrait OU LLM via LlmRouter
  │   ├── budget.increment_steps()
  │   ├── db.complete_step() / db.fail_step()
  │   └── EventBus: StepCompleted { duration_ms } / StepFailed { error, retryable }
  ├── Si step échoue (retryable) + replan_count < max_replans :
  │   ├── EventBus: PlanReplanning { attempt, failed_step, reason }
  │   └── reasoner.replan() → nouveau plan → execute_remaining()
  └── Tous steps complétés :
      ├── db.complete_plan()
      └── EventBus: PlanCompleted { step_count, duration_ms }
```

### 4.2 Replanification (Mode Orchestré uniquement)

Si un step retryable échoue, l'Actor déclenche une replanification LLM plutôt qu'un abandon immédiat. **Maximum 2 replans** — au-delà, `MAX_REPLAN_EXCEEDED` est retourné.

Le Reasoner reçoit le plan original, le step ayant échoué, et son erreur, puis génère un plan alternatif. L'`ActorLoop` reprend l'exécution avec le nouveau plan en sautant les steps déjà complétés.

**Pourquoi max 2 replans :** La replanification sans limite est le vecteur principal de boucles infinies et de coûts LLM incontrôlés. 2 replans offrent une seconde chance pour les erreurs transitoires sans risque de récursion incontrôlée.

### 4.3 Persistance SQLite — PlanRepository

Chaque plan et chaque step sont persistés en temps réel dans `~/.apollia/plans.db` (migration `004_execution_plans.sql`).

```rust
pub struct PlanRepository { /* connexion SQLite */ }

impl PlanRepository {
    pub fn insert_plan(&self, plan_id: &str, task_id: &str, agent_name: &str,
                       steps: &[PlanStep]) -> Result<(), PlanRepositoryError>;
    pub fn start_step(&self, plan_id: &str, step_id: &str) -> Result<(), PlanRepositoryError>;
    pub fn complete_step(&self, plan_id: &str, step_id: &str,
                         output: &str) -> Result<(), PlanRepositoryError>;
    pub fn fail_step(&self, plan_id: &str, step_id: &str,
                     error: &str) -> Result<(), PlanRepositoryError>;
    pub fn complete_plan(&self, plan_id: &str) -> Result<(), PlanRepositoryError>;
    pub fn fail_plan(&self, plan_id: &str, reason: &str) -> Result<(), PlanRepositoryError>;
    pub fn get_plan_with_steps(&self, task_id: &str) -> Result<PlanWithSteps, PlanRepositoryError>;
}
```

`get_plan_with_steps()` est utilisé par `apollia-os task inspect` pour afficher le plan post-exécution sans nécessiter un runtime démarré.

### 4.4 Hook `on_plan_complete()` (optionnel)

Après l'exécution complète du plan, ORIA appelle le hook Python `on_plan_complete()` si l'agent l'expose (duck typing via `hasattr`). L'agent reçoit les outputs de tous les steps et peut appliquer une logique métier finale.

```python
# Hook optionnel — le contrat AIP minimal (manifest + run) reste suffisant
async def on_plan_complete(self, step_results: dict[str, str], ctx) -> str:
    """
    step_results: { "s1": "output du step 1", "s2": "output du step 2", ... }
    Retourne la réponse finale (str).
    """
    rapport = "\n\n".join(step_results.values())
    return f"## Rapport consolidé\n\n{rapport}"
```

Si le hook est absent, ORIA concatène automatiquement les outputs des steps et retourne un `AIPResult::Completed`.

---

## 5. HITL — Human-in-the-Loop (Sprint 11)

Le HITL permet à un agent de **suspendre son exécution** pour demander une décision humaine avant de continuer. ORIA implémente deux variantes selon le mode d'exécution.

### 5.1 Mode Direct — Suspension sur `AIPResult.input_required()`  (STORY-096)

En Mode Direct, l'agent Python initie la suspension en retournant `AIPResult.input_required(prompt, context)` depuis `run()`. ORIA prend alors le relais :

```
agent.run(task, ctx)
  └── AIPResult.input_required(prompt="Confirmer le montant 5 100 €", context={...})

ORIA.execute_direct() :
  ├── 1. Persiste prompt + context → TaskRepository.save_input_required() (SQLite)
  ├── 2. Émet RuntimeEvent::TaskInputRequired { task_id, prompt, step_id: None }
  ├── 3. PendingApprovals.register(task_id) → oneshot::Receiver<InputResponseData>
  ├── 4. await rx  ← SUSPENSION PURE (StepBudget ne progresse pas)
  │         │
  │         └── ResumeHandler.resolve(task_id, approved=true/false, reason?)
  │                  └── oneshot::Sender<InputResponseData>.send(response)
  │
  ├── Si approved=true :
  │   ├── Reconstruit AIPTask avec is_resumed=True + input_response peuplé
  │   └── Rappelle agent.run(resumed_task, ctx) → résultat final
  │
  └── Si approved=false :
      └── AIPResult::failed("REJECTED", reason) — run() n'est PAS rappelé
```

**Contrat Python côté agent :**

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    if task.is_resumed:
        # L'humain a approuvé — input_response contient sa réponse
        response = task.input_response
        if response.approved:
            # Continuer avec l'état sérialisé au moment du suspend
            saved_state = response.context
            return await self._continue_from(saved_state, ctx)

    # Première exécution — calculer, puis demander validation
    amount = await self._calculate_total(task, ctx)

    return AIPResult.input_required(
        prompt=f"Confirmer le devis de {amount} € pour le client ?",
        context={"amount": amount, "step": "validation"}
    )
```

**Persistance SQLite :** `TaskRepository.save_input_required(task_id, step_id=None, prompt, context)` écrit dans la table `task_hitl_state`. En cas d'échec SQLite, ORIA logue un `warn` et continue (fail-safe — Principe #4).

**StepBudget pausé pendant la suspension :** L'attente sur le oneshot channel est un `await` pur — le polling du budget ne tourne pas. Le compteur de steps ne progresse pas tant que l'humain n'a pas répondu.

**Dégradation gracieuse :** Si `PendingApprovals` n'est pas configuré sur l'`ORIAEngine`, ORIA retourne `AIPResult::InputRequired` sans suspendre (la tâche reste dans l'état `input_required` mais n'attend pas de résolution).

### 5.2 `tools_requiring_approval` dans le manifest  (STORY-093)

Le champ `tools_requiring_approval` dans `AgentManifest` permet à l'agent de déclarer les outils qui nécessitent une approbation humaine **avant** leur exécution (utilisé en Mode Orchestré).

```python
from apollia_aip import AgentManifest

def manifest(self):
    return AgentManifest(
        name="envoi-devis",
        tools_required=["file_io", "smtp", "http_client"],
        # Outils qui déclenchent une suspension HITL avant exécution
        tools_requiring_approval=["smtp", "http_client"],
    )
```

**Règles :**
- Champ optionnel — `[]` par défaut (aucune approbation requise).
- Seuls les outils listés déclenchent une suspension ; les autres s'exécutent normalement.
- La liste est lue par l'`ActorLoop` à chaque step (Mode Orchestré).

**Côté Rust — `AgentManifest` :**
```rust
pub struct AgentManifest {
    // ...
    /// Outils qui nécessitent une approbation humaine avant exécution (Mode Orchestré).
    #[serde(default)]
    pub tools_requiring_approval: Vec<String>,
}
```

### 5.3 Mode Orchestré — Suspension par step  (STORY-097)

En Mode Orchestré, c'est l'`ActorLoop` (et non l'agent) qui gère la suspension. Avant d'exécuter un step, il vérifie si l'outil du step est dans `manifest.tools_requiring_approval`.

```
ActorLoop.execute() :
  ├── Pour chaque step dans l'ordre topologique :
  │   ├── Vérifier manifest.tools_requiring_approval
  │   ├── Si step.tool_hint ∈ tools_requiring_approval ET pending_approvals configuré :
  │   │   └── ActorLoop.suspend_for_approval(step, pending_approvals)
  │   │         ├── Clé d'enregistrement : "{task_id}::{step_id}"
  │   │         ├── PendingApprovals.register(key) → oneshot::Receiver
  │   │         ├── Émet RuntimeEvent::TaskInputRequired {
  │   │         │       task_id, prompt, step_id: Some(step_id)  ← distingue Mode Direct
  │   │         │   }
  │   │         ├── await rx  ← SUSPENSION PURE
  │   │         ├── Si approved=true → Ok(()) → le step s'exécute normalement
  │   │         └── Si approved=false → Err(StepError::RejectedByUser { reason })
  │   │
  │   ├── Si rejected → AIPResult::failed("REJECTED: <reason>")
  │   │    └── Les steps restants ne s'exécutent PAS
  │   └── Si approved → execute_step() → outil via ToolProxyTrait
```

**Distinction entre Mode Direct et Mode Orchestré sur l'EventBus :**

| Champ | Mode Direct | Mode Orchestré |
|---|---|---|
| `step_id` | `None` — toute la tâche est suspendue | `Some(step_id)` — un step précis attend |
| Qui suspend | L'agent Python via `AIPResult.input_required()` | L'`ActorLoop` Rust avant `execute_step()` |
| Reprise | Re-appel `agent.run()` avec `is_resumed=True` | Exécution normale du step après `Ok(())` |
| Rejet | `AIPResult::failed` sans rappel Python | `StepError::RejectedByUser` → plan arrêté |

**Comportement si `PendingApprovals` absent :** L'`ActorLoop` logue un `warn` et exécute le step sans approbation (mode dégradé). Cela garantit qu'un runtime non-HITL reste fonctionnel avec des manifests HITL.

### 5.4 TimeoutWatcher — Annulation automatique des suspensions  (STORY-098)

Le `TimeoutWatcher` est un acteur Tokio démarré en position 9 dans le `Supervisor`. Il scanne toutes les **60 secondes** les tâches en état `input_required` et annule celles qui dépassent le délai configuré.

```rust
pub struct TimeoutWatcherConfig {
    /// Délai max avant annulation — défaut : 24 heures.
    pub input_required_timeout: Duration,
    /// Intervalle de scan — défaut : 60 secondes.
    pub scan_interval: Duration,
}
```

**Pipeline d'annulation :**
```
TimeoutWatcher.scan_and_cancel() :
  ├── TaskRepository.find_input_required_older_than(input_required_timeout)
  ├── Pour chaque tâche expirée :
  │   ├── TaskRouter.cancel_task(task_id, "input_required_timeout")
  │   │     └── TaskStatus → Canceled
  │   └── EventBus.broadcast(RuntimeEvent::TaskApprovalTimeout { task_id, after_secs })
  └── Retourne le nombre de tâches annulées
```

**Configuration dans `apollia.toml` :**
```toml
[runtime]
input_required_timeout_hours = 24    # défaut : 24h
```

**Robustesse :** Si `scan_and_cancel()` échoue (ex. SQLite indisponible), l'erreur est loguée et le `TimeoutWatcher` continue son cycle sans propager ni paniquer.

---

## 6. StepBudget — Garde-fou fondamental

Le StepBudget est le mécanisme le plus important pour la robustesse en production.

```rust
pub struct StepBudget {
    pub max: u32,                      // Nombre max de steps (défaut: 10)
    pub current: u32,                  // Steps actuellement utilisés
    pub tool_calls: u32,               // Appels outils actuels
    pub max_tool_calls: u32,           // Max appels outils (défaut: 20)
    pub started_at: Instant,
    pub wall_clock_limit: Duration,    // Durée max (défaut: 5 minutes)
}

impl StepBudget {
    pub fn is_exhausted(&self) -> bool {
        self.current >= self.max
            || self.tool_calls >= self.max_tool_calls
            || self.started_at.elapsed() > self.wall_clock_limit
    }

    pub fn steps_left(&self) -> u32 {
        self.max.saturating_sub(self.current)
    }
}
```

**Valeurs par défaut (configurables) :**

```toml
[oria]
max_steps           = 10
max_tool_calls      = 20
wall_clock_timeout  = 300    # 5 minutes
max_replans         = 2
```

**Override par agent via manifest :**

```python
AgentManifest(
    name="analyse-contrat-complexe",
    step_budget=StepBudgetConfig(
        max_steps=30,
        max_tool_calls=60,
        wall_clock_timeout=900    # 15 minutes pour les analyses longues
    )
)
```

**Le StepBudget est exposé en lecture seule à l'agent :**

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    # L'agent peut adapter sa stratégie selon le budget restant
    if ctx.step_budget.steps_left < 3:
        # Stratégie rapide quand le budget est presque épuisé
        return await self._quick_summary(task, ctx)

    # Stratégie normale
    ...
```

L'agent **lit** le budget mais ne peut pas le modifier. C'est le runtime qui l'applique.

---

## 7. ResilienceLayer — Circuit Breakers et Retry

### 7.1 Architecture

```rust
pub struct ResilienceLayer {
    pub retry_policy: RetryPolicy,
    pub circuit_breakers: HashMap<String, CircuitBreaker>,  // Par outil
}

pub struct RetryPolicy {
    pub max_attempts: u32,      // Défaut : 3
    pub base_delay_ms: u64,     // Défaut : 500ms
    pub max_delay_ms: u64,      // Défaut : 10 000ms
    pub jitter: bool,           // Défaut : true (évite les retry storms)
}

pub struct CircuitBreaker {
    pub state: CircuitState,
    pub failure_count: u32,
    pub failure_threshold: u32,  // Défaut : 5 failures consécutives
    pub cooldown: Duration,      // Défaut : 30s
    pub last_failure: Option<Instant>,
}

pub enum CircuitState {
    Closed,    // Normal — requêtes passent
    Open,      // Circuit ouvert — requêtes rejetées immédiatement
    HalfOpen,  // Test — une requête sonde autorisée
}
```

### 7.2 Machine d'état du circuit breaker

```
CLOSED (normal)
  → Succès : reset failure_count
  → Échec transient : retry (RetryPolicy)
  → failure_count ≥ threshold : → OPEN

OPEN (circuit ouvert)
  → Toute requête : BrokenCircuit error immédiate (pas d'appel outil)
  → Après cooldown : → HALF_OPEN

HALF_OPEN (test)
  → Une requête sonde autorisée
  → Succès : → CLOSED + reset compteurs
  → Échec : → OPEN + reset cooldown
```

**Affichage dans le status CLI :**
```bash
$ apollia-os status
  SANTÉ OUTILS
  bash_executor    ✔  python_executor  ✔  file_io       ✔
  http_client      ✔  mcp_erp_acme    ✗  (circuit ouvert, retry dans 18s)
```

**Réinitialisation manuelle :**
```bash
$ apollia-os tools reset-circuit mcp_erp_acme
  ✔ Circuit breaker de mcp_erp_acme réinitialisé (→ CLOSED)
```

### 7.3 Classification des erreurs

La classification est critique pour savoir quand retenter.

```rust
pub enum ErrorClass {
    Transient,          // Timeout réseau, rate limit LLM temporaire — retryable
    Permanent,          // SIRET invalide, fichier non trouvé — ne jamais retenter
    BudgetExceeded,     // StepBudget atteint — ne pas retenter
    SandboxViolation,   // Tentative de path traversal, accès réseau non autorisé
}
```

Le circuit breaker ne s'incrémente que sur les erreurs `Transient`. Une erreur `Permanent` n'ouvre pas le circuit — elle indique un problème avec l'input, pas avec l'outil.

---

## 8. Trace end-to-end complète

**Tâche :** "Génère un devis pour Dupont SA, 5 jours de conseil, tarif journalier 850€"

```
[ORIA] Task reçue → task_id: t-001, context_id: session-42
[ORIA.Observer] Chargement ContextBundle...
  → memory.search("Dupont SA") → 3 épisodes, 2 clés sémantiques
  → recent_history: 2 tâches précédentes session-42
  → tools disponibles: file_io, python_executor
  → classify() → MODE_DIRECT (2 tools, max_steps=10)
  → task → "working"

[ORIA.Direct] Injection RuntimeContext → agent.run()
  [Step 1/10] Thought: Récupérer les infos client Dupont SA
              Action: ctx.tools.file_io.read("clients/dupont_sa.json")
              [ResilienceLayer] → CircuitBreaker(file_io) CLOSED → passe
              Observe: {siret: "12345678901234", contact: "Marie Dupont"}

  [Step 2/10] Thought: Calculer le montant TTC
              Action: ctx.tools.python_executor.run("print(5*850*1.2)")
              Observe: "5100.0"
              [StepBudget] 2/10 steps, 2/20 tool_calls, 0.8s/300s

  [Step 3/10] Thought: Générer le fichier devis
              Action: ctx.tools.file_io.write("devis/devis-042.json", {...})
              Observe: success

  [Step 4/10] Action: ctx.memory.record("Devis #042 généré pour Dupont SA, 5100€")
  [Step 5/10] Action: ctx.memory.remember("client.dupont_sa.last_devis", {...})

[ORIA] AIPResult(status=COMPLETED, artifacts=[devis-042.json])
[ORIA] task → "completed"
[ORIA] EventBus.broadcast(TaskCompleted{task_id: t-001, success: true})
[ORIA] Audit: t-001, 5 steps, 3 tool_calls, 2.3s, success
```

---

## 9. Intégration dans le lifecycle AIP

```
AIPTask soumise (submitted)
  └── ORIA.Observer.enrich()              → ContextBundle
  └── ORIA.Reasoner.classify()            → ExecutionMode
  └── task → "working"

Mode Direct
  └── ORIA.Actor.run_direct(agent, ctx, budget)
      └── agent.run(task, ctx)             [boucle ReAct interne Python]
      └── supervision StepBudget + ResilienceLayer sur chaque tool_call
      │
      └── Si AIPResult::InputRequired (HITL STORY-096) :
          ├── TaskRepository.save_input_required()   → SQLite
          ├── EventBus: TaskInputRequired { task_id, prompt, step_id: None }
          ├── PendingApprovals.register(task_id)     → oneshot::Receiver
          ├── await rx  ←─── SUSPENSION (StepBudget pausé)
          │         └── ResumeHandler.resolve() envoie InputResponseData
          ├── Si approved=true  → AIPTask{is_resumed=true, input_response} → agent.run() again
          └── Si approved=false → AIPResult::failed("REJECTED", reason)

Mode Orchestré
  └── ORIA.Reasoner.plan(context_bundle)  → ExecutionPlan (JSON LLM)
  └── PlanRepository.insert_plan()       → SQLite (~/.apollia/plans.db)
  └── EventBus: PlanGenerated { plan_id, step_count }
  └── ORIA.ActorLoop.execute(plan)
      └── Pour chaque step (ordre topologique) :
          ├── Si tool_hint ∈ tools_requiring_approval (HITL STORY-097) :
          │   ├── EventBus: TaskInputRequired { task_id, prompt, step_id: Some(step_id) }
          │   ├── await rx  ←── SUSPENSION (StepBudget pausé)
          │   ├── Si rejected → AIPResult::failed("REJECTED: <reason>") — plan arrêté
          │   └── Si approved → continuer vers execute_step()
          ├── EventBus: StepStarted { step_id, step_num, total, desc }
          ├── execute_step() → outil ou LLM
          └── EventBus: StepCompleted / StepFailed
      └── Si step échoue (retryable) + replan_count < 2 :
          ├── EventBus: PlanReplanning { attempt, failed_step }
          └── reasoner.replan() → nouveau plan → reprendre
  └── Si agent.on_plan_complete() → appel hook
  └── PlanRepository.complete_plan() / fail_plan()
  └── EventBus: PlanCompleted / PlanFailed

TimeoutWatcher (toutes les 60s — STORY-098)
  └── Scan TaskRepository.find_input_required_older_than(timeout)
  └── Pour chaque tâche expirée :
      ├── TaskRouter.cancel_task()         → TaskStatus::Canceled
      └── EventBus: TaskApprovalTimeout { task_id, after_secs }

AIPResult → Runtime Core
  └── Audit log SQLite
  └── EventBus.broadcast(TaskCompleted/Failed/ApprovalTimeout)
  └── TaskState → completed | failed | input_required | canceled
  └── SSE stream → tous les events TaskInputRequired/PlanGenerated/Step* émis en temps réel
```

---

## 10. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Deux modes Direct / Orchestré | Évite overhead RP-ReAct sur tâches simples (validé par benchmarks 2025) |
| Classification automatique | Zéro config pour PME, runtime décide selon manifest + input |
| StepBudget tri-dimensionnel | Trois vecteurs d'abus distincts (steps, tool_calls, temps) |
| Max 2 replans | Coût LLM prévisible, comportement déterministe en production |
| Circuit breaker par outil | Isolation fine — un outil défaillant n'affecte pas les autres |
| Classification Transient / Permanent | Retry uniquement sur ce qui peut se résoudre |
| Modèle Reasoner = même LLM que l'agent | Pas de complexité multi-modèle en MVP PME |
| StepBudget exposé à l'agent (lecture seule) | Agent peut adapter sa stratégie proactivement |
| HITL via `oneshot` channel + `PendingApprovals` (ADR-023) | Suspension pure sans polling — le budget ne progresse pas ; résolution thread-safe via HashMap de senders |
| Reprise HITL par re-appel `agent.run()` avec `is_resumed=True` (ADR-023) | Pas de nouveau point d'entrée — contrat minimal préservé (Principe #3) ; l'agent voit explicitement qu'il reprend via `task.is_resumed` |
| `tools_requiring_approval` dans le manifest (ADR-023) | Déclaration décentralisée côté agent — le runtime n'a pas à connaître la sémantique métier de chaque outil |
| `TimeoutWatcher` scan périodique (60s) | Pas de timer par tâche — un seul acteur gère tous les timeouts, O(n) SQLite au lieu de O(n) timers en mémoire |

---

*Prochaine lecture recommandée : [Runtime Core](./Briques-Runtime-Core)*
