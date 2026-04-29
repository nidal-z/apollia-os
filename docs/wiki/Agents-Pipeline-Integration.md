# Intégrer un agent dans un pipeline — Apollia OS

> **Note :** la crate `apollia-pipelines` a été retirée du runtime dans cette version. Cette page est conservée comme référence historique. Pour orchestrer plusieurs agents, utilisez le pattern Director Agent via A2A (voir [Agents-Mode-Orchestre](./Agents-Mode-Orchestre)).

> ~~Guide pratique pour écrire un agent Python qui fonctionne dans un pipeline multi-agent.~~

---

## Vue d'ensemble

Un pipeline Apollia OS enchaîne plusieurs agents via un DAG déclaratif TOML. Chaque step du
pipeline correspond à une tâche soumise à un agent déjà en cours d'exécution dans le runtime.

Du point de vue de l'agent, **l'invocation dans un pipeline est identique à une invocation
directe** : le runtime appelle `run(task, ctx)` avec un `AIPTask` standard. Ce qui change :

- La valeur de `task["input"]["parts"]` est construite à partir des templates de la step
  (`{{trigger.payload}}`, `{{steps.ocr.output}}`, etc.) plutôt que d'une requête CLI ou API.
- L'output de l'agent (le champ `output[0].text` de l'`AIPResult`) est capturé par le pipeline
  et rendu disponible aux steps suivants via `{{steps.<step_id>.output}}`.
- Aucune modification de l'agent n'est nécessaire : un agent qui fonctionne en invocation directe
  fonctionne dans un pipeline sans changement de code.

---

## Ce que reçoit un agent dans un pipeline

Le pipeline executor construit l'`AIPInput` du step en rendant le template `input` de la
`PipelineStepDef`. Ce template est passé tel quel comme texte dans la première `TextPart` de
l'entrée.

### Structure de `task` (Python dict)

L'agent reçoit `task` comme un dict Python correspondant à `AIPTask` sérialisé en JSON :

```python
{
    "task_id": "t-3f7a2b9c",       # Identifiant unique de la tâche (UUID)
    "context_id": "",               # Vide pour les steps pipeline (pas de session)
    "input": {
        "parts": [
            {
                "type": "text",
                "text": "<valeur rendue du template input du step>"
            }
        ]
    },
    "history": [],                  # Toujours vide pour un step pipeline
    "timeout_seconds": None,        # None = timeout du runtime
    "is_resumed": False,            # True si reprise après approbation HITL
    "input_response": None,         # Peuplé si is_resumed == True
}
```

### Contenu de `task["input"]["parts"][0]["text"]`

La valeur provient du rendu du template `input` défini dans `apollia.toml`. Les substitutions
disponibles sont :

| Placeholder | Source |
|---|---|
| `{{trigger.payload}}` | Payload du trigger (chemin de fichier, body webhook, horodatage...) |
| `{{steps.<id>.output}}` | Output textuel du step `<id>` complété avant ce step |
| `{{pipeline.id}}` | Identifiant du pipeline déclaré dans `apollia.toml` |
| `{{pipeline.run_id}}` | Identifiant unique du run en cours (ex: `r-0017`) |

Si un placeholder référence un step non encore complété ou un step skippé, il est remplacé par
une chaîne vide — sans erreur, sans panic.

### Accéder au texte d'entrée

```python
async def run(self, task, ctx):
    # Récupérer l'input textuel du step
    parts = task["input"]["parts"]
    text_input = parts[0]["text"] if parts else ""

    # Ou plus défensif :
    text_input = next(
        (p["text"] for p in parts if p.get("type") == "text"),
        ""
    )
```

---

## Ce que doit retourner l'agent

L'agent retourne un dict JSON-sérialisable correspondant à `AIPResult`. Le pipeline executor
capture le texte de `output[0]` pour construire `{{steps.<step_id>.output}}`.

### Format minimal — succès

```python
async def run(self, task, ctx):
    # Traitement...
    return {
        "status": "completed",
        "output": [{"type": "text", "text": "résultat de ce step"}],
    }
```

Le champ `output[0]["text"]` devient la valeur de `{{steps.<ce_step>.output}}` pour les steps
suivants.

### Format — succès via `AIPResult` (injecté automatiquement)

La classe `AIPResult` est injectée dans `run.__globals__` par le bridge Rust avant chaque appel :

```python
async def run(self, task, ctx):
    return AIPResult.completed("résultat de ce step")
```

### Format — échec

```python
async def run(self, task, ctx):
    return AIPResult.failed("TRAITEMENT_IMPOSSIBLE", "Fichier corrompu")
    # ou :
    return {
        "status": "failed",
        "output": [],
        "error": {"code": "TRAITEMENT_IMPOSSIBLE", "message": "Fichier corrompu"},
    }
```

Selon la politique `on_failure` du step dans `apollia.toml` (`fail`, `skip`, ou `fallback`),
le pipeline réagit différemment à ce résultat.

### Variants de statut reconnus par le pipeline executor

| `status` | Comportement pipeline |
|---|---|
| `"completed"` | Step complété ; `output[0].text` capturé pour templates suivants |
| `"failed"` | Step échoué ; politique `on_failure` appliquée |
| `"input_required"` | Step suspendu ; pipeline passe en `WaitingApproval` |

---

## Exemple complet : agent pipeline simple

Cet agent reçoit du texte, le transforme, et retourne le résultat pour le step suivant :

```python
class ValidationAgent:
    def manifest(self):
        return {
            "name": "validation-agent",
            "version": "1.0.0",
            "description": "Valide le texte extrait par l'étape OCR",
            "tools_required": [],
            "tools_optional": [],
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        texte_ocr = next(
            (p["text"] for p in parts if p.get("type") == "text"),
            ""
        )

        if not texte_ocr.strip():
            return AIPResult.failed("TEXTE_VIDE", "L'OCR n'a produit aucun texte")

        # Traitement de validation
        if "FRAUDE" in texte_ocr.upper():
            verdict = "ALERTE_FRAUDE"
        else:
            verdict = "VALIDE"

        return AIPResult.completed(verdict)
```

Pipeline correspondant dans `apollia.toml` :

```toml
[[pipelines]]
id = "traitement-facture"
description = "OCR → validation → comptabilité"

[[pipelines.steps]]
id = "ocr"
agent = "ocr-agent"
input = "{{trigger.payload}}"

[[pipelines.steps]]
id = "validation"
agent = "validation-agent"
input = "{{steps.ocr.output}}"
depends_on = ["ocr"]
on_failure = "fallback"

[[pipelines.steps]]
id = "validation-manuelle"
agent = "validation-agent"
input = "Validation manuelle requise pour : {{trigger.payload}}"
fallback_for = "validation"
```

Lancer un run de ce pipeline :

```bash
apollia-os pipeline run traitement-facture --payload "acme-2026-03.pdf"
```

---

## Passer des données entre steps

L'output d'un step est capturé depuis le premier `TextPart` du champ `output` de l'`AIPResult`.
Il est disponible via `{{steps.<step_id>.output}}` dans les templates des steps suivants.

```
Step ocr         → AIPResult.completed("texte extrait 2 pages")
                       ↓
{{steps.ocr.output}} == "texte extrait 2 pages"
                       ↓
Step validation  → task["input"]["parts"][0]["text"] == "texte extrait 2 pages"
```

**Cas particulier — step skippé :** si un step est skippé (condition `false` ou `on_failure = skip`),
son placeholder résout à une chaîne vide `""` dans les templates des steps suivants. Votre agent
doit gérer ce cas :

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]
    texte = next((p["text"] for p in parts if p.get("type") == "text"), "")

    if not texte:
        # Le step précédent a été skippé ou n'a produit aucun output
        return AIPResult.completed("rien à traiter")

    # Traitement normal...
