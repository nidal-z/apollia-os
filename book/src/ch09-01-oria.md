# ORIA — Observer, Reasoner, Actor

ORIA (Observer-Reasoner-Actor) est le moteur d'exécution qui transforme une tâche soumise en résultat produit. Il opère dans les deux modes — Direct et Orchestré — mais son rôle diffère selon le mode.

---

## Les trois composants

```
Tâche soumise
      │
      ▼
┌─────────────┐
│  Observer   │  Enrichit le contexte : mémoire, historique, état
└──────┬──────┘
       │ ContextBundle
       ▼
┌─────────────┐
│  Reasoner   │  Planifie l'exécution (Mode Orchestré uniquement)
└──────┬──────┘
       │ ExecutionPlan
       ▼
┌─────────────┐
│    Actor    │  Exécute les steps, appelle les outils, observe les résultats
└──────┬──────┘
       │
       ▼
   Résultat
```

---

## Observer — enrichir avant de raisonner

L'Observer est le premier composant activé à réception d'une tâche. Il construit un `ContextBundle` complet avant tout raisonnement :

```rust
pub struct ContextBundle {
    pub task: AIPTask,
    pub memory_snapshot: Option<MemorySnapshot>,   // si memory_namespace déclaré
    pub agent_state: AgentState,
    pub recent_history: Vec<TaskSummary>,          // 5 dernières tâches du context_id
    pub runtime_metrics: RuntimeMetrics,
}
```

À la réception d'une tâche, l'Observer effectue dans l'ordre :

1. Charger les entrées mémoire pertinentes (si `memory_namespace` déclaré dans le manifest)
2. Charger les 5 dernières tâches du même `context_id` depuis l'audit log
3. Snapshot de l'état runtime (outils disponibles, état du processus)
4. **Classifier la complexité** → sélectionner le mode d'exécution

### Classification automatique (mode `"auto"`)

La classification repose sur un scoring pondéré à 7 facteurs. Chaque facteur contribue un poids ; si le score total dépasse le seuil `0.40`, le mode Orchestré est sélectionné :

| Facteur | Poids | Condition déclenchante |
|---|---|---|
| `WEIGHT_STEPS` | 0.30 | `step_budget.max_steps > 15` |
| `WEIGHT_MULTI_STEP_TAG` | 0.40 | tag `"multi-step"` présent dans le manifest |
| `WEIGHT_PARTS` | 0.20 | `input.parts.len() > 3` |
| `WEIGHT_TOOLS` | 0.20 | `tools_required.len() > 4` |
| `WEIGHT_INPUT_LENGTH` | 0.10 | input texte > 500 caractères |
| `WEIGHT_MEMORY_DEPTH` | 0.10 | mémoire épisodique > 5 entrées |
| `WEIGHT_PLANNING_PROMPT` | 0.10 | input contient des mots-clés de planification |

**Mots-clés de planification détectés :** `"plan"`, `"etape"`, `"step"`, `"sequence"`, `"workflow"`, `"pipeline"`.

Pour un agent avec `execution_mode: "orchestrated"` dans son manifest, le scoring est court-circuité — le mode Orchestré est toujours sélectionné.

---

## Reasoner — générer le plan

En Mode Direct, il n'y a pas de Reasoner : l'agent Python gère son propre raisonnement via `ctx.llm.run_tools()`. ORIA se contente de superviser le budget et d'appliquer la ResilienceLayer.

En Mode Orchestré, le Reasoner appelle le LLM configuré pour produire un `ExecutionPlan` JSON structuré. Il reçoit en entrée le `system_prompt` de l'agent, la tâche soumise, et la liste des outils disponibles :

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

**Robustesse JSON :** Si le LLM produit un JSON invalide, le Reasoner retente jusqu'à 3 fois avec un message de correction explicite. Au-delà, la tâche échoue avec `ReasonerError::PlanParseError`.

### Multi-model routing par step

Depuis le Sprint 20, chaque step du plan peut spécifier un backend LLM différent via le champ `model_hint`. Le Reasoner peut ainsi router les steps de raisonnement complexe vers un modèle puissant, et les steps de formatage simple vers un modèle rapide :

```json
{
  "steps": [
    {"step_id": "s1", "description": "Lire le fichier", "tool_hint": "file_io", "depends_on": []},
    {"step_id": "s2", "description": "Analyser les clauses", "tool_hint": null,
     "model_hint": "claude-opus", "depends_on": ["s1"]},
    {"step_id": "s3", "description": "Formater le rapport", "tool_hint": null,
     "model_hint": "claude-haiku", "depends_on": ["s2"]}
  ]
}
```

Si le backend demandé est introuvable, ORIA bascule silencieusement sur le backend par défaut avec un `WARN` dans les logs.

---

## Actor — exécuter et observer

L'Actor exécute le plan en ordre topologique. En Mode Orchestré, `agent.run()` n'est **jamais** appelé pendant les steps — ORIA invoque les outils et le LLM directement.

### StepContext — chaque step voit les précédents

À chaque step, l'Actor injecte les outputs des steps précédents dans le contexte LLM :

```rust
pub struct StepContext {
    pub previous_outputs: HashMap<String, String>,  // step_id → output
    pub step_index: usize,
    pub total_steps: usize,
    pub remaining_budget: StepBudgetView,
}
```

Le step `s3` voit les outputs de `s1` et `s2` dans son contexte. C'est ce qui permet au Reasoner de produire un plan où les étapes tardives dépendent des résultats des étapes précoces — sans que vous ayez à câbler manuellement ces dépendances.

### Erreurs retryables vs permanentes

L'Actor distingue deux classes d'erreurs par step :

| Classe | Exemples | Comportement |
|---|---|---|
| Retryable | `ToolCallFailed`, `LlmCallFailed` | Déclenche une replanification (max 2) |
| Permanente | `ToolNotFound`, `NoLlmBackend`, `RejectedByUser` | Échec immédiat de la tâche |

Un timeout réseau sur un appel outil est retryable. Un outil qui n'existe pas ne l'est pas.

### Enregistrement mémoire per-step

Après chaque step complété en mode Orchestré, l'Actor enregistre automatiquement un épisode en mémoire épisodique de l'agent (fire-and-forget, importance fixée à `0.6`, contenu tronqué à 200 caractères). C'est l'une des deux exceptions au Principe #6 ("mémoire à initiative de l'agent") — en mode Orchestré, l'agent Python n'est pas appelé pendant les steps, donc ORIA prend en charge cet enregistrement.

---

## Visualisation en temps réel

En Mode Orchestré, `apollia-os run` affiche le plan et la progression step par step :

```bash
$ apollia-os run analyse-contrat "Analyse le contrat Dupont SA"

  Plan généré (3 étapes) :
  ├── [s1] Lire le fichier contrat Dupont SA  → file_io
  ├── [s2] Extraire les clauses clés          → llm  (attend s1)
  └── [s3] Rédiger la synthèse exécutive      → llm  (attend s2)

  ● [1/3] Lire le fichier contrat Dupont SA...
  ✔ [1/3] (complété)  0.1s
  ● [2/3] Extraire les clauses clés...
  ✔ [2/3] (complété)  2.3s
  ● [3/3] Rédiger la synthèse exécutive...
  ✔ [3/3] (complété)  1.9s

  ✔ Tâche complétée en 4.4s
```

Après l'exécution, inspecter le plan persisté :

```bash
apollia-os task inspect t-abc123
```
