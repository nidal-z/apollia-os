# crates/apollia-aip/AGENTS.md

> Local rules for the PyO3 bridge. Read after the root `AGENTS.md` and
> before editing this crate. Pair with `docs/agents/RUST-PATTERNS.md` §5.

`apollia-aip` is the only crate that crosses the Rust/Python boundary. Every
Python interaction in the runtime funnels through here. Patterns are
specific and unforgiving.

---

## 1. PyO3 0.24 conventions

**Always `Bound<'py, T>` on the boundary.** `&PyAny` is deprecated and the
crate carries none; keep it at zero.

`PyObject` (that is, `Py<PyAny>`) is a different case and the rule is not
"never". The crate holds 56 production sites, and they are where an owned
handle has to outlive a GIL scope: a value stored in a struct that a Tokio
task moves, or returned out of a `with_gil` closure. Inside a scope that
already has a `Python<'py>`, use `Bound<'py, T>`; reach for `Py<T>` at the
moment the value leaves that scope, and nowhere else.

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyfunction]
fn build_context<'py>(
    py: Python<'py>,
    workspace: Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    // populate from workspace
    Ok(out)
}
```

`Python::with_gil` is not forbidden here, and never was. The crate carries 59
production sites over 17 files, because a Tokio task resumed after an `await`
has no `Python<'py>` in scope and `pyo3-async-runtimes` offers no equivalent
for those paths. What the rule actually is:

- Inside a `#[pyfunction]` or any function that already takes `Python<'py>`,
  use the parameter. Opening a nested `with_gil` there is redundant.
- Across a `.await`, or inside a closure a Tokio task owns, `with_gil` is the
  way in. Keep the closure short, and never `await` inside one.
- The whole tree holds 64 sites: the 59 above, 4 in `apollia-cli` and 1 in
  `apollia-desktop`, all on the same boundary shape.

Both counts are measurable:
`python3 scripts/check_rust_rules.py --help` names the sweep, and
`git grep -c 'Python::with_gil' -- 'crates/**/*.rs'` gives the raw figure
including tests.

---

## 2. Async interop

`pyo3-async-runtimes::tokio` is the bridge :

```rust
use pyo3_async_runtimes::tokio::future_into_py;

#[pyfunction]
fn dispatch<'py>(
    py: Python<'py>,
    skill_id: String,
    payload: Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    let payload: Payload = payload.extract()?;
    future_into_py(py, async move {
        let reply = run_skill(&skill_id, payload).await?;
        Python::with_gil(|py| Ok(reply.into_py(py)))
    })
}
```

Rules :
- `pyo3_async_runtimes::tokio::future_into_py(py, async { ... })` turns a Rust
  future into a Python awaitable. That is the direction this crate uses.
- Never let `pyo3-async-runtimes` build its own runtime. `pin_async_runtime()`
  (`src/runtime_pin.rs`) calls `init_with_runtime` on the shared
  `apollia_runtime::worker_runtime()` once, guarded by a `Once`, and logs
  `aip.runtime.pin_failed` when it cannot. Call it at the composition root,
  after the worker runtime starts and before the first `future_into_py`.

---

## 3. Error mapping

Python errors must not escape `apollia-aip`. Wrap at the boundary into a
typed `thiserror` enum. There is no single `AipError`: each module owns its
own, and the runtime matches on the one it received.

| Enum | Module | Covers |
|---|---|---|
| `AIPBridgeError` | `src/bridge.rs` | dispatch into a loaded agent |
| `AIPLoaderError` | `src/loader.rs` | importing and instantiating a module |
| `AIPValidationError` | `src/validator.rs` | manifest and tool-requirement validation |
| `PackageLoaderError` | `src/package_loader.rs` | agent packages |
| `ToolProxyError` | `src/context/tool_proxy.rs` | `ctx.tools` invocations |
| `MemoryInterfaceError` | `src/memory.rs` | `ctx.memory` calls |
| `PythonProviderError` | `src/python_provider.rs` | interpreter discovery and the venv |

A new boundary gets its own enum rather than a variant on someone else's, and
a `PyErr` is captured into a `String` message at the point of failure so no
`PyErr` crosses the crate boundary.

---

## 4. Manifest extraction

The decorator-built manifest is read from the
class attribute, not by introspecting the methods at every call.

The attribute is `__apollia_manifest__`, built by the `@agent` decorator, and
the dispatch entry point the bridge requires is `__apollia_dispatch__`: an
object without it is refused, the legacy `manifest()` plus `run()` pair is
gone. Read them with `getattr` on a `Bound<'py, PyAny>` and map the failure
into the module's own error, never `unwrap`.

