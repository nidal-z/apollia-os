# crates/apollia-cli/AGENTS.md

> Local rules for the `apollia-cli` binary. Read after `docs/agents/INDEX.md`
> and before editing this crate.

The CLI is the human-facing entry point and the machine-facing scripting
surface. Both are first-class. Adherence to ADR-004 (noun-verb commands)
and to exit code 0-5 semantics is what keeps scripts portable across
releases.

---

## 1. Command shape

ADR-004. Every command is `apollia <noun> <verb> [args]`. Never
`<verb> <noun>`.

```
apollia agent list
apollia agent install <path>
apollia agent logs <id>
apollia task spawn <agent> --payload @file.json
apollia tool list --filter foo
```

Nouns and verbs are predictable :
- Verbs : `list`, `show`, `create`, `update`, `delete`, `start`, `stop`,
  `restart`, `logs`, `query`, `import`, `export`, `prune`.
- Nouns : `agent`, `task`, `tool`, `trigger`, `hooks`, `notify`, `auth`, `mcp`,
  `permission`, `audit`, `memory`, `session`, `config`.

If you add a new noun, document it here and in `docs/wiki/Briques-CLI.md`.

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

`tests/cli/cli-e2e.sh` at the repo root :
- Phase A (LOCAL) : 180 ok / 0 ko / 15 skipped, ~6s wall clock.
- Phase B (CLOUD, opt-in via `APOLLIA_E2E_CLOUD=1`) : 271 ok / 0 ko / 19
  skipped, ~18s wall clock.

Run Phase A on every PR. Run Phase B before releases.

---

## 6. Output discipline

**Human mode** :
- Tables for lists, with column headers.
- Trees for nested data (agents -> tasks -> steps).
- Color and styling via `crossterm` or `console`. Disabled with
  `--no-color` or when `NO_COLOR` is set.
- Errors go to stderr, results go to stdout. Scripts can `2>/dev/null`.

**Machine mode (`--json`)** :
- Stable schema per command, documented in `docs/wiki/Reference-CLI.md`.
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

1. Add the variant to `Commands` in `src/cli.rs`.
2. Implement in `src/commands/<noun>.rs`.
3. Add parsing tests in the same file.
4. Add the route call via `client::*` helpers.
5. Add `--json` output schema. Document in `docs/wiki/Reference-CLI.md`.
6. Add an entry to Phase A of `tests/cli/cli-e2e.sh`.
7. Update `docs/wiki/Briques-CLI.md` and the relevant book chapter.

---

## 10. When the rules block you

- Need a new noun : open the discussion before implementing. Nouns are
  long-lived contract.
- Need a different exit code semantic : open an ADR. Scripts depend on
  this.
- Need a non-trivial computation in the CLI : it probably belongs in the
  runtime. Push it there and call.
