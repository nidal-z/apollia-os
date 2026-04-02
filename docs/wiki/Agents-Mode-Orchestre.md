# Guide — Agents en Mode Orchestré

> *Comment écrire un agent Python qui délègue sa planification à ORIA : `execution_mode`, `system_prompt`, et le hook optionnel `on_plan_complete`.*

---

## Qu'est-ce que le Mode Orchestré ?

En Mode Direct (par défaut), c'est votre code Python dans `run()` qui décide quels outils appeler et dans quel ordre. En Mode Orchestré, c'est le runtime qui planifie et exécute — votre agent décrit **ce qu'il sait faire** (via `system_prompt`), et le runtime décide **comment** le faire.

**Les composants d'ORIA :**
- **ORIA** (Observer-Reasoner-Actor) : le moteur d'exécution du runtime. C'est lui qui supervise tous les agents, en mode direct comme orchestré.
- **Reasoner** : un appel LLM qui, à partir du `system_prompt` de l'agent et de la tâche soumise, génère un plan d'exécution JSON (quels outils appeler, dans quel ordre, avec quels paramètres).
- **ActorLoop** : la boucle d'exécution qui parcourt le plan step par step, appelle les outils, et gère les erreurs. Elle applique le `StepBudget` à chaque step.

---

## 1. Principe

En **Mode Orchestré**, l'agent n'implémente pas de boucle ReAct dans `run()`. ORIA prend en charge l'intégralité de l'exécution :

1. Le **Reasoner** génère un `ExecutionPlan` JSON à partir du `system_prompt` de l'agent et de l'input de la tâche
2. L'**ActorLoop** exécute les steps dans l'ordre topologique, appelle les outils directement
3. ORIA replanifie automatiquement si un step échoue (max 2 replans)
4. L'agent peut optionnellement post-traiter les résultats via `on_plan_complete()`

**Différence avec le Mode Direct :**

| | Mode Direct | Mode Orchestré |
|---|---|---|
| `run()` appelé par ORIA | Oui — boucle complète | Non — `run()` ignoré |
| Qui planifie | L'agent (boucle ReAct interne) | ORIA (LLM Reasoner) |
| Qui appelle les outils | L'agent via `ctx.tools` | ORIA via `ActorLoop` |
| Replanification | Manuelle dans `run()` | Automatique (max 2) |
| Persistance SQLite | Non | Oui (`~/.apollia/plans.db`) |

### Flux d'exécution comparé

```mermaid
graph LR
    subgraph Direct["Mode Direct"]
        T1[Tâche soumise] --> R1["run() appelé"]
        R1 --> L1["Agent raisonne (LLM)"]
        L1 --> O1["Agent appelle outils"]
        O1 --> L1
        L1 --> D1[Résultat retourné]
    end

    subgraph Orchestrated["Mode Orchestré"]
        T2[Tâche soumise] --> P2["Reasoner génère un plan"]
        P2 --> A2["ActorLoop exécute step 1"]
        A2 --> B2["ActorLoop exécute step 2"]
        B2 --> C2["...step N"]
        C2 --> H2["on_plan_complete() (optionnel)"]
        H2 --> D2[Résultat retourné]
    end
```

---

## 2. Contrat minimal

Le **contrat AIP reste identique** : `manifest()` + `run()` async. `run()` n'est simplement pas appelé pendant l'exécution des steps.

```python
from apollia_aip import AgentManifest, AIPTask, AIPResult, RuntimeContext

class AnalyseContratAgent:

    def manifest(self):
        return AgentManifest(
            name="analyse-contrat",
            version="1.0.0",
            description="Analyse un contrat et extrait les clauses clés",
            execution_mode="orchestrated",       # Force le Mode Orchestré
            system_prompt=(
                "Tu es un expert juridique spécialisé dans l'analyse de contrats commerciaux. "
                "Décompose l'analyse en étapes séquentielles : lecture, extraction, synthèse. "
                "Utilise file_io pour lire les fichiers. Chaque step doit produire un résultat autonome."
            ),
            tools_required=["file_io"],
        )

    async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
        # En Mode Orchestré, run() n'est PAS appelé par ORIA pendant les steps.
        # Cette méthode est requise par le contrat AIP mais ne sera pas invoquée.
        # Laisser vide ou lever une exception pour signaler une invocation inattendue.
        raise NotImplementedError("Mode orchestré — run() non utilisé")
```

---

## 3. Champs `AgentManifest` spécifiques au Mode Orchestré

### `execution_mode`

