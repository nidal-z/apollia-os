# Le Director Agent

Les trois Workers traitent les factures automatiquement via le pipeline. Mais que faire quand un opérateur veut poser une question ad-hoc : "Quelle est la facture la plus élevée de ce mois ?" ou "Retraite manuellement la facture Dupont qui a échoué" ?

Le **Director Agent** répond à ce besoin. Il utilise l'`A2AToolsProvider` pour avoir accès aux Workers comme outils, et consulte la mémoire pour retrouver le contexte des traitements passés.

---

## facture-director.py

```python
"""facture-director — Director Agent pour la gestion ad-hoc des factures fournisseurs.

Répond aux questions sur les factures, peut déclencher le traitement d'une facture
spécifique, et consulte l'historique des écritures comptables.

Délègue aux Workers via A2A :
  - pdf-invoice-worker  → extract-invoice
  - invoice-validator-worker → validate-invoice
  - compta-worker       → record-invoice
"""
from __future__ import annotations
from typing import Any

SYSTEM_PROMPT: str = """Tu es l'assistant de gestion des factures fournisseurs de la PME.

## RÈGLES ABSOLUES

1. TOUJOURS déléguer l'extraction PDF au Worker a2a:extract-invoice.
   RAISON : L'extraction directe sans Worker spécialisé produit des résultats imprévisibles.

2. TOUJOURS déléguer la validation au Worker a2a:validate-invoice après extraction.
   RAISON : Tu ne valides jamais les montants toi-même — les règles TVA sont dans le Worker.

3. TOUJOURS déléguer l'enregistrement au Worker a2a:record-invoice après validation.
   RAISON : Le Worker gère la déduplication et le format CSV — ne jamais écrire directement.

4. Si une facture a "alerte_montant": true dans le résultat de validation, informer l'opérateur
   AVANT de lancer l'enregistrement et attendre sa confirmation explicite.
   RAISON : L'enregistrement d'une grosse facture sans confirmation est un risque métier.

5. TOUJOURS répondre en français, de manière concise et structurée.
   RAISON : L'opérateur est non-technique et attend des réponses claires.

## DÉLÉGATION EN CHAÎNE

Pour traiter une facture de A à Z :

```python
# Étape 1 : extraction
extraction = await ctx.delegate(
    "extract-invoice",
    {"input": {"text": f"Extrais {file_path}"}}
)

# Étape 2 : validation
validation = await ctx.delegate(
    "validate-invoice",
    {"input": {"text": extraction.output}}
)

# Étape 3 : enregistrement (si validé)
import json
val_data = json.loads(validation.output)
if val_data["statut"] == "VALIDE" and not val_data["alerte_montant"]:
    enregistrement = await ctx.delegate(
        "record-invoice",
        {"input": {"text": validation.output}}
)
```

## RÉPONSES STRUCTURÉES

Pour une facture traitée avec succès, résumer ainsi :
✔ Facture FAC-2026-0142 (Acme SA) traitée
   Montant TTC : 5 040,00 €
   Écriture : ECR-20260401-0042
   Export : export/ecritures-comptables.csv

Pour une anomalie :
⚠ Facture rejetée — anomalie détectée :
   → Taux TVA non standard : 18.0% (attendu : 0%, 5.5%, 10% ou 20%)
   Action requise : vérifier la facture originale
"""


def manifest() -> dict[str, Any]:
    return {
        "name": "facture-director",
        "version": "1.0.0",
        "description": (
            "Director Agent pour la gestion des factures fournisseurs. "
            "Répond aux questions, déclenche le traitement ad-hoc d'une facture, "
            "et consulte l'historique des écritures. Délègue aux Workers via A2A."
        ),
        "execution_mode": "orchestrated",
        "tools_required": [],
        "tools_optional": ["file_io", "memory_search"],
        "packages": [],
        "memory_namespace": "factures",
        "supports_a2a": False,          # Director, pas invocable par d'autres agents
        "tags": ["director", "facture", "comptabilite"],
        "dangerous_tools_allowed": False,
        "max_concurrent_tasks": 3,
    }


class FactureDirector:
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 12
    TEMPERATURE = 0.1

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        message = next((p["text"] for p in parts if p.get("type") == "text"), "")

        if not message.strip():
            return AIPResult.failed(
                "message_vide",
                "Requête vide — précisez votre demande.",
            )

        # ORIA orchestre la boucle ReAct avec les outils A2A disponibles
        # Les Workers actifs sont automatiquement exposés comme a2a:extract-invoice, etc.
        try:
            return await self.react(task, ctx, message)
        except Exception as exc:
            return AIPResult.failed("erreur_interne", str(exc))


agent = FactureDirector()
```

---

## Démarrer le Director

```bash
apollia-os agent start agents/facture-director.py
# ✔ Agent facture-director démarré (mode orchestré)
# ✔ Outils A2A disponibles : a2a:extract-invoice, a2a:validate-invoice, a2a:record-invoice
```

---

## Exemples de requêtes ad-hoc

```bash
# Traiter une facture spécifique manuellement
apollia-os agent run facture-director \
  "Traite la facture /home/user/factures/entrant/dupont-2026-04.pdf"

# Résultat :
# ✔ Facture FAC-2026-0099 (Dupont SARL) traitée
#    Montant TTC : 1 428,00 €
#    Écriture : ECR-20260402-0012
#    Export : export/ecritures-comptables.csv

# Question sur l'historique (le Director consulte le CSV via file_io)
apollia-os agent run facture-director \
  "Combien de factures avons-nous enregistrées aujourd'hui ?"

# Traitement d'une facture à montant élevé (confirmation requise)
apollia-os agent run facture-director \
  "Traite /home/user/factures/entrant/grosse-commande.pdf"

# ⚠ Facture FAC-2026-0200 (Fournisseur Pro) — montant élevé détecté
#    Montant TTC : 12 600,00 €  (> 5 000 €)
#    Voulez-vous confirmer l'enregistrement ? (oui/non)
```

---

## Mode orchestré vs Mode direct

Le Director est déclaré en `execution_mode: "orchestrated"`. Il ne doit **pas** implémenter lui-même la boucle d'appel des Workers — ORIA le fait via la boucle ReAct. Le SYSTEM_PROMPT guide le LLM dans l'ordre d'appel des outils A2A.

Si vous avez besoin d'un contrôle plus précis sur l'ordre des délégations (par exemple, forcer la séquence extract → validate → record sans que le LLM puisse dévier), préférez le **mode direct** avec `ctx.delegate` explicite :

```python
# Mode direct — séquence forcée, non orchestrée
async def run(self, task, ctx):
    file_path = self._extract_path(task)

    extraction = await ctx.delegate("extract-invoice",
        {"input": {"text": f"Extrais {file_path}"}}, timeout_secs=60)
    if not extraction.is_success:
        return AIPResult.failed("extraction_echouee", extraction.error)

    validation = await ctx.delegate("validate-invoice",
        {"input": {"text": extraction.output}}, timeout_secs=30)
    if not validation.is_success:
        return AIPResult.failed("validation_echouee", validation.error)

    import json
    val = json.loads(validation.output)
    if val["alerte_montant"]:
        return AIPResult.input_required(
            f"Confirmer l'enregistrement de {val['facture']['numero']} "
            f"({val['facture']['montant_ttc']} €) ?",
            {"validation": validation.output},
        )

    return await ctx.delegate("record-invoice",
        {"input": {"text": validation.output}}, timeout_secs=30)
```

Le mode direct convient si le Director est lui-même un step dans un pipeline — le mode orchestré est plus adapté pour les interactions conversationnelles.
