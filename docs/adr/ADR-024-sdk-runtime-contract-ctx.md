# ADR-024: SDK runtime contract (ctx)

- Status: Accepted
- Date: 2026-06-04

## Context

Every Python agent receives a `ctx` object that exposes the runtime backend:
LLM access, memory, tools, agent-to-agent calls, datasources, templates,
secrets, observability, and more. Without a single source of truth this surface
drifts: the Rust `RuntimeContext` and the SDK type contracts diverge, the
authoring surface becomes a flat list of dozens of attributes with no
categorisation, and the SDK itself stops trusting attribute availability
(defensive `getattr(ctx, "emit_thought", lambda *a: None)` patterns appear).
Strict type checking cannot pass on an average agent.

Several specific frictions follow from the lack of a contract. Agent-to-agent
communication accreted three overlapping APIs with subtly different semantics.
Datasources and templates were declared in agent bundles but had no runtime API,
forcing manual file reads and pulling in non-stdlib parsers. Third-party API
keys had no agent-facing access path, so agents fell back to environment
variables. Observability events were emitted through implicitly named, untyped
methods. Logging was a string-level call that lost structured fields across the
bridge. The return contract required agents to construct a result object
explicitly, mixing business code with error-formatting boilerplate. Vision
content and memory export/import existed on the Rust side but were not typed or
exposed on the Python side. The LLM service carried a legacy buffered streaming
method alongside the modern one.

All of this must be folded into one coherent, typed contract that the Rust
runtime matches exactly, so that any divergence is a fail-fast at load.

## Decision

We adopt a single typed `Ctx` Protocol that exposes the whole backend through
nested, typed services, and the Rust `RuntimeContext` exposes exactly the same
attributes. The contract lives in the SDK (`apollia.types`), is detected by the
`ctx` parameter convention, and is verified at load: any divergence between the
runtime and the Protocol fails fast.

```python
class Ctx(Protocol):
    llm: LlmProxy
    memory: MemoryInterface
    tools: ToolProxy
    a2a: A2AInterface
    datasources: DatasourcesInterface
    templates: TemplatesInterface
    secrets: SecretsInterface
    events: EventsInterface
    logger: Logger  # Logger = logging.Logger
    profile: ProfileInterface
    workspace: WorkspaceContext
    stt: SttInterface
    notify: NotifyInterface
    budget: BudgetView
```

The protocol exposes 14 typed services. There is no `react` field on `Ctx`:
ReAct lives as the free function `apollia.react(ctx, ...)`. No public attribute
lives on the root `ctx` outside these services. Each service
is a `Protocol`, which gives IDE autocomplete, satisfies strict type checking,
and lets a test fake implement only the methods it uses. The service list and
its public methods are part of the SemVer-versioned SDK contract: adding a
service is a minor bump, removing or renaming one is a major bump.

### a2a

A single agent-to-agent service exposing four methods: `invoke()`, `discover()`,
`list_skills()`, and `skill_as_tool()`. `invoke(skill_id, input=None, *,
timeout_secs=120, **kwargs)` targets a skill by id (never by agent name) and
returns the target skill's business dict on success; on a runtime failure the
caller receives a failed `AIPResult` dict rather than a Python exception (see
exceptions below). `discover(skill_id)` returns a `SkillCard` (`skill_id`,
`name`, `description`, `agent_name`, input and output schema) or `None`.
`list_skills()` lists available skills (no agent filter). `skill_as_tool(skill_id)`
wraps a skill as an LLM tool descriptor consumable by `apollia.react(tools=[...])`,
so a director can present discovered workers to the LLM as native tools. The
earlier mailbox `send`/`receive` methods are removed (see below).

### Mailbox removal

The fire-and-forget mailbox `ctx.send(to_agent, message)` and
`ctx.receive(timeout)` are removed with no replacement. They had unspecified
semantics (persistence, TTL, delivery on a stopped recipient), no production
usage, and no clear boundary against synchronous `ctx.a2a.invoke`. Synchronous
inter-agent calls go through `ctx.a2a.invoke`; observability signals go through
`ctx.events`. A genuine asynchronous event bus is deferred until a concrete use
case justifies a proper specification.

### datasources and templates

