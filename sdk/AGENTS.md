# sdk/AGENTS.md

> Local rules for the Apollia AgentKit SDK. Read after the root `AGENTS.md`
> and `docs/agents/PYTHON-PATTERNS.md` before editing this subtree.

The SDK is the contract every Apollia agent depends on. A change here
propagates to every agent in the wild. Stability and clarity are
non-negotiable.

The authority on what `ctx` guarantees is `sdk/apollia/types.py` together with
`sdk/apollia/context/`. The rationale is in the decisions chapter of the
documentation site.

---

## 1. Decorator inventory

| Decorator | Level | Signature | Purpose |
|---|---|---|---|
| `@agent` | class | `agent(*, name, version, description, ...)`, keyword-only | declare the agent identity and manifest metadata |
| `@skill` | async method | `skill(skill_id, *, description="", dangerous=False, examples=None)` | A2A-exposed, LLM-callable capability |
| `@on_message` | async method | `on_message(fn)`, no arguments | the agent's single conversational handler |
| `@orchestrated` | class | `orchestrated(*, system_prompt)` | hand the execution loop to ORIA |

Two of those signatures are not what an older reading of this file said.
`@on_message` takes no `event_kind` and subscribes to nothing: it marks the one
method that receives `(self, message, history, ctx)` and returns the assistant
reply. `@orchestrated` decorates the **class**, not a method, and its only
argument is `system_prompt`.

Rules :
- `@agent` and `@orchestrated` wrap the class. `@skill` and `@on_message` wrap
  async methods. Never wrap a non-async method.
- At most one `@on_message` per class, and `@orchestrated` is mutually
  exclusive with both `@skill` and `@on_message`. Validation runs at `@agent`
  decoration time and raises `AgentConfigError`. Fail fast.
