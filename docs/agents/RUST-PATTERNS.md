# RUST-PATTERNS

> Rules for any Rust change in the Apollia workspace. Read this before editing
> `crates/*`. Pair with the nearest crate `AGENTS.md` for crate-specific rules.

---

## 1. Error handling

**Always `thiserror` per crate. Never `anyhow` in the workspace** (except
`apollia-cli` `main()` as the last barrier).

Pattern : one error enum per crate, named `<Domain>Error`. Variants describe
the failure category. `#[from]` is attached to the inner `source` field, not
to the variant itself (no blind delegation).

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("failed to read {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML at {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("missing required field: {field}")]
    MissingField { field: &'static str },
}
```

Rules :

- `#[non_exhaustive]` on every public error enum.
- `# Errors` rustdoc section on every public fn returning `Result`.
- Error messages : lowercase, no trailing period, factual. Bad : `"Failed to
  open the file."`. Good : `"failed to open {path}"`.
- Propagate with `?`. Add context via a new variant, not via string formatting.
- Test at least one error case per variant.

Forbidden : `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` in
production. Tests only. Production exceptions require an inline `// SAFETY:`
comment proving the invariant.

---

## 2. Async and the Tokio actor pattern

Apollia is a Tokio actor system. The canonical pattern :

```rust
pub struct FooHandle {
    tx: mpsc::Sender<FooMessage>,
}

impl FooHandle {
    pub async fn do_thing(&self, arg: Arg) -> Result<Reply, FooError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(FooMessage::DoThing { arg, reply: reply_tx }).await
            .map_err(|_| FooError::ActorGone)?;
        reply_rx.await.map_err(|_| FooError::ActorGone)?
    }
}

enum FooMessage {
    DoThing { arg: Arg, reply: oneshot::Sender<Result<Reply, FooError>> },
    Shutdown,
}

struct FooActor { /* private state */ }

impl FooActor {
    async fn run(mut self, mut rx: mpsc::Receiver<FooMessage>, cancel: CancellationToken) {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                msg = rx.recv() => match msg {
                    Some(FooMessage::DoThing { arg, reply }) => {
                        let _ = reply.send(self.do_thing(arg).await);
                    }
                    Some(FooMessage::Shutdown) | None => break,
                },
            }
        }
        // cleanup
    }
}
```

Rules :

- One actor, one responsibility. Audit the actor if it grows past one
  enum variant family.
- `mpsc::channel` bounded. Default sizes : `apollia-runtime` 1024, others 256.
  Document the choice in the crate `AGENTS.md` if you deviate.
- `broadcast::channel` for fan-out events. Capacity validated in
  `[64, 65536]`, default 1024 (see `crates/apollia-runtime/src/eventbus.rs`).
- Handle structs are `Clone`. Internal actor state is owned and never `Clone`.
- Never `Arc<Mutex<T>>` between actors. Use messages.
- `tokio::select!` requires cancel-safety on every branch. Add `biased;` when
  shutdown must win.
- Use `CancellationToken` (from `tokio-util`) for coordinated shutdown when
  cleanup is non-trivial. Dropping the `Sender` alone is not enough.
- `tokio::spawn_blocking` for sync I/O and CPU-bound work above a few
  milliseconds. Never `std::thread::sleep` in async code.
- `JoinSet` for supervising tasks. Not `FuturesUnordered<JoinHandle<T>>`.
- Never `#[async_trait]` in new traits. Use return-position `impl Trait` in
  traits (RPITIT) with `Send` bounds.

---

## 3. Tracing and structured logging

Every event is a structured `tracing` call. No format strings.

```rust
// Static message in domain.action[.qualifier] form, then typed fields.
tracing::info!(
    agent_id = %agent_id,
    task_id = %task_id,
    duration_ms = elapsed.as_millis(),
    "task.completed",
);
```

Rules :

- Field prefixes : `?val` (Debug), `%val` (Display), bare `val` (typed).
- Levels : `ERROR` (unrecoverable, user-impacting), `WARN` (degraded but OK),
  `INFO` (business events), `DEBUG` (dev traces), `TRACE` (very verbose).
- Static message format : `domain.action[.qualifier]`. Examples :
  `agent.started`, `tool.invoked`, `memory.recall.failed`.
- Stable field names workspace-wide : `agent_id`, `task_id`, `skill_id`,
  `step`, `tool_name`, `duration_ms`, `bytes_read`, `bytes_written`,
  `error_kind`. Full table in `docs/agents/OBSERVABILITY.md`.
- `#[instrument(skip(large_field), fields(req_id = %req.id))]` on async fns
  worth tracing. Skip heavyweight arguments.
- Log errors once. Either log at the failure site or bubble up, not both.
- `println!`, `eprintln!`, `dbg!` are forbidden outside `apollia-cli`.

---

## 4. Code patterns

**Newtypes for semantic IDs.** `AgentId(String)`, `TaskId(String)`,
`SkillId(String)`, `StepId(String)`, `SessionId(String)`. Each implements
`Display`, `From<&str>`, `AsRef<str>`, `PartialEq`, `Eq`, `Hash`. Source :
`crates/apollia-core/src/events/`.

**Typestate with `PhantomData<State>`** when an API has a required call order
(e.g. builder must observe `with_socket` before `build`).

**PyO3 0.24** : always `Bound<'py, T>` on the Python boundary. `Py<T>` only
for cross-GIL ownership. `Python::with_gil` is forbidden outside one-shot
setup and tests. Pair with `pyo3-async-runtimes` : `future_into_py` for
Rust→Python, `into_future` for Python→Rust.

**Module organization** : prefer `foo.rs + foo/sub.rs` over `foo/mod.rs`
(post-2024 idiom). Apollia matches.

**Visibility minimalism** : `pub(crate)` by default. `pub` only on the
contract surface re-exported from `lib.rs`. Audit : `grep -c "^pub " src/`
should not exceed 30% of items in any crate.

**Re-exports** : `lib.rs` re-exports the public contract by name. Never
`pub use crate::internal::*`.

**Iterator combinators > explicit loops** when the loop body is one
expression. Use `for` when it has side effects, early returns, or breaks
readability.

**Borrowing > ownership.** Take `&str` over `String` unless ownership is
required. `Cow<'_, str>` when the choice is conditional.

**Argument count** : max 5 positional. Above that, take a struct.

**Exhaustive matches.** Avoid `_` catch-all unless the enum is
`#[non_exhaustive]` or you genuinely want to ignore future variants.

**`Vec::with_capacity(n)`** when the size is known.

---

## 5. PyO3 bridge specifics

The `apollia-aip` crate is the boundary. Read `crates/apollia-aip/AGENTS.md`
for crate-local rules.

Highlights :

- Wrap Python errors at the boundary into typed Rust errors. Never let a
  `PyErr` escape past `apollia-aip`.
- Use `pyo3_async_runtimes::tokio::future_into_py(py, async move { ... })` to
  hand a Rust future to Python.
- Free-threaded Python is on the roadmap. Make `#[pyclass]` types `Sync`
  when possible to keep that path open.

---

## 6. Cargo workspace conventions

- `[workspace.dependencies]` centralizes versions. Every crate writes
  `dep = { workspace = true }`. Single source of truth.
- `[workspace.lints.rust]` and `[workspace.lints.clippy]` apply globally,
  inherited per crate with `[lints] workspace = true`.
- `[workspace.package]` shares `edition = "2021"`, `license`,
  `repository`, `rust-version`.
- `rust-version = "1.89"` is the MSRV. Tested in CI.
- Features are additive and kebab-case. `default = []` is documented when
  it includes optional functionality.
- Adding a new dependency : require an ADR if the dep is heavy (>100k LoC),
  GPL-licensed, or replaces an in-tree solution.

---

## 7. Lints

The workspace sets a deliberately small lint floor in the root `Cargo.toml` :

```toml
[workspace.lints.rust]
unsafe_code = "deny"
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(fuzzing)', 'cfg(kani)'] }

[workspace.lints.clippy]
unwrap_used = "deny"
```

`unexpected_cfgs` whitelists the `fuzzing` and `kani` cfgs so `-D warnings`
stays green on normal builds where neither is set.

**A crate inherits those lints only through `[lints] workspace = true`, and
declaring any `[lints]` table of its own replaces the inheritance instead of
adding to it.** Cargo does not merge the two. Five crates needed
`unsafe_code = "allow"` for FFI, wrote it into their own `[lints.rust]`, and
silently lost `unwrap_used = "deny"` with it. Nothing failed: clippy went on
passing, and an `unwrap()` sat in production code in `apollia-tools` without
anyone being told.

So a crate that overrides anything must restate what it still wants:

```toml
[lints.rust]
unsafe_code = "allow"     # FFI / proc-macro generated

[lints.clippy]
unwrap_used = "deny"      # NOT redundant: restates what the override dropped
```

A local table declaring `unexpected_cfgs` must also carry every `check-cfg` the
workspace declares. Adding one is fine, `cfg(loom)` for instance; dropping
`cfg(fuzzing)` or `cfg(kani)` is how a future harness breaks the build in a
crate nobody thought to look at.

Tests re-allow `unwrap` at the crate root, which is where the exemption belongs
because it is scoped to a compilation mode rather than to a crate:

```rust
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
```

**None of the above is enforced by you remembering it.** `scripts/check_crate_lints.py`
runs in the `prose-guard` job and fails the build when a crate declares a local
table without restating the lint, or loses a `check-cfg`. That check exists
because the paragraph above already existed as a habit and did not hold: the
whole point of this rulebook is that a written rule is not a gate.

The rest of the NEVER list (`expect`, `panic!`, `todo!`, `println!`, `dbg!`,
`anyhow`, ...) is enforced by `docs/agents/FORBIDDEN.md`, review, and the
pre-commit hooks, not by a clippy lint line. CI runs
`cargo clippy --workspace --all-targets -- -D warnings`, so any clippy default
lint that fires is still a hard failure.

`unsafe_code = "deny"` is workspace-wide. To use `unsafe`, the crate overrides
with `unsafe_code = "allow"` plus a top-of-crate explanation, and every
`unsafe` block carries a `// SAFETY:` comment.

---

## 8. Documentation comments

- `///` outer on every `pub` item. `//!` inner at the crate or module top
  for overviews.
- First line : short, third-person present indicative. Bad : "Return the
  next id". Good : "Returns the next id".
- Sections, always plural : `# Examples`, `# Errors`, `# Panics`, `# Safety`.
- Code blocks default to `rust`. Mark `no_run`, `ignore`, `compile_fail`,
  `should_panic` when needed.
- Doctests must compile. CI enforces it.

Detailed rules in `docs/agents/DOCS-WRITING.md`.

---

## 9. When the rules block you

Either document an exemption inline (`// SAFETY:`, `// REASON:`, ADR
reference) or surface the conflict to the user before silently bending the
rule.
