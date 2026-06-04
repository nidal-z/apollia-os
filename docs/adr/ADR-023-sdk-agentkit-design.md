# ADR-023: Python SDK / AgentKit design

- Status: Accepted
- Date: 2026-06-04

## Context

The Apollia Interface Protocol (AIP) is the contract between the Rust runtime
and Python agents. The runtime must be adoptable by developers who already
have agents written for LangGraph, CrewAI, or AutoGen. Forcing every agent to
inherit a framework base class would add friction and require rewriting
existing code. Principle #3 (minimal contract) demands that the runtime
contract stay small.

At the same time, agent authors need a real authoring experience: IDE
autocomplete, type checking, test utilities, and a clean packaging story. The
authoring surface must be ergonomic without growing a deep class hierarchy
where each parent class carries its own lifecycle, its own dispatch, and its
own way of declaring a manifest. Such a hierarchy makes the boundary between
agent kinds hard to learn, blocks composition (an agent cannot be both a
multi-skill worker and a conversational responder), and complicates testing
(mocking parent methods). The single contract that the runtime actually needs
is a module that exposes a module-level `agent` whose class carries a
`__apollia_manifest__` dict and an async `__apollia_dispatch__(task, ctx)`. The
SDK must deliver that contract through a flat, composable authoring API and
ship as a pure-Python package with zero Rust coupling at install time.

## Decision

We adopt a decorator-first AgentKit: a module-level `agent` whose class carries
a `__apollia_manifest__` dict and an async `__apollia_dispatch__(task, ctx)` is
AIP-valid, and the canonical way to author one is a single `@agent` class
decorator complemented by additive method decorators (`@skill`, `@on_message`,
`@orchestrated`) that generate both dunders. ReAct is a runtime utility
(`apollia.react(ctx, ...)`), the method signature is the I/O schema source,
`@agent` auto-instantiates the class and exposes `agent` at module level, and
the whole thing ships as a pure-Python pip-installable package.

### Contract built by the decorator

The `@agent` decorator installs two class attributes the Rust validator
requires: a cached `__apollia_manifest__` dict and an async
`__apollia_dispatch__(task, ctx)` hook. Validation is by `hasattr()` on those
two dunders plus an `iscoroutinefunction` check at agent load time (the
`INITIALIZING` state). The legacy `manifest()` plus `run()` escape hatch is
gone: the runtime no longer calls any dynamic `manifest()` method. An existing
LangGraph or CrewAI object can be wrapped without inheriting an Apollia class,
but it still goes through `@agent` (or must manually grow those two dunder
attributes) to be AIP-valid. No mandatory base class, no mandatory import from
the runtime beyond the decorator.

### Decorator-first AgentKit

`@agent(name, version, ...)` is the single class decorator. At load time it
builds `__apollia_manifest__` from the decorated methods, validates them, and
installs the dispatcher boundary (the boundary contract is detailed in
[ADR-024](ADR-024-sdk-runtime-contract-ctx.md)). Additive method decorators
layer capabilities onto the same class:

- `@skill(id, description="", requires_approval=False, dangerous=False,
  examples=None)` declares an async method as an A2A-invokable skill.
- `@on_message` declares the method that drives conversational interaction with
  a human.
- `@orchestrated` declares the director method of an agent that delegates to
  workers through `ctx.a2a`.

A single agent can combine several `@skill` methods, an `@on_message` method,
and an `@orchestrated` method on the same class. Composition is natural because
there is no hierarchy to reconcile.

```python
from apollia import agent, skill, DomainError

@agent(name="docx-worker", version="1.0.0")
class DocxWorker:
    @skill("extract_text")
    async def extract(self, path: str, ctx) -> dict:
        if not path.endswith(".docx"):
            raise DomainError("UNSUPPORTED", f"Not a docx: {path}")
        return {"text": await self._read(path, ctx)}

    @skill("count_pages")
    async def count(self, path: str, ctx) -> dict:
        return {"pages": 12}
```

### ReAct as a runtime utility

The Observer-Reasoner-Actor loop is a utility, not a base class:
`apollia.react(ctx, system=..., user=..., *, tools=..., max_steps=...)` is a free
function imported from `apollia` that runs a deterministic loop driven by the
SDK. It is not a member of `ctx`. The agent stays in control of the sequencing:
it can call `apollia.react()` twice, nest it inside a `@skill`, or not use it at
all.

### Signature as the I/O schema

The Python signature of a handler is the single source of truth for its I/O
schema. At decoration time the SDK introspects `inspect.signature(handler)` and
resolves type hints to a JSON Schema for input and output, validates incoming
payloads, and populates `__apollia_manifest__` without requiring a TOML
descriptor or a TypedDict on the agent side. Rules:

- Input parameters are all positional/keyword parameters except `self` and
  `ctx` (detected by name or by the `Ctx` protocol type).
