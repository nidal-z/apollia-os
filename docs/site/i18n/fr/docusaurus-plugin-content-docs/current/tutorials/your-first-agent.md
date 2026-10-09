---
sidebar_position: 2
title: Votre premier agent
description: "Construire, installer et lancer votre premier paquet Apollia : un petit coach produit déclaré dans apollia.toml, avec un outil Python, d'un dossier vide à une réponse qui fonctionne."
---

# Votre premier agent

Dans ce tutoriel, vous construisez un petit paquet : un coach produit qui
répond aux questions sur Apollia à partir d'un glossaire. L'agent tient en
quelques lignes de `apollia.toml` ; le glossaire est un outil Python d'une
trentaine de lignes. Vous installez le paquet dans un runtime Apollia en cours
d'exécution et vous parlez à l'agent depuis la ligne de commande. Comptez
environ quinze minutes.

À la fin, vous aurez parcouru le cycle complet que traverse chaque paquet :
déclarer l'agent, écrire un outil, décrire, construire, installer, exécuter.

## Avant de commencer

- Apollia installé, avec la commande `apollia-os` disponible dans votre `PATH`.
- Le SDK importable par le Python avec lequel vous travaillez
  (`pip install -e ./sdk` depuis un clone du dépôt, dans un environnement
  virtuel). Il sert à décrire et à tester le paquet, pas à le construire ni à
  l'installer.
- **Le runtime en cours d'exécution**, dans un second terminal :

  ```sh
  apollia-os start --port 7771
  ```

  `package install`, `run` et `llm status` communiquent tous avec lui : sans
  lui, ils échouent avec `runtime not started (connection refused)`. Laissez-le
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
introduit un seul décorateur ; le reste est du Python ordinaire et un fichier
TOML.

## Étape 1 : déclarer l'agent

Un paquet est un dossier qui porte un `apollia.toml` à sa racine. Créez un
dossier nommé `coach` et, dedans, le fichier `apollia.toml` :

```toml
[package]
name = "coach"
version = "0.1.0"
publisher = "You"
description = "A product coach that answers questions about Apollia from a small glossary."
sdk = ">=0.3.0"

[[agents]]
id = "coach"
name = "Coach"
mission = """
You are a product coach for Apollia. Answer in the language of the person, in
two to four sentences.

When the person asks what a term means, call the tool `define` with that term
and build your answer on the definition it returns. When the tool knows no
such term, say so plainly, list the terms it knows, and point the person to
the documentation. Never invent a definition.
"""
examples = [
    "What is a package?",
    "What is the difference between a tool and a workflow?",
]

[agents.defaults]
access = ["pkg:coach/define"]
autonomy = "assisted"
```

L'agent est là tout entier :

- **`[[agents]]`** le déclare. `mission` contient ses instructions, écrites
  comme vous brieferiez un collègue. `examples` liste les demandes que sa carte
  affiche.
- **`[agents.defaults]`** dit ce que le paquet recommande. `access` énumère les
  outils que l'agent peut appeler, un par un : `pkg:coach/define` est l'outil
  `define` du paquet `coach`, que l'étape suivante écrit. La personne qui
  installe le paquet peut ensuite accorder ou retirer chaque accès ; une
  valeur par défaut n'est jamais une contrainte.

<!-- claim:module-level-agent-attribute-is-the-entry-point -->

Un agent est une donnée. Aucune classe ni aucun objet Python ne le représente,
et le runtime ne charge aucun agent depuis un module Python : le moteur
intégré au runtime exécute l'agent à partir de cette entrée, avec sa mission,
les outils qui lui sont accordés et le modèle que vous avez configuré.

## Étape 2 : écrire l'outil

Le Python d'un paquet apporte des outils. Un outil est une fonction `async`
sous `@tool`. Créez `coach/tools/glossary.py` :

```python
"""The glossary tool of the package coach."""

from typing import Annotated

from apollia import tool
from apollia.types import Ctx

GLOSSARY = {
    "agent": "A mission, examples and granted tools, written as data in apollia.toml.",
    "package": "A folder with an apollia.toml: any mix of agents, tools and workflows.",
    "tool": "An async Python function under @tool, which an agent may be granted.",
    "workflow": "An async Python function under @workflow, whose steps are declared ahead.",
}


@tool(
    "define",
    "Look a term up in the Apollia glossary.",
    action="read",
    examples=[{"term": "package"}],
)
async def define(
    ctx: Ctx,
    term: Annotated[str, "The term to define, one or two words."],
) -> dict[str, object]:
    key = term.strip().lower()
    ctx.logger.info("define called for %s", key)
    if key not in GLOSSARY:
        return {"found": False, "known_terms": sorted(GLOSSARY)}
    return {"found": True, "term": key, "definition": GLOSSARY[key]}
```

Quatre choses à lire ici :

- **Le nom et la description** sont ce que le modèle voit. Le nom est la
  dernière partie de `pkg:coach/define`.
