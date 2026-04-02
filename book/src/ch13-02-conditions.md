# Conditions et fallbacks

Les dépendances linéaires (`depends_on`) suffisent pour les workflows séquentiels. Mais dès qu'un step doit s'adapter au résultat d'un précédent — ne s'exécuter que si la validation a produit "VALIDE", passer à un agent de revue manuelle si l'automatique échoue — il faut des mécanismes supplémentaires.

Apollia OS propose deux : les **conditions** (skip si la condition est fausse) et les **fallbacks** (remplacer un step qui a échoué par un agent de secours).

---

## Conditions de step

Un step conditionnel est skippé si la condition est évaluée à `false` au moment de son exécution.

```json
{
  "id": "enrichissement",
  "agent": "crm-agent",
  "input": "{{steps.validation.output}}",
  "depends_on": ["validation"],
  "condition": {
    "when": "contains",
    "field": "steps.validation.output",
    "value": "VALIDE"
  }
}
```

Si `steps.validation.output` ne contient pas `"VALIDE"`, le step `enrichissement` est skippé (`StepRunStatus::Skipped`). Les steps qui dépendent de lui voient `{{steps.enrichissement.output}}` résoudre à `""`.

### Les 5 opérateurs

| Opérateur | Comportement |
|---|---|
| `contains` | Le champ contient la valeur en sous-chaîne |
| `equals` | Égalité stricte (sensible à la casse) |
| `starts_with` | Le champ commence par la valeur |
| `ends_with` | Le champ se termine par la valeur |
| `regex` | La valeur est une expression régulière interprétée sur le champ |

### Exemples d'usage

```json
// Seulement si l'OCR a produit un montant
{ "when": "regex", "field": "steps.ocr.output", "value": "\\d+[,.]\\d{2}\\s*€" }

// Seulement si la réponse du LLM commence par "APPROUVÉ"
{ "when": "starts_with", "field": "steps.analyse.output", "value": "APPROUVÉ" }

// Seulement si le statut est exactement "OK"
{ "when": "equals", "field": "steps.check.output", "value": "OK" }
```

Le `field` référence les mêmes variables que les templates d'input (`steps.<id>.output`, `trigger.payload`, etc.).

---

## Fallbacks de step

Le pattern fallback remplace un step qui a échoué par un agent de secours, sans interrompre le pipeline.

Pour l'activer, deux conditions :
1. Le step principal doit déclarer `"on_failure": "fallback"`
2. Un step avec `"fallback_for": "<id_du_step_principal>"` doit exister dans la définition

```json
{
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
      "depends_on": ["ocr"],
      "on_failure": "fallback"
    },
    {
      "id": "validation-manuelle",
      "agent": "manual-review-agent",
      "input": "Revue manuelle requise : {{steps.ocr.output}}",
      "depends_on": ["ocr"],
      "fallback_for": "validation"
    },
    {
      "id": "comptabilisation",
      "agent": "compta-agent",
      "input": "{{steps.validation.output}}",
      "depends_on": ["validation"]
    }
  ]
}
```

**Comportement :**
- Cas normal : `validation` réussit → `validation-manuelle` est ignoré → `comptabilisation` utilise `{{steps.validation.output}}`
- Cas échec : `validation` échoue → `validation-manuelle` est activé → `comptabilisation` utilise l'output du fallback (accessible via `{{steps.validation.output}}` — transparence pour les steps aval)

Le step fallback est **inactif par défaut** — il n'est jamais soumis tant que son référent réussit.

---

## Politique globale on_failure

La politique `on_failure` du pipeline définit le comportement par défaut quand un step échoue et n'a pas de politique step spécifique :

| Politique | Comportement |
|---|---|
| `"fail"` (défaut) | Arrêt immédiat du pipeline, run marqué `Failed` |
| `"continue"` | Les steps sans dépendance vers le step échoué continuent |

