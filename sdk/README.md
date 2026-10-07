# Apollia Python SDK

> **Apollia AgentKit v0.2.0-preview** - the Python a package brings to [Apollia OS](https://github.com/Apollia-OS/apollia-os), the Rust runtime for sovereign, local-first AI agent execution.

An agent is data: its mission, its examples and what it may reach are written in the `apollia.toml` of a package. No Python stands for an agent. The Python of a package brings two things, and the SDK offers one decorator for each:

- `@tool`, an `async def` function an agent can be granted;
- `@workflow`, an `async def (ctx, inputs)` function whose steps are declared ahead.

Both receive `ctx`, a typed runtime context exposing **17 backend services** (LLM, memory, tools, agents, workflows, user, mail, datasources, templates, secrets, events, logger, profile, workspace, STT, notify, budget).

**Design philosophy** :
- Signature **is** the schema (type hints → JSON Schema)
- Errors are **typed exceptions**
- Zero external dependencies (stdlib only)
- `mypy --strict` clean

## Installation

```bash
pip install -e ./sdk
pip install -e "./sdk[dev]"   # + pytest, mypy
```

Verify:

```bash
python -c "import apollia; print(apollia.__version__)"
# 0.2.0-preview
```

## Quickstart

`apollia-os package new invoice-sorter` writes a folder that builds as written: an `apollia.toml` declaring one agent, and one tool under `tools/`.

### A tool

```python
from typing import Annotated, Any

from apollia import DomainError, tool
from apollia.types import Ctx


@tool(
    "extract_totals",
    "Read the totals of an invoice.",
    action="read",
    examples=[{"path": "invoices/2026-09.pdf"}],
)
async def extract_totals(
    ctx: Ctx,
    path: Annotated[str, "path of the invoice"],
    rounding: int = 2,
) -> dict[str, Any]:
    if not path.endswith(".pdf"):
        raise DomainError("NOT_A_PDF", f"not a PDF: {path}")
    ctx.logger.info("reading %s", path)
    return {"total": round(42.0, rounding)}
```

- `name` and `description` are positional; `name` matches `[a-z0-9_-]+` and is unique in the package.
- `action` is one of the five families `apollia.toml` reads: `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code`. Every family except `read` is sensitive. There is no separate flag for sensitivity: the tool declares the family that says what it really does.
- The input schema is inferred from the signature, `ctx` left out: `path` is a required string carrying its description, `rounding` an optional integer.
- `examples` are sample input payloads, shown to the model beside the schema.

### A workflow

```python
from typing import Annotated, TypedDict

from apollia import WorkflowCtx, workflow


class MonthClose(TypedDict):
    folder: Annotated[str, "folder holding the invoices"]


@workflow(
    "close-month",
    "Extract every invoice of a folder and file the totals.",
    inputs=MonthClose,
    steps=["extract", "file"],
    action="write_or_delete",
)
async def close_month(ctx: WorkflowCtx, inputs: MonthClose) -> dict[str, str]:
    async with ctx.step("extract"):
        answer = await ctx.agents.run("accountant", f"totals of {inputs['folder']}")
    async with ctx.step("file"):
        await ctx.workflows.run("file-ledger", {"text": answer})
    return {"answer": answer}
```

- `inputs` is a `TypedDict`; its schema is the launch form. Each input is a type the form knows: `str`, `apollia.LongText`, `apollia.FileInput` (received as the absolute path of the file), `datetime.date`, `int`, `float`, `bool`, `Literal[...]`, or a list of text or numbers. Any other type is refused when the module is imported.
- `steps` is what the card of the workflow shows without running the code. The body opens each step with `async with ctx.step("name")`, which refuses a name `steps` does not declare.
- A step calls `ctx.llm`, `ctx.tools`, `ctx.agents.run(agent_id, message)`, `ctx.workflows.run(workflow_id, inputs)`, `ctx.user.ask(question)` or plain Python. The loop that alternates model calls and tool calls belongs to an agent: a step that needs one hands the message to an agent.

### The manifest

`python -m apollia describe <folder>` imports the modules of the folder and rewrites the `[[tools]]` and `[[workflows]]` tables of its `apollia.toml` from the decorated functions: name, module, description, action family, input schema, and the steps of a workflow. Everything else in the manifest is kept as written, so reading the manifest never runs the code.

## The `Ctx` Protocol

Every tool and every workflow receives `ctx`. The 17 services:

| Service | Type | Use |
|---|---|---|
| `ctx.llm` | `LlmProxy` | `complete`, `chat`, `stream` (async iterator), `map` |
| `ctx.memory` | `MemoryInterface` | episodic / semantic / procedural + `export` / `import_data` |
| `ctx.tools` | `ToolProxy` | `call(name, input)`, `describe(name)`, `list_tools()` |
| `ctx.agents` | `AgentsInterface` | `run(agent_id, message)`, bound in a workflow run |
| `ctx.workflows` | `WorkflowsInterface` | `run(workflow_id, inputs)`, bound in a workflow run |
| `ctx.user` | `UserInterface` | `ask(question, *, options=None, hint=None)`, bound in a workflow run |
| `ctx.mail` | `MailInterface` | `send`, `receive`, `poll`, `pending`, `list`, `ack`, `nack` - durable inbox, at-least-once |
| `ctx.datasources` | `DatasourcesInterface` | `get(name)` - runtime YAML access |
| `ctx.templates` | `TemplatesInterface` | `render(name, **vars)` |
| `ctx.secrets` | `SecretsInterface` | `get(key)` - read-only credentials |
| `ctx.events` | `EventsInterface` | `emit_token`, `emit_thought`, `emit_retry`, `emit_action_parse_error` |
| `ctx.logger` | `logging.Logger` | piped to Rust `tracing` |
| `ctx.profile` | `ProfileInterface` | canonical user profile (read/write gated) |
| `ctx.workspace` | `WorkspaceContext` | `APOLLIA.md`, git, custom sections |
| `ctx.stt` | `SttInterface` | `transcribe(path)` |
| `ctx.notify` | `NotifyInterface` | desktop / webhook notifications |
| `ctx.budget` | `BudgetView` | `steps_remaining`, `tool_calls_remaining`, `elapsed_seconds`, `wall_clock_remaining` |

All typed via `typing.Protocol`, so IDE autocomplete works everywhere and `mypy --strict` passes. The reach of a service depends on the context: each interface module under `apollia/context/` states its own.

## Error model

Raise typed exceptions, all rooted at `AgentError`:

```python
from apollia import DomainError
from apollia.errors import QuestionDeclined

# A business failure, with a stable code the catching code can branch on
raise DomainError("FILE_TOO_LARGE", "File exceeds 100MB", details={"size_mb": 153})

# Inside a workflow step: the user declined the question ctx.user.ask asked
try:
    answer = await ctx.user.ask("Purge the list?", options=["yes", "no"])
except QuestionDeclined as declined:
    ctx.logger.info("declined: %s", declined.reason)
```

A malformed declaration (`@tool` on a function that is not `async def`, an unknown action family, a workflow input of an unsupported type) raises `AgentConfigError` when the module is imported.

## Documenting parameters with `Annotated`

A tool is seen by the model through its JSON Schema. `typing.Annotated[T, "description"]` is the canonical way to document a parameter: the SDK propagates the second argument into `input_schema.properties[param].description`.

```python
async def bar(
    ctx: Ctx,
    format: Annotated[str, "'png' (default, raster) | 'svg' (vector)."] = "png",
    dpi: int = 150,  # trivial numeric - no Annotated, keeps signature readable
) -> dict: ...
```

## Structured payloads with TypedDict

When a parameter is a complex structure, replace `list[dict[str, Any]]` with a `TypedDict`. The SDK introspects it and produces a strict sub-schema (`properties`, `required`, sub-types).

```python
# schemas.py - DO NOT add `from __future__ import annotations` !
from typing import Literal, NotRequired, TypedDict


class BarSeries(TypedDict):
    """One bar group in a bar chart."""
    name: str
    data: list[float]
    color: NotRequired[str]  # optional, hex #RRGGBB
```

**Why no `from __future__ import annotations` in a module that defines a `TypedDict`**: PEP 563 turns all annotations into strings at class creation, which breaks `TypedDict.__required_keys__` (every field becomes "required"). The SDK uses `__required_keys__` to compute the `required` array of the JSON Schema, so under PEP 563 the schema lies about which fields are mandatory.

## Testing

A tool and a workflow are plain `async def` functions. A test builds a `MockContext`, configures its services and awaits the function as written:

```python
import pytest
from apollia.testing import MockContext, assert_tool_called

from tools.invoices import close_month, extract_totals


@pytest.mark.asyncio
async def test_extract_totals_rounds():
    ctx = MockContext()
    result = await extract_totals(ctx, path="invoices/2026-09.pdf", rounding=1)
    assert result == {"total": 42.0}


@pytest.mark.asyncio
async def test_close_month_hands_the_folder_to_the_accountant():
    ctx = MockContext()
    ctx.agents.answers = {"accountant": "42.00"}
    await close_month(ctx, {"folder": "2026-09"})
    assert ctx.agents.run_calls == [("accountant", "totals of 2026-09")]
```

`MockContext` implements every surface but `ctx.mail`. Pre-configure it in place (`ctx.llm.responses = [...]`, `ctx.datasources.values = {...}`, `ctx.secrets.values = {...}`) and assert on the same surfaces. Under a `MockContext` the steps of a workflow report nothing; `ctx.step` still refuses an undeclared name.

## CLI

```bash
# Write the [[tools]] and [[workflows]] of a package's apollia.toml
# from its @tool and @workflow functions
python -m apollia describe path/to/package
```

Writing a new package, building its archive and installing it are commands of the runtime: `apollia-os package new`, `apollia-os package build`, `apollia-os package install`.

## Architecture references

- **The type contract** in `apollia/types.py` and `apollia/context/` - the
  authority on what `ctx` offers at runtime
- **Design decisions** in the architecture chapter of the documentation site

## Project structure

```
sdk/
├── pyproject.toml
├── README.md          (this file)
└── apollia/
    ├── __init__.py    # public API : tool, workflow, WorkflowCtx, errors, Ctx
    ├── tool_decorator.py      # @tool decorator
    ├── workflow_decorator.py  # @workflow decorator and ctx.step
    ├── errors.py      # DomainError, NeedHumanInput, PayloadError, ...
    ├── hitl.py        # typed pauses: questions and approvals
    ├── types.py       # Ctx Protocol, workflow input types, vision helpers
    ├── context/       # 17 Protocol surfaces (llm, memory, tools, agents, workflows, user, mail, ...)
    ├── _internal/     # inference, logger_bridge
    ├── testing/       # MockContext, assertions
    ├── cli/           # apollia describe
    └── utils/         # parsing, formatting, assertion helpers
```

## License

Apache-2.0 OR MIT - see the root repository.
