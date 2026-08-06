# crates/apollia-aip/AGENTS.md

> Local rules for the PyO3 bridge. Read after the root `AGENTS.md` and
> before editing this crate. Pair with `docs/agents/RUST-PATTERNS.md` §5.

`apollia-aip` is the only crate that crosses the Rust/Python boundary. Every
Python interaction in the runtime funnels through here. Patterns are
specific and unforgiving.

---

## 1. PyO3 0.24 conventions

**Always `Bound<'py, T>` on the boundary.** Never `&PyAny` (deprecated),
never raw `PyObject` in new code.

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

`Py<T>` only for ownership that crosses GIL boundaries (storage in
non-Python data structures).

`Python::with_gil` is forbidden outside one-shot setup code and tests. The
boundary already holds the GIL via `Python<'_>` parameters.

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
- `future_into_py(py, async { ... })` for Rust -> Python coroutine.
- `into_future(py_awaitable)` for Python awaitable -> Rust future.
- Always pin the Tokio runtime explicitly :
  `pyo3_async_runtimes::tokio::init(...)` in the crate init path. Never
  let `pyo3-async-runtimes` create its own runtime.

---

## 3. Error mapping

Python errors must not escape `apollia-aip`. Wrap at the boundary into a
typed Rust error.

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AipError {
    #[error("python exception during {context}: {message}")]
    PythonException { context: String, message: String, #[source] source: PyErr },
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("agent module not found: {module}")]
    ModuleNotFound { module: String },
}

impl From<PyErr> for AipError {
    fn from(err: PyErr) -> Self {
        Python::with_gil(|py| AipError::PythonException {
            context: String::from("anonymous"),
            message: err.to_string(),
            source: err.clone_ref(py),
        })
    }
}
```

The runtime never sees a `PyErr`. It sees `AipError` or a downstream
`RuntimeError` variant.

---

## 4. Manifest extraction

The decorator-built manifest is read from the
class attribute, not by introspecting the methods at every call.

```rust
fn extract_manifest<'py>(agent: Bound<'py, PyAny>) -> Result<Manifest, AipError> {
    let raw = agent
        .getattr("__apollia_manifest__")
        .map_err(|_| AipError::InvalidManifest("missing __apollia_manifest__".into()))?;
    let dict: Bound<PyDict> = raw.downcast_into()
        .map_err(|_| AipError::InvalidManifest("manifest is not a dict".into()))?;
    Manifest::from_pydict(&dict)
}
```

Validation runs at `INITIALIZING`. Fail fast.

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

- Hold the GIL for as little code as possible. Drop into `Python::allow_threads`
  for Rust work that does not touch Python.
- Free-threaded Python is on the roadmap. Make `#[pyclass]` types `Sync`
  when feasible so the migration path stays open.
- Never block the Tokio runtime while holding the GIL. The pattern is :
  acquire GIL -> read or write Python state -> release GIL -> do async
  work -> reacquire if needed.

---

## 7. Lifecycle

Agent lifecycle steps that pass through this crate :

1. **Load** : import the module, instantiate the class, extract the
   manifest. Fail fast if any step fails.
2. **Validate** : check `tools_required`, MCP availability, secrets.
3. **Activate** : ready to accept skill invocations.
4. **Dispatch** : skill call -> future_into_py -> async execution ->
   AIPResult.
5. **Cancel** : `CancellationToken` propagates to the Python side via the
   `Ctx`. The Python coroutine is awaited with shielding for cleanup.
6. **Unload** : drop the agent instance, release the module.

Each step has a tracing span :
`agent.aip.{load,validate,activate,dispatch,cancel,unload}`.

---

## 8. Testing

- Unit : Rust-only mocks for the pyo3 boundary. Use a fixture module
  loaded once per test (`pyo3::prepare_freethreaded_python()`).
- Integration : `tests/integration_*.rs` with a real Python interpreter
  and a fixture agent in `tests/fixtures/`.
- Avoid `#[tokio::test]` for tests that touch the GIL. Use
  `tokio::runtime::Runtime::new()` and `runtime.block_on(...)` to keep
  the GIL handling explicit.

---

## 9. Forbidden in this crate

- `Python::with_gil` outside one-shot init and tests.
- `&PyAny` in new code (use `Bound<'py, PyAny>`).
- Letting `PyErr` escape past the crate boundary.
- Creating a Tokio runtime ad-hoc (use the pinned one from `init`).
- Storing `Py<T>` in a `Send + 'static` context without explicit
  documentation of why.

---

## 10. When the rules block you

- New context method needed : add it to `sdk/apollia/types.py` and
  `sdk/apollia/context/<service>.py` first, then implement here. The
  Python contract is authoritative.
- pyo3 API surface changed (new release) : pin the version, update via
  one focused PR, never bundle pyo3 upgrade with feature work.
- Manifest format change : open an ADR. Manifests are an API surface.