```python
execution_mode="orchestrated"  # Force le Mode Orchestré
execution_mode="direct"        # Force le Mode Direct
execution_mode="auto"          # Heuristique ORIA (défaut)
```

Toute valeur inconnue est traitée comme `"auto"`.

### `system_prompt`

Prompt système injecté dans le Reasoner LLM pour guider la planification. Doit décrire :
- Le domaine métier de l'agent
- Le style de décomposition souhaité (séquentiel, parallèle, itératif)
- Les contraintes spécifiques (format de sortie, outils préférés)

```python
system_prompt=(
    "Tu es un assistant comptable pour PME françaises. "
    "Décompose les tâches en steps atomiques de 1-3 minutes chacun. "
    "Utilise python_executor pour les calculs et file_io pour la persistance. "
    "Chaque step doit avoir un output vérifiable et indépendant."
)
```

> `system_prompt` est **requis** si `execution_mode="orchestrated"`. ORIA émet un avertissement si absent.

---

## 4. Hook optionnel `on_plan_complete()`

Après l'exécution de tous les steps, ORIA détecte via duck typing (`hasattr`) si l'agent expose un hook `on_plan_complete`. Si oui, il l'appelle avec les outputs de tous les steps.

### Signature

```python
async def on_plan_complete(
    self,
    step_results: dict[str, str],  # { "s1": "output_step_1", "s2": "output_step_2", ... }
    ctx: RuntimeContext,
) -> str:
    """
    Post-traitement des résultats de tous les steps.
    Retourne la réponse finale (str) — devient l'output de la tâche.
    """
    ...
```

### Exemple — consolidation d'un rapport

```python
class AnalyseContratAgent:

    def manifest(self):
        return AgentManifest(
            name="analyse-contrat",
            execution_mode="orchestrated",
            system_prompt="Tu es un expert juridique...",
            tools_required=["file_io"],
        )

    async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
        raise NotImplementedError("Mode orchestré — run() non utilisé")

    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx: RuntimeContext,
    ) -> str:
        # Récupérer les outputs par step_id
        clauses = step_results.get("s2", "")
        synthese = step_results.get("s3", "")

        # Construire la réponse finale structurée
        rapport = f"## Analyse du contrat\n\n### Clauses identifiées\n{clauses}\n\n### Synthèse\n{synthese}"

        # Optionnel : persister en mémoire
        await ctx.memory.record(f"Analyse contrat : {len(step_results)} sections traitées")

        return rapport
```

### Si `on_plan_complete` est absent

ORIA concatène automatiquement les outputs de tous les steps dans l'ordre d'exécution :

```
output_s1\n\noutput_s2\n\noutput_s3
```

---

## 5. Visualisation temps réel avec `apollia-os run`

En Mode Orchestré, `apollia-os run` affiche le plan et la progression step par step :

```bash
$ apollia-os run analyse-contrat "Analyse le contrat Dupont SA"

  Plan généré (3 étapes) :
  ├── [s1] Lire le fichier contrat Dupont SA  → file_io
  ├── [s2] Extraire les clauses clés  → llm  (attend s1)
  └── [s3] Rédiger la synthèse exécutive  → llm  (attend s2)

  ● [1/3] Lire le fichier contrat Dupont SA...
  ✔ [1/3] (complété)  0.1s
  ● [2/3] Extraire les clauses clés...
  ✔ [2/3] (complété)  2.3s
  ● [3/3] Rédiger la synthèse exécutive...
  ✔ [3/3] (complété)  1.9s

  ✔ Tâche complétée en 4.4s
```

Si un step échoue et qu'une replanification est déclenchée :

```bash
  ✗ [2/3] Extraire les clauses clés  → Erreur : fichier encodage UTF-16
  ⟳ Replanification 1/2...

  Plan révisé (3 étapes) :
  ├── [s1] Lire le fichier contrat Dupont SA  → file_io       ✔
  ├── [s2b] Convertir l'encodage UTF-16  → python_executor
  └── [s3] Extraire les clauses et rédiger  → llm  (attend s2b)
```

**Inspecter le plan après exécution :**

```bash
$ apollia-os task inspect t-abc123
```

---

## 6. Contraintes et bonnes pratiques

### `system_prompt` efficace

- **Atomique** : chaque step doit produire un résultat indépendant et vérifiable
- **Borné** : indiquer une durée ou une taille maximale par step
- **Outil explicite** : préciser quel outil utiliser par type de tâche (`file_io` pour I/O, `python_executor` pour calcul)
- **Dépendances claires** : si l'ordre compte, le mentionner explicitement

