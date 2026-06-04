# sdk/AGENTS.md

> Local rules for the Apollia AgentKit SDK. Read after `docs/agents/INDEX.md`
> and `docs/agents/PYTHON-PATTERNS.md` before editing this subtree.

The SDK is the contract every Apollia agent depends on. A change here
propagates to every agent in the wild. Stability and clarity are
non-negotiable.

Authoritative ADRs : 098-112 (the 2026-05-19 SDK redesign).

---

## 1. Decorator inventory

| Decorator | Level | Purpose |
|---|---|---|
| `@agent(...)` | class | declare the agent identity and manifest metadata |
| `@skill(skill_id, description, examples=[...])` | async method | LLM-callable capability |
| `@on_message(event_kind)` | async method | EventBus subscriber |
| `@orchestrated(...)` | async method | ORIA-driven multi-step skill |

Rules :
- `@agent` wraps the class. `@skill`, `@on_message`, `@orchestrated` wrap
  async methods. Never wrap a non-async method.
- A method carries at most one of `@skill` / `@on_message` /
  `@orchestrated`. Validation runs at registration with
  `AgentConfigError`. Fail fast.
- Order : `@agent` first (class), then method decorators. The class
  attribute `__apollia_manifest__` is built by `@agent` and read at
  registration.

---

## 2. The contract minimal

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
        examples=[{
            "to": "ops@apollia.fr",
            "subject": "service down",
            "body": "...",
        }],
    )
    async def triage(self, ctx: Ctx, payload: EmailPayload) -> EmailResult:
        ...

agent = EmailTriage()
```

Mandatory shape :
- Module-level `agent = MyClass()` for the runtime to import.
- Every `@skill` carries at least one realistic `examples=[{...}]` payload.
- Every parameter that is LLM-facing and not a trivial scalar carries
  `Annotated[T, "description"]`.
- TypedDict canonical schemas live in `<agent>/schemas.py`.

---

## 3. `Ctx` protocol (ADR-024)

`Ctx` is the capability bundle the runtime injects. The skill body
interacts with the world only through `ctx`. The stubs in
`sdk/apollia/stubs/` are the authoritative type contract.

```python
class Ctx(Protocol):
    def log(self, level: Literal["debug", "info", "warn", "error"], message: str, **fields: Any) -> None: ...

    memory: MemoryCtx
    tool: ToolCtx
    secrets: SecretsCtx
    config: ConfigCtx
    input: InputCtx
```

Sub-protocols :

| Sub-protocol | Methods |
|---|---|
| `MemoryCtx` | `recall(query, limit=10)`, `write(...)`, `forget(...)` |
| `ToolCtx` | `invoke(name, **args)`, `list_available()` |
| `SecretsCtx` | `read(key)` (read-only, ADR-024) |
| `ConfigCtx` | `workspace()`, `profile()` |
| `InputCtx` | `next()` (awaits operator response after `NeedHumanInput`) |

Adding a method to `Ctx` :
1. Update the stub in `sdk/apollia/stubs/`.
2. Implement in `crates/apollia-aip/`.
3. Open or update the relevant ADR (101-104, 111).
4. Update the Wiki reference for `Ctx`.

The stub is the contract. If implementation and stub diverge, the stub
wins and the implementation is broken.

---

## 4. TypedDict schemas (ADR-023, ADR-024)

Schemas live in `<agent>/schemas.py`. The file must NOT contain
`from __future__ import annotations`. PEP 563 turns annotations into
strings and breaks `TypedDict.__required_keys__` that AgentKit reads at
registration.

```python
# my_agent/schemas.py
# NO `from __future__ import annotations`

from typing import NotRequired, TypedDict, Literal

class EmailPayload(TypedDict):
    to: str
    subject: str
    body: str
    cc: NotRequired[list[str]]
    priority: NotRequired[Literal["low", "normal", "high"]]

class EmailResult(TypedDict):
    classified_as: Literal["urgent", "normal", "spam"]
    confidence: float
    rationale: str
