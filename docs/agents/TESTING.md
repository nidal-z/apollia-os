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
| **Unit** | `#[test]`, `#[tokio::test]`, inline `#[cfg(test)] mod tests` | pytest, pytest-asyncio | Vitest, `node` environment, no DOM (see §7) | clap parsing tests | unit fn tests | per-actor message tests | mocked backends |
| **Integration** | per-crate `tests/` directory, or a `#[cfg(test)]` module driving the public API | pytest + `tmp_path` fixtures | Playwright on the production bundle with the Tauri bridge stubbed | `tests/cli/cli-e2e.sh` against a seeded HOME | `tower::ServiceExt::oneshot` on the built router | inter-actor channel exchanges | `wiremock` |
| **E2E** | delegated to CLI tests | n/a | `scripts/automation/` drives a dev build of the real app by `data-testid`; the packaged bundle is driven by nothing (no WKWebView WebDriver) | `tests/cli/cli-e2e.sh` Track 1 (OFFLINE) + Track 2 (RUNTIME) + Track 3 (LLM capture), seeded fixture | CLI → API → DB round-trip | full `Runtime::spawn` test harness | live providers, gated by env vars |
| **Property** | `proptest` | none declared in `sdk/pyproject.toml` | n/a | proptest on argv | proptest on JSON payloads | sequence-shrinking on actor message streams | n/a |
| **Snapshot** | none declared; assert on the value | none declared | none; no visual baseline suite exists | assert on the rendered string and on the `--json` value | assert on the serialized response | event-stream traces | tool outputs |
| **Benchmark** | none in this tree, see §10 | none in this tree | none in this tree | none in this tree | none in this tree | none in this tree | n/a |
| **Fuzzing** | `cargo-fuzz` (libFuzzer) | none declared | n/a | `cargo-fuzz` on argv | `cargo-fuzz` on HTTP payloads | n/a | n/a |

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

**One `assert_eq!` on the whole structure over a chain of field asserts.**
The failure then names the structure that differs, not the first field that
happened to be compared.

**Test naming** : `test_<unit>_<scenario>_<expected>`. See
`docs/agents/NAMING.md` §9.

**At least one error case per public enum variant.** A `Result<T, MyError>`
return type with no test of any error branch is incomplete.

**No `#[ignore]` without naming, in the attribute itself, what has to be
true for the test to run again.** The form is §13 below, and every ignored
test in the tree carries it today. A bare `#[ignore]` is a test nobody will
ever re-enable, because nobody knows what would justify it.

**No tests that depend on ordering.** A test that needs a global resource
takes a process-wide lock declared next to it (`static LOCK: Mutex<()>`);
no serialization crate is declared in any manifest, so do not reach for one
without adding it to the workspace first.

**Never `--test-threads=1`.** It hides deadlocks. The remedy is fixing the
deadlock, not serializing the suite. `scripts/check_ci_workflows.py` holds
this one: a `run:` line that forces it fails the guard.

**Tests run under `cargo test`**, in CI (`ci.yml`, wrapped by
`scripts/check_test_home_isolation.py` so a test cannot reach the real
`HOME`) and locally. Doctests run in the same invocation.

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
- `#[tokio::test(start_paused = true)]` whenever timing matters. Deterministic,
  and the only way a duration assertion stays honest on a loaded machine. No
  test in the tree uses it yet; the wall-clock sites that remain sit on the
  `time-sensitive-tests` ratchet of `scripts/check_rust_rules.py`.
- Mock time via `tokio::time::pause` / `tokio::time::advance` when the test
  is too complex for the macro.