```python
system_prompt=(
    "Décompose la tâche en 3-5 steps maximum. "
    "Chaque step est indépendant et produit un JSON ou une chaîne de texte. "
    "Pour lire/écrire des fichiers : utilise file_io. "
    "Pour les calculs : utilise python_executor. "
    "Pour la génération de texte : utilise llm (aucun tool_hint). "
    "Les steps peuvent avoir des dépendances via depends_on."
)
```

### StepBudget en Mode Orchestré

Le `StepBudget` s'applique par step, pas par tâche. Un agent orchestré avec 10 steps et un budget par défaut de 10 steps sera épuisé au premier step si chaque step consomme 10 tool_calls.

Adapter le budget dans le manifest :

```python
from apollia_aip import AgentManifest, StepBudgetConfig

AgentManifest(
    name="analyse-contrat",
    execution_mode="orchestrated",
    step_budget=StepBudgetConfig(
        max_steps=20,          # Plus de steps pour les tâches complexes
        max_tool_calls=40,
        wall_clock_timeout=600  # 10 minutes max
    ),
    # ...
)
```

### Replanification

La replanification est automatique pour les erreurs retryables (`ToolCallFailed`, `LlmCallFailed`). Les erreurs permanentes (`ToolNotFound`, `NoLlmBackend`) font échouer la tâche immédiatement.

Le `system_prompt` peut guider le comportement de replanification :

```python
system_prompt=(
    "Si un step échoue, propose une approche alternative. "
    "Par exemple, si file_io échoue sur un fichier, essaie python_executor pour le lire."
)
```

---

## 7. Exemple complet — Agent devis multi-étapes

```python
from apollia_aip import AgentManifest, AIPTask, AIPResult, RuntimeContext

class DevisGeneratorOrchestre:
    """Agent de génération de devis en Mode Orchestré."""

    def manifest(self):
        return AgentManifest(
            name="devis-generator-orchestre",
            version="1.0.0",
            description="Génère un devis structuré via planification ORIA",
            execution_mode="orchestrated",
            system_prompt=(
                "Tu es un assistant commercial pour PME. "
                "Pour générer un devis : "
                "1. Lire les infos client depuis clients/<nom>.json (file_io), "
                "2. Calculer les montants HT et TTC (python_executor), "
                "3. Générer le JSON du devis (llm), "
                "4. Sauvegarder dans devis/devis-<date>.json (file_io). "
                "Chaque step est indépendant. Utilise depends_on pour l'ordre."
            ),
            tools_required=["file_io", "python_executor"],
            memory_namespace="commercial",
        )

    async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
        raise NotImplementedError("Mode orchestré — run() non utilisé")

    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx: RuntimeContext,
    ) -> str:
        # Récupérer le chemin du devis généré par le dernier step
        devis_path = step_results.get("s4", "")
        montant = step_results.get("s2", "")

        # Mémoriser pour le suivi commercial
        await ctx.memory.record(f"Devis généré : {devis_path}, montant : {montant}")

        return f"Devis généré avec succès : {devis_path}\nMontant : {montant}"
```

---

## 8. Internals ORIA — Référence technique

### 8.1 Scoring Observer — heuristique `execution_mode: "auto"`

Quand `execution_mode` est `"auto"`, l'Observer calcule un **score de complexité pondéré**. Si ce score atteint le seuil `ORCHESTRATED_THRESHOLD`, la tâche passe en Mode Orchestré.

#### Constantes

| Constante | Valeur |
|---|---|
| `ORCHESTRATED_THRESHOLD` | `0.40` |
| `INPUT_LENGTH_THRESHOLD` | `500` chars |
| `MEMORY_DEPTH_THRESHOLD` | `5` épisodes |
| `COMPLEXITY_STEP_THRESHOLD` | `15` steps |
| `COMPLEXITY_PARTS_THRESHOLD` | `3` parts |
| `COMPLEXITY_TOOLS_THRESHOLD` | `4` outils |

#### Les 7 facteurs pondérés

| Facteur | Condition | Poids |
|---|---|---|
| Budget élevé | `step_budget.max_steps > 15` | `+0.30` |
| Beaucoup de parts | `input.parts.len() > 3` | `+0.20` |
| Tag `"multi-step"` | `manifest.tags` contient `"multi-step"` | `+0.40` |
| Beaucoup d'outils | `tools_required.len() > 4` | `+0.20` |
| Input long | Total chars texte `> 500` | `+0.10` |
| Mémoire profonde | `episodic_recent.len() > 5` | `+0.10` |
| Prompt planifiant | `system_prompt` contient `plan/etape/step/sequence/workflow/pipeline` | `+0.10` |