- Order : `@agent` outermost on the class, then the method decorators. The
  class attribute `__apollia_manifest__` is built by `@agent`; the bridge
  additionally requires `__apollia_dispatch__` and refuses an object without
  it.

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
```

Mandatory shape :
- A module-level `agent` symbol bound to the single instance. **Do not write
  it by hand.** `@agent` instantiates the class and binds the symbol itself
  (`sdk/apollia/_internal/module_registry.py`); a hand-written
  `agent = MyClass()` builds a second instance. None of the tree's
  `@agent` modules carries the line.
- Every `@skill` carries at least one realistic `examples=[{...}]` payload.
- Every parameter that is LLM-facing and not a trivial scalar carries
  `Annotated[T, "description"]`.
- TypedDict canonical schemas live in `<agent>/schemas.py`.

---

## 3. `Ctx` protocol

`Ctx` is the capability bundle the runtime injects. The skill body
interacts with the world only through `ctx`. The authoritative type
contract is the `Ctx` Protocol in `sdk/apollia/types.py` plus the
per-service interface modules in `sdk/apollia/context/*.py`. There are no
`.pyi` stubs.

`Ctx` exposes the whole Apollia backend through 15 typed services :

```python
@runtime_checkable
class Ctx(Protocol):
    llm: LlmProxy
    memory: MemoryInterface
    tools: ToolProxy
    a2a: A2AInterface
    mail: MailInterface
    datasources: DatasourcesInterface
    templates: TemplatesInterface
    secrets: SecretsInterface
    events: EventsInterface
    logger: Logger
    profile: ProfileInterface | None
    workspace: WorkspaceContext
    stt: SttInterface
    notify: NotifyInterface
    budget: BudgetView
```

| Service | Interface (`apollia.context.*`) | Purpose |
|---|---|---|
| `llm` | `LlmProxy` | prompt, stream, `map` over items |
| `memory` | `MemoryInterface` | agent-initiated recall and write |
| `tools` | `ToolProxy` | invoke native and MCP tools |
| `a2a` | `A2AInterface` | synchronous agent-to-agent RPC |
| `mail` | `MailInterface` | durable, auditable inter-agent mailbox |
| `datasources` | `DatasourcesInterface` | read declared datasources |
| `templates` | `TemplatesInterface` | render declared templates |
| `secrets` | `SecretsInterface` | read-only secret access |
| `events` | `EventsInterface` | emit and observe runtime events |
| `logger` | `Logger` | structured logging (`ctx.logger`, not `ctx.log`) |
| `profile` | `ProfileInterface \| None` | canonical user profile, `None` when the agent may not read it |
| `workspace` | `WorkspaceContext` | workspace paths and metadata |
| `stt` | `SttInterface` | speech-to-text |
| `notify` | `NotifyInterface` | desktop and webhook notifications |
| `budget` | `BudgetView` | read the remaining step budget |

`ReAct` is intentionally NOT on `Ctx`. It lives as a free function
`apollia.react(ctx, ...)` so alternative reasoning loops compose without
subclassing the runtime context.

Adding a service or a method to `Ctx` :
1. Update the Protocol in `sdk/apollia/types.py` and the interface module
   in `sdk/apollia/context/<service>.py`.
2. Implement in `crates/apollia-aip/`.
3. Update the decisions chapter of the documentation site (the agent contract, or
   the mailbox).

`sdk/apollia/types.py` is the contract. If implementation and contract
diverge, the contract wins and the implementation is broken.

### `ctx.mail` semantics

`ctx.mail` is the durable, at-least-once inter-agent inbox, distinct from
`ctx.a2a` (synchronous RPC). Backed by a SQLite mailbox with lease-based
delivery. Interface in `sdk/apollia/context/mail.py` :

| Method | Behavior |
|---|---|
| `send(to, payload) -> str` | enqueue a message, returns its id |
| `receive(timeout_secs=None) -> MailMessage \| None` | lease the next message |
| `poll() -> MailMessage \| None` | non-blocking lease |
| `pending() -> int` | count of undelivered messages |
| `list(limit=50) -> list[MailMessage]` | inspect without leasing |
| `ack(message_id) -> None` | confirm processing, removes the lease |
| `nack(message_id) -> None` | release the lease for redelivery |

A leased message not `ack`-ed within the visibility timeout is
redelivered. Runtime config caps live in `RuntimeConfig` (`mailbox_*`
fields); see `crates/apollia-runtime/AGENTS.md` §6.

---

## 4. TypedDict schemas

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
- Validation : `python -m apollia inspect <agent.py> --json` shows the
  generated schema; check that it carries a description on every parameter and
  examples on every skill.

---

## 5. Exceptions

Hierarchy rooted at `AgentError` :

```
AgentError
├── DomainError       # business-level failure, retry meaningless
├── NeedHumanInput    # captured by the dispatcher, returns input_required
├── PayloadError      # malformed input, fail-fast to caller
├── SchemaError       # the declared schema itself is wrong
├── SkillNotFound     # no skill answers the requested skill_id
└── AgentConfigError  # the agent is malformed, raised at decoration time
```

There is no `AgentTimeoutError`: `git grep AgentTimeoutError -- sdk/apollia`
returns nothing. A timeout reaches the caller as a task-level status, not as an
SDK exception.

Rules :
- Never subclass `Exception` directly for new error types. Subclass
  `AgentError`.
- `DomainError(message, *, code=...)` is the one that carries a stable `.code`
  the caller can branch on. The others do not take a code; do not promise one.
- `NeedHumanInput(prompt, context=None)` is the only exception that suspends
  and resumes the task: the dispatcher turns it into an `AIPResult` with
  `status == "input_required"` carrying `prompt` and `context`, and the runtime
  restitutes `context` verbatim on resume. All others terminate the skill
  invocation.

---

## 6. Datasources and templates

The runtime loads datasources and templates declared in the agent
TOML (`[datasources]`, `[templates]`). They are exposed read-only via
the `ctx.datasources` and `ctx.templates` services.

Rules :
- Agents never write to datasources. The runtime owns the lifecycle.
- Templates are rendered by `minijinja` on the Rust side
  (`crates/apollia-aip/src/templates.rs`), so the filter set is minijinja's
  built-in one, not a hand-picked list. `ctx.templates.render(name, **context)`
  is the whole surface. Registering a custom filter is a contract change:
  state it in `docs/site/docs/architecture/08-decisions.md` under
  `#agent-contract` first.

---

## 7. `@orchestrated` semantics

Marks a skill that ORIA drives step-by-step (LLM in the loop). The
agent body is a single function; ORIA decides when to call tools, when
to ask the LLM, when to stop.

`@orchestrated(system_prompt=...)` marks the class. The agent supplies only
metadata and the system prompt; it may define
`async on_plan_complete(step_results, ctx)` to post-process the step outputs,
which defaults to concatenating the step texts.

There is no `cache_plan` argument. ORIA's plan cache is keyed on the agent
name, the agent version, the sorted tool names and the normalized task text,
and it is on whenever the engine was given a cache repository; see
`crates/apollia-oria/AGENTS.md` §4.

---

## 8. Forbidden in this subtree

- `from __future__ import annotations` in any module with TypedDict.
- Relative imports (`from .module import X`).
- Module-level side effects beyond the `agent = MyClass()` instantiation.
- Adding third-party dependencies. The SDK is stdlib-only; the bridge that
  injects `ctx` is Rust, not a Python package the SDK imports.
- Decorator stacking that breaks the validation (multiple of
  `@skill` / `@on_message` / `@orchestrated` on one method).
- `print()` in agent and library code (use `ctx.logger`). The exception is
  `sdk/apollia/cli/`, whose whole job is to write to the terminal: those
  modules carry a written `# REASON: print-call:` exemption that
  `scripts/check_python_rules.py` reads, and nothing else may.
- Catching `CancelledError` without re-raising.

---

## 9. Type-contract synchronization

`sdk/apollia/types.py` (the `Ctx` Protocol) and `sdk/apollia/context/*.py`
(the per-service interfaces) carry the type contract for the
runtime-injected `Ctx`. They are hand-maintained alongside the Rust
implementation in `crates/apollia-aip/` and committed for reproducibility
and IDE support.

Edit policy :
- Allowed when adding a new method that does not exist on the Rust side
  yet (drafting).
- When implementing the Rust side, update the Protocol in the same PR to
  keep parity.
- Contract and implementation must agree on signature and semantic.
  Discrepancy : the Python contract wins, fix the implementation.

---

## 10. Testing

- `pytest` with `asyncio_mode = "strict"`.
- Each new decorator carries unit tests that exercise the validation
  paths (`AgentConfigError` on conflicting decorators, etc.).
- `python -m apollia inspect <agent.py> --json` is the local smoke test. It is
  not run in CI today: no workflow, no `just` recipe and no hook invokes it.
  Run it by hand on the agent you touched, and do not cite it as a gate.
- See `docs/agents/TESTING.md` §6.

---

## 11. Documentation responsibilities

| Change | Update |
|---|---|
| New decorator | `sdk/README.md`, this file (§1), `docs/site/docs/` |
| New `Ctx` method | `types.py` + `context/<service>.py` + the decisions chapter |
| New `AgentError` subclass | this file (§5), `sdk/apollia/_internal/aip_result.py` |
| TypedDict schema convention change | this file (§4), the `#agent-contract` section of the decisions chapter if breaking |

There is no Book and no Wiki corpus in this tree. The two committed
documentation corpora are `docs/site/` and `docs/agents/`.

---

## 12. When the rules block you

- Need a new decorator : state it in the `#agent-contract` section of
  `docs/site/docs/architecture/08-decisions.md` first. Decorators are
  contract.
- Need to add a runtime dependency to an agent : state the decision in
  `docs/site/docs/architecture/08-decisions.md` first. Each one is a
  sovereignty surface.
- Need to break compatibility (rename a decorator, change a schema
  field semantics) : rewrite `#agent-contract` in the same commit, and
  ship under a feature flag with documented sunset.
