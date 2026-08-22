---
sidebar_position: 3
title: Audit and verify a run
---

# Audit and verify a run

This guide covers the accountability workflow around an agent run: read what was
recorded and verify its integrity. It assumes you have run at least one task or
chat session against a daemon.

The runtime keeps **two separate registers**, and each command below reads one of
them:

| Register | What it holds | Commands that read it |
|---|---|---|
| the tool-invocation trail | a flat SQLite table of tool calls: what ran, a hash of its inputs, whether it succeeded, how long it took. No hash chain, no signature. Updates and deletions are both refused by a trigger | `audit list`, `audit stats`, `audit export` |
| the hash-chained journal | run-scoped entries chained twice, to the previous entry of the run and to the previous entry of any run, signed as they are appended, with an exportable head anchor | `audit journal`, `audit show`, `audit verify`, `audit anchor`, `audit replay` |

For the reasoning behind this model and how it maps to regulatory requirements,
see [The accountability model](/explanation/accountability-model).

## Read the audit trail

`audit list` and `audit stats` read the tool-invocation trail, so what they show
is the flat record of tool calls, not the chained journal. List recent events:

```sh
apollia-os audit list --limit 20
apollia-os audit stats
```

To read a single run's entries from the hash-chained journal, including the
captured model completions, resolve it by run identifier or by a task identifier
that maps to one:

```sh
apollia-os audit show <run-or-task-id>
```

Reading the journal that way needs a run identifier in hand. To browse it
without one, newest entry first across every run:

```sh
apollia-os audit journal --limit 20
apollia-os audit journal --limit 20 --offset 20
```

This is the only read of the chained journal that does not name a run up front.
It shows one line per entry, with its run, its position in that run's chain, and
whether the entry carries a signature. A single tool call appears as two entries,
one when it starts and one when it completes, because the journal records events
rather than invocations.

To take the tool-invocation trail out for archival or external review:

```sh
apollia-os audit export --output audit.json --limit 100000
```

<!-- claim:audit-export-pages-past-the-server-ceiling -->
**The endpoint serves at most 500 events per request**, and the command pages
through it until a short page comes back, so `--limit` bounds the export rather
than the reachable history. It warns on stderr when the export stopped on your
`--limit` instead of on the end of the trail, which is the signal to raise it.

The same records are available over the HTTP API for a host integration; see the
audit operations in the
[HTTP API reference](/reference/api/apollia-os-runtime-api).

Two limits are worth knowing before you rely on either register. A tool call made
in a **chat** session reaches neither: the trail is written from an agent's
`ctx.tools`, and the journal is fed from run-scoped events the chat path does not
emit. And the trail is written fire-and-forget, so a record is dropped, with a
warning in the logs, when its channel is saturated.

## Verify a run's integrity

The journal is a hash chain with signatures, so tampering is detectable.
`audit verify` has two forms, and they do not check the same thing.

With a run identifier, it recomputes that run's own chain and its signatures:

```sh
apollia-os audit verify <run-id>
```

A successful run-scoped verification tells you the entries of that run were not
mutated and were signed by the expected key. It does not tell you that the run is
whole: a truncation that removes the last entries leaves a shorter chain that
still verifies.

Without an argument, it walks the global chain across every run and compares the
terminal head to the persisted anchor, which is what detects an interior
deletion, a whole run deleted, and a truncated tail:

```sh
apollia-os audit verify
```

Run the argument-free form when the question is whether anything is missing, and
the run-scoped form when the question is whether one run's entries are
authentic. Neither form can defend against a holder of the signing key who
re-signs a shorter chain; storing the exported anchor off-machine is what covers
that, and `audit anchor` prints it.

Add `--json` to `audit list`, `audit show`, `audit stats` and `audit verify` for
machine-readable output. `audit export` always writes JSON and takes no
`--json`.

## Putting it together

A typical accountability pass is: `audit show` to read what a run did, then
`audit verify` with no argument to confirm nothing was removed, then
`audit verify <run-id>` on the run in question. All three are read-only, so none
of them changes anything on disk.

## Related

- [The accountability model](/explanation/accountability-model) for how these
  primitives fit together and what they support.
- The [CLI reference](/reference/cli) for every flag on `audit`.
- The [HTTP API reference](/reference/api/apollia-os-runtime-api) for the audit
  endpoints a host integration uses.