**Score maximum théorique : ~1.40** (tous facteurs actifs).

#### Exemples

```
Agent simple (2 outils, 10 steps, pas de tag, input court)
→ score = 0.0 → Direct

Agent avec tag "multi-step" seul
→ score = 0.40 ≥ 0.40 → Orchestrated

Agent avec 5 outils + 20 steps
→ score = 0.20 (outils) + 0.30 (steps) = 0.50 ≥ 0.40 → Orchestrated

Agent complexe (5+ outils, 20 steps, tag "multi-step", input long)
→ score = 0.20 + 0.30 + 0.40 + 0.10 = 1.00 → Orchestrated
```

> **Note :** Le tag `"multi-step"` seul atteint exactement le seuil (0.40) et suffit à forcer le Mode Orchestré sans autre signal.

#### Priorité absolue de l'override explicite

L'override de `execution_mode` dans le manifest prime **toujours** sur le scoring :

```python
"execution_mode": "orchestrated"  # → Orchestrated, scoring ignoré
"execution_mode": "direct"        # → Direct, même avec 10 outils
"execution_mode": "auto"          # → scoring pondéré
```

---

### 8.2 `StepContext` — contexte injecté dans chaque step

`StepContext` est la struct interne construite par `ActorLoop` avant d'exécuter chaque step. Elle accumule les sorties des steps précédents et expose l'état courant du budget.

```rust
pub struct StepContext {
    /// Outputs de tous les steps déjà complétés, indexés par step_id.
    pub previous_outputs: HashMap<String, String>,
    /// Index 0-based du step courant dans l'ordre topologique.
    pub step_index: usize,
    /// Nombre total de steps dans le plan.
    pub total_steps: usize,
    /// Snapshot du budget restant au moment de la construction.
    pub remaining_budget: StepBudgetView,
}
```

**Comment `previous_outputs` est utilisé :**

- Pour les **steps LLM** : formaté en message système `"Previous step results:\n- s1: …\n- s2: …"` pour enrichir le prompt.
- Pour les **steps outil** : interpolé via des placeholders `{{step_id}}` dans les paramètres de l'outil.

**Exemple (depuis `on_plan_complete`)** : `step_results` dans le hook Python est exactement `previous_outputs` converti en dict Python.

---

### 8.3 Replanification — heuristiques et conditions

La replanification est déclenchée automatiquement par l'`ActorLoop` lorsqu'un step échoue avec une erreur **retryable**.

#### Constante

```
MAX_REPLANS = 2
```

#### Erreurs retryables vs permanentes

| Type d'erreur | Retryable | Comportement |
|---|---|---|
| `ToolCallFailed` | oui | Déclenche replanification |
| `LlmCallFailed` | oui | Déclenche replanification |
| `ToolNotFound` | **non** | Échec immédiat de la tâche |
| `NoLlmBackend` | **non** | Échec immédiat de la tâche |
| `RejectedByUser` | **non** | Échec immédiat (décision HITL) |
| `ApprovalChannelClosed` | **non** | Arrêt propre (shutdown runtime) |

#### Flux de replanification

```
Step N échoue (ToolCallFailed ou LlmCallFailed)
  └── replan_count < MAX_REPLANS (2) ?
        ├── Oui → emit RuntimeEvent::PlanReplanning
        │         → Reasoner.replan(context, completed_outputs, failed_step_id, error)
        │         → Nouveau plan partiel (steps restants seulement)
        │         → Mise à jour SQLite (begin_replan, status="replanning")
        │         → execute_remaining() sur le nouveau plan
        └── Non → AIPResult::failed("MAX_REPLAN_EXCEEDED", "2 replanifications dépassées")
```

#### Ce que le `system_prompt` peut guider

```python
system_prompt=(
    "Si un step échoue, propose une approche alternative. "
    "Par exemple, si file_io échoue sur un fichier, "
    "essaie python_executor pour le lire."
)
```

Le Reasoner reçoit le contexte d'échec (`failed_step_id` + `error_message` + `completed_outputs`) et génère un plan **partiel** — uniquement les steps restants, en tenant compte de ce qui est déjà complété.

---

### 8.4 `model_hint` — routing multi-LLM par step

Chaque `PlanStep` peut optionnellement spécifier un `model_hint` pour router ce step vers un backend LLM spécifique. Cela permet d'utiliser un modèle lourd pour la synthèse et un modèle léger pour les étapes simples.

