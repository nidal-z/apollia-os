# Le hook on_plan_complete()

Quand tous les steps d'un plan sont exécutés, ORIA a une liste de résultats intermédiaires — un output par step. Par défaut, il les concatène dans l'ordre et retourne le résultat final. C'est souvent suffisant.

Mais parfois, vous voulez faire plus : consolider les outputs en un rapport structuré, calculer un total à partir de plusieurs steps numériques, persister un résumé en mémoire. C'est le rôle du hook `on_plan_complete()`.

---

## Signature

```python
async def on_plan_complete(
    self,
    step_results: dict[str, str],   # { "s1": "output step 1", "s2": "output step 2", ... }
    ctx: RuntimeContext,
) -> str:
    """
    Post-traitement des résultats de tous les steps.
    Retourne la réponse finale (str) — devient l'output de la tâche.
    """
    ...
```

Le hook est **optionnel**. ORIA le détecte via duck typing (`hasattr(agent, "on_plan_complete")`). Le contrat minimal reste `manifest()` + `run()` — `on_plan_complete()` est un enrichissement, pas une obligation.

`step_results` est un dictionnaire `step_id → output` contenant les résultats de tous les steps complétés. Si un step a échoué (et que la replanification n'a pas réussi), son entrée est absente du dictionnaire.

---

## Comportement sans le hook

Si `on_plan_complete()` est absent, ORIA concatène les outputs dans l'ordre d'exécution :

```
output_s1

output_s2

output_s3
```

Ce comportement par défaut est suffisant pour les tâches où chaque step produit une section autonome du résultat final.

---

## Exemple — consolidation d'un rapport d'analyse

L'agent `analyse-contrat` exécute trois steps : lecture, extraction des clauses, synthèse. Le hook consolide ces trois outputs en un rapport structuré :

```python
class AnalyseContratAgent:

    def manifest(self):
        return AgentManifest(
            name="analyse-contrat",
            version="1.0.0",
            execution_mode="orchestrated",
            system_prompt=(
                "Tu es un expert juridique spécialisé dans l'analyse de contrats. "
                "Décompose en 3 steps : lecture du fichier, extraction des clauses clés, "
                "rédaction d'une synthèse exécutive. Utilise file_io pour la lecture."
            ),
            tools_required=["file_io"],
        )

    async def run(self, task, ctx):
        raise NotImplementedError("Mode orchestré — run() non utilisé")

    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx,
    ) -> str:
        # Récupérer les outputs par step_id
        clauses  = step_results.get("s2", "(extraction non disponible)")
        synthese = step_results.get("s3", "(synthèse non disponible)")

        # Construire la réponse finale structurée
        rapport = (
            "## Analyse du contrat\n\n"
            "### Clauses identifiées\n"
            f"{clauses}\n\n"
            "### Synthèse exécutive\n"
            f"{synthese}"
        )

        # Persister en mémoire pour le suivi
        await ctx.memory.record(
            f"Analyse contrat : {len(step_results)} sections traitées"
        )

        return rapport
```

---

## Exemple — devis avec calcul de montant total

L'agent `devis-generator` exécute quatre steps. Le hook extrait le montant calculé par le step numérique et l'intègre dans la réponse finale :

```python
class DevisGeneratorOrchestre:

    def manifest(self):
        return AgentManifest(
            name="devis-generator",
            execution_mode="orchestrated",
            system_prompt=(
                "Tu es un assistant commercial pour PME. "
                "Pour générer un devis : "
                "1. Lire les infos client depuis clients/<nom>.json (file_io), "
                "2. Calculer les montants HT et TTC (python_executor), "
                "3. Générer le JSON du devis (llm), "
                "4. Sauvegarder dans devis/devis-<date>.json (file_io)."
            ),
            tools_required=["file_io", "python_executor"],
            memory_namespace="commercial",
        )

    async def run(self, task, ctx):
        raise NotImplementedError("Mode orchestré — run() non utilisé")

    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx,
    ) -> str:
        devis_path = step_results.get("s4", "")
        montant    = step_results.get("s2", "")

        # Mémoriser pour le suivi commercial
        await ctx.memory.record(
            f"Devis généré : {devis_path}, montant : {montant}"
        )

        return (
            f"Devis généré avec succès : {devis_path}\n"
            f"Montant calculé : {montant}"
        )
```

---

## Accéder aux steps par ID vs par ordre

`step_results` est indexé par `step_id` — pas par ordre d'exécution. Pour un plan dont les `step_id` sont générés dynamiquement par le Reasoner (et que vous ne connaissez pas à l'avance), itérez sur les valeurs dans l'ordre :

```python
async def on_plan_complete(self, step_results, ctx) -> str:
    # Accès par step_id connu (si vous contrôlez le plan)
    synthese = step_results.get("s3", "")

    # Concaténation de tous les outputs dans l'ordre d'insertion
    # (Python 3.7+ : les dicts préservent l'ordre d'insertion)
    rapport_complet = "\n\n---\n\n".join(step_results.values())

    return rapport_complet
```

---

## Erreurs dans on_plan_complete()

Si `on_plan_complete()` lève une exception, ORIA la capture et retourne la concaténation par défaut des outputs — le hook est fail-safe. L'exception est loguée comme `WARN` :

```
WARN apollia_oria: on_plan_complete() raised ValueError: ... — using default output
```

Pour propager une erreur métier depuis le hook, retournez une chaîne d'erreur plutôt que de lever une exception :

```python
async def on_plan_complete(self, step_results, ctx) -> str:
    if "s3" not in step_results:
        # Step de synthèse manquant — retourner ce qui est disponible
        return "Synthèse partielle (step s3 non complété) :\n\n" + \
               "\n\n".join(step_results.values())
    ...
```

---

## Récapitulatif — quand utiliser on_plan_complete()

| Besoin | Solution |
|---|---|
| Outputs des steps sufisent tels quels | Ne pas implémenter le hook — comportement par défaut |
| Consolider les outputs en rapport structuré | `on_plan_complete()` avec formatage |
| Extraire une valeur d'un step spécifique | `step_results.get("sN", "")` |
| Persister un résumé en mémoire | `ctx.memory.record(...)` dans le hook |
| Calculer une valeur agrégée sur plusieurs steps | Traitement Python dans le hook |
| Retourner un format structuré (JSON) | `return json.dumps({...})` dans le hook |
