# Topologie et dépendances

> **Note :** contenu archivé — le moteur de pipelines a été retiré du runtime. Voir ch13-00 pour le contexte.

---

La topologie d'un pipeline est son graphe de dépendances : quels steps attendent quels autres, dans quel ordre ils s'exécutent, comment les données circulent de l'un à l'autre.

---

## depends_on — déclarer les dépendances

Chaque step peut déclarer les steps qui doivent être terminés avant lui :

```json
{
  "steps": [
    { "id": "ocr",      "agent": "ocr-agent",      "input": "{{trigger.payload}}" },
    { "id": "extract",  "agent": "extract-agent",   "input": "{{trigger.payload}}" },
    { "id": "validate", "agent": "valid-agent",
      "input": "{{steps.ocr.output}} --- {{steps.extract.output}}",
      "depends_on": ["ocr", "extract"] },
    { "id": "store",    "agent": "store-agent",
      "input": "{{steps.validate.output}}",
      "depends_on": ["validate"] }
  ]
}
```

Un step sans `depends_on` fait partie du premier layer — il démarre immédiatement.

---

## Layers topologiques — Kahn BFS

Le `PipelineExecutor` calcule les layers via un tri topologique (Kahn BFS). Tous les steps d'un même layer s'exécutent en parallèle (`FuturesUnordered`). Le layer suivant démarre quand tous les steps du layer courant sont terminés (complétés, skippés, ou fallback activé).

```
Layer 0 : [ocr, extract]     → soumis simultanément
Layer 1 : [validate]         → attend Layer 0
Layer 2 : [store]            → attend Layer 1
```

Si un step du layer 1 produit une erreur et que `on_failure = fail`, le pipeline s'arrête et le layer 2 n'est jamais soumis.

---

## Variables de template

L'`input` de chaque step est un template résolu au moment de l'exécution du step :

| Variable | Source |
|---|---|
| `{{trigger.payload}}` | Payload du déclenchement (chemin fichier, body webhook, etc.) |
| `{{steps.<id>.output}}` | Output textuel du step `<id>` après sa completion |
| `{{pipeline.id}}` | Identifiant du pipeline |
| `{{pipeline.run_id}}` | Identifiant unique du run (`r-3f7a2b9c`) |

Les variables non résolues (step non encore terminé, step skippé) sont remplacées par une chaîne vide — sans erreur, sans panic. Votre agent doit gérer l'input vide :

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]
    texte = next((p["text"] for p in parts if p.get("type") == "text"), "")

    if not texte:
        # Step précédent skippé ou sans output
        return AIPResult.completed("rien à traiter")

    # Traitement normal...
```

**Cas fallback :** si le step `validation` a un fallback activé (`validation-manuelle`), `{{steps.validation.output}}` retourne l'output du fallback — transparence totale pour les steps aval.

---

## Fan-out et fan-in

**Fan-out** : un step amont alimente plusieurs steps en parallèle.

```json
[
  { "id": "parse",    "agent": "parse-agent", "input": "{{trigger.payload}}" },
  { "id": "index",    "agent": "index-agent", "input": "{{steps.parse.output}}", "depends_on": ["parse"] },
  { "id": "notify",   "agent": "notif-agent", "input": "{{steps.parse.output}}", "depends_on": ["parse"] },
  { "id": "archive",  "agent": "arch-agent",  "input": "{{steps.parse.output}}", "depends_on": ["parse"] }
]
```

`index`, `notify` et `archive` s'exécutent simultanément dès que `parse` est terminé.

**Fan-in** : plusieurs steps amont alimentent un step aval unique.

```json
{ "id": "consolidate", "agent": "merge-agent",
  "input": "A: {{steps.index.output}} B: {{steps.notify.output}}",
  "depends_on": ["index", "notify"] }
```

`consolidate` démarre quand `index` ET `notify` sont tous les deux terminés.

---

## HITL dans un pipeline

Un agent peut suspendre un step en retournant `AIPResult.input_required`. Le pipeline détecte ce résultat et se fige :

```
PipelineExecutor reçoit StepResult::InputRequired { task_id }
  → Run passe en WaitingApproval { step_id, task_id }
  → RuntimeEvent::PipelineSuspended émis
  → notification desktop/webhook
  → await TaskResumed...
```

Les steps du layer courant et des layers suivants ne sont pas soumis tant que la suspension dure.

```python
async def run(self, task, ctx):
    if task["is_resumed"]:
        ir = task["input_response"]
        if not ir.approved:
            return AIPResult.failed("REJETE", ir.reason or "Refusé par l'opérateur")
        saved = ir.context
        return await self._enregistrer(saved["montant"], ctx)

    parts = task["input"]["parts"]
    montant = next((p["text"] for p in parts if p.get("type") == "text"), "")
    return AIPResult.input_required(
        f"Confirmer le virement de {montant} €?",
        {"montant": montant}
    )
```

Approuver ou rejeter depuis la CLI :

```bash
# Voir les pipelines suspendus
apollia-os pipeline list

# Approuver
apollia-os task resume t-abc123 --approve

# Rejeter (le step est marqué Failed, on_failure s'applique)
apollia-os task resume t-abc123 --reject --reason "Montant incorrect"
```

---

## Persistance SQLite et reprise après restart

Chaque run et chaque step sont persistés dans `pipelines.db` en temps réel :

```sql
-- État d'un run
SELECT run_id, pipeline_id, status_json, started_at
FROM pipeline_runs
WHERE pipeline_id = 'traitement-facture';

-- État de chaque step du run
SELECT step_id, status, output, error, started_at, ended_at
FROM pipeline_step_runs
WHERE run_id = 'r-3f7a2b9c'
ORDER BY started_at;
```

Au démarrage du runtime, le `PipelineEngine` scanne les runs avec `status = "running"` et relance leurs executors automatiquement. Les steps déjà `completed` ou `failed` sont ignorés — seuls les steps `pending` ou `running` (interrompus en cours de soumission) sont re-soumis.

---

## Inspecter un run

```bash
# Historique des runs d'un pipeline
apollia-os pipeline runs traitement-facture
# RUN ID       STATUT        DÉMARRÉ          DURÉE
# r-3f7a2b9c   Completed     2026-04-01 10:01   1m23s
# r-2e6b1a8b   Failed        2026-04-01 09:47   0m08s

# Détail d'un run
apollia-os pipeline status r-3f7a2b9c
# Pipeline : traitement-facture
# Run      : r-3f7a2b9c
# Statut   : Completed
#
# STEP              STATUT      DURÉE
# ocr               Completed   13.2s
# validation        Completed    1.8s
# comptabilisation  Completed    5.1s
```

---

## Les 9 événements runtime pipeline

| Événement | Émis quand |
|---|---|
| `PipelineStarted` | Run démarré |
| `PipelineStepStarted` | Step soumis au TaskRouter |
| `PipelineStepCompleted` | Step terminé avec succès |
| `PipelineStepFailed` | Step en erreur |
| `PipelineStepSkipped` | Step sauté (condition false ou skip) |
| `PipelineSuspended` | Step en attente HITL |
| `PipelineResumed` | Reprise après approbation |
| `PipelineCompleted` | Run terminé avec succès |
| `PipelineFailed` | Run interrompu en erreur |

Ces événements alimentent les notifications (section `pipeline.suspended` → canal webhook/desktop) et le stream SSE de l'API.
