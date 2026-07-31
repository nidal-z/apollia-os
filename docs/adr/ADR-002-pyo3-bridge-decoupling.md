# ADR-002: PyO3 bridge and trait-based decoupling

- Status: Accepted
- Date: 2026-06-04

> **Amended by [ADR-023](ADR-023-sdk-agentkit-design.md).** Passages below
> describing the agent contract as a `manifest()` method plus an async `run()`
> record the state at the time of writing. That contract was removed: the runtime
> calls no dynamic `manifest()`, and the bridge refuses an object without
> `__apollia_dispatch__`. The decision this ADR records is otherwise unchanged.

## Context

The Rust runtime ([ADR-001](ADR-001-foundations-stack.md)) hosts Python agents
in-process. The crate `apollia-aip` owns the PyO3 bridge: it loads an agent
module, validates the minimal AIP contract (`manifest()` plus an async `run()`),
and calls the agent's coroutines. Two distinct problems must be solved together.

First, the bridge mechanics. Python coroutines (`run()`, `on_start()`,
`on_stop()`) must be driven from Tokio without holding the GIL on an async
worker, and the bridge must build on every developer machine. On macOS the
system Python (`/usr/bin/python3`) points at a Command Line Tools framework whose
library path does not match what PyO3 expects, producing a systematic link
failure (`ld: library 'python3.9' not found`). On Linux the system Python links
without extra configuration.

Second, the coupling problem. PyO3 types such as `Py<PyAny>` cannot be
instantiated without a real interpreter. If the pure-Rust crates (`apollia-oria`,
`apollia-runtime`, the tool layer) depended on `apollia-aip`, then every one of
their unit tests would require Python and the whole runtime would transitively
link CPython, even though the logic under test (budget checks, `tokio::select!`,
error propagation, manifest registration) is one hundred percent Rust.

## Decision

We adopt a `spawn_blocking` plus `asyncio.run()` bridge, a documented
`PYO3_PYTHON` build configuration for macOS, and a single injectable-trait
pattern that keeps the pure-Rust crates free of PyO3.

### Bridge execution: spawn_blocking plus asyncio.run

To run a Python coroutine from Rust async, the bridge clones the Send-safe
Python references, moves execution onto the Tokio blocking pool with
`tokio::task::spawn_blocking`, and inside the closure acquires the GIL with
`Python::with_gil`, calls the method, obtains the coroutine, runs it with
`asyncio.run(coroutine)`, and extracts the result. The `JoinHandle` is awaited
from async Rust. This guarantees the GIL is never held on a Tokio worker, the
coroutine is driven by an event loop that `asyncio.run()` creates and destroys
per call, and tests run under standard `#[tokio::test]` with no global event-loop
initialization. Existing Python agents work unchanged.

### macOS build configuration: PYO3_PYTHON

On macOS, developers point `PYO3_PYTHON` at a Homebrew Python 3.12 or newer
(for example `export PYO3_PYTHON=/opt/homebrew/bin/python3.13`). The workspace
`.cargo/config.toml` does not hardcode this, because the Homebrew path varies by
Python version and CPU architecture; the prerequisite is documented in project
setup. On Linux, CI and production need no extra configuration. This is a
development-time prerequisite only: the shipped binary embeds the Python runtime
through PyO3, so it does not weaken the zero-dependency principle.

### Trait-based decoupling: one pattern, three injection points

Each pure-Rust component that needs a PyO3-backed capability declares a small
`Send + Sync` trait and receives the implementation by injection, typically as an
`Arc<dyn Trait>`. The PyO3-backed logic lives in `apollia-aip`, and the concrete
trait objects are assembled in the binary crates (`apollia-cli`,
`apollia-desktop`), which already depend on every crate, so the consuming crates
never link PyO3 and stay unit-testable with mocks.

