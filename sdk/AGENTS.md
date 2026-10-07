# sdk/AGENTS.md

> Local rules for the Apollia AgentKit SDK. Read after the root `AGENTS.md`
> and `docs/agents/PYTHON-PATTERNS.md` before editing this subtree.

The SDK is the contract the Python of every Apollia package depends on. A
change here propagates to every package in the wild. Stability and clarity are
non-negotiable.

The authority on what `ctx` guarantees is `sdk/apollia/types.py` together with
`sdk/apollia/context/`. The rationale is in the decisions chapter of the
documentation site.

---

## 1. Decorator inventory

An agent is data: its mission, its examples and what it may reach are written
in the `apollia.toml` of a package. No decorator declares an agent, and no
Python stands for one. The Python of a package brings tools and workflows, and
the SDK offers exactly two decorators:

| Decorator | Level | Signature | Purpose |
|---|---|---|---|
| `@tool` | async function | `tool(name, description, *, action, examples=None)` | a Python tool a package ships, grantable to any agent |
| `@workflow` | async function | `workflow(name, description, *, inputs, steps, action)` | a workflow with its steps declared ahead, run as `(ctx, inputs)` |

`@tool` and `@workflow` decorate module-level functions, not methods.
`action` is one of the five families `apollia.toml` reads (`read`,
`read_personal`, `write_or_delete`, `send_external`, `execute_code`); every
family except `read` is sensitive, and sensitivity follows the family alone:
`@tool` takes no `dangerous` flag. A workflow's `inputs` is a `TypedDict` whose fields are each a type the launch
form knows: `str`, `apollia.LongText`, `apollia.FileInput` (received as the
absolute path of the file), `datetime.date` (received as a `date`), `int`,
`float`, `bool`, `Literal[...]`, or a list of text or numbers; any other type is
refused at decoration time. The schema carries `format` `date`, `long-text` or
`file-path` for the three typed texts, and `x-order`, the declared order, which
the form follows. Its body opens each step
with `async with ctx.step("name")`, which refuses a name missing from `steps`.
Run by the runtime, a step reports its opening, its end and its failure through
a private hook of the runtime context (`_apollia_step_hook`); `MockContext` has
none, so a workflow under test reports nothing.
`python -m apollia describe <folder>` writes both into the folder's
`apollia.toml` as `[[tools]]` and `[[workflows]]` descriptors, so reading the
manifest never runs the code.

The package root publishes nothing else that declares behaviour. There is no
class decorator, no method decorator, no conversational handler, no
reasoning-loop helper and no dispatch hook: a module that tries to import one
fails at import, and `sdk/tests/test_public_api.py` freezes that surface in
both directions.

Rules :
- Both decorators wrap an `async def` function. Never wrap a method, never
  wrap a non-async function: either raises `AgentConfigError`.
- One decorator per function. Applying `@tool` or `@workflow` twice to the
  same function raises `AgentConfigError`.
- `name` matches `[a-z0-9_-]+` and is unique in the package; `description`
  is a non-empty string. Validation runs at decoration time. Fail fast.
- A workflow function takes exactly two positional parameters,
  `(ctx, inputs)`. A tool takes `ctx` first, then its inputs.

---

## 2. The contract minimal

```python
from typing import Annotated

from apollia import tool
from apollia.types import Ctx

from my_package.schemas import EmailPayload, EmailResult


@tool(
    "triage",
    "Score an email by priority.",
    action="read",
    examples=[{
        "email": {"to": "ops@apollia.fr", "subject": "service down", "body": "..."},
    }],
)
async def triage(
    ctx: Ctx,
    email: Annotated[EmailPayload, "the email to score"],
) -> EmailResult:
    ...
```

Mandatory shape :
- The agent that uses the tool is declared in `apollia.toml` (`[[agents]]`,
  with its mission and its `access`), never in Python.
