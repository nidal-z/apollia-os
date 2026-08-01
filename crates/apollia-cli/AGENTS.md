# crates/apollia-cli/AGENTS.md

> Local rules for the `apollia-cli` binary. Read after the root `AGENTS.md`
> and before editing this crate.

The CLI is the human-facing entry point and the machine-facing scripting
surface. Both are first-class. Adherence to ADR-004 (noun-verb commands)
and to exit code 0-5 semantics is what keeps scripts portable across
releases.

---

## 1. Command shape

ADR-004, refined by ADR-034 (taxonomy v2). Every command is
`apollia <noun> <verb> [args]`, EXCEPT the bare-verb whitelist below.

```
apollia agent list
apollia agent show <id>
apollia agent create <name>
apollia mcp add <name> <url>
apollia task list --pending-approval
```

**Canonical verbs (ADR-034).** One verb per intent:
- `show` is the only entity-detail read (absorbs the old `info`, `describe`,
  and entity-`get`).
- `create` is the only entity-creation verb (absorbs the old `new`).
- `delete` for entity deletion.
- Kept distinct on purpose (verified against real usage):
  - `get` / `set` for config key/value pairs (`config`, `chat config`,
    `tools config`, `notify events`), NOT entity reads.
  - `add` / `remove` for relationships and registrations, git-style
    (`mcp add`, `project agents add/remove`): they attach/detach, they do not
    create/delete an entity.
  - `clear` and `evict` are distinct in `plan cache` (wipe all vs purge by age).
- Lifecycle verbs stay: `install` / `uninstall`, `revoke`, `enable` / `disable`,
  `start` / `stop` / `restart`, `reset`, `reload`, `update`.
- Unambiguous remainder: `list`, `status`, `logs`, `test`, `fire`, `validate`,
  `export`, `import`, `forget`.

**Bare-verb whitelist (top level, git-style).** These are the only commands
allowed WITHOUT a noun; everything else is strictly noun-verb:
`start`, `stop`, `status`, `run`, `doctor`, `inspect`, `logs`, `trace`,
`version`, `digest`, `onboard`. Plus the meta commands `completions`, `guide`,
`do`, `explain` (ADR-035/036). Adding to this whitelist requires a note here.

**Folded / renamed nouns (ADR-034):** `mcp-server` -> `mcp server`,
`chat-config` -> `chat config`, `plan-cache` -> `plan cache`, `user-memory` ->
`profile`; `hitl` removed (use `task list --pending-approval`).

If you add a new noun, document it here and in `docs/site/docs/reference/cli/`.

---

## 2. Global flags

| Flag | Effect |
|---|---|
| `--json` | machine-readable output; stable schema per command |
| `--quiet` | suppress non-essential output |
| `--socket <path>` | connect to a non-default runtime socket |
| `--verbose` (`-v`) | increase tracing verbosity for this invocation |
| `--no-color` | disable ANSI styling |
| `--help` (`-h`) | show help |
| `--version` (`-V`) | show version |

TTY auto-detection : when stdout is not a TTY, the CLI assumes machine
mode and emits compact output. `--json` forces JSON regardless of TTY.

---

## 3. Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | general error (CLI usage, parsing, IO) |
| 2 | runtime error (HTTP call failed, runtime not reachable) |
| 3 | task error (task spawned but reported failure) |
| 4 | timeout |
| 5 | interrupt (SIGINT / SIGTERM during a long-running command) |

A new exit code requires an ADR. Scripts depend on this mapping.

---

## 4. Parsing tests

Every sub-command has parsing tests in
`crates/apollia-cli/src/commands/<noun>.rs` or in a sibling
`<noun>_tests.rs` module.

```rust
#[test]
fn test_agent_logs_parse_with_id_and_json() {
    let cli = Cli::try_parse_from(["apollia", "agent", "logs", "abc", "--json"]).unwrap();
    let Commands::Agent(AgentCommands::Logs { id, json, .. }) = cli.command else {
        panic!("unexpected variant");
    };
    assert_eq!(id, "abc");
    assert!(json);
}
```

