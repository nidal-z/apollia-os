# Le pipeline et le trigger

Le Director Agent traite les factures à la demande. Le pipeline automatise ce même traitement pour chaque fichier déposé dans le dossier surveillé — sans intervention humaine, sauf pour les gros montants.

---

## Créer le pipeline

```bash
curl -X POST http://localhost:7771/api/v1/pipelines \
  -H "Content-Type: application/json" \
  -d '{
    "id": "traitement-factures",
    "description": "Extraction PDF → validation → comptabilisation automatique des factures fournisseurs",
    "on_failure": "fail",
    "steps": [
      {
        "id": "extraction",
        "agent": "pdf-invoice-worker",
        "input": "Extrais la facture : {{trigger.payload}}"
      },
      {
        "id": "validation",
        "agent": "invoice-validator-worker",
        "input": "{{steps.extraction.output}}",
        "depends_on": ["extraction"],
        "on_failure": "fallback"
      },
      {
        "id": "validation-manuelle",
        "agent": "invoice-validator-worker",
        "input": "Validation manuelle requise. Données brutes : {{steps.extraction.output}}",
        "depends_on": ["extraction"],
        "fallback_for": "validation"
      },
      {
        "id": "comptabilisation",
        "agent": "compta-worker",
        "input": "{{steps.validation.output}}",
        "depends_on": ["validation"]
      }
    ]
  }'
```

### Le HITL pour les gros montants

La clé du pipeline est dans le `compta-worker` : s'il reçoit des données avec `"alerte_montant": true`, il retourne `AIPResult.input_required` avant d'enregistrer. Le pipeline se suspend automatiquement.

```
[validation] → {"statut": "VALIDE", "alerte_montant": true, ...}
     │
     ▼
[comptabilisation]  ← compta-worker reçoit ces données
     │
     └── AIPResult.input_required(
             "Confirmer FAC-2026-0200 (12 600 €) ?",
             context={"validation_output": "..."}
         )
     │
     ▼
Pipeline: WaitingApproval
     │
     └── Notification desktop/webhook envoyée
     │
     └── apollia-os task resume t-abc123 --approve
     │
     ▼
Pipeline reprend → écriture enregistrée
```

Pour que le `compta-worker` gère la reprise après HITL, voici le pattern à ajouter dans `run()` :

```python
async def run(self, task, ctx):
    import json as _json

    # Reprise après approbation HITL
    if task["is_resumed"]:
        ir = task["input_response"]
        if not ir.approved:
            return AIPResult.failed(
                "comptabilisation_refusee",
                f"Enregistrement refusé par l'opérateur : {ir.reason or 'sans motif'}",
            )
        # Récupérer les données sauvegardées au moment du suspend
        input_text = ir.context.get("validation_output", "")
        return await self._enregistrer(input_text, ctx)

    parts = task["input"]["parts"]
    input_text = next((p["text"] for p in parts if p.get("type") == "text"), "")

    # ... guardrails (voir ch15-02) ...

    # Si montant > 5 000 € → suspension HITL
    validation_data = _json.loads(input_text)
    if validation_data.get("alerte_montant"):
        facture = validation_data.get("facture", {})
        return AIPResult.input_required(
            f"Confirmer l'enregistrement de {facture.get('numero')} "
            f"TTC {facture.get('montant_ttc', '?')} € ?",
            {"validation_output": input_text},
        )

    return await self._enregistrer(input_text, ctx)
```

---

## Créer le trigger FileWatch

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "import-factures",
    "pipeline": "traitement-factures",
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

`on_busy: "queue"` garantit qu'une deuxième facture déposée pendant le traitement de la première sera mise en file d'attente — pas perdue.

---

## Vérifier la configuration complète

```bash
# Les trois Workers sont actifs
apollia-os agent list
# pdf-invoice-worker       [Active]
# invoice-validator-worker [Active]
# compta-worker            [Active]
# facture-director         [Active]

# Le pipeline est créé
curl http://localhost:7771/api/v1/pipelines/traitement-factures | jq '.id, .steps | length'
# "traitement-factures"
# 4

# Le trigger surveille le dossier
apollia-os trigger list
# ID               PIPELINE               TYPE       ENABLED
# import-factures  traitement-factures    file_watch  ✓
```

---

## Tester le pipeline manuellement

Avant d'activer le trigger, testez le pipeline avec un vrai PDF :

```bash
# Lancer manuellement avec suivi de progression
apollia-os pipeline run traitement-factures \
  --payload "/home/user/factures/entrant/acme-2026-04.pdf"

# [10:01:32]  ⟿ [extraction] running
# [10:01:45]  ✔ [extraction] completed
# [10:01:45]  ⟿ [validation] running
# [10:01:47]  ✔ [validation] completed
# [10:01:47]  ⟿ [comptabilisation] running
# [10:01:48]  ✔ [comptabilisation] completed
# ✔ Pipeline traitement-factures terminé en 16.4s

# Vérifier l'export
cat export/ecritures-comptables.csv
# ecriture_id,date,numero_facture,fournisseur,compte_debit,compte_credit,montant,libelle
# ECR-20260402-0001,2026-04-01,FAC-2026-0142,Acme SA,60700,401000,4200.00,Facture Acme SA - HT
# ECR-20260402-0001,2026-04-01,FAC-2026-0142,Acme SA,44566,401000,840.00,Facture Acme SA - TVA
```

---

## Activer la surveillance automatique

Une fois le test validé, le trigger est déjà actif (`"enabled": true`). Déposez un fichier pour le vérifier :

```bash
# Copier une facture dans le dossier surveillé
cp ~/bureau/fournisseur-dupont-avril.pdf ~/factures/entrant/

# Suivre l'exécution déclenchée automatiquement
apollia-os pipeline runs traitement-factures
# RUN ID       STATUT     DÉMARRÉ          DURÉE
# r-4a8c2d1f   Completed  2026-04-02 10:15  18.2s
```

---

## Gérer les approbations HITL

Quand une facture dépasse 5 000 €, le pipeline se suspend et une notification est envoyée :

```bash
# Voir les pipelines en attente
apollia-os pipeline list
# traitement-factures  WaitingApproval  r-7b3f9e2a

# Identifier la tâche en attente
apollia-os pipeline status r-7b3f9e2a
# [comptabilisation]  waiting_approval  t-d8e4f2a1

# Inspecter avant de décider
apollia-os task inspect t-d8e4f2a1
# Prompt : "Confirmer l'enregistrement de FAC-2026-0200 TTC 12 600.00 € ?"

# Approuver
apollia-os task resume t-d8e4f2a1 --approve
# ✔ Tâche reprise

# Ou rejeter avec motif
apollia-os task resume t-d8e4f2a1 --reject --reason "Facture à vérifier avec le fournisseur"
```

---

## Notifications desktop/webhook

Pour être alerté dès qu'un HITL est requis, configurez une notification :

```bash
curl -X POST http://localhost:7771/api/v1/notifications \
  -H "Content-Type: application/json" \
  -d '{
    "event": "pipeline.suspended",
    "channel": "webhook",
    "url": "https://hooks.slack.com/services/VOTRE/WEBHOOK/URL",
    "template": "⏸ Approbation requise — pipeline {{pipeline_id}} (run {{run_id}})"
  }'
```

Apollia OS envoie une notification Slack dès qu'un pipeline se suspend, avec un lien vers la tâche en attente.
