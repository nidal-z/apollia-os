# ADR-004: CLI design

- Status: Accepted
- Date: 2026-06-04

## Context

The `apollia` CLI is the human surface of the runtime (Principle #8) and the
primary tool for operators and agent authors. It exposes dozens of commands
across several domains (agents, tasks, tools, memory, audit), so its structure
must be uniform, discoverable, and scriptable. Two style families dominate modern
CLIs: `verb-noun` (for example `kubectl get pod`) and `noun-verb` (for example
`docker container ls`).

A second, sharper need comes from the agent authoring loop. Validating that a
Python agent conforms to the runtime used to require actually starting it, so a
malformed manifest, an inconsistent skill signature, or a missing datasource only
surfaced on the first call, or silently degraded behavior. The author paid a full
install, start, invoke, read-the-logs cycle for every change. Because the SDK is
decorator-first with signature inference ([ADR-023](ADR-023-sdk-agentkit-design.md)),
the entire agent contract is statically introspectable at Python load time, with
no need to start the Rust runtime. The CLI should turn that property into a fast
feedback tool.

## Decision

We adopt a uniform `noun-verb` command structure with a global `--json` flag and
an exit-code contract, and a first-class `apollia inspect <agent.py>` command.

### Structure: noun-verb, global --json, exit codes

Commands follow `noun-verb`: `apollia agent start`, `apollia task list`,
`apollia memory inspect`. A small set of high-frequency, universal level-one
commands (`start`, `stop`, `status`, `run`) are deliberate exceptions; `apollia
run <agent_id> <input>` is a top-level command that submits a task to an agent
and waits for its result. The structure maps cleanly
onto `clap` v4 derive subcommands per domain and makes shell completion
self-documenting: `apollia agent <TAB>` lists every action available on an agent.

Every command honors a global `--json` flag (machine output, TTY auto-detected
for human output otherwise) and maps outcomes onto a stable exit-code contract,
so the CLI is usable from both a terminal and a pipeline.

### apollia inspect

`apollia inspect <agent.py>` loads the agent's Python module in isolation,
without starting the runtime, the bridge, the EventBus, the actors, or the HTTP
API, and without ever instantiating `ctx`. It assembles `sys.path` (the SDK plus
the enclosing package root), calls `importlib.invalidate_caches()`, then loads
the module through the PyO3 `PyModule::from_code` path, reads
`agent.__apollia_manifest__`, runs the validator, and prints a complete report:
manifest (name, version, description, required packages), skills (id, description,
the inferred input schema if published, and the declared output modes), declared
datasources, templates, secrets, and tool permissions, plus warnings and errors. Output is human-readable by default (colored on a TTY),
`--json` for pipelines and IDE or desktop integration, `--quiet` for errors and
warnings only.

Validation is static and systematic: skill id uniqueness; skill, message, and
orchestration signatures inferable to JSON Schema, otherwise an error; declared
datasources exist and parse; declared templates exist; declared secrets listed
statically with a single advisory warning, since static inspection cannot reach
the secret store; declared tool permissions checked against the native tool
catalog.

The command makes no runtime call, so its exit-code contract has exactly two
outcomes: `0` for success (possibly with warnings) and `1` for an inspection
failure (an invalid manifest, a missing datasource, an uninferable signature). It
does not use exit code `2`, because nothing in inspection reaches the runtime.

This makes inspection usable as a pre-commit hook, a CI gate over agent files, and
a sub-second development feedback loop. Surfacing the same report inside a desktop
install dialog before an agent is installed is an aspirational use of the `--json`
output that no desktop dialog wires up yet. Inspection covers the static contract
only;
runtime conditions such as an expired API token are out of its scope, which is
documented.

## Alternatives considered

### verb-noun (rejected)
- Pros: familiar to `kubectl` users.
- Cons: harder to explore the actions available on an object, less natural
  completion.

### Mixed style per command (rejected)
- Pros: each command reads naturally on its own.
- Cons: inconsistent, hard to document, confusing for operators.

### Validate only at runtime boot, the prior status quo (rejected)
- Pros: no extra tool.
- Cons: slow feedback, errors scattered across runtime logs, unusable in
  pre-commit or CI.

### A separate apollia-lint binary, or a desktop-only inspection view (rejected)
- Pros: modular, or visually rich.
- Cons: a separate binary duplicates introspection logic and is less
  discoverable; a desktop-only view serves neither CI nor an author working in a
  terminal.

### Chosen: uniform noun-verb plus a first-class apollia inspect
- Pros: shell completion guides discovery, the structure matches familiar CLIs,
  `--json` makes every command scriptable, and inspection gives sub-second
  feedback runnable in pre-commit, CI, and development, with a `--json` surface
  ready for a future desktop integration.
- Trade-offs: the level-one exceptions add slight inconsistency; if an agent
  performs side effects at module load, inspection triggers them, so the load path
  is documented as required to be pure.

## Consequences

- Positive: discoverable, consistent commands with a stable machine surface;
  malformed agents are caught in under a second, before install, in a hook or in
  CI.
- Negative / trade-off: the level-one command exceptions are a small inconsistency
  to document; inspection cannot catch runtime-only failures.
- Watch: consistency as new commands are added; pre-commit adoption among external
  builders; stabilizing a versioned `--json` schema so third-party tools do not
  break.

## Architectural principles

- Principle #3 (Minimal contract): inspection reads the full agent contract
  without starting the runtime, which proves the contract is genuinely minimal and
  static.
- Principle #4 (Fail fast): inspection materializes fail-fast at the authoring
  ergonomics level.
- Principle #8 (Human CLI, machine API): `noun-verb` keeps the CLI human, and the
  global `--json` (including `apollia inspect --json`) keeps it scriptable.

## Related

- [ADR-023](ADR-023-sdk-agentkit-design.md) defines the decorator-first SDK whose
  signature inference makes static inspection possible.
- [ADR-001](ADR-001-foundations-stack.md) defines the local API the CLI consumes
  for runtime commands.