```

Rules :
- TypedDict inheritance only when you genuinely want field union.
- `NotRequired[T]` is preferred over `total=False` when most fields are
  required and a few are optional.
- Validation : `python -m apollia inspect <agent.py> --json` must show
  descriptions on every param and examples on every skill.

---

## 5. Exceptions (ADR-023)

Hierarchy rooted at `AgentError` :

```
AgentError
├── DomainError       # business-level failure, retry meaningless
├── PayloadError      # malformed input, fail-fast to caller
├── NeedHumanInput    # captured by dispatcher, returns input_required
└── AgentTimeoutError # boundary wrap of timeouts
```

Rules :
- Never subclass `Exception` directly for new error types. Subclass
  `AgentError`.
- Each subclass carries a stable `.code` attribute (`TIMEOUT`,
  `FILE_NOT_FOUND`, `RATE_LIMITED`, ...).
- `NeedHumanInput(question, schema)` is the only exception that resumes
  the task. All others terminate the skill invocation.

---

## 6. Datasources and templates (ADR-024)

The runtime loads datasources and templates declared in the agent
TOML (`[datasources]`, `[templates]`). They are exposed read-only via
`ctx.config.workspace().datasources` and `.templates`.

Rules :
- Agents never write to datasources. The runtime owns the lifecycle.
- Templates are Jinja2-style strings with limited filters (`upper`,
  `lower`, `length`). Custom filters require an ADR.

---

## 7. `@orchestrated` semantics

Marks a skill that ORIA drives step-by-step (LLM in the loop). The
agent body is a single function; ORIA decides when to call tools, when
to ask the LLM, when to stop.

Requires `[llm.routing]` precise in the agent TOML (ADR feedback from
2026-05-21). The router needs a deterministic backend per orchestrated
skill.

`cache_plan=True` opt-in for cacheable plans (see
`crates/apollia-oria/AGENTS.md` §4).

---

## 8. Forbidden in this subtree

- `from __future__ import annotations` in any module with TypedDict.
- Relative imports (`from .module import X`).
- Module-level side effects beyond the `agent = MyClass()` instantiation.
- Adding third-party dependencies (the SDK is stdlib-only except for
  the documented `pyo3` runtime bridge).
- Decorator stacking that breaks the validation (multiple of
  `@skill` / `@on_message` / `@orchestrated` on one method).
- `print()` (use `ctx.log(...)`).
- Catching `CancelledError` without re-raising.

---

## 9. Stubs synchronization

`sdk/apollia/stubs/` carries the type contract for the runtime-injected
`Ctx` and related capabilities. The stubs are hand-maintained alongside
the Rust implementation in `crates/apollia-aip/` and committed for
reproducibility and IDE support.

Edit policy :
- Allowed when adding a new method that does not exist on the Rust side
  yet (drafting).
- When implementing the Rust side, update the stub in the same PR to
  keep parity.
- Stub and implementation must agree on signature and semantic.
  Discrepancy : the stub is the contract, fix the implementation.

---

## 10. Testing

- `pytest` with `asyncio_mode = "strict"`.
- Each new decorator carries unit tests that exercise the validation
  paths (`AgentConfigError` on conflicting decorators, etc.).
- `python -m apollia inspect <agent.py> --json` is a smoke test : run
  it against every example agent in CI.
- See `docs/agents/TESTING.md` §6.

---

## 11. Documentation responsibilities

| Change | Update |
|---|---|
| New decorator | `sdk/README.md`, this file, Wiki reference, Book chapter |
| New `Ctx` method | stub + Wiki reference + ADR (101-104, 111) |
| New AgentError subclass | this file (§5), Wiki reference, `apollia-aip` dispatcher |
| TypedDict schema convention change | this file (§4), book chapter, ADR if breaking |

---

## 12. When the rules block you

- Need a new decorator : open an ADR. Decorators are contract.
- Need to add a runtime dependency to an agent : open an ADR. Each one
  is a sovereignty surface.
- Need to break compatibility (rename a decorator, change a schema
  field semantics) : open an ADR, ship under a feature flag with
  documented sunset.
