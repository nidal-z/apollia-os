# Pipelines multi-agents

L'A2A (chapitre 11) permet à un agent d'en appeler un autre de façon ponctuelle. Mais certains workflows sont plus complexes : extraire un PDF, valider le résultat, comptabiliser si la validation passe, alerter un humain si elle échoue. Quatre agents, des dépendances entre eux, des règles de fallback.

Câbler tout ça manuellement dans le code d'un Director Agent, c'est fragile. Si le runtime redémarre en cours de route, le workflow s'arrête sans reprise. Si la validation échoue, l'erreur est silencieuse.

Les **pipelines** d'Apollia OS permettent de décrire ce type de workflow comme un **graphe déclaratif** (DAG), persisté en SQLite. Chaque step est une tâche soumise à un agent. Le runtime orchestre l'exécution couche par couche, gère les échecs, les conditions, les fallbacks, et reprend après un redémarrage.

---

## Un pipeline en une image

```
trigger : nouveau fichier ~/factures/entrant/acme.pdf
              │
              ▼
          [ocr]          Layer 0
         ocr-agent
              │
     ┌────────┴────────┐
     ▼                 ▼
[validation]     [extraction]    Layer 1  (fan-out)
validation-agent  extract-agent
     │                 │
     └────────┬────────┘
              ▼
       [comptabilisation]        Layer 2  (fan-in)
       compta-agent
```

Les steps du même layer s'exécutent en parallèle. Un step n'attend que ses dépendances directes — pas la fin de tous les steps précédents.

---

## Définir un pipeline

Les pipelines se créent via l'API REST et sont persistés dans SQLite (`pipelines_def.db`).

```bash
curl -X POST http://localhost:7771/api/v1/pipelines \
  -H "Content-Type: application/json" \
  -d '{
    "id": "traitement-facture",
    "description": "OCR → validation → comptabilisation",
    "on_failure": "fail",
    "steps": [
      {
        "id": "ocr",
        "agent": "ocr-agent",
        "input": "{{trigger.payload}}"
      },
      {
        "id": "validation",
        "agent": "validation-agent",
        "input": "{{steps.ocr.output}}",
        "depends_on": ["ocr"]
      },
      {
        "id": "comptabilisation",
        "agent": "compta-agent",
        "input": "{{steps.validation.output}}",
        "depends_on": ["validation"]
      }
    ]
  }'
```

Le pipeline est validé avant écriture (DAG acyclique, IDs uniques, références valides). S'il est invalide, l'API retourne une erreur avec la cause exacte.

---

## Déclencher un run

```bash
# Manuel — avec suivi de progression
apollia-os pipeline run traitement-facture --payload "acme-2026-03.pdf"
# [10:01:32]  ⟿ [ocr] running
# [10:01:45]  ✔ [ocr] completed
# [10:01:45]  ⟿ [validation] running
# [10:01:47]  ✔ [validation] completed
# [10:01:47]  ⟿ [comptabilisation] running
# [10:01:52]  ✔ [comptabilisation] completed

# Fire-and-forget
apollia-os pipeline run traitement-facture --payload "acme-2026-03.pdf" --detach
# ● traitement-facture › démarré (run r-3f7a2b9c)
```

Les pipelines peuvent aussi être déclenchés automatiquement par un Trigger (chapitre 14).

---

## Statuts d'un run

| Statut | Signification |
|---|---|
| `Running` | Au moins un step en cours |
| `WaitingApproval` | Un step est suspendu en attente HITL |
| `Completed` | Tous les steps ont réussi (ou ont été skippés) |
| `Failed` | Un step a échoué avec `on_failure = fail` |

Chaque step a son propre cycle : `Pending → Running → Completed / Failed / Skipped / FallbackActive`.

---

## Du point de vue de l'agent

Pour un agent Python, **être appelé dans un pipeline est identique à une invocation directe**. `run(task, ctx)` reçoit un `AIPTask` standard. Le runtime injecte l'output du step précédent dans `task["input"]["parts"][0]["text"]` via les templates. Aucune modification de code n'est nécessaire.

```python
class ValidationAgent:
    def manifest(self):
        return {
            "name": "validation-agent",
            "version": "1.0.0",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        texte_ocr = next(
            (p["text"] for p in parts if p.get("type") == "text"),
            ""
        )

        if not texte_ocr.strip():
            return AIPResult.failed("TEXTE_VIDE", "OCR n'a produit aucun texte")

        verdict = "ALERTE_FRAUDE" if "FRAUDE" in texte_ocr.upper() else "VALIDE"
        return AIPResult.completed(verdict)
```

Le retour de chaque step est capturé par le pipeline engine et disponible via `{{steps.validation.output}}` pour les steps suivants.

---

## Ce que vous allez apprendre

- **Section 1 — Topologie et dépendances** : `depends_on`, layers topologiques, fan-out et fan-in, variables de template, HITL dans un pipeline, persistance SQLite et reprise après restart
- **Section 2 — Conditions et fallbacks** : les 5 opérateurs de condition, le pattern `on_failure = fallback`, la politique globale `on_failure`, l'intégration avec les Triggers
