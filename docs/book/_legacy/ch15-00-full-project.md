# Solution de bout en bout

Les chapitres précédents vous ont fourni tous les outils : Worker Agents, mode orchestré, HITL, A2A, pipelines, triggers. Dans ce chapitre, vous assemblez tout ça en une **solution PME complète** — pas un exemple de démonstration, une solution que vous pouvez déployer telle quelle.

---

## Le projet : traitement automatisé des factures fournisseurs

Une PME reçoit chaque jour des factures PDF dans un dossier partagé. Aujourd'hui, une personne les ouvre une par une, vérifie les montants, les ressaisit dans un tableur, et envoie le résumé au comptable chaque semaine. C'est répétitif, source d'erreurs, et chronophage.

**Ce que vous allez construire :**

Quand un nouveau PDF apparaît dans `~/factures/entrant/`, Apollia OS :
1. Extrait automatiquement les données de la facture (numéro, date, fournisseur, montants)
2. Valide la structure et les montants (TVA cohérente, date valide, fournisseur connu)
3. Demande une confirmation humaine si le montant dépasse 5 000 €
4. Enregistre l'écriture comptable dans un CSV d'export
5. Met à jour le rapport de synthèse hebdomadaire

```
~/factures/entrant/acme-2026-04.pdf
         │
         ▼  (FileWatch trigger)
   [extraction]       → pdf-invoice-worker
         │
         ▼
   [validation]       → invoice-validator-worker
         │              (HITL si montant > 5 000 €)
         ▼
   [comptabilisation]  → compta-worker
         │
         ▼
   export/ecritures-comptables.csv  (mis à jour)
```

---

## Les composants que vous allez créer

| Composant | Fichier | Rôle |
|---|---|---|
| Worker Agent | `agents/pdf-invoice-worker.py` | Extraction PDF → JSON structuré |
| Worker Agent | `agents/invoice-validator-worker.py` | Validation métier, détection anomalies |
| Worker Agent | `agents/compta-worker.py` | Écriture comptable, export CSV |
| Director Agent | `agents/facture-director.py` | Orchestration A2A pour requêtes ad-hoc |
| Pipeline | API REST | Enchaîne les 3 Workers automatiquement |
| Trigger | API REST | FileWatch sur `~/factures/entrant/` |

---

## Ce que vous allez apprendre

- **Section 1 — Architecture cible** : les décisions de conception, pourquoi Workers plutôt que Director seul, le schéma de données entre composants
- **Section 2 — Les Worker Agents** : code complet des 3 Workers (PDF, validation, comptabilité), SYSTEM_PROMPT, guardrails, manifest A2A
- **Section 3 — Le Director Agent** : pattern `A2AToolsProvider`, gestion du contexte conversationnel, réponses structurées
- **Section 4 — Le pipeline et le trigger** : définition du DAG, HITL pour les gros montants, trigger FileWatch, mise en production
- **Section 5 — Résultat final** : exécution de bout en bout, test avec un vrai PDF, lecture du rapport généré

---

## Prérequis

```bash
# Runtime en cours d'exécution
apollia-os start --foreground &

# Outils disponibles
apollia-os tool list | grep -E "file_io|python_executor"
# ✔ file_io      (natif)
# ✔ python_executor (natif)

# Packages Python nécessaires — installés automatiquement au premier lancement
# pdfplumber, pandas

# Dossier de réception des factures
mkdir -p ~/factures/entrant ~/factures/traites export
```

---

## Structure finale du projet

```
agents/
├── pdf-invoice-worker.py
├── invoice-validator-worker.py
├── compta-worker.py
├── facture-director.py
└── tests/
    ├── conftest.py
    ├── test_pdf_invoice_worker.py
    ├── test_invoice_validator_worker.py
    └── test_compta_worker.py

export/
└── ecritures-comptables.csv   (généré par compta-worker)

~/factures/
├── entrant/    (surveille par le trigger)
└── traites/    (archives après traitement)
```
