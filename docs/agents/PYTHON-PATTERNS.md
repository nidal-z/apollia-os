# PYTHON-PATTERNS

> Rules for any Python change in `sdk/` and `agents/`. Read this before editing.
> Pair with `sdk/AGENTS.md` for SDK-internal patterns and the relevant
> `agents/<name>/AGENTS.md` if present.

Apollia is stdlib-only by default. Every third-party dependency in an agent or
worker is a sovereignty surface and a maintenance liability. Justify each one
in an ADR.

---

## 1. Typing

**Never `from __future__ import annotations` in any module that defines
`TypedDict`.** PEP 563 turns annotations into strings, which breaks
`TypedDict.__required_keys__` consumed by AgentKit to build skill schemas at
registration time. The agent will be silently malformed.

**TypedDict for agent payload contracts.** Canonical schemas live in
`<agent>/schemas.py` per agent. Use `total=False` or `NotRequired[T]` for
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
    self,
    to: Annotated[str, "RFC 5322 email address of the recipient"],
    priority: Annotated[Literal["low", "normal", "high"], "delivery priority"],
    body: str,
) -> EmailResult: ...
```

**Adopt PEP 695 syntax** (`type X = ...`, `class Foo[T]:`) on Python 3.12+
(SDK target).

**Use `typing.Self` for fluent return types**, `typing.override` on overridden
methods (PEP 698), `LiteralString` for params that must be literal
(injection safety).

**Type check** : `pyright --strict` on `sdk/` and `agents/`. `mypy --strict`
is acceptable in repos that depend on Django or SQLAlchemy plugins. Apollia
defaults to `pyright`.

---

## 2. The Apollia AgentKit decorators

`@agent` is class-level. `@skill`, `@on_message`, `@orchestrated` are
method-level. Mutual exclusion is validated fail-fast at registration with
`AgentConfigError`. Order matters : `@agent` must wrap the class before any
method decorator is read.

```python
from apollia import agent, skill
from apollia.types import Ctx

from my_agent.schemas import EmailPayload, EmailResult

@agent(
    name="email-triage",
    version="0.1.0",
    description="Triages incoming emails into priority buckets.",
)
class EmailTriage:
    @skill(
        skill_id="triage",
        description="Score an email by priority.",
        examples=[{"to": "ops@apollia.fr", "subject": "down", "body": "..."}],
    )
    async def triage(self, ctx: Ctx, payload: EmailPayload) -> EmailResult: ...

agent = EmailTriage()
```

Rules :

- Module-level `agent = MyClass()` is mandatory in every `.py` agent file.
  The runtime imports this symbol.
- Every `@skill` carries at least one realistic `examples=[{...}]` payload.
  AgentKit propagates them into the LLM tool descriptor.
- `@orchestrated` requires `[llm.routing]` precise in the agent TOML
  (sovereign config, see ADR-... and the SDK book).
- Never decorate a non-async method. Skills are always `async def`.
- Never share mutable state across skill invocations on `self`. Treat the
  agent as request-scoped.

See `sdk/AGENTS.md` for full validation rules and references to ADRs
098-112.

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
AgentError
├── DomainError       # business-level failure, retry is meaningless
├── PayloadError      # malformed input, fail-fast to the caller
├── NeedHumanInput    # captured by the dispatcher, returns AIPResult.input_required
└── AgentTimeoutError # wrap timeouts at the boundary
```

Rules :

- Never subclass `Exception` directly for new error types. Subclass
  `AgentError` so the dispatcher can map to `AIPResult`.
- Stable error codes (`TIMEOUT`, `FILE_NOT_FOUND`, `RATE_LIMITED`). Document
  the codes that the calling director may want to match on.
- Never raise inside cleanup code. Wrap in `try/except` and log.
- The dispatcher translates each `AgentError` subclass into a specific
  `AIPResult` shape. See `sdk/apollia/_internal/dispatch.py`.

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
- `print()` is forbidden in agents and SDK. Use `ctx.log(...)` from the
  RuntimeContext, or the stdlib `logging` module routed to the runtime
  tracer.
- Format : Ruff format (replaces Black). `target-version = "py312"`,
  `line-length = 100`.
- Lint : Ruff with `select = ["ALL"]` and an ignore-list documented in
  `pyproject.toml`. See `docs/agents/CI-TOOLING.md`.

---

## 7. Testing

- pytest with `asyncio_mode = "strict"`. Each async test carries
  `@pytest.mark.asyncio` explicitly. Reason : prevents collision with
  `trio` / `anyio` runners and surfaces forgotten markers.
- Declared markers : `unit`, `integration`, `slow`. Run `pytest -m "not slow"`
  during development, full suite in CI.
- Hypothesis for property-based testing.
- `syrupy` for snapshot testing.
- `respx` for HTTP mocks.
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
- Type stubs `.pyi` for the runtime-injected `Ctx` are committed in
  `sdk/apollia/stubs/`.
- Local install for development : `pip install -e .` (or
  `uv pip install -e .`) from the SDK directory.
- Version bump : both `sdk/pyproject.toml` and the workspace `Cargo.toml`.

---

## 9. When the rules block you

Document the exemption inline (`# REASON:` comment, ADR reference) or
surface the conflict before silently bending the rule. Adding a Python
dependency without an ADR is the most common temptation here. Resist.