- **`action="read"`** est la famille de l'outil : ce que l'appeler fait au
  monde. Il en existe cinq (`read`, `read_personal`, `write_or_delete`,
  `send_external`, `execute_code`), et toute famille autre que `read` est
  sensible : le runtime demande avant qu'un tel outil s'exécute.
- **La signature est le schéma.** `ctx` vient en premier et n'en fait pas
  partie ; chaque autre paramètre est une entrée, et `Annotated` porte la
  description que le modèle lit. `examples` donne des entrées types, montrées à
  côté du schéma.
- **`ctx`** est le contexte que le runtime remet à la fonction. Cet outil
  n'utilise que `ctx.logger`, un `logging.Logger` standard. Le contrat complet
  se trouve dans la [référence SDK / ctx](/reference/sdk).

Utilisez des imports absolus (`from apollia import ...`), jamais relatifs.

## Étape 3 : tester l'outil

Un outil est une simple fonction `async def` : un test l'attend telle qu'elle
est écrite, avec un `MockContext` à la place du runtime. Créez
`coach/tests/test_glossary.py` :

```python
import asyncio

from apollia.testing import MockContext

from tools.glossary import define


def test_define_returns_the_definition_of_a_known_term():
    # GIVEN a context and a term of the glossary, in another case
    ctx = MockContext()

    # WHEN the tool is awaited as written
    result = asyncio.run(define(ctx, " Package "))

    # THEN the definition comes back under the normalised term
    assert result["found"] is True
    assert result["term"] == "package"
```

Lancez-le depuis le dossier du paquet :

```sh
cd coach
python -m pytest -p no:cacheprovider tests
cd ..
```

`-p no:cacheprovider` empêche pytest de laisser un dossier `.pytest_cache`
derrière lui : la construction de l'étape suivante prend tous les fichiers du
dossier du paquet.

## Étape 4 : décrire et construire

`apollia.toml` doit dire quels outils le Python définit, pour que lire le
manifeste n'exécute jamais le code. Vous n'écrivez pas ces entrées à la main :

```sh
python -m apollia describe coach
```

La commande importe les modules du dossier et écrit une entrée `[[tools]]`
pour `define` à la fin de `apollia.toml` : son module, sa description, sa
famille et le schéma de ses entrées. Relancez-la après chaque modification
d'une fonction décorée.

Construisez ensuite l'archive et relisez-la :

```sh
apollia-os package build coach
apollia-os package inspect coach-0.1.0.apo
```

`build` valide le manifeste et écrit `coach-0.1.0.apo` dans le dossier
courant. Aucun Python ne s'exécute. Si le manifeste nomme un outil que le
paquet ne définit pas, ou si le Python en définit un que le manifeste ne
décrit pas, c'est ici que vous l'apprenez. `inspect` affiche le manifeste, les
entrées de l'archive et l'état de sa signature, ici `unsigned`.

## Étape 5 : installer

`preview` montre ce que le paquet contient et demande, avant toute écriture.
`install` le copie dans le magasin de paquets du runtime :

```sh
apollia-os package preview coach-0.1.0.apo
apollia-os package install coach-0.1.0.apo
```

Un paquet non signé s'installe avec un avertissement. Le runtime lit les outils
des paquets installés à son démarrage : redémarrez-le donc, avec
`apollia-os stop`, puis en le relançant dans son terminal par
`apollia-os start --port 7771`. Vérifiez que l'agent est là :

```sh
apollia-os agent list --definitions
```

## Étape 6 : lui parler

`run` envoie un message à l'agent et affiche la réponse :

```sh
apollia-os run coach "What is a package?"
```

Vous devriez obtenir une réponse de deux à quatre phrases bâtie sur le
glossaire. Interrogez-le sur un terme que le glossaire ne contient pas et il
vous le dira, parce que la mission le lui demande.

`apollia-os run` est un appel unique. Pour un échange continu, utilisez
`apollia-os chat` ou l'application de bureau, où l'agent apparaît sur l'écran
Agents avec sa mission, ses exemples et ses accès.

## Ce que vous avez construit

Un paquet avec un agent et un outil. L'agent est une donnée dans
`apollia.toml` : une mission, des exemples et la liste de ce qu'il peut
atteindre. L'outil est une fonction Python accordée à l'agent. C'est le plus
petit paquet complet qui porte du code ; un paquet qui ne contient que des
`[[agents]]` et aucun Python est valide aussi.

## Étapes suivantes

- Modifiez la mission, construisez et installez de nouveau : une mise à jour
  conserve les changements que la personne a faits de son côté sur l'agent.
- Partez, pour le paquet suivant, du dossier qu'écrit
  `apollia-os package new <name>`, qui se construit tel quel.
- Signez l'archive avec `apollia-os package build coach --sign`, pour que la
  personne qui l'installe voie qui l'a publiée.
- Enchaînez plusieurs agents et une question à l'utilisateur dans un
  workflow :
  [Construire un assistant multi-agents](/tutorials/build-a-multi-agent-assistant).
