# Apollia Python SDK

> **Apollia AgentKit v0.1.0-preview** - Python toolkit for building agents that run on [Apollia OS](https://github.com/Apollia-OS/apollia-os), the Rust runtime for sovereign, local-first AI agent execution.

The SDK is **decorator-first**: an agent is a Python class decorated with `@agent`, with methods marked `@skill`, `@on_message`, or `@orchestrated`. The runtime introspects the class, generates the manifest from your code, validates payloads from your function signatures, and wires `ctx` - a typed runtime context exposing **15 backend services** (LLM, memory, tools, A2A, mail, datasources, templates, secrets, events, logger, profile, workspace, STT, notify, budget).

**Design philosophy** :
- Signature **is** the schema (type hints → JSON Schema → runtime validation)
- Errors are **typed exceptions** trapped by the SDK at the dispatch boundary
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
# 0.1.0-preview
```

## Quickstart - four canonical patterns

### 1. Conversational agent

```python
from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="apollia-guide",
    version="0.1.0",
    description="Friendly product coach for new Apollia users",
)
class ApolliaGuide:
    SYSTEM_PROMPT = "You are a helpful product coach..."

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        full = ""
        async for token in ctx.llm.stream(
            messages=[
                {"role": "system", "content": self.SYSTEM_PROMPT},
                *history,
                {"role": "user", "content": message},
            ]
        ):
            ctx.events.emit_token(token)
            full += token
        return full
```

### 2. Multi-skill worker

```python
from apollia import agent, skill, DomainError
from apollia.types import Ctx


@agent(
    name="pdf-worker",
    version="0.1.0",
    description="Read, extract, and parse PDF files",
    packages=["pypdf>=4.0"],
    tags=("file", "pdf"),
    agent_type="worker",
)
class PdfWorker:
    @skill("pdf.read_text", description="Extract text content from a PDF")
    async def read_text(
        self,
        path: str,
        page_range: str | None = None,
        max_chars_per_page: int = 100_000,
        ctx: Ctx = None,
    ) -> dict:
        import os
        from pypdf import PdfReader

        if not os.path.exists(path):
            raise DomainError("FILE_NOT_FOUND", f"PDF not found: {path}")

        reader = PdfReader(path)
        text = "\n".join(p.extract_text()[:max_chars_per_page] for p in reader.pages)
        return {"text": text, "pages": len(reader.pages)}
```

The SDK:
- Generates a JSON Schema from your signature (`path: str` → required string, `max_chars_per_page: int = 100_000` → optional integer)
- Validates the incoming payload against it
- Catches `DomainError` and converts it to `AIPResult.failed(code, message)` automatically
- Wraps your return dict in `AIPResult.completed(...)`

### 3. Director using ReAct + A2A

```python
from apollia import agent, on_message, react
from apollia.types import Ctx, Message


