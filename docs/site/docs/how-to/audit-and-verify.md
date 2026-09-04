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

One command in that second list does less than its name suggests. `audit replay`
re-derives a run from its own captured trace, consuming the recorded model
responses and tool outputs in `step_ordinal` order; it does not re-run the live
agent, because the journal never captured the original prompt. It answers whether
a trace is complete and self-consistent, not whether today's code would behave
the same way.

## Read the audit trail

`audit list` and `audit stats` read the tool-invocation trail, so what they show
is the flat record of tool calls, not the chained journal. List recent events:

```sh
apollia-os audit list --limit 20
apollia-os audit stats
```

The endpoint behind `audit list` serves at most 500 events per request, and this
command does not page past that ceiling: `--limit 2000` returns the newest 500
events, presented as the whole answer and with no warning. Use `audit export`,
which does page, when you need more than 500.

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

Two things are worth knowing before you rely on either register. A tool call made
in a **chat** session does not reach the tool-invocation trail: that trail is
written from an agent's `ctx.tools`, and the chat path does not go through it, so
`audit list` and `audit export` will not show the call. It does reach the
hash-chained journal: the chat loop emits every tool output and every model
completion as a run-scoped event, and the journal subscriber turns them into
chained entries. Read them with `audit show` on the run, or with `audit journal`.
Both are stored verbatim, which is what makes a chat auditable and also what puts
its content on disk. And the trail is written fire-and-forget, so a record is
dropped, with a warning in the logs, when its channel is saturated.

## Verify a run's integrity

The journal is a hash chain with signatures, so tampering is detectable.
`audit verify` has two forms, and they do not check the same thing.

With a run identifier, it recomputes that run's own chain and its signatures:

```sh
apollia-os audit verify <run-id>
```

A successful run-scoped verification tells you the entries of that run were not
mutated, and that they carry a valid signature **when the journal is a signed
one**. Signing is not guaranteed: when the HMAC key can be neither read nor
written, the runtime opens the journal unsigned and logs
`audit.journal.unsigned_fallback`. Verification then asks for no signature at
all, and an unsigned chain still comes back ok. Check that event in the runtime
logs, or the signature column of `audit journal`, before reading a green verify
as proof of authorship. The signature is an HMAC-SHA256, a symmetric one: it
shows the chain was written by a holder of the key on this machine, and it is not
something a third party can check on its own.

Verification also does not tell you that the run is whole: a truncation that
removes the last entries leaves a shorter chain that still verifies.

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
machine-readable output. `audit export` always writes JSON. `--json` is a global
flag so `audit export --json` is accepted too, but it changes nothing about the
export itself; it only switches the shape of an error message.

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
