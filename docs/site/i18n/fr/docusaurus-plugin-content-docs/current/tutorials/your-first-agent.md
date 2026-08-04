---
sidebar_position: 2
title: Votre premier agent
---

# Votre premier agent

Dans ce tutoriel, vous construisez un petit agent conversationnel, un coach
produit qui répond aux questions sur Apollia à partir d'une base de
connaissances embarquée. Vous écrirez environ soixante-dix lignes de Python,
installerez l'agent dans un daemon Apollia en cours d'exécution, et lui
parlerez depuis la ligne de commande. Comptez environ quinze minutes.

À la fin, vous aurez parcouru le cycle complet que traverse chaque agent :
écrire une classe, déclarer un point d'entrée, installer, activer, exécuter.

## Avant de commencer

- Apollia installé, avec la commande `apollia-os` disponible dans votre `PATH`.
- **Le daemon en cours d'exécution**, dans un second terminal :

  ```sh
  apollia-os start --port 7771
  ```

  `agent enable`, `run` et `llm status` communiquent tous avec lui : sans lui,
  ils échouent avec `runtime not started (connection refused)`. Laissez-le
  tourner pendant tout le tutoriel et arrêtez-le avec `apollia-os stop` à la
  fin.
- Un backend LLM configuré. Vérifiez-le avec `apollia-os llm status`. Si rien
  n'est encore configuré, enregistrez un modèle local avec
  `apollia-os llm setup --local --model /path/to/model.gguf`, ou un backend
  cloud avec
  `apollia-os llm backends create --provider <p> --model <m> --api-key <key>`.
  Voir
  [Installer et exécuter le runtime](/how-to/install-and-run#step-3-configure-a-model-backend)
  et la [référence CLI](/reference/cli) pour chaque sous-commande `llm`.

Vous n'avez pas besoin de connaître le SDK pour l'instant. Ce tutoriel
introduit un seul décorateur et un seul service ; le reste est du Python
ordinaire.

## Étape 1 : écrire l'agent

Créez un fichier nommé `coach.py` :

```python
"""A friendly conversational product coach for Apollia OS."""

from apollia import agent, on_message
from apollia.types import Ctx, Message


KNOWLEDGE_BASE = """
Apollia OS is a local runtime for autonomous AI agents.

Three agent patterns:
- Worker: exposes A2A skills (a pdf worker, a chart worker, and so on).
- Conversational: replies to a human through @on_message.
- Director: orchestrates workers through apollia.react.

Three essential Ctx services:
- ctx.llm: generation.
- ctx.memory: episodic and semantic persistence.
- ctx.a2a: calling other agents.
"""

SYSTEM_PROMPT = f"""\
You are a helpful product coach for Apollia OS. Answer in the user's
language. Stay concise, two to four sentences. When the user asks for a
capability that is not in the knowledge base below, say so honestly and
point them to the documentation.

KNOWLEDGE BASE
==============
{KNOWLEDGE_BASE}
"""


@agent(
    name="coach",
    version="0.1.0",
    description="Friendly product coach for Apollia OS users.",
    agent_type="assistant",
)
class Coach:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                *history,
                {"role": "user", "content": message},
            ],
        )
        return response.content


agent = Coach()
```

Trois éléments font de ce code un agent :

- **`@agent(...)`** déclare le manifest. `name`, `version` et `description`
  sont obligatoires. `agent_type` est un label optionnel.
- **`@on_message`** marque l'unique point d'entrée conversationnel. Sa
  signature est fixe : `(self, message, history, ctx)`, qui retourne la
  réponse sous forme de chaîne de caractères.
<!-- claim:module-level-agent-attribute-is-the-entry-point -->

- **`agent = Coach()`** en bas du module est ce que le runtime charge. Chaque
  module d'agent Apollia se termine par cette ligne. Utilisez des imports
  absolus (`from apollia import ...`), jamais relatifs.

À l'intérieur du gestionnaire, `ctx.llm.complete(...)` envoie la conversation
au backend configuré et retourne une réponse dont `.content` est le texte
généré. La forme exacte de chaque service `ctx` se trouve dans la
[référence SDK / contrat ctx](/reference/sdk) ; ce tutoriel n'a besoin que de
[`ctx.llm`](/reference/sdk/llm).

## Étape 2 : l'inspecter

Avant d'installer quoi que ce soit, vérifiez le fichier de manière statique.
`inspect` lit le manifest et rapporte ce que l'agent déclare, sans démarrer de
runtime :

```bash
apollia-os inspect coach.py
```

Si vous avez mal saisi un argument de décorateur ou oublié le `agent = ...`
au niveau du module, c'est ici que vous le découvrez.

## Étape 3 : installer et activer

L'installation copie le fichier dans le magasin d'agents d'Apollia, puis
l'activation le rend chargeable :

```bash
apollia-os agent install ./coach.py
apollia-os agent enable coach
```

Confirmez qu'il est actif :

```bash
apollia-os agent list
```

## Étape 4 : lui parler

`run` envoie un message à l'agent et affiche la réponse :

```bash
apollia-os run coach "How does the Director pattern work?"
```

Vous devriez obtenir une réponse de deux à quatre phrases, puisée dans la
base de connaissances. Posez-lui une question hors de cette base et il vous
dira qu'il ne sait pas, parce que le prompt système le lui a demandé.

`apollia-os run` est un appel unique. Pour un échange continu, utilisez
`apollia-os chat` ou l'application desktop ; les deux conservent
l'historique (`history`) que votre gestionnaire accepte déjà.

## Ce que vous avez construit

Un agent conversationnel est une classe avec une unique méthode
`@on_message` qui transforme un message entrant en réponse, en utilisant
`ctx.llm` pour la génération. C'est le plus petit agent complet qu'Apollia
exécute.

## Étapes suivantes

- Diffuser la réponse token par token avec
  [`ctx.llm`](/reference/sdk/llm) `stream` et
  [`ctx.events`](/reference/sdk/events) `emit_token`, pour que
  l'utilisateur voie le texte au fur et à mesure qu'il est produit.
- Donner de la mémoire à l'agent en ajoutant `memory_namespace="coach"` à
  `@agent` et en enregistrant les tours d'échange avec
  [`ctx.memory`](/reference/sdk/memory). La mémoire est optionnelle par
  agent : sans espace de nommage (`namespace`), `ctx.memory` n'est pas
  disponible, par conception.
- Exposer des capacités réutilisables plutôt qu'une boucle de discussion :
  [Écrire un worker](/how-to/write-a-worker).
- Laisser un agent orchestrer plusieurs workers :
  [Écrire un director](/how-to/write-a-director).