```

**Cas particulier — step fallback :** si un step fallback est activé pour remplacer un step
principal, son output est également injecté sous le nom du step principal. Ainsi
`{{steps.validation.output}}` résout vers l'output du fallback `validation-manuelle` si ce
dernier a été activé.

---

## Cas HITL dans un pipeline

Un agent peut suspendre un step en retournant `AIPResult.input_required(prompt, context)`.
Le pipeline executor détecte ce résultat (`status == "input_required"`) et :

1. Persiste le statut `WaitingApproval` pour le run et le step dans SQLite.
2. Émet `RuntimeEvent::PipelineSuspended` sur l'EventBus.
3. Bloque l'exécution du pipeline (les steps suivants ne sont pas soumis).
4. Attend un événement `TaskResumed` sur l'EventBus (émis par le `ResumeHandler` quand
   l'opérateur approuve ou rejette via `apollia-os task resume`).

Après approbation, le runtime rappelle `run(task, ctx)` avec `task["is_resumed"] = True`
et `task["input_response"]` peuplé. L'agent doit lire ce champ pour connaître la décision :

```python
async def run(self, task, ctx):
    if task["is_resumed"]:
        ir = task["input_response"]
        if not ir.approved:
            return AIPResult.failed("REJETE", ir.reason or "Refusé par l'opérateur")
        # Reprendre le traitement après approbation...
        return AIPResult.completed("traitement approuvé et finalisé")

    # Premier appel : demander une approbation avant de continuer
    parts = task["input"]["parts"]
    montant = next((p["text"] for p in parts if p.get("type") == "text"), "")
    return AIPResult.input_required(
        f"Confirmer le virement de {montant} ?",
        {"montant": montant, "devise": "EUR"}
    )
