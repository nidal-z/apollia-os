---
sidebar_position: 6
title: Exécuter un agent orchestré
---

# Exécuter un agent orchestré

Un agent orchestré délègue toute la boucle au runtime. Vous fournissez un
system prompt et déclarez les outils qu'il peut utiliser ; ORIA, le moteur
d'orchestration d'Apollia, planifie les étapes, les exécute, observe les
résultats, et replanifie si nécessaire. Vous n'écrivez presque aucun flux de
contrôle. Ce guide construit un agent de briefing qui recherche un sujet et
en renvoie un résumé d'une page.

Utilisez ce patron quand le travail comporte plusieurs étapes et est piloté
par une intention exprimée en langage naturel. Quand vous voulez garder la
boucle entre vos propres mains, utilisez plutôt un director
([Écrire un director](/how-to/write-a-director)).

## Prérequis

- Un backend LLM configuré. Le mode orchestré échoue immédiatement sans
  cela. Vérifiez avec `apollia-os llm status`.

## Empiler `@agent` sur `@orchestrated`

`@orchestrated(system_prompt=...)` est le point d'entrée : il est mutuellement
exclusif avec `@skill` et `@on_message`. Déclarez les outils natifs que
l'agent peut utiliser avec `tools_required=(...)` ; ORIA échoue rapidement
au démarrage si un outil requis manque.

Créez `briefing_agent.py` :

```python
"""Briefing assistant. ORIA plans and executes the steps."""

from apollia import agent, orchestrated


SYSTEM_PROMPT = """\
You are a briefing assistant. Given a topic, build a one-page briefing that
covers:

1. Context: two to three sentences on why the topic matters now.
2. Key facts: five to seven bullet points, each with a source.
3. Open questions: three questions a decision-maker would ask.

Use the available tools to gather facts:
- `web_search` to find recent information.
- `web_read` to retrieve a full article when needed.

Stay grounded. If you cannot verify a fact, say so explicitly in the briefing.
"""


@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    pass
```

C'est un agent complet. Il ne déclare aucune méthode : ORIA exécute la
boucle à partir du system prompt et, par défaut, concatène le texte de
chaque étape du plan pour former la réponse finale. Les noms d'outils
`web_search` et `web_read` sont des outils natifs d'Apollia ; parcourez
l'ensemble complet dans la [référence des outils](/reference/native-tools).

## Installer et exécuter

```bash
apollia-os inspect briefing_agent.py
apollia-os agent install ./briefing_agent.py
apollia-os agent enable briefing
apollia-os run briefing "Give me a briefing on Microsoft's Permanent Beta culture."
```

ORIA observe le prompt et les outils disponibles, raisonne pour produire un
plan de trois à six étapes, l'exécute (en recherchant et en lisant selon les
besoins), et renvoie le briefing assemblé.

## Façonner la sortie avec `on_plan_complete`

Pour post-traiter vous-même les résultats des étapes, définissez un hook
`on_plan_complete`. Le runtime l'appelle avec les résultats des étapes et le
contexte, et attend une chaîne de caractères en retour :

```python
from apollia.types import Ctx


@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx: Ctx,
    ) -> str:
        sections = []
        for step_id, text in step_results.items():
            if "facts" in step_id and text:
                sections.append(f"- {text}")
        return "## Key facts\n\n" + "\n".join(sections)
```

`step_results` associe chaque identifiant d'étape (par exemple
`step_3_facts`) au texte produit par cette étape. Si vous omettez le hook,
ORIA concatène les textes des étapes dans l'ordre.

## Variante : un step budget personnalisé

Chaque exécution est bornée par un step budget que le runtime impose et
qu'aucun agent ne peut contourner. Redéfinissez les valeurs par défaut sur
`@agent` :

```python
@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
    step_budget={"max_steps": 25, "max_tool_calls": 40, "wall_clock_secs": 600},
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    pass
```

Un budget surdimensionné est ramené au plafond du runtime. Voir
[`ctx.budget`](/reference/sdk/budget) pour lire le budget restant depuis
l'intérieur d'une exécution.

## `@orchestrated` face à `apollia.react`

Les deux pilotent une boucle multi-étapes. Ce qui les distingue, c'est qui
garde le contrôle.

| Critère | `@orchestrated` | `apollia.react` |
|---|---|---|
| Qui pilote la boucle | Le runtime ORIA | Vous, dans `@on_message` |
| Idéal pour | Une intention autonome multi-étapes exprimée en langage naturel | Un workflow connu que vous voulez contrôler |
| Volume de code | Très court | Court |
| Pré et post-traitement | Limité au hook `on_plan_complete` | Python libre avant et après `react` |
| Branches conditionnelles | Difficiles à exprimer | Naturelles |
| Mode conversationnel | Pas de mode chat libre | Oui, via `@on_message` |

## Configurer le backend

Les exécutions orchestrées ont besoin d'un backend LLM. Définissez un
backend par défaut unique :

```bash
apollia-os llm backends set-default <name>
```

Pour plusieurs backends routés par tâche, configurez `[llm.routing]` dans
votre configuration Apollia. Voir la
[référence de configuration](/reference/configuration) et les
[commandes CLI `llm`](/reference/cli).

## Étapes suivantes

- Combiner orchestration et workers dans un assistant complet :
  [Construire un assistant multi-agent](/tutorials/build-a-multi-agent-assistant).
