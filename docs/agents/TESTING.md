# TESTING

> Cross-stack testing matrix and conventions for Apollia. Read this before
> writing or reviewing any test.

Apollia is multi-language (Rust + Python + TypeScript) and multi-layer
(crates, SDK, desktop UI, CLI, HTTP API, actor mesh, LLM and MCP backends).
The matrix in §1 maps every test level to the tools and conventions that
apply.

---

## 1. Matrix : level × scope

|  | Rust crates | Python SDK | Tauri desktop | CLI | HTTP API | Actor mesh | LLM / MCP backends |
|---|---|---|---|---|---|---|---|
| **Unit** | `#[test]`, `#[tokio::test]`, inline `#[cfg(test)] mod tests` | pytest, pytest-asyncio | Vitest + @testing-library/svelte | clap parsing tests | unit fn tests | per-actor message tests | mocked backends |
| **Integration** | per-crate `tests/` directory | pytest + `tmp_path` fixtures | Playwright on the bundle with the Tauri bridge stubbed | `assert_cmd` + `predicates` | `axum-test::TestServer`, `tower::ServiceExt::oneshot` | inter-actor channel exchanges | `wiremock`, `respx` |
| **E2E** | delegated to CLI tests | n/a | none on the packaged app (no WKWebView WebDriver) | `tests/cli/cli-e2e.sh` Track 1 (OFFLINE) + Track 2 (RUNTIME) + Track 3 (LLM capture), seeded fixture | CLI → API → DB round-trip | full `Runtime::spawn` test harness | live providers, gated by env vars |
| **Property** | `proptest` | `hypothesis` | n/a | proptest on argv | proptest on JSON payloads | sequence-shrinking on actor message streams | n/a |
| **Snapshot** | `insta` | `syrupy` | none; no visual baseline suite exists | snapshot CLI human + `--json` output | `insta` on serialized response | event-stream traces | tool outputs |
| **Benchmark** | `criterion` | `pytest-benchmark` | Lighthouse | `hyperfine` | `wrk` or criterion | criterion ops/sec | n/a |
| **Fuzzing** | `cargo-fuzz` (libFuzzer) | `atheris` if needed | n/a | `cargo-fuzz` on argv | `cargo-fuzz` on HTTP payloads | n/a | n/a |

---

## 2. Discipline (applies to every test level)

**GIVEN / WHEN / THEN comments mark each block.** This discipline exposed
every argument-parsing defect in the CLI tree. Do not skip it.

```rust
#[tokio::test]
async fn test_eventbus_publish_blocks_when_capacity_full() {
    // GIVEN an EventBus with capacity 2 and 2 unread events
    let bus = EventBus::with_capacity(2);
    let _rx = bus.subscribe();
    bus.publish(RuntimeEvent::AgentStarted { id: "a".into(), at: now() });
    bus.publish(RuntimeEvent::AgentStarted { id: "b".into(), at: now() });

    // WHEN publishing a third event with no consumer drain
    let result = bus.try_publish(RuntimeEvent::AgentStarted { id: "c".into(), at: now() });

    // THEN it returns Lagged, not Ok
    assert!(matches!(result, Err(EventBusError::Lagged(_))));
}
```

**One test = one behavior.** Multi-assertion tests that touch unrelated paths
become impossible to triage when one fails.

**`pretty_assertions::assert_eq!(full_struct, expected)` over multi-asserts**
when comparing structures. The diff output is dramatically better.

**Test naming** : `test_<unit>_<scenario>_<expected>`. See
`docs/agents/NAMING.md` §9.

**At least one error case per public enum variant.** A `Result<T, MyError>`
return type with no test of any error branch is incomplete.

**No `#[ignore]` merged without a story link.** A skipped test that no one
tracks is dead code.

**No tests that depend on ordering.** Use `serial_test` if a global mutex is
genuinely required.

**Never `--test-threads=1`.** It hides deadlocks. The remedy is fixing the
deadlock, not serializing the suite.

**Tests run in CI with `cargo nextest`** for parallelism and isolation
(3x faster, process-level isolation). Doctests still run with `cargo test
--doc`.

---

## 3. Rust unit tests

Place `#[cfg(test)] mod tests` at the bottom of the file under test.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_valid() { /* ... */ }

    #[tokio::test]
    async fn test_actor_handles_shutdown() { /* ... */ }

    #[tokio::test(start_paused = true)]
    async fn test_timeout_after_5s() {
        let task = spawn_with_timeout(Duration::from_secs(5));
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert!(task.is_finished());
    }
}
```

Rules :
- `#[tokio::test(start_paused = true)]` whenever timing matters. Deterministic.
- Mock time via `tokio::time::pause` / `tokio::time::advance` when the test
  is too complex for the rune macro.