- Every `@tool` carries at least one realistic `examples=[{...}]` payload.
- Every parameter that is LLM-facing and not a trivial scalar carries
  `Annotated[T, "description"]`.
- TypedDict canonical schemas live in `<package>/schemas.py`.
- After a change to a decorated function, `python -m apollia describe
  <folder>` rewrites the descriptors of `apollia.toml`.

---

## 3. `Ctx` protocol

`Ctx` is the capability bundle the runtime injects. The body of a tool or
of a workflow interacts with the world only through `ctx`. The authoritative type
contract is the `Ctx` Protocol in `sdk/apollia/types.py` plus the
per-service interface modules in `sdk/apollia/context/*.py`. There are no
`.pyi` stubs.

`Ctx` exposes the whole Apollia backend through 17 typed services :

```python
@runtime_checkable
class Ctx(Protocol):
    llm: LlmProxy
    memory: MemoryInterface
    tools: ToolProxy
    agents: AgentsInterface
    workflows: WorkflowsInterface
    user: UserInterface
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
| `agents` | `AgentsInterface` | `run(agent_id, message)`, bound in a workflow run |
| `workflows` | `WorkflowsInterface` | `run(workflow_id, inputs)`, bound in a workflow run, three levels deep at most |
| `user` | `UserInterface` | `ask(question, *, options=None, hint=None)`, bound in a workflow run: the run waits in its step until the user answers, a refusal raises `QuestionDeclined` |
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

There is no synchronous call between agents on `Ctx` other than
`ctx.agents.run` and `ctx.workflows.run`, and `ctx.llm` offers no loop that
alternates model calls and tool calls: that loop belongs to an agent, and a
workflow step that needs one hands the message to an agent.

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
`ctx.agents.run` (a synchronous run). Backed by a SQLite mailbox with lease-based
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

Schemas live in `<package>/schemas.py`. The file must NOT contain
`from __future__ import annotations`. PEP 563 turns annotations into
strings and breaks `TypedDict.__required_keys__` that AgentKit reads at
registration.

```python
# my_package/schemas.py
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
- Validation : `python -m apollia describe <folder>` writes the generated
  schema into `apollia.toml`; check that it carries a description on every
  parameter and examples on every tool.

---

## 5. Exceptions

Hierarchy rooted at `AgentError` :

```
AgentError
├── DomainError       # business-level failure, retry meaningless
├── NeedHumanInput    # a pause awaiting a human
├── QuestionDeclined  # the user declined a question `ctx.user.ask` asked
├── PayloadError      # malformed input, fail-fast to caller
├── SchemaError       # the declared schema itself is wrong
└── AgentConfigError  # a declaration is malformed, raised at decoration time
```

There is no `AgentTimeoutError`: `git grep AgentTimeoutError -- sdk/apollia`
returns nothing. A timeout reaches the caller as a task-level status, not as an
SDK exception.

Rules :
- Never subclass `Exception` directly for new error types. Subclass
  `AgentError`.
- `DomainError(message, *, code=...)` is the one that carries a stable `.code`
  the caller can branch on. The others do not take a code; do not promise one.
- `NeedHumanInput(prompt, context=None)` is the exception a pause rides:
  `ctx.tools.call` raises it for a call that needs an approval. It carries
  `prompt` and `context`, and the runtime restitutes `context` verbatim on
  resume, through `ctx.input_response`. A workflow that wants to ask the user
  something calls `ctx.user.ask`, which waits inside the step.
- `QuestionDeclined(question, *, reason=None)` is raised inside a workflow
  step by `ctx.user.ask` when the user refuses to answer. It does not suspend
  anything: the wait already happened inside `ask`. Caught, the workflow goes
  on; uncaught, the step and the run fail.

---

## 6. Datasources and templates

Datasources and templates are exposed read-only via the
`ctx.datasources` and `ctx.templates` services.

