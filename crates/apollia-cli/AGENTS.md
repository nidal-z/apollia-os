# crates/apollia-cli/AGENTS.md

> Local rules for the `apollia-cli` binary. Read after the root `AGENTS.md`
> and before editing this crate.

The CLI is the human-facing entry point and the machine-facing scripting
surface. Both are first-class. Adherence to the noun-verb taxonomy
and to exit code 0-5 semantics is what keeps scripts portable across
releases.

---

## 1. Command shape

Every command is `apollia-os <noun> <verb> [args]`, except the bare-verb
whitelist below. The tree carries 199 leaves today; read it with
`apollia-os --help` recursively, never from memory.

**Verb taxonomy.** Every leaf's last token belongs to one of the categories
below, and `scripts/check_cli_json_contract.py taxonomy` refuses a leaf whose
verb is in none of them, a bare verb outside the whitelist, and a verb declared
here that no leaf carries. Adding a verb means adding it to the right row, in
the same commit as the command.

| Category | Verbs |
|---|---|
| entity | `show`, `create`, `delete`, `list`, `rename`, `edit` |
| config | `get`, `set`, `raw-config`, `set-default` |
| relationship | `add`, `remove`, `link` |
| lifecycle | `install`, `uninstall`, `enable`, `disable`, `start`, `stop`, `restart`, `reset`, `reload`, `update`, `revoke`, `login`, `logout`, `setup`, `init`, `resume`, `cancel` |
| maintenance | `clear`, `evict`, `purge`, `repair`, `forget` |
| report | `status`, `logs`, `stats`, `costs`, `report`, `journal`, `audit`, `pending`, `resolved`, `approvals`, `messages`, `skills`, `chats`, `accounts`, `hardware`, `schema`, `anchor`, `inspect`, `trace`, `version`, `digest` |
| action | `run`, `test`, `fire`, `invoke`, `validate`, `verify`, `replay`, `search`, `discover`, `download`, `transcribe`, `ping`, `chat`, `learn-procedure`, `seed-builtins`, `set-approval`, `revoke-approval`, `list-pending`, `doctor`, `onboard`, `review`, `guide`, `explain`, `do`, `completions` |
| interchange | `export`, `import` |

**One verb per intent.** `show` is the only entity-detail read (it absorbed
`info`, `describe` and entity-`get`), `create` the only entity-creation verb
(it absorbed `new`), `delete` the only entity deletion. Kept distinct on
purpose, verified against real usage:

- `get` / `set` are config key/value pairs (`config`, `chat config`,
  `tools config`, `notify events`), not entity reads.
- `add` / `remove` attach and detach a relationship, git-style (`mcp add`,
  `project agents add`); they do not create or delete an entity.
- `clear` and `evict` are distinct in `plan cache`: wipe all against purge by
  age.

**Bare-verb whitelist (top level, git-style).** These 17 are the only commands
allowed without a noun; everything else is strictly noun-verb:
`start`, `stop`, `status`, `run`, `doctor`, `inspect`, `logs`, `trace`,
`version`, `digest`, `onboard`, `update`, `review`, `completions`, `guide`,
`do`, `explain`.

**Noun leaves.** One leaf ends on a noun rather than a verb and is exempt by
name: `mcp server`. Adding a second means adding it here, and the guard will
say so.

**Folded / renamed nouns:** `mcp-server` -> `mcp server`,
`chat-config` -> `chat config`, `plan-cache` -> `plan cache`, `user-memory` ->
`profile`; `hitl` removed (use `task list --pending-approval`).

If you add a new noun, document it here and in `docs/site/docs/reference/cli/`.

---

## 2. Global flags

| Flag | Effect |
|---|---|
| `--json` | machine-readable output; stable schema per command |
| `--quiet` (`-q`) | stdout carries the requested data and nothing else |
| `--socket <path>` | connect to a non-default runtime socket |
| `--verbose` (`-v`) | increase tracing verbosity for this invocation |
| `--no-color` | disable ANSI styling |
| `--help` (`-h`) | show help |
| `--version` (`-V`) | show version |

TTY auto-detection : when stdout is not a TTY, the CLI assumes machine
mode and emits compact output. `--json` forces JSON regardless of TTY.

**`--quiet` is not passed down, it is read.** `main` records it once with
`output::set_quiet`, and one place reads it: the `note!` macro. A line that is
NOT the requested data (a section header, a blank spacer, a separator rule, a
hint, a confirmation of the action just asked for) is written with `note!`; a
line that IS the data stays a `println!`. Handing the flag down each call chain
is what produced the state this rule replaces: accepted by 199 leaves, honoured
by two nouns. The rule is measured by `scripts/check_cli_json_contract.py`,
which drives every leaf under `-q` and refuses a blank line, a separator rule, a
bare section header or a hint on stdout.

