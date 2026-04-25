# Tool Approval en Mode Orchestré

En Mode Orchestré, l'agent Python n'est pas appelé pendant l'exécution des steps — c'est ORIA qui pilote. Le mécanisme `AIPResult.input_required` ne s'applique donc pas directement. À la place, l'agent déclare dans son manifest quels outils nécessitent une approbation humaine avant d'être exécutés.

---

## Déclarer tools_requiring_approval

```python
from apollia_aip import AgentManifest

def manifest(self):
    return AgentManifest(
        name="envoi-devis",
        version="1.0.0",
        execution_mode="orchestrated",
        system_prompt=(
            "Tu es un assistant commercial. "
            "Pour envoyer un devis : lire les données client, calculer les montants, "
            "générer le PDF, puis envoyer par email."
        ),
        tools_required=["file_io", "python_executor", "smtp"],
        # smtp nécessite une confirmation humaine avant chaque envoi
        tools_requiring_approval=["smtp"],
    )
```

**Règles :**

- Champ optionnel — `[]` par défaut (aucune approbation requise)
- N'a d'effet qu'en `execution_mode: "orchestrated"` — en Mode Direct, le champ est ignoré
- L'outil doit aussi figurer dans `tools_required` ou `tools_optional` pour être résolu
- Plusieurs outils peuvent être listés : `["smtp", "http_client", "bash_executor"]`

---

## Ce que fait l'ActorLoop

Avant chaque step, l'ActorLoop vérifie si l'outil suggéré par ce step figure dans `tools_requiring_approval`. Si oui, il suspend avant d'exécuter l'outil :

```
ActorLoop.execute() :
  Pour chaque step dans l'ordre topologique :
    ├── Vérifier manifest.tools_requiring_approval
    ├── Si step.tool_hint ∈ tools_requiring_approval :
    │   ├── Clé d'enregistrement : "{task_id}::{step_id}"
    │   ├── PendingApprovals.register(key) → oneshot::Receiver
    │   ├── Émet RuntimeEvent::TaskInputRequired {
    │   │       task_id, prompt, step_id: Some("s3")   ← distingue du Mode Direct
    │   │   }
    │   ├── await rx  ← SUSPENSION PURE
    │   ├── Si approved=true  → execute_step() normalement
    │   └── Si approved=false → StepError::RejectedByUser { reason }
    │                           → plan arrêté, tâche failed
    └── Si tool_hint ∉ tools_requiring_approval → execute_step() directement
```

La suspension se produit **avant** l'exécution de l'outil — jamais après. Si l'opérateur rejette, le step ne s'exécute pas du tout et les steps suivants n'ont pas lieu.

---

## Différences entre Mode Direct et Mode Orchestré

| Aspect | Mode Direct | Mode Orchestré |
|---|---|---|
| Qui décide de suspendre | L'agent Python (`input_required`) | L'ActorLoop Rust (avant `execute_step`) |
| Quand | À n'importe quel moment dans `run()` | Avant l'exécution d'un outil spécifique |
| `step_id` dans l'événement | `None` | `Some("s3")` — le step exact qui attend |
| Reprise | Re-appel `agent.run` avec `is_resumed=True` | Exécution normale du step après `approved` |
| Rejet | `AIPResult::failed` sans re-appel Python | `StepError::RejectedByUser` → plan arrêté |
| Déclaration | Dans `run()` au runtime | Dans `manifest()` statiquement |

---

## Visualisation CLI avec approbation requise

En CLI, les steps nécessitant approbation sont affichés avec une indication visuelle :

```bash
$ apollia-os run envoi-devis "Envoyer le devis à Dupont SA"

  Plan généré (4 étapes) :
  ├── [s1] Lire les données client Dupont SA      → file_io
  ├── [s2] Calculer les montants HT/TTC            → python_executor
  ├── [s3] Générer le PDF du devis                 → python_executor
  └── [s4] Envoyer par email [approbation requise] → smtp

  ● [1/4] Lire les données client...
  ✔ [1/4] (complété)  0.1s
  ● [2/4] Calculer les montants...
  ✔ [2/4] (complété)  0.3s
  ● [3/4] Générer le PDF...
  ✔ [3/4] (complété)  1.2s

  ⏸ [4/4] Envoi par email — en attente d'approbation
  Prompt : "Envoyer le devis de 12 400 € à dupont@example.com ?"

  # L'opérateur répond via l'API ou l'interface desktop
```

---

## Reprendre depuis l'API

La reprise fonctionne de la même façon qu'en Mode Direct, via le même endpoint :

```bash
# Approuver — le step smtp s'exécute
curl -X POST http://localhost:7771/api/v1/tasks/t-abc123/resume \
  -H "Content-Type: application/json" \
  -d '{"approved": true}'

# Rejeter — le plan s'arrête, tâche failed
curl -X POST http://localhost:7771/api/v1/tasks/t-abc123/resume \
  -H "Content-Type: application/json" \
  -d '{"approved": false, "reason": "Adresse email incorrecte — corriger d abord"}'
```

---

## Comportement en l'absence de PendingApprovals

Si le runtime n'a pas de `PendingApprovals` configuré (cas d'un déploiement sans HITL), l'ActorLoop logue un `WARN` et exécute le step directement sans suspension :

```
WARN apollia_oria: tools_requiring_approval: smtp — PendingApprovals not configured, executing without approval
```

Ce comportement de dégradation gracieuse garantit qu'un manifest HITL fonctionne sur un runtime non-HITL sans erreur fatale. C'est le Principe #4 appliqué au HITL : l'outil manquant ne fait pas planter le runtime, il le dégrade.

---

## Outil dans tools_requiring_approval mais absent du plan

Si un plan généré par le Reasoner n'utilise pas d'outil listé dans `tools_requiring_approval`, aucune suspension n'a lieu. La déclaration est statique — elle s'applique uniquement si le Reasoner choisit cet outil pour un step.

C'est un comportement voulu : `tools_requiring_approval` déclare une politique de sécurité ("si smtp est utilisé, toujours demander"), sans forcer le Reasoner à l'utiliser.
