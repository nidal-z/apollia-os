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
| **Integration** | per-crate `tests/` directory | pytest + `tmp_path` fixtures | Playwright on built app, `tauri-driver` for native shell | `assert_cmd` + `predicates` | `axum-test::TestServer`, `tower::ServiceExt::oneshot` | inter-actor channel exchanges | `wiremock`, `respx` |
| **E2E** | delegated to CLI tests | n/a | full Playwright user journeys + screenshots | `tests/cli/cli-e2e.sh` Phase A (LOCAL) + Phase B (CLOUD, opt-in) | CLI → API → DB round-trip | full `Runtime::spawn` test harness | live providers, gated by env vars |
| **Property** | `proptest` | `hypothesis` | n/a | proptest on argv | proptest on JSON payloads | sequence-shrinking on actor message streams | n/a |
| **Snapshot** | `insta` | `syrupy` | Playwright screenshots, baseline-tracked | snapshot CLI human + `--json` output | `insta` on serialized response | event-stream traces | tool outputs |
| **Benchmark** | `criterion` | `pytest-benchmark` | Lighthouse | `hyperfine` | `wrk` or criterion | criterion ops/sec | n/a |
| **Fuzzing** | `cargo-fuzz` (libFuzzer) | `atheris` if needed | n/a | `cargo-fuzz` on argv | `cargo-fuzz` on HTTP payloads | n/a | n/a |

---

## 2. Discipline (applies to every test level)

**GIVEN / WHEN / THEN comments mark each block.** This discipline exposed
every CLI bug in the Sprint 43 E2E sweep. Do not skip it.

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

Shell script with two phases :
- **Phase A (LOCAL)** : 180 ok / 0 ko / 15 skipped, ~6s wall clock. Runs
  on every PR.
- **Phase B (CLOUD, opt-in)** : exercises Anthropic, OpenAI, Google, MCP
  remote. Gated by `APOLLIA_E2E_CLOUD=1` and the relevant API keys.

Exit codes 0-5 are tested explicitly. See `crates/apollia-cli/AGENTS.md`
for the contract.

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
- **E2E** : Playwright against a built Tauri app, or `tauri-driver` for
  native shell integration tests.
- **Visual regression** : Playwright screenshots in `tests/visual/`.
  Baselines committed. Update with `pnpm test:visual:update`.

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

`cargo test --workspace` and `pytest` both pass before a PR can merge.
Pre-commit hook enforces this locally. Do not bypass.

Sequence : `cargo fmt --check` -> `cargo clippy --workspace -- -D warnings`
-> `cargo nextest run --workspace` -> `cargo test --doc` -> `pytest` ->
`pnpm test` (desktop) -> `bash tests/cli/cli-e2e.sh` (Phase A).

---

## 13. When the rules block you

- Test is genuinely flaky : open an ADR explaining the source of
  non-determinism and the mitigation. Never `#[ignore]` without an ADR.
- Need to test private internals : restructure to expose a `pub(crate)`
  facade for the test. Do not reach into private modules.
- Long-running test : mark `@pytest.mark.slow` or `#[ignore]` with a
  story link.