- Mock traits via `mockall` for traits with many methods. Hand-written
  mocks for two- or three-method traits.

---

## 4. Rust integration tests

Per-crate `tests/` directory. Each file is its own integration binary.

```
crates/apollia-permissions/
├── src/
└── tests/
    ├── integration_governance.rs
    └── integration_audit_trail.rs
```

Rules :
- Use the public API only. If you reach into private modules, that is a
  unit test misplaced.
- Real SQLite (in-memory or `tmp_path`). Never mock the database for
  integration tests of the persistence layer.
- Use `axum-test::TestServer` to exercise HTTP handlers. It mounts the
  router and dispatches requests without a real bind.
- `wiremock` for external HTTP dependencies (LLM cloud providers, OAuth
  providers).

---

## 5. CLI tests

CLI carries two distinct levels :

### 5.1 Parsing tests (unit-level)

Inline in `crates/apollia-cli/src/commands/<noun>.rs`. One test per
sub-command + flag combination.

```rust
#[test]
fn test_agent_list_parse_json() {
    let cli = Cli::try_parse_from(["apollia", "agent", "list", "--json"]).unwrap();
    assert!(matches!(cli.command, Commands::Agent(AgentCommands::List { json: true, .. })));
}
```

Target : 150+ parsing tests workspace-wide (acquired in the CLI sprint).

### 5.2 End-to-end (`tests/cli/cli-e2e.sh`)

Orchestrator (bash) over a fixed, deterministically-seeded HOME (built by
`tests/cli/seed/build-seed.sh`, never its optional narrative overlay),
with a machine + human report (`tests/cli/report/report.{json,md}`). Three
tracks :
- **Track 1 (OFFLINE)** : every daemon-free command against the seeded HOME.
  Asserts KNOWN seeded content (not empty states) + the exit-code contract.
  Runs on every PR (`cli-e2e` job in `ci.yml`).
- **Track 2 (RUNTIME, opt-in `APOLLIA_REQUIRE_RUNTIME=1`)** : daemon booted on
  the seeded HOME; seeded runtime reads + CRUD + runtime-only leaves.
- **Track 3 (LLM CAPTURE, opt-in + a real model `APOLLIA_TEST_MODEL_GGUF`)** :
  non-deterministic commands (`run --stream`, `chat` REPL via pty, `llm chat`,
  `do`, `explain`). Asserts STRUCTURE only (exit, streaming happened, timing);
  the input/output is captured into `report.md` for human review.

Exit codes 0-5 are tested explicitly. See `crates/apollia-cli/AGENTS.md`
for the contract, and `tests/cli/README.md` for the full layout.

---

## 6. Python SDK tests

```toml
# pyproject.toml
[tool.pytest.ini_options]
asyncio_mode = "strict"
markers = [
  "unit: fast unit test",
  "integration: integration test, may touch the filesystem",
  "slow: skipped unless --run-slow is passed",
]
```

Rules :
- `asyncio_mode = "strict"` : every async test carries
  `@pytest.mark.asyncio` explicitly. Prevents collision with `trio` /
  `anyio` runners.
- Hypothesis for property-based tests :

  ```python
  from hypothesis import given, strategies as st

  @given(st.text())
  def test_normalize_idempotent(s: str) -> None:
      assert normalize(normalize(s)) == normalize(s)
  ```

- `syrupy` for snapshots :

  ```python
  def test_agent_manifest_shape(snapshot):
      assert build_manifest(my_agent) == snapshot
  ```

- `respx` for HTTP mocks. Never `requests_mock` (sync-only).
- Fixture scoping : function > module > session. `autouse=True` only for
  project-wide setup.

---

## 7. Desktop tests

- **Component tests** : Vitest + `@testing-library/svelte`. Co-located in
  `*.test.ts` next to the component.
- **Browser tests** : Playwright, in `crates/apollia-desktop/ui/tests/`, run
  against the production bundle served by `vite preview` with the Tauri bridge
  stubbed. They cover machinery that needs a real browser: dirty state, nav
  guards, hotkey capture, responsive layout, perf. They do **not** exercise the
  packaged application.
- **E2E on the packaged application** : none in this repository. macOS has no
  WebDriver for WKWebView, so the Tauri shell cannot be driven by a standard
  browser harness. The runtime paths behind the UI are covered through
  `tests/cli/cli-e2e.sh`, which drives the same commands against a seeded
  throwaway `HOME`.
