# Quickstart : agent worker

Un worker expose une ou plusieurs **skills** appelables par la CLI, l'API REST, ou un autre agent via A2A. C'est le pattern `@skill` (cf. [chapitre 7](../part-ii-the-decorators/07-skill-decorator.md)).

**Objectif :** écrire un worker qui lit le texte d'un PDF et compte ses pages, en quelques dizaines de lignes. Deux skills, deux fonctions async, des erreurs typées. Temps : 20 minutes.

---

## Le fichier `pdf_worker.py`

```python
"""Worker PDF minimal : lecture de texte et comptage de pages."""

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

Deux skills, environ 50 lignes utiles. Pas de classe parente, pas de manifest manuel.

---

## Anatomie du code

**`@agent(...)`.** `name`, `version`, `description` obligatoires. `packages=("pypdf>=4",)` déclare la dépendance PyPI : le runtime installera `pypdf` dans le venv isolé du worker au boot. `agent_type="worker"` signale la taxonomie.

**`@skill(...)`.** L'id est `dot.snake_case`. `description` est ce que voit un LLM appelant. `examples=[{...}]` est un payload réaliste qui sera exposé dans le tool descriptor, utile pour les LLM mid-market (cf. [chapitre 20](../part-iv-llm-friendly-design/20-examples-payloads.md)).

**`Annotated[str, "..."]`.** Le type reste `str`, l'annotation ajoute une description de paramètre qui apparaît dans l'input schema généré (cf. [chapitre 19](../part-iv-llm-friendly-design/19-annotated-descriptions.md)). Le LLM voit cette description pour deviner les payloads.

**`raise DomainError("CODE", "message")`.** Le boundary du dispatcher (cf. [chapitre 22](../part-v-error-handling/22-domain-errors.md)) trappe et produit un `AIPResult.failed` typé. L'agent ne manipule jamais `AIPResult` à la main.

**Retour `dict`.** Une simple struct JSON-sérialisable. Le boundary l'emballe en `AIPResult.completed`. Pas de cérémonie.

---

## Tester en local

Validation statique :

```bash
python -m apollia inspect pdf_worker.py
```

Vous devriez voir un agent `pdf-quickstart` avec deux skills (`pdf.read_text`, `pdf.count_pages`), chacune avec son `input_schema` inféré depuis la signature et ses `examples`.

Installation et invocation :

```bash
apollia agent install ./pdf_worker.py
apollia invoke pdf-quickstart pdf.count_pages path=/tmp/some.pdf
# {"page_count": 12}
```

Le runtime installe le worker (copie du fichier, création du venv isolé, installation de `pypdf`), résout les outils, puis dispatch l'invocation à la bonne skill.

---

## Variations

**Ajouter une plage de pages :** ajoutez un argument optionnel à `read_text` :

```python
async def read_text(
    self,
    path: Annotated[str, "Path to the .pdf file."],
    page_range: Annotated[
        str | None,
        "1-based page selection, e.g. '1-5,7'. Omit to read all pages.",
    ] = None,
    ctx: Ctx = None,
) -> dict:
    ...
```

Le schéma JSON l'expose comme paramètre optionnel avec sa description.

**Renvoyer plus d'erreurs typées :** chiffrez les modes d'échec connus :

```python
if file_too_big(path):
    raise DomainError("FILE_TOO_LARGE", f"{path} > 100 MB")
if encrypted(path):
    raise DomainError("ENCRYPTED", "Cannot read encrypted PDFs")
```

L'appelant peut brancher sur `result["error"]["code"]` pour proposer une remédiation.

**Tester unitairement :** voir [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md) pour `apollia.testing.mock(PdfQuickstart)` qui rend l'agent appelable depuis pytest sans démarrer le runtime.

---

## Prochaines étapes

- **Director :** [chapitre 4](04-quickstart-director.md), orchestrer ce worker depuis un agent qui fait du raisonnement multi-étapes.
- **Patterns LLM-friendly :** [Partie IV](../part-iv-llm-friendly-design/19-annotated-descriptions.md), pourquoi `Annotated` et `examples` changent la qualité des appels d'un LLM mid-market.
- **TypedDict canon :** [chapitre 21](../part-iv-llm-friendly-design/21-typeddict-schemas.md), comment passer de `dict` à un schéma structurellement strict.
