# Résultat final

La solution est en place. Voici une exécution complète de bout en bout, depuis le dépôt d'un fichier jusqu'à l'écriture comptable.

---

## Démarrage de la solution

```bash
# 1. Démarrer le runtime
apollia-os start --foreground &

# 2. Démarrer les Workers et le Director
apollia-os agent start agents/pdf-invoice-worker.py
apollia-os agent start agents/invoice-validator-worker.py
apollia-os agent start agents/compta-worker.py
apollia-os agent start agents/facture-director.py

# 3. Vérifier que tout est actif
apollia-os agent list
# NAME                     STATUS   MODE         SINCE
# pdf-invoice-worker       Active   direct       10:00:05
# invoice-validator-worker Active   direct       10:00:06
# compta-worker            Active   direct       10:00:07
# facture-director         Active   orchestrated 10:00:08

# 4. Le trigger et le pipeline sont déjà créés (section précédente)
apollia-os trigger list
# import-factures  traitement-factures  file_watch  ✓  0 fires
```

---

## Scénario 1 — Facture standard (< 5 000 €)

```bash
# Déposer une facture dans le dossier surveillé
cp tests/fixtures/acme-2026-04.pdf ~/factures/entrant/

# Le trigger se déclenche en quelques secondes
apollia-os trigger logs import-factures --last 5
# 2026-04-02 10:15:03  fired  r-4a8c2d1f  12ms

# Suivre l'exécution du pipeline
apollia-os pipeline status r-4a8c2d1f
# Pipeline : traitement-factures
# Run      : r-4a8c2d1f
# Statut   : Completed  ✔
# Durée    : 18.4s
#
# STEP              STATUT      DURÉE
# extraction        Completed   13.2s
# validation        Completed    1.8s
# comptabilisation  Completed    3.4s

# Lire le résultat
cat export/ecritures-comptables.csv
```

```
ecriture_id,date,numero_facture,fournisseur,compte_debit,compte_credit,montant,libelle
ECR-20260402-0001,2026-04-01,FAC-2026-0142,Acme SA,60700,401000,4200.00,Facture Acme SA - HT
ECR-20260402-0001,2026-04-01,FAC-2026-0142,Acme SA,44566,401000,840.00,Facture Acme SA - TVA
```

---

## Scénario 2 — Facture à montant élevé (> 5 000 €, HITL)

```bash
cp tests/fixtures/grosse-commande-12600.pdf ~/factures/entrant/

# Le pipeline se suspend après la validation
apollia-os pipeline list
# traitement-factures  WaitingApproval  r-7b3f9e2a

apollia-os pipeline status r-7b3f9e2a
# STEP              STATUT           DURÉE
# extraction        Completed        15.1s
# validation        Completed         2.0s
# comptabilisation  waiting_approval  —

# Inspecter la demande
apollia-os task inspect t-d8e4f2a1
# Prompt  : "Confirmer l'enregistrement de FAC-2026-0200 TTC 12 600.00 € ?"
# Context : {"validation_output": "..."}

# Approuver
apollia-os task resume t-d8e4f2a1 --approve
# ✔ Pipeline r-7b3f9e2a repris

# Résultat
apollia-os pipeline status r-7b3f9e2a
# Statut : Completed  ✔
# Durée totale (hors attente) : 21.3s
```

---

## Scénario 3 — Facture avec anomalie TVA

```bash
cp tests/fixtures/facture-tva-incorrecte.pdf ~/factures/entrant/

apollia-os pipeline status r-2c9e7f1b
# STEP              STATUT    NOTE
# extraction        Completed
# validation        Failed    "Taux TVA non standard : 18.0%"
# validation-manuelle FallbackActive
# comptabilisation  Failed    "Facture avec statut 'ANOMALIE' — non comptabilisable"
#
# Statut pipeline : Failed

# Le pipeline a échoué proprement, sans écriture comptable incorrecte
# Une notification "pipeline.failed" a été envoyée
```

L'anomalie est détectée et remontée sans écriture incorrecte — c'est exactement le comportement attendu.

---

## Scénario 4 — Requête ad-hoc via le Director

```bash
# Question sur l'historique
apollia-os agent run facture-director \
  "Quelles factures avons-nous enregistrées aujourd'hui ?"
# Réponse :
# 2 factures enregistrées le 2 avril 2026 :
# • FAC-2026-0142 — Acme SA — 5 040,00 € TTC  (ECR-20260402-0001)
# • FAC-2026-0200 — Pro Fournisseur — 12 600,00 € TTC  (ECR-20260402-0002)
# Total du jour : 17 640,00 € TTC

# Retraiter une facture qui a échoué
apollia-os agent run facture-director \
  "Retraite la facture /home/user/factures/entrant/facture-tva-incorrecte.pdf — \
   j'ai vérifié, le bon taux est 20%"
# ⚠ Extraction réussie mais validation échoue encore (TVA dans le PDF = 18%).
# Conseil : corriger le PDF source ou contacter le fournisseur pour une facture rectificative.
```

---

## Tableau de bord de la solution

```bash
# Vue d'ensemble en temps réel
apollia-os agent list
apollia-os trigger logs import-factures --last 20
apollia-os pipeline runs traitement-factures

# Export comptable du mois
cat export/ecritures-comptables.csv | grep "2026-04"

# Statistiques du trigger
apollia-os trigger status import-factures
# Fires  : 12
# Skips  : 0
# Errors : 1  (PDF corrompu le 2026-04-01)
# Taux succès : 91.7%
```

---

## Ce que vous avez construit

En assemblant les concepts des chapitres précédents, vous avez créé une solution qui :

**Automatise** le traitement des factures dès qu'un fichier est déposé — zéro intervention humaine pour les cas courants.

**Protège** grâce à trois niveaux de garde-fous : SYSTEM_PROMPT des Workers (guidance LLM), guardrails Python (blocs hard), et pipeline `on_failure` (politique d'arrêt ou fallback).

**Contrôle** les cas à risque via HITL : les factures > 5 000 € ne sont jamais enregistrées sans approbation explicite.

**Survit** aux pannes : si le runtime redémarre pendant un traitement, le pipeline reprend depuis l'étape interrompue, les écritures déjà enregistrées ne sont pas dupliquées.

**S'étend** facilement : ajouter un Worker `email-worker` pour notifier le comptable après chaque écriture est une affaire d'un step de plus dans le pipeline.

---

## Pour aller plus loin

- Ajouter un rapport hebdomadaire automatique avec un trigger `cron` + un agent de synthèse
- Étendre le `pdf-invoice-worker` à d'autres formats (XML, EDI, CSV fournisseur)
- Ajouter un step `archivage` qui déplace le PDF dans `~/factures/traites/` après comptabilisation
- Exposer le `compta-worker` comme skill A2A pour d'autres pipelines de l'entreprise

```bash
# Exemple : archivage après comptabilisation (step à ajouter au pipeline)
{
  "id": "archivage",
  "agent": "archivage-worker",
  "input": "Archive {{trigger.payload}} vers ~/factures/traites/",
  "depends_on": ["comptabilisation"],
  "on_failure": "skip"
}
```

`on_failure: "skip"` sur l'archivage garantit qu'un échec de déplacement de fichier ne compromet pas l'écriture comptable déjà enregistrée.
