# PYTHON-PATTERNS

> Rules for any Python change in `sdk/` and `agents/`. Read this before editing.
> Pair with `sdk/AGENTS.md` for SDK-internal patterns and the relevant
> `agents/<name>/AGENTS.md` if present.

Apollia is stdlib-only by default. Every third-party dependency in an agent or
worker is a sovereignty surface and a maintenance liability. State each one in
`docs/site/docs/architecture/08-decisions.md` before adding it.

---

## 1. Typing

**Never `from __future__ import annotations` in any module that defines
`TypedDict`.** PEP 563 turns annotations into strings, which breaks
`TypedDict.__required_keys__` consumed by AgentKit to build the schema of a
tool or of a workflow. The schema will be silently malformed.

**TypedDict for payload contracts.** Canonical schemas live in
`<package>/schemas.py` per package. Use `total=False` or `NotRequired[T]` for
optional fields. Do not subclass `TypedDict` from another `TypedDict` unless
you genuinely want field union semantics.

```python
from typing import NotRequired, TypedDict

class EmailPayload(TypedDict):
    to: str
    subject: str
    body: str
    cc: NotRequired[list[str]]
    attachments: NotRequired[list[str]]
```

**`Annotated[T, "description"]` on every LLM-facing parameter** whose values
are enumerated or whose structure is non-obvious. Skip for trivial scalars
(plain `int`, `bool`, `str` without constraints).

```python
from typing import Annotated, Literal

async def send_email(
    ctx: Ctx,
    to: Annotated[str, "RFC 5322 email address of the recipient"],
    priority: Annotated[Literal["low", "normal", "high"], "delivery priority"],
    body: str,
) -> EmailResult: ...
```

**Do not use PEP 695 syntax** (`type X = ...`, `class Foo[T]:`). The SDK
source floor is Python 3.10 (`requires-python = ">=3.10"`), and ruff's
`target-version = "py310"` exists to stop it proposing 3.12-only generics.
The runtime embeds a 3.12+ interpreter, which is what the installation
prerequisite refers to; the source the SDK ships must still parse on 3.10.

**Use `typing.Self` for fluent return types.** `typing.override` (PEP 698)
and `LiteralString` are 3.11+ and 3.11+ respectively, so they are out of reach
of the 3.10 floor unless imported behind an `ImportError` fallback, the way
`sdk/apollia/_internal/inference.py` imports `Required` / `NotRequired`.

**Type check** : `mypy --strict` on `sdk/apollia`, configured by
`[tool.mypy] strict = true` in `sdk/pyproject.toml` and run from `sdk/`. It is
the only type checker the tree configures or invokes.

---

## 2. The Apollia AgentKit decorators

An agent is data, declared in the `apollia.toml` of a package. The Python of a
package brings tools and workflows, and the SDK has one decorator for each:
`@tool` and `@workflow`. Both wrap a module-level `async def` function, and
both validate fail-fast at import time with `AgentConfigError`.

```python
from typing import Annotated

from apollia import tool
from apollia.types import Ctx

from my_package.schemas import EmailPayload, EmailResult

@tool(
    "triage",
    "Score an email by priority.",
    action="read",
    examples=[{"email": {"to": "ops@apollia.fr", "subject": "down", "body": "..."}}],
)
async def triage(
    ctx: Ctx, email: Annotated[EmailPayload, "the email to score"]
) -> EmailResult: ...
```

Rules :

- **No Python stands for an agent.** There is no class to decorate and no
  module-level `agent` symbol to write. The agent that uses a tool is an
  `[[agents]]` entry of `apollia.toml`.
- Every `@tool` carries at least one realistic `examples=[{...}]` payload.
  AgentKit propagates them into the LLM tool descriptor.
- `action` is the family that says what the code really does (`read`,
  `read_personal`, `write_or_delete`, `send_external`, `execute_code`).
  Sensitivity follows from it; there is no flag to set beside it.
- A `@workflow` lists its `steps` on the decorator and opens each one with
  `async with ctx.step("name")` (see `sdk/AGENTS.md` §7).
- Never decorate a non-async function, and never a method.
- Never keep mutable state at module level across invocations. Treat each
  call as request-scoped.

See `sdk/AGENTS.md` for the full validation rules and the runtime contract.

---

## 3. Async

**Always `asyncio.TaskGroup` for fan-out.** Never `asyncio.gather`. Structured
concurrency, ExceptionGroup (PEP 654), clean cancellation.

