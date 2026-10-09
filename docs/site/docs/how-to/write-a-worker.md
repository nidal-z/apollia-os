---
sidebar_position: 4
title: Write a tool
---

# Write a tool

A tool is the Python a package brings to its agents: one `async def` function
under `@tool`, which an agent calls when its mission needs it. The agent itself
is not code. It is declared in the `apollia.toml` of the package, with its
mission and the tools it may reach. This guide writes a package with one agent
and one tool that counts the pages of a PDF, then runs it.

This is a how-to. It assumes Apollia is installed and its runtime is running;
if not, see [Install and run the runtime](/how-to/install-and-run).

## Start from the template

```bash
apollia-os package new pdf-reader
```

The command writes a folder that builds as it is:

```text
pdf-reader/
  apollia.toml
  tools/pdf_reader.py
```

`apollia.toml` declares the package, one agent and one tool. `tools/` holds the
Python module of the tool.

## Write the function

Replace `tools/pdf_reader.py` with the tool:

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

What the decorator takes, and nothing else:

| Argument | Meaning |
| --- | --- |
| `name` | Identifier of the tool, `[a-z0-9_-]+`, unique in the package. |
| `description` | What the tool does. The model reads it to decide when to call the tool, and the sheet of the agent shows it. |
| `action` | The action family, keyword-only and required: `read`, `read_personal`, `write_or_delete`, `send_external` or `execute_code`. |
| `examples` | Optional list of sample inputs, each a dict. |

Notes on the contract:

- The function is `async def`. Anything else is refused when the module is
  imported, with an `AgentConfigError` that names the tool.
- The inputs are the parameters of the function, `ctx` excluded. Annotate each
  one with `Annotated[type, "description"]`: the input schema the model sees is
  inferred from the signature.
- Return a value that serialises to JSON, a `dict` in most cases.
- Raise `DomainError("CODE", "message")` for an expected failure. The call
  fails, and the agent reads the error and decides what to do next.
- The action family says what the call does to the world, and every family
  except `read` makes the tool sensitive. There is no separate flag: declare
  the family that says what the tool really does. The families are listed in
  the [package manifest reference](/reference/package-manifest#action-families).
- Use absolute imports, and `ctx.logger` instead of `print()`.

## What `ctx` gives a tool

A tool runs with a bare context: `ctx.logger`, which reports under the name of
the agent that called it. It carries no memory and no user profile, and
`ctx.llm` and `ctx.tools` are `None`: reasoning and calling other tools belong
to the agent, which runs the loop. A tool does one thing and returns.

## Describe the tool in the manifest

The manifest describes every tool, so that Apollia knows what a package ships
without running it. Write the description from the code:

```bash
python -m apollia describe pdf-reader
```

The command imports the modules of the folder and rewrites the `[[tools]]`
entries of `apollia.toml`: name, module, description, action family and input
schema. Everything else in the manifest is kept as written. It needs the
`apollia` SDK, and whatever your modules import at their top level, in the
Python environment that runs it.

Then edit the rest of `apollia.toml` by hand. Grant the tool to the agent, and
declare the dependency:

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

A tool is granted under the name `pkg:<package>/<tool>`. The same list takes
native tools by their name and MCP tools as `mcp:<server>/<tool>`. Every table
and key is in the [package manifest reference](/reference/package-manifest).

## Build, install and run

```bash
apollia-os package build pdf-reader
apollia-os package install pdf-reader-0.1.0.apo --confirm-deps
```

`package build` validates the manifest and writes the archive without running
any Python. `package install` needs the runtime; `--confirm-deps` accepts the
Python dependencies the package installs. Then send the agent a request:

```bash
apollia-os run pdf-reader "How many pages are in /tmp/report.pdf?"
```

The agent answers by calling `count_pages`. Because the family is
`read_personal`, the call is a sensitive one: you are asked before it runs,
unless you already authorised it.

## Variation: an optional input

A parameter with a default value is an optional input:

```python
async def count_pages(
    ctx: Ctx,
    path: Annotated[str, "Absolute path of the .pdf file."],
    skip_blank: Annotated[bool, "Leave blank pages out of the count."] = False,
) -> dict[str, Any]:
    ...
```

Run `python -m apollia describe pdf-reader` again after any change to a
signature or to a decorator, so that the manifest keeps saying what the code
does. `package build` refuses a folder that defines a tool the manifest does
not describe.

## Next steps

- Chain several steps in Python, with agents doing the reasoning:
  [Write a workflow](/how-to/write-a-director).
- Unit-test the function with a mocked context:
  [Test your tools and workflows](/how-to/test-your-agents).
- Sign and share the archive:
  [Package and distribute](/how-to/package-and-distribute-an-agent).
