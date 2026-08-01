---
sidebar_position: 3
title: Audit and verify a run
---

# Audit and verify a run

This guide covers the accountability workflow around an agent run: read the
signed audit trail and verify its integrity. It assumes you have run at least
one task or chat session against a daemon.

For the reasoning behind this model and how it maps to regulatory requirements,
see [The accountability model](/explanation/accountability-model).

## Read the audit trail

Every governed action an agent takes is recorded in an append-only, hash-chained
journal that is signed as it grows. List recent events:

```sh
apollia-os audit list --limit 20
apollia-os audit stats
```

To read a single run's full journal, including the captured model completions,
resolve it by run identifier or by a task identifier that maps to one:

```sh
apollia-os audit show <run-or-task-id>
```

To take the trail out for archival or external review:

```sh
apollia-os audit export --output audit.json --limit 500
```

**One export returns at most 500 events.** The endpoint clamps every request to
that ceiling, so the `--limit` default of 10000 is misleading and a larger value
returns nothing more. The command warns on stderr when the export comes back full
at the ceiling, which is the signal that older entries were left behind. There is
no pagination on this endpoint in `v0.1.0-preview`, so a busy install cannot be
archived in full from the CLI; read the journal directly under `~/.apollia` if you
need everything.

The same records are available over the HTTP API for a host integration; see the
audit operations in the
[HTTP API reference](/reference/api/apollia-os-runtime-api).

## Verify a run's integrity

The journal is a hash chain with signatures, so tampering is detectable. Verify
a run to check that its chain and signatures are intact:

```sh
apollia-os audit verify <run-id>
```

A successful verification tells you the recorded sequence has not been altered
since it was written. This is the check you run when you need to trust that the
trail of what an agent did is authentic.

Add `--json` to `audit list`, `audit show`, `audit stats` and `audit verify` for
machine-readable output. `audit export` always writes JSON and takes no
`--json`.

## Putting it together

A typical accountability pass is: `audit show` to read what a run did, then
`audit verify` to confirm the record is authentic. Both are read-only, so
neither changes anything on disk.

## Related

- [The accountability model](/explanation/accountability-model) for how these
  primitives fit together and what they support.
- The [CLI reference](/reference/cli) for every flag on `audit`.
- The [HTTP API reference](/reference/api/apollia-os-runtime-api) for the audit
  endpoints a host integration uses.
