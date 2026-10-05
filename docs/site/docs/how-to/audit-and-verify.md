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
| the tool-invocation trail | a flat SQLite table of tool calls: what ran, a hash of its inputs, whether it succeeded, how long it took. No hash chain, no signature. Updates and deletions are both refused by a trigger | `audit list`, `audit stats` |
| the hash-chained journal | run-scoped entries chained twice, to the previous entry of the run and to the previous entry of any run, signed as they are appended, with an exportable head anchor | `audit journal`, `audit export`, `audit show`, `audit verify`, `audit anchor`, `audit replay` |

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
events, presented as the whole answer and with no warning. Over the API the
route takes an `offset` that walks past the ceiling, as in
`GET /api/v1/audit?limit=500&offset=500`.

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
It shows one line per entry: when, which agent, what happened, with which tool,
on what and with which result, then the run of the entry, its position in that
run's chain, and whether the entry carries a signature. The line is a summary
and holds no content. Of a tool call it keeps the target, which is the one
argument the tool designates as such (a path, a recipient, the host of an
address) and no other; an MCP tool designates none. A single tool call appears
as two entries, one when it starts and one when it completes, because the
journal records events rather than invocations.

The arguments and the output of a call are read on demand, from the entries the
run kept for replay: `audit show` prints the run whole, and over the API
`GET /api/v1/audit/journal/<run>/entries/<seq>` answers one entry with the
arguments and the output of its call. A call made by an agent's own code, with
no model asking for it, has no captured arguments to show.

The journal holds every run of the instance. Five filters narrow a page, and
they add up; `--limit` and `--offset` then count the entries that pass them:

| Filter | Keeps | Over the API |
|---|---|---|
| `--agent` | the entries that name the agent, and the runs of its tasks | `agents=` |
| `--family` | one action family: `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code` | `family=` |
| `--nature` | `action` (a call, a consent, a refusal), `access_change` (an access, a connection, a secret, an installation, a rule), `policy`, or `system` (what the journal keeps for replay) | `nature=` |
| `--result` | `success`, `failure` or `refused` | `result=` |
| `--since`, `--until` | a period, as days read in UTC or RFC 3339 instants, the end excluded | `since=`, `until=` |

Each filter repeats on the command line and takes comma-separated values over
the API, as in `GET /api/v1/audit/journal?agents=<agent>,<agent>&result=refused`:

```sh
apollia-os audit journal --agent flux-quotes --agent flux-invoices --limit 20
apollia-os audit journal --nature access_change --since 2026-10-01
apollia-os audit journal --family send_external --result refused
```

## Export the journal

To hand the journal to an audit or keep it for review, export it to a file, as
CSV or JSON. `--format` and `--output` are both required, and the five filters
of `audit journal` narrow the export the same way:

```sh
apollia-os audit export --format csv --output journal.csv
apollia-os audit export --format json --since 2026-10-01 --until 2026-11-01 --output october.json
apollia-os audit export --format csv --agent flux-quotes --result refused --detail --output refused.csv
```

The file holds every entry that passes the filters, newest first. The export is
not paged: there is no `--limit`, so a file is never the newest part of the
journal under a name that claims the whole.

<!-- claim:audit-export-holds-no-content-without-detail -->
**Without `--detail`, an export holds no content.** Each entry is the summary
`audit journal` prints (`ts`, `agent`, `nature`, `family`, `what`, `tool`,
`target`, `outcome`) and its place in the chain (`run_id`, `seq`, `kind`,
`hash`). `--detail` adds the payload of each entry and, for a tool call, its
arguments, its output and its status: in CSV as four more columns (`payload`,
`arguments`, `output`, `status`, the first three holding JSON), in JSON as a
`detail` object on each entry. That is where personal data may be, and the
command says so on stderr when it writes such a file.

The runtime answers the content of the file and the command writes it, so an
export goes to a path on this machine and nowhere else. Every export is itself
recorded in the journal, under the run `governance:journal:export`, with its
format, its filters, the number of entries and whether the detail went with it.
With no runtime the command exits 2 and writes nothing.

The same export is available over the HTTP API for a host integration, as
`GET /api/v1/audit/export?format=csv&detail=true&agents=<agent>,<agent>`; see
the audit operations in the
[HTTP API reference](/reference/api/apollia-os-runtime-api).

Two things are worth knowing before you rely on either register. A tool call made
in a **chat** session does not reach the tool-invocation trail: that trail is
written from an agent's `ctx.tools`, and the chat path does not go through it, so
`audit list` will not show the call. It does reach the
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
machine-readable output. `audit export` writes the format `--format` names.
`--json` is a global flag, so `audit export --json` is accepted too, but it
changes nothing about the file: it switches what the command prints once the
file is written, and the shape of an error message.

## Retention and the organisation policy

The journal has no erase function: no command, no route and no screen removes an
entry. What exists is retention, and only a signed organisation policy sets it.
Its `[journal]` section holds four keys:

| Key | Effect |
|---|---|
| `required = true` | Without an open journal, the runtime refuses every sensitive action and every new connection. Reading continues, and the window says why. |
| `retention_days = N` | At startup, then once a day, the entries older than `N` days are removed. |
| `export.detail = "forbid"` or `"require"` | The detail of an export is no longer a choice: `audit export` asked the other way is refused with the reason. |
| `export.daily_summary_dir = "<folder>"` | One summary file per UTC day, `journal-YYYY-MM-DD.json`, is written to that folder of the machine, with the head anchor of the chain. |

A removal by retention is itself an entry, `journal_purged`, chained and signed
like the others. It carries the anchor of what was removed, so `audit verify`
with no argument starts from it and stays green, and says where it started:

```text
OK    whole journal  412 entries across 18 runs verified
      verified from global_seq=9051 (2026-07-02T06:14:09Z): earlier entries were removed by the retention of the organisation policy on 2026-09-30T00:01:00Z
```

With `--json`, the same information is the `start` object of the report. An
entry altered before or after the purge entry is still reported, and so is a
journal that begins later than its last purge entry says.

The daily export runs before the retention, and catches up at startup on the
days the runtime was not running, so a day reaches the folder before its entries
are removed. A folder that cannot be written refuses nothing: the window says
so, and the journal records `journal_export_failed`. Without a policy, nothing
is exported on a schedule and nothing is ever removed.

What this does not do is prevent: the journal and its key sit in your own
folder, so a change is detected by `audit verify`, never blocked. A journal cut
at its end is only detected against an anchor kept outside the machine, which is
what the daily files, or `audit anchor`, give you to keep.

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
