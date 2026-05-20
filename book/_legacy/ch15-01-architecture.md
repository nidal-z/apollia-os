# Architecture cible

Avant d'écrire une ligne de code, définissons les décisions de conception. Chaque choix dans ce chapitre découle d'un principe Apollia OS.

---

## Pourquoi trois Workers plutôt qu'un seul agent ?

Un agent générique pourrait théoriquement tout faire : extraire, valider, enregistrer. Mais appliquer la règle des 2 conditions sur 3 (chapitre 8) à chaque sous-tâche impose trois Workers distincts.

| Tâche | Séquence non-triviale | Garde-fous critiques | Erreurs domaine | Verdict |
|---|---|---|---|---|
| Extraction PDF | ✓ (encoding, layout, regex) | ✓ (données manquantes → reject) | ✓ (PDFSyntaxError, page vide) | **Worker** |
| Validation métier | ✓ (règles TVA, plafonds, doublons) | ✓ (faux positif = écriture incorrecte) | ✓ (montants incohérents) | **Worker** |
| Écriture comptable | ✓ (format CSV, débit/crédit) | ✓ (JAMAIS écraser une entrée existante) | ✓ (doublons silencieux) | **Worker** |

Un Director Agent seul qui ferait les trois serait un agent générique avec des règles dans le prompt — exactement ce que le pattern Worker évite.

---

## Schéma de données entre composants

Chaque step du pipeline passe ses données au suivant via `{{steps.<id>.output}}`. Ce sont des chaînes JSON — le Worker suivant les parse à l'entrée de `run()`.

### Sortie de pdf-invoice-worker

```json
{
  "numero": "FAC-2026-0142",
  "date": "2026-04-01",
  "fournisseur": "Acme SA",
  "siret": "12345678901234",
  "montant_ht": 4200.00,
  "tva": 840.00,
  "montant_ttc": 5040.00,
  "devise": "EUR",
  "fichier_source": "/home/user/factures/entrant/acme-2026-04.pdf"
}
```

### Sortie de invoice-validator-worker

```json
{
  "statut": "VALIDE",
  "facture": { /* données PDF reprises */ },
  "anomalies": [],
  "alerte_montant": true
}
```

`"alerte_montant": true` signale que le montant dépasse 5 000 € et déclenche le HITL dans le pipeline.

### Sortie de compta-worker

```json
{
  "statut": "ENREGISTRE",
  "ecriture_id": "ECR-20260401-0042",
  "fichier_export": "export/ecritures-comptables.csv",
  "lignes_ajoutees": 2
}
```

---

## Architecture du pipeline

```
FileWatch trigger
~/factures/entrant/
        │  {{filepath}}
        ▼
┌───────────────────────────────────────────────────────────┐
│  Pipeline : traitement-factures                           │
│                                                           │
│  [extraction]                                             │
│  pdf-invoice-worker                                       │
│  input: "Extrais {{filepath}}"                            │
│       │                                                   │
│       │ {{steps.extraction.output}} (JSON)                │
│       ▼                                                   │
│  [validation]          on_failure: fallback               │
│  invoice-validator-worker                                 │
│  input: "{{steps.extraction.output}}"                     │
│       │                                                   │
│  ┌────▼─────────────────────────────────────┐             │
│  │ HITL : alerte_montant = true             │             │
│  │ "Confirmer la facture {{numero}} TTC     │             │
│  │  {{montant_ttc}} € ?"                    │             │
│  └────────────────────────────────────────┘│             │
│       │ {{steps.validation.output}} (JSON)  │             │
│       ▼                                                   │
│  [comptabilisation]                                       │
│  compta-worker                                            │
│  input: "{{steps.validation.output}}"                     │
│       │                                                   │
│       ▼                                                   │
│  export/ecritures-comptables.csv                          │
└───────────────────────────────────────────────────────────┘
```

---

## Rôle du Director Agent

Le Director Agent (`facture-director`) n'est pas dans le pipeline — il est pour les **requêtes ad-hoc** :

```bash
apollia-os agent run facture-director \
  "Combien de factures avons-nous reçues d'Acme ce mois-ci ?"
```

Il utilise l'`A2AToolsProvider` (chapitre 11) pour appeler les Workers selon le contexte, et consulte la mémoire pour retrouver les écritures précédentes.

---

## Vue d'ensemble des dépendances

```
pdf-invoice-worker      ← file_io, python_executor, pdfplumber
invoice-validator-worker ← python_executor
compta-worker           ← file_io, python_executor, pandas

facture-director        ← a2a:extract-invoice
                           a2a:validate-invoice
                           a2a:record-invoice
                           memory (namespace: factures)

Pipeline traitement-factures
  ├── step extraction    → pdf-invoice-worker
  ├── step validation    → invoice-validator-worker (HITL si > 5 000 €)
  └── step comptabilisation → compta-worker

Trigger import-factures → pipeline traitement-factures
  source: file_watch ~/factures/entrant/
  events: [create]
```