```python
async with asyncio.TaskGroup() as tg:
    a = tg.create_task(fetch_one(url_a))
    b = tg.create_task(fetch_one(url_b))
# results available as a.result(), b.result() here
```

**Use `asyncio.timeout(...)`** (3.11+), not `asyncio.wait_for`.

**Re-raise `CancelledError` always.** Never swallow. If cleanup is required
during cancellation, use `with asyncio.shield(cleanup()): ...`.

**`anyio`** is allowed only when interop with `trio` is required, which is
rare. Default to `asyncio`.

---

## 4. Exceptions

Hierarchy rooted at `AgentError` :

```
AgentError            # carries `code`, `message`, `details`
├── DomainError       # business-level failure, retry is meaningless
├── PayloadError      # malformed input, fail-fast to the caller
├── NeedHumanInput    # a pause awaiting a human
├── QuestionDeclined  # the user declined a question `ctx.user.ask` asked
├── SchemaError       # a tool signature that cannot be turned into a schema
└── AgentConfigError  # a `@tool` or `@workflow` declaration is malformed
```

`sdk/apollia/errors.py` is the source. There is no timeout subclass: a timeout
at the boundary is raised as the `asyncio.TimeoutError` it is, and the bridge
maps it. Do not write `AgentTimeoutError`, nothing defines it.

Rules :

- Never subclass `Exception` directly for new error types. Subclass
  `AgentError`.
- Stable error codes (`TIMEOUT`, `FILE_NOT_FOUND`, `RATE_LIMITED`). Document
  the codes that the calling code may want to match on.
- Never raise inside cleanup code. Wrap in `try/except` and log.

---

## 5. Imports

**Absolute imports only.** No relative imports.

```python
# WRONG
from .schemas import EmailPayload
from ..util import normalize

# RIGHT
from my_agent.schemas import EmailPayload
from my_agent.util import normalize
```

Reason : the runtime loads agents into ad-hoc namespaces. Relative imports
fail intermittently depending on load order. Absolute imports are
deterministic.

**Explicit symbol imports.** Never `from typing import *`.

---

## 6. Style

- Docstrings : Google style exclusively. Never NumPy or Sphinx style.
- `print()` is forbidden in agents and SDK. Use `ctx.logger` (a stdlib
  `logging.Logger` routed to the runtime tracer), e.g.
  `ctx.logger.info(...)`.
- Format : Ruff format (replaces Black). `target-version = "py310"`,
  `line-length = 100`.
- Lint : Ruff with a curated `select` list in `sdk/pyproject.toml` (thirteen
  families, not `ALL`) plus a short ignore list. See
  `docs/agents/CI-TOOLING.md`, and read the manifest rather than a copy of
  it: the list moves.

---

## 7. Testing

- pytest with `asyncio_mode = "strict"`. Each async test carries
  `@pytest.mark.asyncio` explicitly. Reason : prevents collision with
  `trio` / `anyio` runners and surfaces forgotten markers.
- Declared markers : `unit`, `integration`, `slow`. Select with
  `pytest -m "not slow"`; there is no `conftest.py`, so the marker selects,
  it does not skip on its own.
- No property, snapshot or HTTP-mocking library is declared. The SDK ships
  `dependencies = []` and its dev extras are pytest, pytest-asyncio,
  pytest-cov, ruff and mypy. Adding one is an ASK FIRST.
- Fixture scoping : prefer `function` > `module` > `session`. `autouse` only
  for project-wide setup.

Detailed conventions in `docs/agents/TESTING.md`.

---

## 8. Packaging

- `pyproject.toml` follows PEP 621. `[project]` declares metadata,
  `[build-system]` selects the backend.
- SDK build backend : `setuptools` (pure-Python package; the Rust runtime is
  a separate binary, not bundled in the wheel).
- `py.typed` marker present in `sdk/apollia/` for PEP 561 typed-package
  status.
- The type contract for the runtime-injected `Ctx` is committed as plain
  `.py` modules : the Protocol in `sdk/apollia/types.py` and the per-service
  interfaces in `sdk/apollia/context/*.py` (no `.pyi` stubs).
- Local install for development : `pip install -e .` (or
  `uv pip install -e .`) from the SDK directory.
- Version bump : both `sdk/pyproject.toml` and the workspace `Cargo.toml`.

---

## 9. When the rules block you

Document the exemption inline (`# REASON:` comment) or surface the
conflict before silently bending the rule. Adding a Python dependency
without stating the decision first is the most common temptation here.
Resist.
