---
sidebar_position: 4
title: Écrire un worker
---

# Écrire un worker

Un worker est un expert de domaine. Il expose un ou plusieurs skills A2A que
n'importe quel director peut appeler, et il fait une seule chose, mais bien.
Ce guide construit un petit worker PDF doté de deux skills, lire du texte et
compter des pages, puis l'invoque depuis la ligne de commande.

Ceci est un how-to, pas un tutoriel. Il suppose qu'Apollia est déjà installé
et que vous avez déjà écrit [votre premier agent](/tutorials/your-first-agent).

## Déclarer l'agent et ses skills

Un worker est une classe `@agent` dont les points d'entrée sont des méthodes
`@skill`. Chaque skill est un `async def`, son `skill_id` est en snake case à
espaces de noms séparés par des points, et ses paramètres (tout sauf `self`
et `ctx`) deviennent le schéma d'entrée du skill.

Créez `pdf_worker.py` :

```python
"""Minimal PDF worker: read text and count pages."""

from pathlib import Path
from typing import Annotated

from apollia import DomainError, agent, skill
from apollia.types import Ctx


def _ensure_pdf(path: str) -> None:
    p = Path(path)
    if not p.exists():
        raise DomainError("FILE_NOT_FOUND", f"PDF not found: {path}")
    if p.suffix.lower() != ".pdf":
        raise DomainError("WRONG_EXTENSION", f"Expected .pdf, got {p.suffix}")


@agent(
    name="pdf-quickstart",
    version="0.1.0",
    description="Read text and count pages of PDF files.",
    packages=("pypdf>=4",),
    tags=("pdf", "worker"),
    agent_type="worker",
)
class PdfQuickstart:
    @skill(
        "pdf.read_text",
        description="Extract text from a PDF, page by page.",
        examples=[{"path": "/tmp/report.pdf"}],
    )
    async def read_text(
        self,
        path: Annotated[str, "Absolute path to the .pdf file."],
        ctx: Ctx,
    ) -> dict:
        _ensure_pdf(path)
        from pypdf import PdfReader

        reader = PdfReader(path)
        pages = [page.extract_text() or "" for page in reader.pages]
        return {"text": "\n\n".join(pages), "page_count": len(pages)}

    @skill(
        "pdf.count_pages",
        description="Count the pages of a PDF file.",
        examples=[{"path": "/tmp/report.pdf"}],
    )
    async def count_pages(
        self,
        path: Annotated[str, "Absolute path to the .pdf file."],
        ctx: Ctx,
    ) -> dict:
        _ensure_pdf(path)
        from pypdf import PdfReader

        return {"page_count": len(PdfReader(path).pages)}
```

Remarques sur le contrat :

- `@skill("pdf.read_text", ...)` prend d'abord le `skill_id`, puis trois
  arguments nommés : `description`, `dangerous` (signale une compétence
  potentiellement destructrice), et `examples`, une liste optionnelle de
  dictionnaires de payloads d'exemple. C'est toute la surface du décorateur. La
  [référence SDK / contrat ctx](/reference/sdk) indexe les services `ctx` qu'un
  corps de compétence utilise.
- Les paramètres de domaine viennent en premier ; `ctx` est le dernier
  paramètre. Annotez chaque paramètre avec `Annotated[type, "description"]`
  pour que le schéma généré s'auto-documente.
- Retournez un `dict` brut. Le runtime l'enveloppe dans un résultat terminé.
- Levez `DomainError("CODE", "message")` pour les échecs attendus. Le
  dispatcher le transforme en résultat échoué typé plutôt qu'en plantage.
- `packages=(...)` déclare les dépendances tierces dont le worker a besoin.
  Les workers sont stdlib-only par défaut : ne déclarez un paquet que
  lorsqu'il est réellement nécessaire.
- Terminez le module par `agent = PdfQuickstart()`, et utilisez des imports
  absolus.

Ce worker n'appelle aucun service `ctx` : c'est du Python pur plus `pypdf`.
Les workers n'ont recours à `ctx` que lorsqu'ils ont besoin de génération, de
mémoire, d'outils ou de secrets.

## Installer et invoquer

Inspectez, installez et activez exactement comme pour n'importe quel agent :

```bash
apollia-os inspect pdf_worker.py
apollia-os agent install ./pdf_worker.py
apollia-os agent enable pdf-quickstart
```

Puis appelez un skill directement avec `a2a invoke`, en passant le payload en
JSON :

```bash
apollia-os a2a invoke pdf.count_pages --args '{"path": "/tmp/some.pdf"}'
```

Ajoutez `--json` pour obtenir le résultat complet lisible par machine. Pour
voir chaque skill exposé par les workers actifs, exécutez
`apollia-os a2a skills`. Chaque sous-commande `a2a` figure dans la
[référence CLI](/reference/cli).

## Variante : un paramètre optionnel

Comme `ctx` est un paramètre requis, tout paramètre doté d'une valeur par
défaut doit venir après lui :

```python
async def read_text(
    self,
    path: Annotated[str, "Path to the .pdf file."],
    ctx: Ctx,
    page_range: Annotated[
        str | None,
        "1-based page selection, for example '1-5,7'. Omit to read all pages.",
    ] = None,
) -> dict:
    ...
```

## Variante : des erreurs plus finement typées

Donnez aux appelants des codes précis sur lesquels brancher leur logique :

```python
if file_too_big(path):
    raise DomainError("FILE_TOO_LARGE", f"{path} exceeds 100 MB")
if encrypted(path):
    raise DomainError("ENCRYPTED", "Cannot read encrypted PDFs")
```

Un appelant lit `result["error"]["code"]` et réagit en conséquence, plutôt
que d'analyser une chaîne de message.

## Tester votre worker

Apollia fournit un harnais de test isomorphe, `apollia.testing`, qui permet
d'exécuter les skills en process avec un `ctx` simulé et sans démon. Voir
[Tester vos agents](/how-to/test-your-agents).

## Étapes suivantes

- Faire appeler plusieurs workers par un agent :
  [Écrire un director](/how-to/write-a-director).
- Lire les formes d'entrée et de sortie de [`ctx.tools`](/reference/sdk/tools),
  [`ctx.secrets`](/reference/sdk/secrets), et des autres services qu'utilise
  un worker plus riche.