@agent(
    name="veille-ia",
    version="0.2.0",
    description="Competitive intelligence director",
)
class VeilleIA:
    SYSTEM_PROMPT = "You orchestrate research workers..."

    @on_message
    async def run_cycle(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return await react(
            ctx,
            system=self.SYSTEM_PROMPT,
            user=message,
            tools=[
                await ctx.a2a.skill_as_tool("research.search_and_extract"),
                await ctx.a2a.skill_as_tool("research.extract_entities"),
                await ctx.a2a.skill_as_tool("research.synthesize_report"),
            ],
            max_steps=15,
        )
```

### 4. Orchestrated agent (ORIA-piloted)

```python
from apollia import agent, orchestrated


@agent(name="email-triage", version="0.1.0", description="Sort and route emails")
@orchestrated(system_prompt="You triage incoming emails using available tools...")
class EmailTriage:
    async def on_plan_complete(self, step_results: dict[str, str], ctx) -> str:
        return "\n\n".join(text for text in step_results.values() if text)
```

The ORIA engine (Rust) generates and executes a plan from the system prompt. The agent only provides metadata + optional post-processing.

## The `Ctx` Protocol

Every handler receives `ctx: Ctx`. The 15 services:

| Service | Type | Use |
|---|---|---|
| `ctx.llm` | `LlmProxy` | `complete`, `chat`, `stream` (async iterator), `map`, `run_tools` |
| `ctx.memory` | `MemoryInterface` | episodic / semantic / procedural + `export` / `import_data` |
| `ctx.tools` | `ToolProxy` | `call(name, input)`, `describe(name)`, `list_tools()` |
| `ctx.a2a` | `A2AInterface` | `invoke(skill_id)`, `discover`, `list_skills`, `skill_as_tool` |
| `ctx.mail` | `MailInterface` | `send`, `receive`, `poll`, `pending`, `list`, `ack`, `nack` - durable inbox, at-least-once |
| `ctx.datasources` | `DatasourcesInterface` | `get(name)` - runtime YAML access |
| `ctx.templates` | `TemplatesInterface` | `render(name, **vars)` - Jinja2 |
| `ctx.secrets` | `SecretsInterface` | `get(key)` - read-only credentials, gated by manifest |
| `ctx.events` | `EventsInterface` | `emit_token`, `emit_thought`, `emit_retry`, `emit_action_parse_error` |
| `ctx.logger` | `logging.Logger` | piped to Rust `tracing` |
| `ctx.profile` | `ProfileInterface` | canonical user profile (read/write gated) |
| `ctx.workspace` | `WorkspaceContext` | `APOLLIA.md`, git, custom sections |
| `ctx.stt` | `SttInterface` | `transcribe(path)` |
| `ctx.notify` | `NotifyInterface` | desktop / webhook notifications |
| `ctx.budget` | `BudgetView` | `steps_remaining`, `tool_calls_remaining`, `elapsed_seconds`, `wall_clock_remaining` |

All typed via `typing.Protocol` - IDE autocomplete works everywhere, `mypy --strict` passes.

## Error model

Raise typed exceptions; the SDK traps them at the dispatch boundary:

```python
from apollia import DomainError, NeedHumanInput

# Business error → AIPResult.failed(code, message, details)
raise DomainError("FILE_TOO_LARGE", "File exceeds 100MB", details={"size_mb": 153})

# HITL suspension → AIPResult.input_required(prompt, context)
raise NeedHumanInput("Approve processing this 100MB file?", context={"path": "/tmp/big.pdf"})
```

You never construct `AIPResult` yourself - it's internal to the SDK.

## Manifest declarations

`@agent` accepts:

```python
@agent(
    name="...",                      # required
    version="...",                   # required, semver
    description="...",               # required
    packages=("pypdf>=4",),          # PyPI deps installed in agent venv
    tags=("file", "pdf"),            # discovery
    datasources=("competitors",),    # YAML files in <agent_dir>/datasources/
    templates=("report",),           # Jinja2 files in <agent_dir>/templates/
    secrets=("brave_api_key",),      # credentials accessible via ctx.secrets
    tools_required=("file_read",),   # native tools the agent will call
    memory_namespace="my-agent",
    user_memory_write=False,         # gate for ctx.profile.set()
    step_budget={"max_steps": 30, "max_tool_calls": 50, "wall_clock_secs": 300},
    agent_type="worker",             # worker / assistant / system
)
class MyAgent:
    ...
```

`apollia inspect path/to/agent.py` shows the generated manifest before running.

## Documenting parameters with `Annotated`

Skills exposed via A2A are seen by LLM callers (Chat Libre, director ReAct loops, other workers) as **tools**. The richer the JSON Schema description, the higher the chance a mid-market LLM (Mistral Small, Haiku, Llama 70B) builds a valid payload on the first try.

`typing.Annotated[T, "description"]` is the canonical way to document a parameter. The SDK introspects the second argument and propagates it into `input_schema.properties[param].description` - visible to every LLM that calls the skill as a tool.

```python
from typing import Annotated

from apollia import agent, skill
from apollia.types import Ctx


@agent(name="chart-worker", version="0.1.0", description="Render charts to PNG/SVG.", agent_type="worker")
class ChartWorker:
    @skill("chart.bar", description="Render a bar chart.")
    async def bar(
        self,
        series: list[dict],  # see "Structured payloads with TypedDict" below
        format: Annotated[str, "'png' (default, raster) | 'svg' (vector)."] = "png",
        orientation: Annotated[
            str,
            "'vertical' (bars rise from baseline) | 'horizontal' (bars extend right).",
        ] = "vertical",
        dpi: int = 150,  # trivial numeric - no Annotated, keeps signature readable
        ctx: Ctx = None,
    ) -> dict:
        ...
```

Skip `Annotated` for trivial numerics/booleans (`dpi: int = 150`, `overwrite: bool = False`) - keep the signature readable.

## Providing examples for the LLM

`@skill(examples=[{...}])` attaches one or more **payload templates** to the skill. The SDK propagates them to the tool descriptor LLM-facing - the LLM sees not only the JSON Schema but also a concrete, valid call shape.

```python
@skill(
    "pdf.read_text",
    description="Extract text from a PDF, optionally limited to a page range.",
    examples=[
        {"path": "/tmp/report.pdf"},                              # minimal
        {"path": "/tmp/report.pdf", "page_range": "1-10"},        # with range
    ],
)
async def read_text(
    self,
    path: str,
    page_range: str | None = None,
    ctx: Ctx = None,
) -> dict:
    ...
```

Guidelines:
- At least 1 realistic example per skill - must cover every `required` field.
- Demonstrate the exact structure of complex parameters (`list[TypedDict]`, nested dicts).
- The SDK does **not** validate examples against the inferred schema - author responsibility to keep them in sync.

`apollia inspect <agent.py> --json` shows `manifest.skills[].examples` so you can confirm propagation.

## Structured payloads with TypedDict

When a parameter is a complex structure (`list[dict[str, Any]]`, nested config), the inferred JSON Schema is just `object` / `array of object` - opaque, no `properties`, no `required`. LLMs guess the shape and frequently get it wrong.

Replace `list[dict[str, Any]]` with a `TypedDict` declared in a sibling `schemas.py`. The SDK introspects the TypedDict and produces a **structurally strict** sub-schema (`properties` + `required` + sub-types).

```python
# schemas.py - DO NOT add `from __future__ import annotations` !
from typing import Literal, NotRequired, TypedDict


BarOrientation = Literal["vertical", "horizontal"]


class BarSeries(TypedDict):
    """One bar group in a bar chart."""
    name: str
    data: list[float]
    color: NotRequired[str]  # optional, hex #RRGGBB
```

```python
# chart-worker.py
from schemas import BarSeries  # type: ignore[import-not-found]


@skill("chart.bar", description="...", examples=[...])
async def bar(self, series: list[BarSeries], ctx: Ctx = None) -> dict:
    ...
```

**Why no `from __future__ import annotations` in `schemas.py`** - PEP 563 turns all annotations into strings at class creation, which breaks `TypedDict.__required_keys__` (every field becomes "required"). The SDK uses `__required_keys__` to compute the `required: [...]` array of the JSON Schema, so under PEP 563 the schema lies about which fields are mandatory. Keep `schemas.py` free of `from __future__ import annotations`; the worker `.py` can still use it freely (only the TypedDict definitions are sensitive).

Single source of truth: the TypedDict documents the contract for the LLM (via the schema), for callers (via type hints), and for tests / eval cases. No drift between schema, code, and docs.

## Testing - isomorphic mocks

```python
import pytest
from apollia.testing import mock, assert_result_completed, assert_skill_called

from pdf_worker import PdfWorker  # your agent class


@pytest.mark.asyncio
async def test_read_text_happy_path(tmp_path):
    # Create a tiny PDF fixture
    pdf = tmp_path / "test.pdf"
    pdf.write_bytes(b"%PDF-1.4\n...minimal pdf...")

    agent, ctx = mock(PdfWorker)
    result = await agent.invoke_skill("pdf.read_text", path=str(pdf))

    assert_result_completed(result)
    assert ctx.events.tokens == []  # no streaming for this skill


@pytest.mark.asyncio
async def test_read_text_missing_file():
    agent, ctx = mock(PdfWorker)
    result = await agent.invoke_skill("pdf.read_text", path="/nonexistent.pdf")
    assert result["status"] == "failed"
    assert result["error"]["code"] == "FILE_NOT_FOUND"
```

`mock(AgentClass)` returns `(instance, ctx)` where:
- `instance.invoke_skill(skill_id, **kwargs)` bypasses runtime dispatch
- `ctx` is a `MockContext` implementing 14 of the 15 surfaces (`ctx.mail` has no mock yet) - pre-configure via `ctx.llm.responses = [...]`, `ctx.datasources.values = {...}`, `ctx.secrets.values = {...}`, etc.

## CLI

```bash
# Inspect an agent module before running it
python -m apollia inspect agents/examples/hello/agent.py
python -m apollia inspect agents/examples/hello/agent.py --json

# Scaffold a new agent
python -m apollia new my-agent --type worker
```

## Architecture references

- **The type contract** in `apollia/types.py` and `apollia/context/` - the
  authority on what `ctx` offers an agent at runtime
- **Design decisions** in the architecture chapter of the documentation site
- **Example agents** in `agents/examples/` - the shape a working agent takes
- **Scaffolding**: `python -m apollia new <name> --type worker` generates a
  starting point that already satisfies the minimal contract

## Project structure

```
sdk/
├── pyproject.toml
├── README.md          (this file)
└── apollia/
    ├── __init__.py    # public API : agent, skill, on_message, orchestrated, react, errors
    ├── agent.py       # @agent decorator
    ├── skills.py      # @skill decorator
    ├── messages.py    # @on_message decorator
    ├── orchestration.py  # @orchestrated decorator
    ├── react.py       # apollia.react(ctx, ...) utility
    ├── errors.py      # DomainError, NeedHumanInput, PayloadError, ...
    ├── types.py       # Ctx Protocol, vision types, helpers
    ├── context/       # 15 Protocol surfaces (llm, memory, tools, a2a, mail, ...)
    ├── _internal/     # dispatch, inference, manifest, aip_result, logger_bridge
    ├── testing/       # mock(), MockContext, assertions
    ├── cli/           # apollia inspect, apollia new
    └── utils/         # parsing, formatting, assertion helpers (legacy compat)
```

## License

Apache-2.0 OR MIT - see the root repository.
