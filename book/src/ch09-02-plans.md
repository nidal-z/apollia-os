# Plans et replanification

Un `ExecutionPlan` est le produit du Reasoner : une liste ordonnée de steps avec leurs dépendances, leurs outils suggérés, et leurs hints de modèle. C'est l'artefact central du mode Orchestré.

---

## Structure d'un ExecutionPlan

```rust
pub struct ExecutionPlan {
    pub plan_id: String,      // identifiant unique "p-xyz..."
    pub steps: Vec<PlanStep>,
}

pub struct PlanStep {
    pub step_id: String,              // "s1", "s2", ...
    pub description: String,          // "Lire le fichier contrat Dupont SA"
    pub tool_hint: Option<String>,    // "file_io", "python_executor", null (LLM pur)
    pub depends_on: Vec<String>,      // step_ids dont ce step dépend
    pub model_hint: Option<String>,   // backend LLM spécifique pour ce step
}
```

En JSON, un plan typique ressemble à ceci :

```json
{
  "plan_id": "p-7f3a9c",
  "steps": [
    {
      "step_id": "s1",
      "description": "Lire les informations client depuis clients/dupont.json",
      "tool_hint": "file_io",
      "depends_on": []
    },
    {
      "step_id": "s2",
      "description": "Calculer les montants HT et TTC",
      "tool_hint": "python_executor",
      "depends_on": ["s1"]
    },
    {
      "step_id": "s3",
      "description": "Générer le JSON du devis structuré",
      "tool_hint": null,
      "depends_on": ["s1", "s2"]
    },
    {
      "step_id": "s4",
      "description": "Sauvegarder le devis dans devis/devis-2026-04.json",
      "tool_hint": "file_io",
      "depends_on": ["s3"]
    }
  ]
}
```

`tool_hint` est un conseil, pas une contrainte — l'Actor peut utiliser un autre outil si celui suggéré n'est pas disponible. `depends_on: []` signifie que le step peut démarrer immédiatement.

---

## Ordre topologique — garantir les dépendances

L'ActorLoop trie les steps en ordre topologique avant l'exécution. Les steps sans dépendances peuvent théoriquement s'exécuter en parallèle ; les steps avec `depends_on` attendent que tous leurs prérequis soient complétés.

Pour le plan ci-dessus :

```
s1 → s2 → s3 → s4
       ↗
s1 ──
```

`s3` ne démarre qu'après la complétion de `s1` ET `s2`. `s4` ne démarre qu'après `s3`. L'ordre est garanti par le tri topologique — même si le Reasoner génère les steps dans le mauvais ordre dans son JSON.

---

## Guider le plan avec system_prompt

Le `system_prompt` de votre manifest est la principale façon de contrôler la qualité du plan généré. Des instructions précises produisent des plans cohérents et exécutables :

```python
def manifest(self):
    return AgentManifest(
        name="devis-generator",
        execution_mode="orchestrated",
        system_prompt=(
            "Tu es un assistant commercial pour PME. "
            "Pour générer un devis : "
            "1. Lire les infos client depuis clients/<nom>.json avec file_io, "
            "2. Calculer les montants HT et TTC avec python_executor, "
            "3. Générer le JSON du devis avec llm, "
            "4. Sauvegarder dans devis/devis-<date>.json avec file_io. "
            "Décompose en 3-5 steps maximum. "
            "Chaque step doit avoir un output indépendant et vérifiable. "
            "Utilise depends_on pour garantir l'ordre quand nécessaire."
        ),
        tools_required=["file_io", "python_executor"],
    )
```

Règles pratiques pour un `system_prompt` efficace en mode Orchestré :

- **Atomique** : chaque step doit produire un résultat indépendant et vérifiable
- **Borné** : indiquer un nombre de steps maximum (3–5 pour la plupart des tâches)
- **Outil explicite** : préciser quel outil utiliser par type d'opération
- **Dépendances claires** : si l'ordre compte, le mentionner dans le prompt

---

## Replanification automatique

Si un step retryable échoue, l'Actor déclenche une replanification plutôt qu'un abandon immédiat. Le Reasoner reçoit le plan original, le step ayant échoué, et l'erreur — et génère un plan alternatif.

**Maximum 2 replans.** Au-delà, la tâche échoue avec `MAX_REPLAN_EXCEEDED`.

```bash
  ✗ [2/3] Extraire les clauses clés  → Erreur : encodage UTF-16 non supporté
  ⟳ Replanification 1/2...

  Plan révisé (3 étapes) :
  ├── [s1] Lire le fichier contrat Dupont SA   → file_io       ✔ (déjà complété)
  ├── [s2b] Convertir l'encodage UTF-16         → python_executor
  └── [s3] Extraire les clauses et synthétiser  → llm  (attend s2b)

  ● [2/3] Convertir l'encodage UTF-16...
  ✔ [2/3] (complété)  0.4s
  ● [3/3] Extraire les clauses et synthétiser...
  ✔ [3/3] (complété)  2.1s
```

Les steps déjà complétés ne sont pas ré-exécutés. L'ActorLoop reprend à partir du step ayant échoué avec le plan révisé.

**Pourquoi 2 replans maximum :** La replanification sans limite est le vecteur principal de boucles infinies et de coûts LLM incontrôlés. Deux replans offrent une seconde chance pour les erreurs transitoires sans risque de dérive.

Le `system_prompt` peut guider le comportement de replanification :

```python
system_prompt=(
    "..."
    "Si un step échoue sur un problème d'encodage, essaie de convertir "
    "le fichier avec python_executor avant de le relire."
)
```

---

## Persistance SQLite

Chaque plan et chaque step sont persistés en temps réel dans `~/.apollia/plans.db`. La persistance est transactionnelle : un crash du runtime pendant l'exécution d'un plan laisse une trace lisible de l'état au moment du crash.

```bash
# Inspecter un plan après exécution (ne nécessite pas le runtime démarré)
apollia-os task inspect t-abc123

# Sortie :
# Plan p-7f3a9c — devis-generator — 4 steps
# ✔ s1  Lire les informations client    0.08s
# ✔ s2  Calculer les montants HT/TTC    0.32s
# ✔ s3  Générer le JSON du devis        1.87s
# ✔ s4  Sauvegarder le devis            0.11s
# Durée totale : 2.38s
```

Les colonnes d'observabilité (`input_rendered`, `output_text`, `tool_used`, `duration_ms`, `error_detail`) sont disponibles pour chaque step — utiles pour déboguer un plan qui a échoué.

---

## StepBudget en mode Orchestré

Le StepBudget s'applique **par step**, pas par tâche. Un plan de 5 steps avec le budget par défaut (10 steps) a 2 steps de marge. Adapter le budget dans le manifest pour les workflows longs :

```python
from apollia_aip import AgentManifest, StepBudgetConfig

AgentManifest(
    name="analyse-contrat",
    execution_mode="orchestrated",
    step_budget=StepBudgetConfig(
        max_steps=20,
        max_tool_calls=40,
        wall_clock_timeout=600,  # 10 minutes pour un workflow long
    ),
    ...
)
```

Si le budget est épuisé pendant l'exécution d'un step, ce step échoue avec `STEP_BUDGET_EXCEEDED`. Si l'erreur est retryable et qu'un replan est disponible, la replanification est déclenchée. Sinon, la tâche échoue.
