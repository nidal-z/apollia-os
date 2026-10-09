---
sidebar_position: 4
title: Écrire un outil
---

# Écrire un outil

Un outil est le Python qu'un paquet apporte à ses agents : une fonction
`async def` sous `@tool`, qu'un agent appelle quand sa mission le demande.
L'agent, lui, n'est pas du code. Il est déclaré dans le `apollia.toml` du paquet,
avec sa mission et les outils qu'il peut joindre. Ce guide écrit un paquet doté
d'un agent et d'un outil qui compte les pages d'un PDF, puis l'exécute.

Ceci est un guide pratique. Il suppose qu'Apollia est installé et que son
runtime tourne ; sinon, voir
[Installer et exécuter le runtime](/how-to/install-and-run).

## Partir du gabarit

```bash
apollia-os package new pdf-reader
```

La commande écrit un dossier qui se construit tel quel :

```text
pdf-reader/
  apollia.toml
  tools/pdf_reader.py
```

`apollia.toml` déclare le paquet, un agent et un outil. `tools/` contient le
module Python de l'outil.

## Écrire la fonction

Remplacez `tools/pdf_reader.py` par l'outil :

```python
"""The tool of the package pdf-reader: count the pages of a PDF."""

from pathlib import Path
from typing import Annotated, Any

from apollia import DomainError, tool
from apollia.types import Ctx


@tool(
    "count_pages",
    "Count the pages of a PDF file.",
    action="read_personal",
    examples=[{"path": "/tmp/report.pdf"}],
)
async def count_pages(
    ctx: Ctx,
    path: Annotated[str, "Absolute path of the .pdf file."],
) -> dict[str, Any]:
    file = Path(path)
    if not file.exists():
        raise DomainError("FILE_NOT_FOUND", f"PDF not found: {path}")
    if file.suffix.lower() != ".pdf":
        raise DomainError("WRONG_EXTENSION", f"Expected .pdf, got {file.suffix}")

    from pypdf import PdfReader

    pages = len(PdfReader(path).pages)
    ctx.logger.info("counted %d pages", pages)
    return {"path": path, "page_count": pages}
```

Ce que prend le décorateur, et rien d'autre :

| Argument | Sens |
| --- | --- |
| `name` | Identifiant de l'outil, `[a-z0-9_-]+`, unique dans le paquet. |
| `description` | Ce que fait l'outil. Le modèle la lit pour décider quand l'appeler, et la fiche de l'agent l'affiche. |
| `action` | La famille d'action, par mot-clé et obligatoire : `read`, `read_personal`, `write_or_delete`, `send_external` ou `execute_code`. |
| `examples` | Liste facultative d'exemples d'entrées, chacun un dictionnaire. |

Remarques sur le contrat :

- La fonction est `async def`. Toute autre forme est refusée à l'import du
  module, par une `AgentConfigError` qui nomme l'outil.
- Les entrées sont les paramètres de la fonction, `ctx` exclu. Annotez chacun
  avec `Annotated[type, "description"]` : le schéma d'entrée que voit le modèle
  est inféré de la signature.
- Renvoyez une valeur sérialisable en JSON, un `dict` le plus souvent.
- Levez `DomainError("CODE", "message")` pour un échec attendu. L'appel échoue,
  et l'agent lit l'erreur et décide de la suite.
- La famille d'action dit ce que l'appel fait au monde, et toute famille sauf
  `read` rend l'outil sensible. Il n'existe pas d'indicateur séparé : déclarez
  la famille qui dit ce que l'outil fait réellement. Les familles sont listées
  dans la [référence du manifeste de paquet](/reference/package-manifest#action-families).
- Utilisez des imports absolus, et `ctx.logger` plutôt que `print()`.

## Ce que `ctx` donne à un outil

Un outil s'exécute avec un contexte nu : `ctx.logger`, qui journalise sous le
nom de l'agent qui l'a appelé. Il ne porte ni mémoire ni
profil de l'utilisateur, et `ctx.llm` et `ctx.tools` valent `None` : raisonner et
appeler d'autres outils reviennent à l'agent, qui tient la boucle. Un outil fait
une chose et rend la main.

## Décrire l'outil dans le manifeste

Le manifeste décrit chaque outil, afin qu'Apollia sache ce qu'un paquet livre
sans l'exécuter. Écrivez la description depuis le code :

```bash
python -m apollia describe pdf-reader
```

La commande importe les modules du dossier et réécrit les entrées `[[tools]]`
de `apollia.toml` : nom, module, description, famille d'action et schéma
d'entrée. Tout le reste du manifeste est conservé tel qu'écrit. Elle a besoin
du SDK `apollia`, et de ce que vos modules importent à leur premier niveau, dans
l'environnement Python qui l'exécute.

Modifiez ensuite le reste de `apollia.toml` à la main. Accordez l'outil à
l'agent, et déclarez la dépendance :

```toml
[[agents]]
id = "pdf-reader"
name = "PDF reader"
mission = """
Answer questions about the PDF files the user names. Count the pages with
the tool before you answer; never guess.
"""
examples = ["How many pages are in /tmp/report.pdf?"]

[agents.defaults]
access = ["pkg:pdf-reader/count_pages"]
autonomy = "assisted"

[pip]
packages = ["pypdf==4.3.1"]
```

Un outil s'accorde sous le nom `pkg:<paquet>/<outil>`. La même liste prend les
outils natifs par leur nom et les outils MCP sous `mcp:<serveur>/<outil>`.
Chaque table et chaque clé figurent dans la
[référence du manifeste de paquet](/reference/package-manifest).

## Construire, installer et exécuter

```bash
apollia-os package build pdf-reader
apollia-os package install pdf-reader-0.1.0.apo --confirm-deps
```

`package build` valide le manifeste et écrit l'archive sans exécuter de Python.
`package install` a besoin du runtime ; `--confirm-deps` accepte les
dépendances Python que le paquet installe. Envoyez ensuite une demande à
l'agent :

```bash
apollia-os run pdf-reader "How many pages are in /tmp/report.pdf?"
```

L'agent répond en appelant `count_pages`. Comme la famille est `read_personal`, l'appel est sensible : une
question vous est posée avant qu'il ne s'exécute, sauf si vous l'avez déjà
autorisé.

## Variante : une entrée facultative

Un paramètre doté d'une valeur par défaut est une entrée facultative :

```python
async def count_pages(
    ctx: Ctx,
    path: Annotated[str, "Absolute path of the .pdf file."],
    skip_blank: Annotated[bool, "Leave blank pages out of the count."] = False,
) -> dict[str, Any]:
    ...
```

Relancez `python -m apollia describe pdf-reader` après tout changement d'une
signature ou d'un décorateur, pour que le manifeste continue de dire ce que fait
le code. `package build` refuse un dossier qui définit un outil que le manifeste
ne décrit pas.

## Pour aller plus loin

- Enchaîner plusieurs étapes en Python, des agents se chargeant du
  raisonnement : [Écrire un workflow](/how-to/write-a-director).
- Tester la fonction avec un contexte simulé :
  [Tester vos outils et vos workflows](/how-to/test-your-agents).
- Signer et partager l'archive :
  [Empaqueter et distribuer](/how-to/package-and-distribute-an-agent).