- There is no `tauri-driver` setup and no `tests/visual/` baseline suite. Do not
  write a test that assumes either, and note the package manager is `npm`.

Rules :
- A component test must not call Tauri IPC. Mock the IPC wrapper instead.
- An E2E test does call Tauri IPC. It runs against a built app with a
  scratch profile (`APOLLIA_HOME=$(mktemp -d)`).

---

## 8. Property-based testing

Use `proptest` (Rust) or `hypothesis` (Python) when the input space is
large and invariants are well-defined :

- Serializers : `roundtrip(value)` returns the same value.
- Idempotent operations : `f(f(x)) == f(x)`.
- Commutative operations : `merge(a, b) == merge(b, a)`.
- Parser robustness : parsing arbitrary bytes never panics.

`proptest` is preferred over `quickcheck` for explicit, composable shrinking
strategies.

---

## 8b. Fuzzing (untrusted-input parsers)

Parsers that consume untrusted input (LLM output text, remote web content,
natural-language automation text, tool specs, network payloads) are fuzzed with
`cargo-fuzz` (libFuzzer). A panic in one of these is a reachable crash, so the
invariant is simple : parsing any input never panics.

The `fuzz/` crate is a standalone package, excluded from the workspace (see
`exclude = ["fuzz"]` in the root `Cargo.toml`). It targets nightly because
libFuzzer needs `-Zsanitizer=address`. Targets live in `fuzz/fuzz_targets/` and
the committed seed corpus in `fuzz/seeds/<target>/`.

```sh
cargo +nightly fuzz build                    # compile every target
cargo +nightly fuzz run parse_automation fuzz/corpus/parse_automation fuzz/seeds/parse_automation
cargo +nightly fuzz tmin <target> <crash>    # minimize a crash artifact
```

Rules :

- A target must call the **real** production parsing function, never a copy.
  When the function is private, expose it through a `#[cfg(fuzzing)] pub` shim in
  the owning crate (`fuzzing` is declared as a known cfg in `[workspace.lints.rust]`
  and in `apollia-tools`). The shim is compiled only under `--cfg fuzzing`.
- A crash is a real bug. Minimize it, anchor it to `file:line`, and fix it (or
  file it) rather than masking it.
- CI runs a short smoke over the seed corpus on each PR (`Fuzz (smoke)` in
  `ci.yml`, advisory) and a longer session weekly (`Deep Fuzz` in `nightly.yml`,
  uploads any crash artifact). This cargo-fuzz integration also satisfies the
  OpenSSF Scorecard Fuzzing check.

---

## 8c. Concurrency and UB verification (Loom + Miri)

Two model-checking tools guard the concurrent core and the FFI frontier. Both
are dev-only (no runtime dependency) and advisory in CI.

**Loom** exhaustively permutes thread interleavings to prove a concurrency
algorithm race-free. It cannot instrument Tokio (the runtime actors use
`tokio::sync` channels + `Semaphore` on the Tokio scheduler, none of which Loom
sees), and `--cfg loom` is a global flag that poisons `tokio::net`. So the Loom
models are **abstract**: they re-implement each actor's core algorithm with Loom
primitives and live in the standalone `crates/apollia-loom-models` crate,
excluded from the workspace exactly like `fuzz/`. Each model cites the prod
`file:line` it mirrors.

```sh
RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=3 \
  cargo test --manifest-path crates/apollia-loom-models/Cargo.toml --release
```

Honesty boundary: a Loom model proves the **algorithm** is race-free, not that
the exact Tokio code is. `mailbox_lease_exclusivity` modelled the recommended
lease-owner fence before prod carried it; that fence now ships (the `lease_owner`
column + null-safe `IS` fence on `ack`/`nack`, also proven bit-precise by Kani, 8d
below). `shutdown_drain_snapshot_gap` still models the recommended fix for a
hazard prod does not yet implement; that delta remains a tracked finding, not a
proof about current prod.

**Miri** interprets MIR to detect undefined behavior (bad casts, invalid
pointers, data races). It cannot execute the PyO3 boundary (`Python::with_gil`
calls into libpython, an unsupported foreign function), so the suite targets
only interpreter-free helpers near the boundary, named `miri_pure` in
`apollia-aip`.

```sh
cargo +nightly miri test -p apollia-aip --lib miri_pure
```

Out of Miri's reach and covered instead by integration tests + manual SAFETY
review: every `with_gil`/`extract`/`future_into_py` path, and the whisper.cpp
`unsafe impl Send/Sync` in `apollia-runner`.

Rules :

- Keep Loom models abstract and prod-decoupled. Never add `loom` to a
  workspace crate; it would drag `--cfg loom` into Tokio.