`ctx.datasources` loads versioned YAML resources bundled with the agent;
`ctx.templates` renders minijinja templates. Both are manifest-gated:
`@agent(datasources=("topics", "sources"), templates=("digest",))` declares what
the agent may read, and a `get` on an undeclared name raises `FileNotFoundError`.
`ctx.datasources.get(name)` returns the parsed YAML (cached after first load) and
`ctx.datasources.list_names()` lists the declared names; `ctx.templates.render(
name, **vars)` renders a sandboxed template (no `os`/`subprocess` access, includes
disabled) and `ctx.templates.list_names()` lists the declared templates. YAML
parsing and template rendering happen on the Rust side (the workspace uses
`serde_yaml` and the `minijinja` crate), so the Python side stays stdlib-only.
Declared but missing or malformed resources fail fast at load. Both services are
read-only.

### secrets

`ctx.secrets` is a read-only, manifest-gated accessor over the local encrypted
credential store, exposing `get(key) -> str | None` and `has(key) -> bool`. An
agent reads only the keys it declares in `@agent(secrets=(...))`; `get` returns
`None` for a key never configured by the operator rather than raising. The lookup
is synchronous (sub-millisecond keyring read). There is no `set`/`delete` API:
configuration stays operator-driven. Each read is logged through `ctx.logger`
with the agent id and key name (never the value) for auditability. OAuth tokens
(Gmail, Calendar, Drive) are deliberately not exposed to agent code: agents
reach those services through native connector tools that refresh tokens
internally.

### events

`ctx.events` is a typed service exposing the four canonical event kinds an agent
may emit: `emit_token`, `emit_thought`, `emit_retry`, and
`emit_action_parse_error`. All are synchronous, non-blocking sends toward the
EventBus, and all are gracefully no-op when the runtime is not wired in (testing,
mocks), which removes the old defensive `getattr` helper. Authors do not invent custom events; business data
that must travel goes through `ctx.logger` with structured fields. The runtime
must expose all of these methods, verified by the Protocol check at load.

### logger

`ctx.logger` is a stdlib `logging.Logger` pre-configured under the hierarchical
name `apollia.agent.<agent_name>`, whose records are piped to Rust tracing
through a custom handler that preserves `extra` fields as structured tracing
fields. Authors use their familiar idiom (`ctx.logger.info("fetching",
extra={"url": url})`). The hierarchical name allows per-agent filtering and log
levels, the runtime auto-adds `agent_id`, `task_id`, and `step_id`, and captured
`stdout`/`stderr` is redirected into the logger so legacy `print` output stops
disappearing.

### Exceptions at the boundary

Agents raise typed exceptions from `apollia.errors`; the SDK boundary traps them
at dispatch and formats the result. The exception base is `AgentError` (the
abstract root, never raised directly). Its subclasses are `DomainError(code,
message, details=None)` for known business errors, `PayloadError` raised
automatically when input does not match the inferred signature schema,
`SchemaError` for an unmappable handler signature (load-time), `SkillNotFound`
for an unknown skill id, `NeedHumanInput(prompt, context=None)` for
human-in-the-loop, and `AgentConfigError` for an invalid decorator configuration
(load-time). Any other exception is trapped and mapped to a failed result with
code `EXECUTION_FAILED`, logged with a stacktrace. A normal return is a business
`dict` (or `None`); the boundary wraps it as a completed result. Agent code is
therefore linear: validate with a `raise` at the top, return a dict on success.

### AIPResult kept SDK-internal

The result envelope (`AIPResult`) is internal to the SDK. The bridge no longer
injects a result class into the handler globals; instead the agent returns a
plain business dict or raises, the SDK boundary (`_internal/dispatch.py`)
orchestrates the call and delegates result building to
`_internal/aip_result.py` (`from_handler_return` / `from_exception`), which
produces the normalized result dict from the return value or the trapped
exception, and the bridge deserializes that dict into the Rust `AIPResult`. The agent never sees `AIPResult`, so there is no magic injected
symbol for the IDE to flag.

```python
@skill("extract")
async def extract(self, path: str, ctx) -> dict:
    if not Path(path).exists():
        raise DomainError("FILE_NOT_FOUND", f"Path does not exist: {path}")
    text = await self._read(path, ctx)
    return {"text": text, "chars": len(text)}
```

### Typed vision and memory export/import

`apollia.types.llm` defines public `TypedDict`s for multimodal messages:
`LlmMessage`, `TextContent`, `ImageContent`, and the `MessageContent` union, plus
helpers (`text`, `image_from_path`, `image_from_bytes`, `image_from_url`). An
`ImageContent` carries a nested `source` typed as `ImageSourceBase64 |
ImageSourceUrl`; the base64 source carries `media_type` and `data`, the URL
source a `url`. Cloud providers route to
the right multimodal endpoint; the local llama-cpp-2 engine is text-only, so a
message containing an `ImageContent` raises `DomainError("VISION_UNSUPPORTED",
...)` at the boundary. `ctx.memory` gains `export() -> dict` and
`import_data(data: dict)` so an agent can checkpoint or restore its memory; the
JSON format is stable and `import_data` merges rather than replaces.

