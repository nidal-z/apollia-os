---
sidebar_position: 4
title: Write a worker
---

# Write a worker

A worker is a domain expert. It exposes one or more A2A skills that any director
can call, and it does one thing well. This guide builds a small PDF worker with
two skills, reading text and counting pages, then invokes it from the command
line.

This is a how-to, not a tutorial. It assumes you have already installed Apollia
and written [your first agent](/tutorials/your-first-agent).

## Declare the agent and its skills

A worker is an `@agent` class whose entry points are `@skill` methods. Each skill
is an `async def`, its `skill_id` is dot-namespaced snake case, and its
parameters (everything except `self` and `ctx`) become the skill's input schema.

Create `pdf_worker.py`:

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

Notes on the contract:

- `@skill("pdf.read_text", ...)` takes the `skill_id` first, then three
  keyword-only arguments: `description`, `dangerous` (marks the skill as
  potentially destructive), and `examples`, an optional list of sample payload
  dicts. That is the whole decorator surface. The
  [SDK / ctx contract](/reference/sdk) indexes the `ctx` services a skill body
  uses.
- Domain parameters come first; `ctx` is the last parameter. Annotate each
  parameter with `Annotated[type, "description"]` so the generated schema is
  self-documenting.
- Return a plain `dict`. The runtime wraps it into a completed result.
- Raise `DomainError("CODE", "message")` for expected failures. The dispatcher
  turns it into a typed failed result instead of a crash.
- `packages=(...)` declares third-party dependencies the worker needs. Workers
  are stdlib-only by default; declare a package only when you truly need it.
- End the module with `agent = PdfQuickstart()`, and use absolute imports.

This worker calls no `ctx` service: it is pure Python plus `pypdf`. Workers reach
`ctx` only when they need generation, memory, tools, or secrets.

## Install and invoke

Inspect, install, and enable exactly as for any agent:

```bash
apollia-os inspect pdf_worker.py
apollia-os agent install ./pdf_worker.py
apollia-os agent enable pdf-quickstart
```

Then call a skill directly with `a2a invoke`, passing the payload as JSON:

```bash
apollia-os a2a invoke pdf.count_pages --args '{"path": "/tmp/some.pdf"}'
```

Add `--json` for the full machine-readable result. To see every skill exposed by
active workers, run `apollia-os a2a skills`. Every `a2a` subcommand is in the
[CLI reference](/reference/cli).

## Variation: an optional parameter

Because `ctx` is a required parameter, any parameter with a default value must
come after it:

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

## Variation: more typed errors

Give callers precise codes to branch on:

```python
if file_too_big(path):
    raise DomainError("FILE_TOO_LARGE", f"{path} exceeds 100 MB")
if encrypted(path):
    raise DomainError("ENCRYPTED", "Cannot read encrypted PDFs")
```

A caller reads `result["error"]["code"]` and reacts accordingly, rather than
parsing a message string.

## Test your worker

Apollia ships an isomorphic testing harness, `apollia.testing`, so skills run
in-process with a mocked `ctx` and no daemon. See
[Test your agents](/how-to/test-your-agents).

## Next steps

- Have an agent call several workers:
  [Write a director](/how-to/write-a-director).
- Read the input and output shapes of [`ctx.tools`](/reference/sdk/tools),
  [`ctx.secrets`](/reference/sdk/secrets), and the other services a richer worker
  uses.
