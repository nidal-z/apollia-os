---
sidebar_position: 5
title: Écrire un director (A2A)
---

# Écrire un director (A2A)

Un director est un agent qui répond en orchestrant des workers. Il expose un
point d'entrée conversationnel, transmet les skills des workers au modèle sous
forme d'outils, et laisse une boucle ReAct décider lesquels appeler. Ce guide
construit un assistant documentaire qui pilote le worker PDF de
[Écrire un worker](/how-to/write-a-worker).

## Prérequis

- Le worker `pdf-quickstart` installé et actif, exposant `pdf.read_text` et
  `pdf.count_pages`. Vérifiez avec `apollia-os a2a skills`.
- Un backend LLM configuré (`apollia-os llm status`).

## Exposer les workers comme outils, puis `react`

Le director est un agent `@on_message`. À l'intérieur du gestionnaire,
appelez la fonction libre `apollia.react(...)` : donnez-lui un system prompt,
le message utilisateur, et une liste d'outils. Transformez chaque skill de
worker en outil avec `await ctx.a2a.skill_as_tool(skill_id)`, une fonction
asynchrone qui résout le schéma du skill au moment de l'appel.

Créez `document_director.py` :

```python
"""Document assistant: drives the PDF worker through apollia.react."""

from apollia import agent, on_message, react
from apollia.types import Ctx, Message


SYSTEM_PROMPT = """\
You are a document assistant. Answer the user's question about a local PDF
by using the available tools:

- `pdf.read_text`: extract the text of a PDF, page by page.
- `pdf.count_pages`: count the pages of a PDF.

Reason step by step. When you have enough information, write a concise answer
and mention the file you inspected.
"""


@agent(
    name="document-director",
    version="0.1.0",
    description="Answers questions about local PDF files by orchestrating a worker.",
)
class DocumentDirector:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        return await react(
            ctx,
            system=SYSTEM_PROMPT,
            user=message,
            tools=[
                await ctx.a2a.skill_as_tool("pdf.read_text"),
                await ctx.a2a.skill_as_tool("pdf.count_pages"),
            ],
            max_steps=8,
        )


agent = DocumentDirector()
```

`react` est une fonction libre, pas une méthode de `ctx` : vous lui passez
`ctx` en premier argument. Elle exécute la boucle observer, raisonner, agir
et renvoie la réponse finale du modèle sous forme de chaîne de caractères.
Sa signature complète (y compris `temperature` et `max_steps`, qui vaut 15
par défaut) se trouve dans le [contrat SDK / ctx](/reference/sdk) ; les
méthodes `skill_as_tool` et les autres méthodes A2A se trouvent sur
[`ctx.a2a`](/reference/sdk/a2a).

Si le director référence un skill qu'aucun worker actif n'expose, il échoue
rapidement à l'exécution avec une erreur de skill inconnu. Installez et
activez le worker au préalable.

## Installer et exécuter

```bash
apollia-os inspect document_director.py
apollia-os agent install ./document_director.py
apollia-os agent enable document-director
apollia-os run document-director "How many pages are in /tmp/report.pdf, and what is it about?"
```

Le modèle décide d'appeler `pdf.count_pages`, puis `pdf.read_text`, puis
rédige sa réponse.

## Variante : construire la liste d'outils dynamiquement

Plutôt que de nommer les skills un par un, découvrez-les et filtrez-les par
espace de noms :

```python
all_skills = await ctx.a2a.list_skills()
tools = [
    await ctx.a2a.skill_as_tool(s["skill_id"])
    for s in all_skills
    if s["skill_id"].startswith("pdf.")
]
```

Gardez la liste d'outils réduite. Exposer plus d'une dizaine d'outils à la
fois rend les choix du modèle moins fiables.

## Variante : appeler un worker directement

Quand vous savez déjà quel skill appeler et n'avez pas besoin que le modèle
décide, invoquez-le directement avec `ctx.a2a.invoke`. Il renvoie l'enveloppe
A2A complète : dépliez le dict du skill avec `a2a_result_data` :

```python
from apollia.utils import a2a_result_data

envelope = await ctx.a2a.invoke("pdf.count_pages", {"path": "/tmp/report.pdf"})
data = a2a_result_data(envelope)
page_count = data["page_count"]
```

Utilisez `react` quand l'enchaînement des appels dépend de résultats
intermédiaires ; utilisez `invoke` pour une étape fixe et connue à l'avance.

## Variante : gérer une boucle bloquée

Si la boucle épuise `max_steps` sans converger, `react` laisse l'erreur
sous-jacente remonter sous forme de `RuntimeError`. Interceptez-la pour
dégrader gracieusement le comportement :

```python
try:
    return await react(ctx, system=SYSTEM_PROMPT, user=message, tools=tools, max_steps=5)
except RuntimeError:
    return "I could not finish the analysis in time. Could you narrow the question?"
```

`react` ne lève `DomainError("REACT_MAX_STEPS")` que pour un appel mal
configuré (`max_steps <= 0`), ce qui relève d'une erreur de programmation et
non d'un résultat d'exécution à intercepter.

## Étapes suivantes

- Laissez le runtime planifier et exécuter le travail multi-étapes à votre
  place, sans boucle ReAct à écrire vous-même :
  [Exécuter un agent orchestré](/how-to/run-an-orchestrated-agent).
- Assemblez le tout à travers plusieurs workers :
  [Construire un assistant multi-agent](/tutorials/build-a-multi-agent-assistant).