- Mock traits by hand. No mocking crate is declared in any manifest, and a
  trait small enough to test is small enough to implement twice.

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
- Exercise HTTP handlers through `tower::ServiceExt::oneshot` on the built
  router. It dispatches a request without a real bind, which is what the
  `routes_*.rs` test modules do today.
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
with a machine + human report written under `tests/cli/report/` (gitignored,
rebuilt by each run). Three tracks :
- **Track 1 (OFFLINE)** : every daemon-free command against the seeded HOME.
  Asserts KNOWN seeded content (not empty states) + the exit-code contract.
  Runs on every PR (`cli-e2e` job in `ci.yml`).
- **Track 2 (RUNTIME, opt-in `APOLLIA_REQUIRE_RUNTIME=1`)** : daemon booted on
  the seeded HOME; seeded runtime reads + CRUD + runtime-only leaves.
- **Track 3 (LLM CAPTURE, opt-in + a real model `APOLLIA_TEST_MODEL_GGUF`)** :
  non-deterministic commands (`run --stream`, `chat` REPL via pty, `llm chat`,
  `do`, `explain`). Asserts STRUCTURE only (exit, streaming happened, timing);
  the input/output is captured into the run's report for human review.

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
  "slow: long test; select with `pytest -m 'not slow'` (no conftest skips it)",
]
```

The `slow` marker selects, it does not skip. There is no `conftest.py` in the
SDK, so no `--run-slow` option exists; a test marked `slow` runs like any
other unless the invocation deselects it. The marker text said the opposite
for as long as it existed.

Rules :
- `asyncio_mode = "strict"` : every async test carries
  `@pytest.mark.asyncio` explicitly. Prevents collision with `trio` /
  `anyio` runners.
- Property-based and snapshot testing have no library here. `dependencies`
  is empty by principle 2 and the dev extras carry pytest, pytest-asyncio,
  pytest-cov, ruff and mypy, nothing else. Adding one is an ASK FIRST.
- Mock HTTP by injecting a fake transport, not by patching a client library.
- Fixture scoping : function > module > session. `autouse=True` only for
  project-wide setup.

---

## 7. Desktop tests

- **Unit tests** : Vitest, `npm test` from `crates/apollia-desktop/ui/`,
  co-located in `*.test.ts` next to the unit they cover. `vitest.config.ts`
  sets `environment: "node"` and `include: ["src/**/*.test.ts"]`, and
  `package.json` carries no rendering library, so a test that mounts a
  component cannot run here. Cover a component by exporting the logic under
  test from its `<script module>` block and asserting on that export, the way
  `crates/apollia-desktop/ui/src/components/observability/TaskTimeline.test.ts` does. Anything that
  needs a rendered tree is a Playwright test. One exemption: a file may
  declare `// @vitest-environment jsdom` (the `jsdom` devDependency exists
  for it) when the unit under test is inert without a DOM,
  the way `crates/apollia-desktop/ui/src/lib/utils/markdown-sanitize.test.ts`
  drives DOMPurify, whose
  sanitize is a no-op in the `node` environment. That buys a document, not a
  renderer: mounting a component still needs Playwright.
- **Browser tests** : Playwright, in `crates/apollia-desktop/ui/tests/`, run
  against the production bundle served by `vite preview` with the Tauri bridge
  stubbed. They cover machinery that needs a real browser: dirty state, nav
  guards, hotkey capture, responsive layout, perf. They do **not** exercise the
  packaged application.
- **Gestural tests on the running application** : `scripts/automation/`, a
  dev-only automaton. macOS has no WebDriver for WKWebView, so it drives a
  `cargo tauri dev` build by injecting steps addressed by `data-testid`,
  against a seeded throwaway `HOME`. Read `scripts/automation/README.md`
  before touching a recipe. It is tree-shaken out of release builds and it
  never drives the packaged bundle.
- **E2E on the packaged application** : none in this repository. The runtime
  paths behind the UI are covered through `tests/cli/cli-e2e.sh`, which drives
  the same commands against a seeded throwaway `HOME`.
- There is no `tauri-driver` setup and no `tests/visual/` baseline suite. Do not
  write a test that assumes either, and note the package manager is `npm`.