Target : 150+ parsing tests in this crate. Acquired during the Sprint 43
CLI sweep. Do not regress.

---

## 5. End-to-end tests

`tests/cli/cli-e2e.sh` at the repo root : an orchestrator over a fixed,
deterministically-seeded HOME (the shared `scripts/automation/seed` fixture),
producing `tests/cli/report/report.{json,md}`. Three tracks :
- Track 1 (OFFLINE) : every daemon-free command against the seeded HOME,
  asserting KNOWN content + the exit-code contract. Runs on every PR.
- Track 2 (RUNTIME, opt-in `APOLLIA_REQUIRE_RUNTIME=1`) : daemon on the seeded
  HOME; seeded reads + CRUD + runtime-only leaves.
- Track 3 (LLM CAPTURE, opt-in + `APOLLIA_TEST_MODEL_GGUF`) : non-deterministic
  commands captured for human review (structure asserted, content not).

Run Track 1 on every PR. Run Tracks 2 and 3 before releases. Full layout in
`tests/cli/README.md`.

---

## 6. Output discipline

**Human mode** :
- Tables for lists, with column headers.
- Trees for nested data (agents -> tasks -> steps).
- Color and styling via `crossterm` or `console`. Disabled with
  `--no-color` or when `NO_COLOR` is set.
- Errors go to stderr, results go to stdout. Scripts can `2>/dev/null`.

**Machine mode (`--json`)** :
- Stable schema per command, documented in `docs/site/docs/reference/cli/`.
- One JSON document per invocation. Either an object or an array, never
  newline-delimited streams (the runtime API does NDJSON, the CLI does
  not).
- Errors emit `{"error": {"code": "...", "message": "..."}}` and exit
  non-zero.

---

## 7. Runtime calls

The CLI is a thin shell over the runtime HTTP API. Routes :
`crates/apollia-runtime/src/api/routes_*.rs`.

Rules :
- Never duplicate runtime logic in the CLI. The CLI calls, parses, and
  renders.
- HTTP client : shared in `crates/apollia-cli/src/client.rs`.
- Connection target : Unix socket by default (`~/.apollia/runtime.sock`),
  TCP 7771 fallback. Override with `--socket`.
- HTTP errors map to exit code 2. Task-level failures returned by the
  runtime map to exit code 3.

If a CLI command needs a route that does not exist, do not invent a CLI
shortcut. Add the route to the runtime first.

---

## 8. `anyhow` exception

`apollia-cli` is the only crate allowed to use `anyhow`, and only in
`main()` as the last-resort barrier between typed errors and the user
shell. The boundary :

```rust
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let exit_code = match commands::dispatch(cli) {
        Ok(()) => 0,
        Err(CliError::Runtime(_)) => 2,
        Err(CliError::Task(_)) => 3,
        Err(CliError::Timeout) => 4,
        Err(CliError::Interrupt) => 5,
        Err(CliError::Generic(_)) => 1,
    };
    std::process::exit(exit_code);
}
```

Every other function in the crate uses `Result<_, CliError>` or a more
specific error type. `anyhow` is forbidden everywhere except `main`.

---

## 9. Adding a new command checklist

1. Add the variant to `Commands` in `src/main.rs`.
2. Implement in `src/commands/<noun>.rs`.
3. Add parsing tests in the same file.
4. Add the route call via `client::*` helpers.
5. Add `--json` output schema. Document in `docs/site/docs/reference/cli/`.
6. Add an assertion to the right track in `tests/cli/tracks/` (offline →
   `track1`, runtime → `track2`, model-backed → `track3`).
7. Update `docs/site/docs/reference/cli/`.

---

## 10. When the rules block you

- Need a new noun : open the discussion before implementing. Nouns are
  long-lived contract.
- Need a different exit code semantic : open an ADR. Scripts depend on
  this.
- Need a non-trivial computation in the CLI : it probably belongs in the
  runtime. Push it there and call.