Rules :
- A tool or a workflow never writes to datasources. The runtime owns the lifecycle.
- Templates are rendered by `minijinja` on the Rust side
  (`crates/apollia-aip/src/templates.rs`), so the filter set is minijinja's
  built-in one, not a hand-picked list. `ctx.templates.render(name, **context)`
  is the whole surface. Registering a custom filter is a contract change:
  state it in `docs/site/docs/architecture/08-decisions.md` under
  `#agent-contract` first.

---

## 7. Workflow steps

`@workflow(..., steps=[...])` lists the steps ahead, in order, and that list
is what the card of the workflow shows without running the code. The body
opens each one with `async with ctx.step("name")`:

- `ctx.step` refuses a name the decorator did not declare, with an
  `AgentConfigError` naming the declared ones, so the body cannot drift from
  the card.
- A workflow called directly from a step of another validates its steps
  against its own list, not the caller's.
- A step calls `ctx.llm`, `ctx.tools`, `ctx.agents.run`, `ctx.workflows.run`,
  `ctx.user.ask` or plain Python. `ctx.agents`, `ctx.workflows` and
  `ctx.user` are bound in a workflow run only.

---

## 8. Forbidden in this subtree

- `from __future__ import annotations` in any module with TypedDict.
- Relative imports (`from .module import X`).
- Module-level side effects. A module of a package is imported by
  `python -m apollia describe` and by the runtime; importing it does nothing
  but define its functions.
- Adding third-party dependencies. The SDK is stdlib-only; the bridge that
  injects `ctx` is Rust, not a Python package the SDK imports.
- Decorator stacking (`@tool` and `@workflow` on one function, or either
  applied twice).
- Any class, decorator or helper that would let Python stand for an agent.
  An agent is data in `apollia.toml`.
- `print()` in package and library code (use `ctx.logger`). The exception is
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
- A tool and a workflow are plain `async def` functions: a test builds a
  `MockContext` (`from apollia.testing import MockContext`), configures its
  services in place and awaits the function as written. There is no factory
  and no dispatch between the test and the function, so a raised
  `DomainError` reaches the test as itself.
- `MockContext` mirrors every service of `Ctx` but `ctx.mail`;
  `scripts/check_testing_mocks.py` holds that pairing. Under it a workflow's
  steps report nothing, and `ctx.step` still refuses an undeclared name.
- Each decorator carries unit tests that exercise its validation paths
  (`AgentConfigError` on a non-async function, an unknown action family, an
  unsupported workflow input, a duplicated step).
- A change to the public surface changes `sdk/tests/test_public_api.py` in
  the same commit: it snapshots `apollia.__all__` and the `__all__` of every
  public submodule exactly.
- `python -m apollia describe <folder>` is covered by
  `sdk/tests/test_describe_cli.py` on the fixture package
  `sdk/tests/fixtures/invoice_pkg`.
- See `docs/agents/TESTING.md` §6.

---

## 11. Documentation responsibilities

| Change | Update |
|---|---|
| New decorator | `sdk/README.md`, this file (§1), `docs/site/docs/` |
| New `Ctx` method | `types.py` + `context/<service>.py` + the decisions chapter |
| New `AgentError` subclass | this file (§5), `sdk/apollia/errors.py` |
| TypedDict schema convention change | this file (§4), the `#agent-contract` section of the decisions chapter if breaking |

There is no Book and no Wiki corpus in this tree. The two committed
documentation corpora are `docs/site/` and `docs/agents/`.

---

## 12. When the rules block you

- Need a new decorator : state it in the `#agent-contract` section of
  `docs/site/docs/architecture/08-decisions.md` first. Decorators are
  contract.
- Need to add a runtime dependency to a package : state the decision in
  `docs/site/docs/architecture/08-decisions.md` first. Each one is a
  sovereignty surface.
- Need to break compatibility (rename a decorator, change a schema
  field semantics) : rewrite `#agent-contract` in the same commit, and
  ship under a feature flag with documented sunset.