Validation runs before the agent is marked ready. Fail fast.

---

## 5. `RuntimeContext` exposure

`Ctx` is the Python-side capability bundle. Source of truth for the type
contract : the `Ctx` Protocol in `sdk/apollia/types.py` and the per-service
interfaces in `sdk/apollia/context/*.py`. The implementation in this crate
must keep parity with that contract.

`Ctx` exposes 15 typed services (`ctx.llm`, `ctx.memory`, `ctx.tools`,
`ctx.a2a`, `ctx.mail`, `ctx.datasources`, `ctx.templates`, `ctx.secrets`,
`ctx.events`, `ctx.logger`, `ctx.profile`, `ctx.workspace`, `ctx.stt`,
`ctx.notify`, `ctx.budget`). Read the exact method signatures from
`sdk/apollia/context/<service>.py`. Note `ctx.logger` (structured logging),
not a bare `ctx.log`, and `ctx.tools` (plural).

When you add a method, update :
1. The Protocol in `sdk/apollia/types.py` and the service module in
   `sdk/apollia/context/<service>.py`.
2. The implementation here.
3. The decisions chapter of the documentation site if the addition changes
   what `ctx` guarantees, or how the mailbox behaves.

---

## 6. GIL discipline

- Hold the GIL for as little code as possible.
- Never block the Tokio runtime while holding the GIL. The pattern is :
  acquire the GIL -> read or write Python state -> leave the closure -> do the
  async work -> reacquire if needed.
- `Python::allow_threads` is the documented way to release the GIL around long
  Rust work, and the crate uses it nowhere today (0 sites). Prefer leaving the
  `with_gil` closure to wrapping the work in `allow_threads`; reach for it only
  when the Rust work genuinely has to run inside a scope that holds the GIL.
- Free-threaded Python is on the roadmap. Make `#[pyclass]` types `Sync`
  when feasible so the migration path stays open.

---

## 7. Lifecycle

Agent lifecycle steps that pass through this crate :

1. **Load** : `load_agent_module` (`src/loader.rs`) imports the module and
   returns the agent object. Fail fast into `AIPLoaderError`.
2. **Validate** : `src/validator.rs` checks the manifest, the declared tools
   and the secrets, into `AIPValidationError`.
3. **Dispatch** : `AIPBridge::call_run` (`src/bridge.rs`) invokes the skill,
   `call_on_plan_complete` the orchestrated post-processing.

There is no `activate` step, no `unload` step and no cancellation path through
this crate: the bridge holds no `CancellationToken`, and a running Python
coroutine is not interrupted from the Rust side. Cancellation is enforced one
level up, by the budget and the task status, not here. Do not write a rule
against a step this crate does not have.

These steps are not instrumented with a span family of their own: there is no
`agent.aip.*` span in the crate. What exists is per-call tracing events on the
domain-action form the root `AGENTS.md` requires. Adding the span family is a
change to `docs/agents/OBSERVABILITY.md` first, then here.

---

## 8. Testing

- The tests are inline, in `#[cfg(test)]` modules beside the code. This crate
  has no `tests/` directory and no fixture agent tree; do not cite one.
- A test that touches the interpreter calls
  `pyo3::prepare_freethreaded_python()` first (`src/a2a.rs`, `src/events.rs`,
  `src/llm.rs`, `src/loader.rs`).
- `#[tokio::test]` is used where the code under test is async; keep the GIL
  handling explicit inside it, and never hold the GIL across the `.await`.
- GIVEN / WHEN / THEN, as everywhere.

---

## 9. Forbidden in this crate

- `await` inside a `Python::with_gil` closure, or a nested `with_gil` under a
  scope that already carries `Python<'py>`.
- `&PyAny` anywhere (use `Bound<'py, PyAny>`). The crate is at zero.
- Letting `PyErr` escape past the crate boundary.
- Creating a Tokio runtime ad-hoc (use the pinned one from `init`).
- Storing `Py<T>` in a `Send + 'static` context without explicit
  documentation of why.

---

## 10. When the rules block you

- New context method needed : add it to `sdk/apollia/types.py` and
  `sdk/apollia/context/<service>.py` first, then implement here. The
  Python contract is authoritative.
- pyo3 API surface changed (new release) : the version is pinned in the
  workspace `Cargo.toml`; update it in one focused commit, never bundled with
  feature work.
- Manifest format change : state the new shape in
  `docs/site/docs/architecture/08-decisions.md` under
  `#agent-contract` first. Manifests are an API surface.