### ctx.llm.stream

`ctx.llm` exposes four awaitables, `complete`, `chat`, `stream`, and `embed`,
plus a `default_backend` property. `stream` yields tokens as an async iterator
and is the renamed form of the former `stream_complete`; the legacy buffered
`stream` method is removed. The ReAct
loop no longer silently rewrites shorthand actions: an unrecognized action shape
raises an `ActionParseError` surfaced through
`ctx.events.emit_action_parse_error`, so prompt bugs become visible instead of
being masked.

## Alternatives considered

### Flat dict-like ctx (rejected)
- Pros: flexible, easy runtime introspection.
- Cons: no autocomplete, no typing, and the author hunts keys in documentation
  rather than in the IDE.

### Abstract base classes instead of Protocols (rejected)
- Pros: explicit typing through inheritance.
- Cons: forces test fakes to inherit too, defeating cheap mocking, and gives up
  structural typing.

### Keep the flat surface and add new services at the same level (rejected)
- Pros: smaller migration.
- Cons: the flat surface becomes unmanageable past forty entries and does
  nothing for author cognition.

### Keep the parallel a2a APIs, keep the mailbox, keep AIPResult on the agent (rejected)
- Pros: no breaking change.
- Cons: perpetuates three ways to call an agent, an unspecified mailbox, and an
  injected magic symbol the IDE flags, all of which the contract is meant to
  remove.

### YAML/Jinja and logging through third-party Python libraries (rejected)
- Pros: mature, simple to wire.
- Cons: violates zero external dependency on the Python side; stdlib `logging`
  plus a custom tracing handler, and Rust-side parsing/rendering, cover the need.

### Chosen: one typed Ctx Protocol with nested services
- Pros: a single source of truth shared by the runtime and the SDK, categorised
  and explorable autocomplete, strict type checking that passes, trivial mocking,
  load-time divergence detection, and a versioned stable surface.
- Trade-offs: a larger total typed surface (categorised rather than flat), and a
  Rust-side refactor to expose the nested services.

## Consequences

- Positive: the `ctx` surface is categorised and self-documenting, strict type
  checking passes without escape hatches, the runtime and SDK share one source
  of truth, agent code is linear with errors raised at the boundary, datasources
  and templates are usable at runtime stdlib-only, secrets are gated and
  auditable, observability events are typed, vision and memory export are
  exposed, and the LLM service exposes four well-named awaitables.
- Negative / trade-off: a total rewrite of the `ctx` surface (mechanical but
  broad), no native fire-and-forget channel until a real event bus is specified,
  and the local engine cannot serve vision so a local-then-cloud author must
  switch provider.
- Watch: service count growth past the current set (introduce domains if it
  passes twenty), EventBus saturation on step-heavy agents, the rate of
  `ActionParseError` post-release as a prompt-quality signal, and the per-log
  cross-bridge cost on tight logging loops.

## Architectural principles

- Principle #1 (Local-first): secrets never leave the machine, on the store side
  and now on the agent side.
- Principle #2 (Zero external dependency): the Python side stays stdlib-only;
  YAML parsing and template rendering run in Rust.
- Principle #3 (Minimal contract): the invocation contract stays an async
  `__apollia_dispatch__(task, ctx)`; what is enriched is the typed surface of
  `ctx`, and the agent returns a business dict or raises.
- Principle #4 (Fail fast): divergence between the runtime and the Protocol,
  malformed datasources, and unconfigured declared secrets are all detectable at
  load.
- Principle #5 (One actor, one responsibility): each `ctx` service maps to a
  Tokio actor or crate on the Rust side.
- Principle #6 (Memory at agent initiative): `ctx.memory.export()` and
  `import_data()` are explicit agent calls, never automatic.
- Principle #7 (Non-negotiable safeguards): manifest gating for secrets,
  datasources, and templates is enforced by the runtime and not bypassable from
  agent code, and the StepBudget is enforced by the runtime, surfacing as a
  failed result rather than a bypassable Python exception.

## Related

- [ADR-023](ADR-023-sdk-agentkit-design.md) the AgentKit decorators that declare
  the gating and consume the `ctx` services.
- [ADR-025](ADR-025-worker-agents-a2a-routing.md) the worker and A2A routing
  pattern that `ctx.a2a` drives.