#### Structure `PlanStep`

```rust
pub struct PlanStep {
    pub step_id: String,        // ex: "s1"
    pub description: String,    // description en langage naturel
    pub tool_hint: Option<String>,   // outil suggéré, ou "llm"
    pub depends_on: Vec<String>,     // step_ids prérequis
    pub model_hint: Option<String>,  // backend LLM pour ce step — None = défaut
}
```

#### Comportement du `model_hint`

| Cas | Comportement |
|---|---|
| `model_hint: None` | Backend LLM par défaut du `LlmRouter` |
| `model_hint: Some("fast-7b")` et backend `"fast-7b"` existe | Route vers `"fast-7b"` |
| `model_hint: Some("unknown")` et backend inconnu | `tracing::warn!` + fallback vers défaut |
| `tool_hint` est un outil natif (pas `"llm"`) | `model_hint` **ignoré** — step outil |

#### Utilisation côté LLM Reasoner

Le Reasoner reçoit la liste des backends disponibles via `{llm_backend_names}` dans son prompt système. Il peut alors produire des plans avec `model_hint` peuplé :

```json
{
  "steps": [
    {"step_id": "s1", "description": "Lire le fichier", "tool_hint": "file_io", "depends_on": []},
    {"step_id": "s2", "description": "Analyser les données", "tool_hint": "llm",
     "model_hint": "fast-7b", "depends_on": ["s1"]},
    {"step_id": "s3", "description": "Rédiger la synthèse", "tool_hint": "llm",
     "model_hint": "anthropic", "depends_on": ["s2"]}
  ]
}
```

---

### 8.5 `PlanCacheRepository` — cache de plans (ADR-036)

ORIA évite de régénérer des plans identiques via un cache SQLite. Sur un cache hit, le plan est retourné directement sans appel LLM.

#### Clé de cache (SHA-256)

```
SHA-256("{agent_name}:{agent_version}:{tools_sorted_joined}:{normalized_text}")
```

Règles de normalisation du texte de tâche :
1. Lowercase
2. Espaces multiples collapés en un seul espace
3. Trimé
4. Tronqué à **500 caractères**

Les outils sont triés alphabétiquement avant d'être joints par `","` — l'ordre de déclaration dans `tools_required` n'affecte pas la clé.

#### Paramètres de cache

| Paramètre | Valeur | Source |
|---|---|---|
| TTL | **7 jours** | `evict_expired(7)` (ADR-036) |
| Max entries | **1000** | ADR-036 (LRU eviction) |
| Stockage | `~/.apollia/plan_cache.db` | SQLite WAL mode |
| Invalidation | Agent version change | `agent_version` dans la clé |

#### API REST

| Endpoint | Description |
|---|---|
| `GET /api/v1/plan-cache/stats` | Statistiques agrégées |
| `POST /api/v1/plan-cache/clear` | Vider le cache |

**Réponse `GET /api/v1/plan-cache/stats` :**

```json
{
  "total_entries": 42,
  "cache_hits": 187,
  "oldest_entry_at": "2026-03-25T10:00:00",
  "newest_entry_at": "2026-04-01T15:30:00"
}
```

**Réponse `POST /api/v1/plan-cache/clear` :**

```json
{
  "deleted": 42
}
```

#### CLI

```bash
$ apollia-os plan-cache stats
$ apollia-os plan-cache clear
```

#### Comportement sur cache hit / miss

- **Hit** : `hit_count` incrémenté, `last_used_at` mis à jour — plan retourné sans appel LLM
- **Miss** : Reasoner génère le plan, plan stocké en cache via `INSERT OR REPLACE`
- **503 Service Unavailable** : retourné par les routes REST si `plan_cache` n'est pas configuré dans l'état du runtime

---

## 9. Décisions architecturales associées

| Décision | Référence |
|---|---|
| Option B — ORIA exécute les outils directement | [ADR-022](./Decisions-Log#adr-022) |
| Deux modes Direct / Orchestré | [ADR-004](./Decisions-Log#adr-004) |
| Duck typing pour le contrat AIP | [ADR-003](./Decisions-Log#adr-003) |
| PlanCache SHA-256 + TTL 7j + LRU 1000 | [ADR-036](./Decisions-Log#adr-036) |

---

*Lecture recommandée : [ORIA Engine](./Briques-ORIA-Engine) | [RuntimeContext Guide](./Agents-RuntimeContext-Guide) | [ADR-036](../adr/ADR-036-plan-cache)*
