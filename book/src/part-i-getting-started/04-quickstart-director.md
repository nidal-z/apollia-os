# Quickstart : agent director

Un director enchaîne des appels à d'autres agents (workers) via A2A, en utilisant un LLM comme cerveau de raisonnement. Le pattern canonique : `@on_message` qui appelle `apollia.react(...)` avec une liste de workers exposés comme tools.

**Objectif :** écrire un assistant qui répond à une question en utilisant deux workers (`pdf.read_text` du quickstart précédent + un worker de recherche web fictif). Code complet : 50 lignes. Temps : 25 minutes.

---

## Le fichier `research_director.py`

```python
"""Assistant de recherche : pilote des workers via apollia.react."""

from apollia import agent, on_message, react
from apollia.types import Ctx, Message


SYSTEM_PROMPT = """\
You are a research assistant. Answer the user's question by orchestrating
the available tools:

- `pdf.read_text`: read the text of a local PDF file.
- `web.search`: query the web and return a short summary.

Reason step by step. When you have enough information, write a concise
answer (4 to 8 sentences) citing the sources you used.
"""


@agent(
    name="research-director",
    version="0.1.0",
    description="Orchestrates PDF and web search workers to answer questions.",
)
class ResearchDirector:
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
                await ctx.a2a.skill_as_tool("web.search"),
            ],
            max_steps=10,
        )
```

C'est tout. Pas de boucle manuelle, pas de parsing JSON, pas de gestion de tool calls.

---

## Anatomie du code

**`@on_message`.** Le director est un agent conversationnel : il reçoit un message utilisateur et renvoie une réponse texte. Pas de skill A2A à exposer.

**`apollia.react(ctx, system, user, tools, max_steps)`.** Une fonction libre (`from apollia import react`), pas une méthode de `ctx`. Elle pilote une boucle Reason+Act :

1. Envoie le system prompt et la question au LLM avec la liste de tools.
2. Si le LLM demande un tool call, le runtime exécute l'appel A2A vers le worker correspondant.
3. Le résultat du worker est ré-injecté dans le LLM.
4. Boucle jusqu'à une réponse finale, ou jusqu'à `max_steps` (par défaut 15).

`ctx.a2a.skill_as_tool(skill_id)` est **asynchrone** : le bridge consulte le registre A2A. Toujours préfixer par `await`. Elle transforme un id de skill (`"pdf.read_text"`) en descripteur tool au format Anthropic / OpenAI (cf. [chapitre 14](../part-iii-the-ctx-protocol/14-ctx-a2a.md)).

**Aucun parsing manuel.** Le runtime LLM gère le format tool-use natif (Anthropic, OpenAI), Apollia n'utilise pas de parsing JSON dans des balises XML maison.

---

## Pré-requis

Ce director suppose que **deux workers sont installés** sur la machine :

1. `pdf-quickstart` du [quickstart précédent](03-quickstart-worker.md) qui expose `pdf.read_text`.
2. Un worker `web-search` qui expose `web.search`. Pour ce quickstart, soit vous écrivez un stub local, soit vous utilisez le worker `web-search` bundlé avec Apollia.

Vérifiez l'installation avant de lancer le director :

```bash
apollia-os agent list
#   NAME              VERSION    STATUS    AUTO-LOAD  SOURCE
#   pdf-quickstart    0.1.0      active    yes        installed
#   web-search        0.1.0      active    yes        installed

# Pour les skills exposées :
apollia-os a2a skills
```

Si l'un des workers manque, l'agent démarre quand même (la déclaration A2A est résolue à l'invocation, pas au boot), mais le LLM aura les tools dans son contexte sans pouvoir les exécuter. Soit vous les installez, soit vous adaptez les `skill_as_tool` à des workers que vous avez.

---

## Lancer le director

```bash
python -m apollia inspect research_director.py
apollia-os agent install ./research_director.py
apollia-os agent enable research-director
apollia-os run research-director
> Compare les approches de cache LLM décrites dans /tmp/paper1.pdf et /tmp/paper2.pdf.
```

L'UI affiche les étapes ReAct au fil de l'eau (chaque appel d'outil, chaque résultat) si vous êtes en mode développeur. La réponse finale arrive en streaming.

---

## Variations

**Plus de workers :** ajoutez autant de `skill_as_tool` que vous voulez. Le LLM choisira selon le contexte. Évitez d'en exposer 20 : un LLM se perd au-delà d'une dizaine de tools simultanés.

**Construire la liste dynamiquement :** si vous voulez exposer toutes les skills disponibles sur la machine :

```python
all_skills = await ctx.a2a.list_skills()
tools = [await ctx.a2a.skill_as_tool(s["skill_id"]) for s in all_skills if s["skill_id"].startswith(("pdf.", "web."))]
```

**Réduire le budget :** `max_steps=5` force une réponse rapide même si le LLM voudrait fouiller plus. Utile pour des cas où la latence importe plus que l'exhaustivité.

**Catcher l'échec :** si le LLM épuise `max_steps`, `react` propage l'erreur. Le director peut retomber sur un message poli :

```python
from apollia import DomainError

try:
    return await react(ctx, system=..., user=message, tools=[...], max_steps=5)
except DomainError as exc:
    if "REACT_MAX_STEPS" in exc.code:
        return "Je n'ai pas pu finir l'analyse dans les temps. Reformulez la question ?"
    raise
```

---

## Prochaines étapes

- **Quickstart orchestré :** [chapitre 5](05-quickstart-orchestrated.md), le pattern où le runtime ORIA pilote tout, vous décrivez seulement l'intention.
- **Tests d'un director :** [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md), mock du LLM et des workers A2A, assertions sur la séquence d'invocations.
- **Capstone :** [Partie IX](../part-ix-capstone/37-capstone-overview.md), un director complet en production qui orchestre trois workers spécialisés.