```

Après rejet, le step est marqué `Failed` et la politique `on_failure` du step est appliquée
(identique à un échec normal). Si le step a `on_failure = skip`, le pipeline continue
sans ce step ; si `on_failure = fail`, le pipeline est interrompu.

Le timeout HITL est configuré via `[runtime] input_required_timeout_hours` dans `apollia.toml`
(défaut : 24 heures). Passé ce délai, la tâche est automatiquement annulée par le `TimeoutWatcher`.

---

## Points d'attention

### Différences comportementales vs invocation directe

| Aspect | Invocation directe | Invocation pipeline |
|---|---|---|
| `task["context_id"]` | Identifiant de session | Chaîne vide |
| `task["history"]` | Peut contenir des échanges | Toujours vide |
| `task["input"]["parts"]` | Fourni par le caller | Rendu depuis le template du step |
| Output capturé | Non (retourné au caller) | `output[0]["text"]` utilisé par les steps suivants |

### L'agent doit être démarré avant le pipeline

Le pipeline executor appelle `TaskSubmitter::submit_task(agent_name, input)` qui soumet la tâche
au `TaskRouter`. Si l'agent n'est pas démarré et en état `Active`, la soumission échoue et le
step est immédiatement marqué `Failed`.

Vérifier que l'agent est actif avant de lancer un run :

```bash
# Démarrer l'agent
apollia-os agent start agents/validation_agent.py

# Vérifier l'état
apollia-os agent info validation-agent

# Lancer le pipeline
apollia-os pipeline run traitement-facture --payload "fichier.pdf"
```

### Le budget de steps s'applique par tâche, pas par pipeline

Le `StepBudget` est appliqué par ORIA Engine pour chaque tâche individuelle. Un pipeline avec
5 steps crée 5 tâches indépendantes, chacune avec son propre budget. La limite globale du
pipeline est contrôlée par `on_failure` et le timeout du run (60 secondes par step par défaut).

### Les erreurs Python non gérées sont des échecs de step

Une exception non attrapée dans `run()` est convertie en `AIPBridgeError::PythonException` par
le bridge Rust, ce qui se traduit en `StepResult::Failed` pour le pipeline executor. La politique
`on_failure` du step est alors appliquée.