Rules :
- A Vitest test must not call Tauri IPC. Mock the IPC wrapper instead.
- An E2E test does call Tauri IPC. It runs against a built app with a
  scratch profile (`APOLLIA_HOME=$(mktemp -d)`).

---

## 8. Property-based testing

Use `proptest` (Rust) when the input space is large and invariants are
well-defined. Python has no property library here, so the same invariants are
written as table-driven cases :

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

No snapshot library is declared, in either language. Wide outputs (ORIA plans,
manifests, tool descriptors, serialized HTTP responses, the rendered table of
`apollia agent list`) are asserted against a value written in the test.

That is a decision, not an omission: a snapshot file is reviewed once and
accepted forever after, and this tree has already shipped documents nobody
re-read. If you want one, adding the crate is an ASK FIRST, and it lands with
the review discipline that makes a snapshot worth having.

---

## 10. Benchmarks

There is no benchmark in this repository: `git ls-files | grep '/benches/'`
returns nothing, and no benchmarking harness is declared in any manifest. The
performance work done so far lives in throwaway scripts and in measurements
recorded outside the tree.

If a benchmark lands, it lands with its harness declared as a workspace
dependency (an ASK FIRST), with the environment written next to it, and never
built with `target-cpu=native` or with a profile that differs from the
production one. Until then, do not write a rule here describing tooling this
tree does not have: the section that used to sit here prescribed three
harnesses and a per-crate README, none of which ever existed.

---

## 11. Coverage

Two things, kept apart because conflating them is how a target gets read as a
state.

**The target**, unchanged: more than 80 % of lines on the core crates
(`apollia-core`, `apollia-runtime`, `apollia-oria`, `apollia-permissions`,
`apollia-memory`).

**What is gated today**, in the `coverage` job of `ci.yml`: a workspace floor
(`COVERAGE_FLOOR`) and one floor per core crate, each set to the measured
baseline rounded down and ratcheted up, never down. `apollia-runtime` sits
below the target and carries the floor that says so. The numbers live in
`ci.yml`; read them there rather than trusting a copy here.

Nothing is uploaded anywhere: the run produces `lcov.info` as a build
artifact, and the gate is the floor, not a dashboard.

Locally, the same measure runs without a rustup component if LLVM is
installed out of band :

```sh
LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov \
LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata \
  cargo llvm-cov -p apollia-prompts --summary-only
```

`cargo llvm-cov` otherwise refuses to start, because the `llvm-tools`
component is absent from every toolchain installed here. The two variables
point it at the Homebrew LLVM instead; adapt the prefix to your machine.

---

## 12. CI gate

`cargo test --workspace --no-fail-fast` and `pytest` both pass before a PR can merge.
The flag matters: cargo otherwise stops at the first failing test binary, and a run
that covered a third of the suite reads exactly like a full green one.

No hook runs the test suite. The pre-commit entry is `cargo check --workspace`,
and `clippy` is staged on `pre-push`; the tests are on you, before the commit.
Do not read the green hook output as a green suite.

Sequence, as `ci.yml` chains it : `cargo fmt --all --check` ->
`cargo clippy --workspace --all-targets -- -D warnings` ->
`cargo test --workspace --no-fail-fast` -> `pytest` -> `npm test` (desktop,
Vitest) -> `bash tests/cli/cli-e2e.sh` (Track 1, offline).

---

## 13. When the rules block you

- Test is genuinely flaky : write the source of non-determinism and the
  mitigation in a comment on the test itself. Never `#[ignore]` a test
  without naming, on the spot, what has to be true for it to run again.
- Need to test private internals : restructure to expose a `pub(crate)`
  facade for the test. Do not reach into private modules.
- Long-running test : mark `@pytest.mark.slow`, or `#[ignore = "..."]` whose
  string names the condition, the way the desktop end-to-end tests name the
  job that starts the runtime and the app before running them.
