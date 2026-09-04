# `tests/cli/smoke/` - one real invocation of every clap leaf

The assertion suite next door (`tests/cli/cli-e2e.sh`) proves what a few dozen
commands *say*. This sweep proves that every command *runs*: it enumerates the
whole clap tree from the built binary, plays each leaf once against a throwaway
seeded HOME, and holds the result to three contracts. It is a smoke instrument,
not an oracle: it never asserts content.

```sh
# Offline, the everyday form (a few seconds):
python3 tests/cli/smoke/sweep.py

# With a daemon of its own, which also measures the API routes the CLI reaches:
python3 tests/cli/smoke/sweep.py --daemon

# The instrument's own proof that it can fail:
python3 tests/cli/smoke/sweep.py --selftest
```

## Verdict, by exit code

| code | meaning |
|---|---|
| 0 | every played leaf held all three contracts |
| 1 | a leaf broke one, or the policy excludes a leaf the binary no longer has |
| 2 | nothing was measured |

A 2 must never read as a success. It is returned when there is no binary, when
`scripts/binary_freshness.py` says the binary did not come from this working
tree, when the tree walk yields no leaf, when the seed builder fails, and when
the workspace path would overflow the socket limit (below). The sweep never
falls through an empty result list to 0.

## The three contracts

**exit** The process exits with a code `crates/apollia-cli/src/exit_codes.rs`
declares: 0 success, 1 general error, 2 runtime absent, 3 task failed, 4
timeout, 5 interrupted. Anything else, or a death by signal, is a defect.

**json** Every leaf is played with the global `--json` flag. When stdout opens
with `{` or `[` it must read as a sequence of JSON values, which accepts a
single document, JSON Lines, and the concatenated documents a streaming command
emits. Truncated or prose-interleaved output is a defect.

**panic** Neither stream carries `panicked at`, a backtrace note, or a fatal
runtime error.

A leaf that does not return inside its deadline is a defect too, and so is a
leaf clap rejected: a rejected invocation never reaches the command's body, so
counting it would be the argument-shaped version of "a mention is not a
coverage". That check is what caught eighteen wrong argument vectors in this
sweep's own table on its first honest run, `--yes` where the CLI spells
`--confirm`, `--schedule` where it spells `--detail`.

## What it plays, and what it refuses

The count that matters is what the sweep PLAYED. Refusals live one by one in
`policy.py`, each with a class (`blocking`, `destructive-host`, `browser`,
`network`) and a reason, and the report prints them in full. A leaf skipped in
silence would be a hole disguised as coverage.

The policy is also held to its own freshness: an entry naming a leaf the binary
no longer has fails the run, because a stale exclusion is the same hole one
release later.

Argument vectors are synthesised from each leaf's own `--help` usage line, so a
new subcommand is played the day it is merged. `policy.ARGV` overrides that
where a seeded identifier turns a `not found` into a real read path.

## Isolation

One HOME per leaf, built by the shared committed seed builder
(`tests/cli/seed/build-seed.sh`), and the working directory is that HOME too, so
a leaf that writes into the current directory (`workspace init` creates
`APOLLIA.md`) cannot reach the repository. The real `~/.apollia` is never
opened. stdin is `/dev/null` and `EDITOR` is `/usr/bin/true`, so a command that
would wait for a human returns instead of hanging.

**The workspace root is short on purpose.** `sockaddr_un.sun_path` is 104 bytes
on macOS, and the CLI derives its socket from `$HOME/.apollia/runtime.sock`. A
HOME under the default `$TMPDIR` overflows it, and the CLI then reports `io
error: path must be shorter than SUN_LEN` with exit 1 instead of reaching its
real refusal, exit 2 `runtime not started`. Measured on this tree: the same
sweep reported 121 leaves at exit 1 and 4 at exit 2 under a `$TMPDIR` HOME, and
42 at 1 and 76 at 2 under a short one. The sweep therefore roots itself under
`/tmp` (override with `APOLLIA_SMOKE_TMP`) and refuses to run, code 2, when the
path it would use is still too long. Without that refusal the sweep would
report a number about path lengths as if it were about the product.

## `--daemon`: the API reached through the CLI, measured

The runtime serves axum over a Unix socket and installs no request-tracing
layer, so the only place a route is legible without touching `crates/` is the
wire. In `--daemon` mode the sweep boots a daemon on the seeded HOME, puts a
forwarding proxy in front of its socket, points every leaf at the proxy, and
records the HTTP request line of each request. Routes are attributed per leaf by
a before/after snapshot, which is sound because daemon mode runs serially.

`operations` in the report is that list with the arguments the sweep itself
passed folded back into their path segments (`/agents/seed-classifier` becomes
`/agents/{id}`). The OpenAPI document is never consulted: this is what crossed
the wire, not a claim about the router's declared paths, and a route the CLI
reaches only after a response the sweep did not wait for is not in it.

## Reports

`smoke.json` (machine) and `smoke.md` (human) land in `tests/cli/report/` by
default, beside the assertion suite's own report; the directory is git-ignored.
`smoke.md` carries the refusal table, the defects with the exact command line,
the observations, and, in daemon mode, the operations table.

## Extending

Add nothing here for a new subcommand: it is enumerated and played on its own.
Touch `policy.py` when a leaf needs a seeded identifier to reach its real path,
when its deadline is too short, or when it must be refused, and write the reason
next to it. `--selftest` must stay green: it is the only evidence the sweep can
still tell red from green.