**A leaf that destroys asks first.** A leaf that deletes, removes, clears,
resets, purges, uninstalls, revokes, forgets, evicts or cancels persisted state
publishes `--confirm` (`--yes` on `update` and `permissions revoke`, whose flag
name predates this rule), and calls `output::require_confirmation` before it
acts:

- with the flag, it acts;
- without it, on a terminal, it asks and stops unless the answer is `y`;
- without it, anywhere else (a pipe, a script, `--json`), it refuses with
  `use --confirm to <action>` and exit 1 rather than destroying silently.

The question and the cancellation go to stderr, never stdout: a `--json` caller
reads one document, and a human piping stdout keeps the data clean. Place the
call after the existence check, so an absent target reports "not found" instead
of demanding a confirmation that could never succeed. The same guard drives
every destructive leaf and refuses one whose `--help` does not name the flag.

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

A new exit code is a contract change: state it in
`docs/site/docs/architecture/08-decisions.md` under `#cli` before adding it.
Scripts depend on this mapping.

---

## 4. Parsing tests

Every leaf command has at least one parsing test, in
`crates/apollia-cli/src/commands/<noun>.rs`, in a sibling `<noun>_tests.rs`
module, or in `crates/apollia-cli/src/parse_tests.rs` for the leaves whose
noun file carries no test module of its own.

```rust
#[test]
fn test_cli_parses_agent_package_list() {
    // GIVEN "apollia-os agent package list"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "package", "list"]);
    // THEN the package sub-command is List
    let Commands::Agent {
        command: AgentCommand::Package { cmd },
    } = cli.command
    else {
        panic!("expected agent package");
    };
    assert!(matches!(cmd, PackageCommand::List));
}
```

The variant is `Commands::Agent { command }` with a named field, and the noun
enum is `AgentCommand`, singular. Copying a shape from an older draft of this
file is how a test ends up written against a variant the crate does not have.

The floor is the leaves, not the total. `scripts/check_cli_parse_tests.py`
enumerates them from the built binary by walking `--help`, reads the argv
literal of every `parse_from` in the crate, and refuses a leaf that no sequence
drives. Counting the total is what let the statement above be false for 59 of
the 199 leaves while the crate carried 247 sequences against a target of 150.

---

## 5. End-to-end tests

`tests/cli/cli-e2e.sh` at the repo root : an orchestrator over a fixed,
deterministically-seeded HOME (the shared `tests/cli/seed` fixture,
never its optional narrative overlay), producing
`tests/cli/report/report.{json,md}`. Three tracks :
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
- Connection target : on Unix, the Unix socket alone, defaulting to
  `~/.apollia/runtime.sock` (`client::default_socket_path`, which falls back
  to `<tempdir>/apollia.sock` when the home directory cannot be resolved).
  Override with `--socket`. There is no TCP fallback on Unix: TCP 7771 is
  what `connect_runtime` uses under `#[cfg(windows)]`, where the Unix socket
  does not exist, and `--socket` is ignored there.
- HTTP errors map to exit code 2. Task-level failures returned by the
  runtime map to exit code 3.

If a CLI command needs a route that does not exist, do not invent a CLI
shortcut. Add the route to the runtime first.

---

## 8. Errors and the `main` barrier

The root `AGENTS.md` allows `anyhow` in this crate's `main()` as the
last-resort barrier. The crate does not use the allowance and should not start:
`git grep anyhow -- crates/apollia-cli/` returns nothing outside this file, and
`apollia-cli/Cargo.toml` does not declare the dependency. There is no
`CliError` enum either.

What `main()` actually does (`src/main.rs`) :

```rust
fn main() {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => { /* --help and --version exit 0; a usage error exits 1 */ }
    };
    // ...
    let exit_code = rt.block_on(async { /* dispatch, each arm returns i32 */ });
    std::process::exit(exit_code);
}
```

Every command handler returns the process exit code as an `i32`, taken from
`exit_codes::{SUCCESS, GENERAL_ERROR, RUNTIME_ERROR, TASK_FAILED, TIMEOUT,
INTERRUPTED}` (`src/exit_codes.rs`), and reports through
`output::emit_error(json, code, message)` so the `--json` envelope and the
human message come out of one place. A handler that needs its own error type
declares a `thiserror` enum in its own module (`WorkspaceCliError` in
`src/commands/workspace.rs` is the shape) and maps it to a code at the edge.

Note the parse branch: clap's own default for a usage error is 2, and the
published contract says 1. `e.exit()` would leave clap's default, and a script
could not tell a typo from a stopped daemon.

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
- Need a different exit code semantic : change the `#cli` section of
  `docs/site/docs/architecture/08-decisions.md` first, in the same commit.
  Scripts depend on this.
- Need a non-trivial computation in the CLI : it probably belongs in the
  runtime. Push it there and call.