La politique step (`on_failure` au niveau d'un step) prend priorité sur la politique globale :

| `on_failure` step | Comportement |
|---|---|
| `"fail"` | Arrêt immédiat (comme la politique globale `fail`) |
| `"skip"` | Step marqué `Skipped`, les steps suivants continuent avec `""` en output |
| `"fallback"` | Activation du step fallback déclaré |

```json
// Politique globale : arrêt si un step critique échoue
// Politique locale : skip si ce step optionnel échoue
{
  "on_failure": "fail",
  "steps": [
    { "id": "ocr",      "agent": "ocr-agent",    "input": "{{trigger.payload}}" },
    { "id": "metadata", "agent": "meta-agent",   "input": "{{trigger.payload}}",
      "on_failure": "skip" },
    { "id": "store",    "agent": "store-agent",
      "input": "{{steps.ocr.output}} {{steps.metadata.output}}",
      "depends_on": ["ocr", "metadata"] }
  ]
}
```

Si `metadata` échoue, il est skippé et `store` s'exécute avec `{{steps.metadata.output}} == ""`. Si `ocr` échoue, le pipeline entier s'arrête (politique globale `fail`).

---

## Déclencher un pipeline depuis un Trigger

Un Trigger peut pointer vers un pipeline au lieu d'un agent. Le champ `pipeline` est exclusif avec `agent`.

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "import-factures",
    "pipeline": "traitement-facture",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "file_watch",
      "path": "~/factures/entrant/",
      "events": ["create"]
    },
    "input_template": "{{filepath}}"
  }'
```

Quand un nouveau fichier apparaît dans `~/factures/entrant/`, un run de `traitement-facture` est démarré avec le chemin du fichier comme payload. `on_busy: "queue"` garantit qu'un deuxième fichier déposé pendant un run en cours sera mis en file d'attente plutôt que perdu.

---

## Gérer les pipelines via l'API REST

| Méthode | Endpoint | Action |
|---|---|---|
| `POST` | `/api/v1/pipelines` | Créer un pipeline |
| `PUT` | `/api/v1/pipelines/{id}` | Modifier (re-valide le DAG) |
| `DELETE` | `/api/v1/pipelines/{id}` | Supprimer |
| `GET` | `/api/v1/pipelines/{id}` | Lire la définition |
| `GET` | `/api/v1/pipelines` | Lister tous les pipelines |
| `POST` | `/api/v1/pipelines/{id}/run` | Démarrer un run |
| `GET` | `/api/v1/pipelines/{id}/runs` | Historique des runs |
| `GET` | `/api/v1/pipelines/{id}/runs/{run_id}` | État d'un run |

La modification d'un pipeline re-valide le DAG complet avant écriture SQLite. Si la nouvelle définition est invalide (cycle, référence manquante), l'API retourne une erreur et l'ancienne définition reste intacte.

---

## Points d'attention

**L'agent doit être démarré avant le pipeline.** Le pipeline executor soumet la tâche via `TaskRouter` — si l'agent n'est pas en état `Active`, le step est immédiatement marqué `Failed`.

```bash
# Démarrer tous les agents du pipeline
apollia-os agent start agents/ocr_agent.py
apollia-os agent start agents/validation_agent.py
apollia-os agent start agents/compta_agent.py

# Vérifier
apollia-os agent list

# Lancer le pipeline
apollia-os pipeline run traitement-facture --payload "acme.pdf"
```

**Le StepBudget s'applique par tâche, pas par pipeline.** Chaque step crée une tâche indépendante avec son propre budget ORIA. Un pipeline avec 5 steps crée 5 tâches — chacune peut consommer jusqu'à son budget configuré.

**Les exceptions Python non gérées sont des échecs de step.** Une exception dans `run()` est convertie en `AIPBridgeError::PythonException` → `StepResult::Failed`. La politique `on_failure` du step s'applique comme pour n'importe quel échec.