- A Miri `miri_pure` test must not call into libpython, directly or transitively.
- CI runs both weekly in `nightly.yml` (`loom`, `miri`), advisory.

---

## 8d. Bounded symbolic proof (Kani)

Where a property must hold for **every** input, not a sampled subset, Kani (the
AWS bit-precise Rust model checker) proves it exhaustively over a bounded space.
It is reserved for the cardinal invariants; proving the whole codebase this way
is not the goal.

Two invariants carry Kani harnesses today, each paired with an in-tree proptest
mirror that runs under `cargo test` :

- **Non-bypassable StepBudget** (`crates/apollia-oria/src/budget.rs`) : the
  effective cap is `min(agent, runtime)` so an agent can never raise the runtime
  ceiling; exhaustion is stable once reached; the counter increment is
  overflow-free. Proven over the whole `u32` domain.
- **Mailbox lease/ack fence** (`crates/apollia-runtime/src/mailbox.rs`) : a stale
  consumer whose lease was re-leased to another run cannot ack or nack the
  message; owner match is exactly the null-safe SQL `IS`; an expired lease stays
  redeliverable (at-least-once).

The harnesses prove the **pure decision helpers** (`effective_cap`,
`dimension_exhausted`, `remaining`; `is_deliverable`, `owner_matches`, the lease
transitions), which each re-encode one prod predicate and cite the line they
mirror. The real atomic `StepBudget` and the real SQLite store are bound to the
model by the proptest and the end-to-end regression test
(`test_ack_fenced_to_lease_owner`), not by Kani.

```sh
cargo install --locked kani-verifier && cargo kani setup
cargo kani -p apollia-oria
cargo kani -p apollia-runtime
```

Honesty boundary: a harness proves the property only over its **bounded** space
(the whole `u32` for budget; owner identities and Unix-second times bounded by
`kani::assume` for mailbox). Do not claim "proven correct" beyond that. The
wall-clock budget dimension (`Instant`) and the `None`/`None` owner collision are
out of scope and stay documented as such.

Rules :

- Kani links its own toolchain via rustup: it is CI-only (advisory `kani` job in
  `nightly.yml`). Locally, the proptest mirrors are the runnable proof.
- Gate every harness on `#[cfg(kani)]`; the cfg is whitelisted workspace-wide.
- A harness must exercise the same pure helper the prod path uses. Never prove a
  divergent model.

---

## 9. Snapshot testing

Use `insta` (Rust) or `syrupy` (Python) for :
- Complex structured outputs (ORIA plans, manifests, tool descriptors).
- CLI human-readable output (the rendered table for `apollia agent list`).
- Serialized HTTP responses where the schema is wide.

Update workflow : `cargo insta review` (Rust) or `pytest --snapshot-update`
(Python). Never commit snapshot updates without reviewing the diff.

---

## 10. Benchmarks

- `criterion` for Rust micro-benchmarks. Place in `benches/` per crate.
- `pytest-benchmark` for Python.
- `hyperfine` for full-binary CLI invocations.
- Never benchmark with `target-cpu=native` or `--release` differing from
  production build configuration.
- Run benchmarks in isolation (no parallel workload). Document the
  environment in `BENCH-README.md` per crate.

---

## 11. Coverage targets

- Lines : > 80% on core crates (`apollia-core`, `apollia-runtime`,
  `apollia-oria`, `apollia-permissions`, `apollia-memory`).
- Branches : > 70% on the same.
- Workspace-wide : aspirational, not gated.

Tooling : `cargo llvm-cov nextest --lcov --output-path lcov.info` then
upload to Codecov.

---

## 12. CI gate

`cargo test --workspace --no-fail-fast` and `pytest` both pass before a PR can merge.
The flag matters: cargo otherwise stops at the first failing test binary, and a run
that covered a third of the suite reads exactly like a full green one.
Pre-commit hook enforces this locally. Do not bypass.

Sequence : `cargo fmt --check` -> `cargo clippy --workspace -- -D warnings`
-> `cargo nextest run --workspace` -> `cargo test --doc` -> `pytest` ->
`pnpm test` (desktop) -> `bash tests/cli/cli-e2e.sh` (Track 1, offline).

---

## 13. When the rules block you

- Test is genuinely flaky : open an ADR explaining the source of
  non-determinism and the mitigation. Never `#[ignore]` without an ADR.
- Need to test private internals : restructure to expose a `pub(crate)`
  facade for the test. Do not reach into private modules.
- Long-running test : mark `@pytest.mark.slow` or `#[ignore]` with a
  story link.
