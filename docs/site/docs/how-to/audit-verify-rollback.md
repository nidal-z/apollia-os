---
sidebar_position: 3
title: Audit, verify and roll back a run
---

# Audit, verify and roll back a run

This guide covers the accountability workflow around an agent run: read the
signed audit trail, verify a run's integrity, and roll back the filesystem
changes a chat session made. It assumes you have run at least one task or chat
session against a daemon.

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

To take the whole trail out for archival or external review:

```sh
apollia-os audit export --output audit.json
```

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

## Roll back filesystem changes

When an agent modifies files during a chat session, those mutations are written
to a reversible journal under `~/.apollia/journal/<session-id>/`. You can undo
them by replaying the inverse of each mutation in reverse order.

First see what is available, and preview before applying:

```sh
apollia-os rollback --list
apollia-os rollback --dry-run <session-id>
```

The dry run prints exactly what would be reverted without touching anything.
When you are satisfied, apply it:

```sh
apollia-os rollback <session-id>
```

To undo the most recent sessions instead of naming one, use `--last-n`:

```sh
apollia-os rollback --last-n 1
```

Add `--json` to any of these for machine-readable output.

## Putting it together

A typical accountability pass is: `audit show` to read what a run did,
`audit verify` to confirm the record is authentic, and `rollback` to reverse the
filesystem effects of a session that went the wrong way. Read is always safe;
always `--dry-run` a rollback before applying it.

## Related

- [The accountability model](/explanation/accountability-model) for how these
  primitives fit together and what they support.
- The [CLI reference](/reference/cli) for every flag on `audit` and `rollback`.
- The [HTTP API reference](/reference/api/apollia-os-runtime-api) for the audit
  endpoints a host integration uses.