- Supported types without configuration: `str`, `int`, `float`, `bool`, `bytes`
  (base64), `list[T]`, `tuple[T, ...]`, `dict[str, T]`, `T | None`,
  `Optional[T]`, `Union[...]`, `Literal[...]`, `Annotated[T, "description"]`, and
  stdlib `@dataclass` / `TypedDict` / `NamedTuple`. Types such as
  `datetime.date`, `datetime.datetime`, `pathlib.Path`, and `Enum` are not
  supported and raise `SchemaError`.
- Defaults come from `inspect.Parameter.default`; a parameter without a default
  is `required`.
- The first docstring line becomes the skill description; Google-style `Args:`
  sections feed per-field descriptions.
- Complex inputs (deep nesting, shared structures) use a stdlib `@dataclass` or
  `TypedDict`, introspected recursively.
- An unsupported annotation raises `SchemaError` at import, before any call.

### Auto-instantiation and module-level `agent`

The PyO3 bridge loads an agent module and reads a module-level `agent`
attribute. The `@agent` decorator produces that attribute itself: it
instantiates the decorated class and assigns the instance to the module's
`agent` attribute, so the author never writes `agent = MyClass()` at the bottom
of the file. The decorator returns the class (not the instance), so a test can
still build its own instance. Rules: exactly one `@agent` class per module
(a second one fails fast with an `AgentConfigError`, a subclass of `AgentError`),
the `__init__` takes no required arguments, and imports stay absolute.

### Pure-Python pip-installable package

The SDK is the `apollia` package under `sdk/`, installable via
`pip install -e ./sdk`. It carries zero Rust dependency at install time: type
contracts are pure Python with a PEP 561 `py.typed` marker, the package ships
the decorators, the test mocks, and the scaffolding CLI (`apollia new`). The
PyO3 classes are exposed to the runtime at execution, never at development time.

## Alternatives considered

### Mandatory base class hierarchy (rejected)
- Pros: stricter static typing, a fixed lifecycle per agent kind.
- Cons: forces every agent to inherit a loop it may not use, blocks composition,
  makes the boundary between agent kinds a constant source of confusion, and
  carries a large volume of defensive glue.

### Composable mixins instead of decorators (rejected)
- Pros: an agent could combine a worker mixin and a conversational mixin.
- Cons: the author cognition worsens (mixin order and the Python MRO are traps),
  the SDK still implements several internal dispatchers, and duplication moves
  into the mixins rather than disappearing.

### External DSL (YAML plus Python snippets) (rejected)
- Pros: little or no Python for simple cases.
- Cons: betrays the minimal duck-typing contract and the "real agents, not
  flowcharts" philosophy, breaks IDE autocomplete, and forces a custom parser
  that makes security review harder.

### TOML manifest as the I/O source of truth (rejected)
- Pros: readable by non-Python tooling without interpreting Python.
- Cons: keeps the triple description (signature, manual validation, TOML),
  allows silent drift between code and manifest, and gives no IDE feedback.

### Bundle the SDK in the Rust binary via PyO3 (rejected)
- Pros: a single distributable artifact.
- Cons: requires a Rust build for Python development, complicates packaging, and
  works against the goal of making agents easy to write.

### Chosen: decorator-first AgentKit, pure-Python package
- Pros: one concept on the author side (`@agent` plus additive decorators),
  static introspection at load time so tooling can display the full manifest
  without starting the runtime, natural composition, trivial unit tests (the
  class instantiates like any other), standard Python packaging, and IDE
  autocomplete through `py.typed`.
- Trade-offs: a class decorator with an instantiation side effect (documented),
  one `@agent` per module by design, and the typed contract
  (`apollia.types` plus `apollia.context.*`) that must stay in sync with the
  runtime contract.

## Consequences

- Positive: an agent is a decorated class with no parent to learn, the manifest
  is built statically and validated at import, composition is free, agents test
  with plain `pytest`, and the SDK installs with standard Python tooling and
  full IDE support.
- Negative / trade-off: static type checking cannot verify AIP conformance
  before load, a signature or duplicate-skill error surfaces at import rather
  than at the first call, and the typed contract (`apollia.types` plus
  `apollia.context.*`) requires manual synchronization with the runtime.
- Watch: the import-time cost of introspection on hosts with many installed
  agents, the emergence of recurring unsupported annotations (UUID, Decimal),
  and the need for derived-framework extension points (decorator-based, never
  via inheritance).

## Architectural principles

- Principle #2 (Zero external dependency): the decorators and schema inference
  use only the standard library (`functools`, `inspect`, `typing`), and the SDK
  carries no runtime dependency.
- Principle #3 (Minimal contract): pushed to its maximum. The runtime contract
  stays a `__apollia_manifest__` dict plus an async
  `__apollia_dispatch__(task, ctx)`, both generated by `@agent`; the signature is
  the schema, nothing else to declare.
- Principle #4 (Fail fast): the manifest is built at import, so an invalid
  signature, a duplicate skill id, a double `@agent`, or an unmappable type
  raises before any invocation.

## Related

- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) the PyO3 bridge that loads the
  agent module and reads the module-level `agent` attribute.
- [ADR-024](ADR-024-sdk-runtime-contract-ctx.md) the typed `ctx` contract the
  decorators and the dispatcher boundary rely on.