- `ToolExecutor` in `apollia-aip::context` exposes a single
  `execute(tool_name, input) -> Result<Value, String>`. The `ToolProxy` exposed
  to agents (`ctx.tools.call(...)`) holds an `Arc<dyn ToolExecutor>`, so the tool
  catalog stays a pure registry and the proxy can be tested without real tools or
  Python. This `ToolExecutor` is the agent-facing bridge trait; it is distinct
  from the unrelated, async per-native-tool `ToolExecutor` trait in
  `apollia-tools`, which is the execution interface implemented by each native
  tool.
- `AgentRunner` in `apollia-oria::engine` exposes `call_run(task)` returning a
  boxed future. `ORIAEngine::execute_direct()` takes `&dyn AgentRunner` rather
  than the concrete bridge, so the engine enforces the `StepBudget` and supervises
  execution without depending on `apollia-aip` and without any import cycle.
- `AgentLoader` is the trait defined in `apollia-runtime`, exposing
  `load_and_validate(path) -> Result<AgentManifest, String>` and held by
  `AppState` as `Arc<dyn AgentLoader>`. The loading logic itself lives in
  `apollia-aip` (`loader` plus `validator`), and the concrete `dyn AgentLoader`
  implementation is assembled in the binary crates (`apollia-cli`,
  `apollia-desktop`), which already depend on every crate. Those impls delegate
  into the `apollia-aip` loading logic, so `apollia-runtime` itself links no
  PyO3 while the API handler that starts an agent still loads and validates the
  real Python module through the injected trait object.

## Alternatives considered

### into_future with a custom test harness (rejected)
- Pros: native `pyo3-async-runtimes` integration, a shared event loop across
  calls.
- Cons: requires a custom harness incompatible with `#[tokio::test]` and complex
  global initialization; without it, tests deadlock because no event loop drives
  the coroutine.

### asyncio.run inside with_gil without spawn_blocking (rejected)
- Pros: no extra thread.
- Cons: blocks the Tokio worker for the whole Python call, starving other async
  tasks.

### Forcing PYO3_PYTHON in .cargo/config.toml, or requiring full Xcode (rejected)
- Pros: zero per-developer setup.
- Cons: a hardcoded path breaks across machines; a full Xcode install is 12+ GB,
  disproportionate to a Python link issue.

### Consuming crates depend on apollia-aip directly, or behind a feature flag (rejected)
- Pros: no trait indirection.
- Cons: every consuming crate would transitively link PyO3 and need Python for
  its tests; a `python` feature flag would fork the code into two maintained
  paths.

### Chosen: spawn_blocking bridge plus injectable traits
- Pros: non-blocking for Tokio, standard test harness, pure-Rust crates testable
  with mocks, no dependency cycle, and the traits extend naturally to future
  non-Python runners or loaders (WASM, containers).
- Trade-offs: each Python call monopolizes one blocking-pool thread and creates a
  fresh event loop; each component carries one extra injected field; dynamic
  dispatch through a vtable, which is negligible.

## Consequences

- Positive: the runtime and engine crates compile and test without Python; the
  GIL is never held on an async worker; existing agents run unmodified.
- Negative / trade-off: a blocking thread per concurrent agent call (Tokio's
  default pool is 512); an extra setup step on macOS; a less explicit link error
  when `PYO3_PYTHON` is unset.
- Watch: pressure on the blocking pool past roughly fifty concurrent agents;
  these boxed-future traits can be simplified once async fn in traits is fully
  ergonomic.

## Architectural principles

- Principle #2 (Zero external dependency): the embedded Python is shipped in the
  binary; Homebrew Python is a dev-time prerequisite only.
- Principle #3 (Minimal contract): `AgentLoader` exposes only
  `load_and_validate`, mirroring the minimal agent contract.
- Principle #5 (One actor, one responsibility): the bridge only calls Python, the
  executor only executes, the engine only supervises, the loader only loads.
- Principle #7 (Non-negotiable safeguards): the `StepBudget` is enforced by
  `ORIAEngine` independently of which concrete runner is injected.

## Related

- [ADR-001](ADR-001-foundations-stack.md) defines the Rust runtime this bridge
  plugs into.
