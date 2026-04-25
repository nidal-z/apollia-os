# Mode Orchestré

La boucle ReAct du chapitre précédent est parfaite pour les tâches où le LLM décide au fur et à mesure : lire un fichier, décider quoi en faire, agir. Une boucle, un agent, un contexte continu.

Mais certaines tâches sont différentes. Analyser un contrat juridique en cinq phases distinctes. Générer un devis en lisant les données client, calculant les montants, puis produisant le document final. Ces tâches ont une structure — des étapes avec des dépendances, des résultats intermédiaires qui alimentent les étapes suivantes. Demander à un LLM d'improviser cette structure à chaque itération, c'est fragile. Sur un modèle léger, c'est souvent raté.

Le **Mode Orchestré** résout ce problème. Au lieu de laisser l'agent improviser sa stratégie étape par étape, ORIA planifie l'exécution complète avant de commencer — puis l'exécute.

---

## Direct vs Orchestré

| | Mode Direct | Mode Orchestré |
|---|---|---|
| Qui planifie | L'agent (boucle ReAct interne) | ORIA (LLM Reasoner) |
| Qui appelle les outils | L'agent via `ctx.tools` | ORIA via l'ActorLoop |
| `run()` est appelé | Oui — boucle complète | Non — `run()` n'est pas invoqué pendant les steps |
| Replanification | Manuelle dans `run()` | Automatique (max 2 replans) |
| Persistance des steps | Non | Oui (`~/.apollia/plans.db`) |
| Cas d'usage | Tâches atomiques, exploration | Workflows multi-étapes, séquences structurées |

80 % des tâches relèvent du Mode Direct — plus léger, plus prévisible. Le Mode Orchestré est disponible pour les 20 % de cas qui en ont besoin, sans imposer son overhead aux cas simples.

---

## Comment ORIA choisit le mode

Le champ `execution_mode` dans le manifest contrôle ce comportement :

```python
def manifest(self):
    return {
        "name": "analyse-contrat",
        "execution_mode": "orchestrated",  # Force le Mode Orchestré
        # "execution_mode": "direct"       # Force le Mode Direct
        # "execution_mode": "auto"         # Heuristique ORIA (défaut)
    }
```

En mode `"auto"`, ORIA calcule un score de complexité à partir de 7 facteurs (nombre de steps demandés, outils requis, longueur de l'input, mots-clés de planification...). Si le score dépasse 0.40, le mode Orchestré est sélectionné.

Pour un Worker Agent spécialisé avec un workflow connu, déclarer `"orchestrated"` explicitement est plus fiable que laisser ORIA deviner.

---

## Le contrat Python reste le même

En Mode Orchestré, le contrat AIP ne change pas : `manifest()` + `run()` async. La différence est que `run()` n'est **pas appelé** pendant l'exécution des steps — ORIA prend en charge tout le travail.

```python
from apollia_aip import AgentManifest, AIPTask, AIPResult, RuntimeContext

class AnalyseContratAgent:

    def manifest(self):
        return AgentManifest(
            name="analyse-contrat",
            version="1.0.0",
            description="Analyse un contrat et extrait les clauses clés",
            execution_mode="orchestrated",
            system_prompt=(
                "Tu es un expert juridique spécialisé dans l'analyse de contrats. "
                "Décompose l'analyse en étapes séquentielles : lecture, extraction, synthèse. "
                "Utilise file_io pour lire les fichiers."
            ),
            tools_required=["file_io"],
        )

    async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
        # En Mode Orchestré, cette méthode n'est pas appelée pendant les steps.
        # Elle reste requise par le contrat AIP.
        raise NotImplementedError("Mode orchestré — run() non utilisé")
```

Le champ `system_prompt` dans le manifest est la clé : c'est lui qui guide ORIA pour générer le plan d'exécution adapté au domaine de votre agent.

---

## Ce que vous allez apprendre

- **Section 1 — ORIA** : les trois composants Observer, Reasoner, Actor — comment ils collaborent, comment ORIA classe automatiquement la complexité d'une tâche
- **Section 2 — Les plans** : la structure d'un `ExecutionPlan`, l'ordre topologique, la replanification automatique, la persistance SQLite
- **Section 3 — Le hook** : `on_plan_complete` — comment post-traiter les résultats de tous les steps pour produire une réponse finale structurée
