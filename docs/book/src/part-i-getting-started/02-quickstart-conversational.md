# Quickstart : agent conversationnel

Un agent conversationnel répond à un utilisateur en langue naturelle, en streaming, dans l'app Desktop ou le chat CLI. C'est le pattern `@on_message` (cf. [chapitre 8](../part-ii-the-decorators/08-on-message-decorator.md)).

**Objectif :** écrire un coach produit qui répond aux questions des utilisateurs en s'appuyant sur une base de connaissances embarquée. Code complet : 70 lignes. Temps : 15 minutes.

---

## Le fichier `coach.py`

```python
"""Coach produit conversationnel."""

from pathlib import Path

from apollia import agent, on_message
from apollia.types import Ctx, Message


KNOWLEDGE_BASE = """
Apollia OS est un runtime local pour agents IA autonomes.

Trois patterns d'agent :
- Worker : expose des skills A2A (pdf-worker, chart-worker, ...).
- Conversational : répond à un humain via @on_message.
- Director : orchestre des workers via apollia.react.

Trois services Ctx essentiels :
- ctx.llm : génération.
- ctx.memory : persistance épisodique + sémantique.
- ctx.a2a : appel d'autres agents.
"""


SYSTEM_PROMPT = f"""\
You are a helpful product coach for Apollia OS. Answer in the user's
language. Stay concise (2 to 4 sentences). When the user asks for a
capability that is not in the knowledge base below, say so honestly and
suggest reading the relevant documentation.

KNOWLEDGE BASE
==============
{KNOWLEDGE_BASE}
"""


@agent(
    name="coach",
    version="0.1.0",
    description="Friendly product coach for Apollia OS users.",
    agent_type="system",
    memory_namespace="coach",
)
class Coach:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        full = ""
        stream = await ctx.llm.stream(
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                *history,
                {"role": "user", "content": message},
            ],
        )
        async for token in stream:
            ctx.events.emit_token(token)
            full += token
        await ctx.memory.record(
            f"Q: {message[:120]} | A: {full[:200]}",
            importance=0.4,
        )
        return full
```

> **Pourquoi `memory_namespace="coach"`.** Sans cette déclaration, `ctx.memory` est `None` (principe #6 : la mémoire est opt-in par agent) et l'appel à `ctx.memory.record(...)` lèverait `'NoneType' object has no attribute 'record'`. Cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md) pour les autres options gating.

C'est tout. Un fichier, un décorateur de classe, une méthode.

---

## Anatomie du code

**Knowledge base inline.** Pour ce quickstart, la base de connaissances est une chaîne Python en haut du fichier. Dans un vrai projet, elle vivrait dans `datasources/knowledge.yaml` (cf. [chapitre 15](../part-iii-the-ctx-protocol/15-ctx-datasources-templates.md)) pour qu'un opérateur puisse la modifier sans toucher au code.

**`@agent(...)`.** Trois champs obligatoires (`name`, `version`, `description`), plus `agent_type="system"` pour signaler que c'est un agent fourni par l'auteur (vs. un agent utilisateur). Le décorateur instancie la classe et expose `module.agent` au bridge PyO3 (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)).

**`@on_message`.** Sans arguments. Le runtime cherche cette méthode pour router les messages utilisateur. Signature obligatoire : `(self, message: str, history: list[Message], ctx: Ctx) -> str`.

**Streaming.** `ctx.llm.stream(...)` retourne un `AsyncIterator[str]`. À chaque token, on appelle `ctx.events.emit_token(token)` pour pousser vers l'UI, et on accumule dans `full` pour la trace finale.

**Persistance.** Un appel `ctx.memory.record` enregistre la trace question/réponse en mémoire épisodique. Importance `0.4` (modéré). Une vraie politique de mémoire ajusterait le score selon la sensibilité de la question (cf. [chapitre 12](../part-iii-the-ctx-protocol/12-ctx-memory.md)).

---

## Tester en local

Validez d'abord le manifeste :

```bash
python -m apollia inspect coach.py
```

Vous devriez voir un agent `coach` avec un `@on_message` détecté et aucune skill (normal, l'agent est purement conversationnel).

Installez l'agent et lancez une invocation :

```bash
apollia-os agent install ./coach.py
apollia-os agent enable coach
apollia-os run coach "Comment fonctionne le pattern Director ?"
```

Le runtime charge l'agent (création du venv isolé), `enable` l'active dans le registre, puis `run` envoie le message au handler `@on_message`. La réponse arrive en streaming.

Pour une vraie session multi-tour, ouvrez l'app Desktop et sélectionnez l'agent dans la sidebar. `apollia-os run` est un appel one-shot, pratique pour scripter ou tester.

---

## Variations

**Personnaliser par utilisateur :** ajoutez `await ctx.profile.get("user.name")` au début de la méthode et glissez le résultat dans le system prompt.

**Garder le contexte d'une session précédente :** ajoutez un `await ctx.memory.search(message, limit=3)` au début, puis injectez les hits comme contexte additionnel dans le system prompt.

**Limiter le streaming :** si vous n'avez pas besoin de l'affichage token par token, remplacez la boucle par un appel one-shot :

```python
response = await ctx.llm.complete(messages=[...])
return response.content
```

L'UI affichera la réponse complète une fois prête, sans animation incrémentale.

---

## Prochaines étapes

- **Worker A2A :** [chapitre 3](03-quickstart-worker.md), exposer des skills appelables par d'autres agents.
- **Director :** [chapitre 4](04-quickstart-director.md), orchestrer plusieurs workers depuis un agent conversationnel.
- **Tests isomorphiques :** [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md), tester un `@on_message` sans démarrer le runtime.
