# ADR-006: Tool subsystem and native tools

- Status: Accepted
- Date: 2026-06-04

## Context

Tools are how an agent acts on the world. Three problems shaped the tool
subsystem. First, early native tools had too broad a scope: a single `file_io`
tool folded read, write, and list into one ambiguous JSON schema, which made
LLMs hesitate and produce validation errors. A tool whose schema mixes several
operations is a friction point regardless of its description. Second, an
observation-heavy plan that reads twenty files serially wastes most of its wall
time waiting; independent read-only calls should run concurrently. Third, the
same tool needs to be callable from several execution contexts, and wiring each
new tool family into each context separately guarantees drift. On top of these,
agents that research the web need first-class search and read tools without
introducing an external runtime, and any outgoing network call must require
explicit consent (principle #1).

## Decision

We adopt atomic native tools, concurrent batch execution of read-only tools, a
single converged dispatch path, and native two-stage web tools that are opt-in
per session.

### Atomic native tools

The rule is one tool, one semantic action, one unambiguous JSON schema. The
file surface is decomposed into `file_read`, `file_write`, `file_edit`,
`file_list`, plus search tools `file_glob` (recursive pattern match) and
`file_grep` (native Rust regex search, no subprocess). The surface also includes
`http_fetch` (validates the agent network allowlist before any network call),
`memory_search` (FTS5 BM25 over local memory), and the standard executors for
shell and Python. Clear schemas cut tool-call validation errors and give agents
a complete surface to navigate a codebase, reach an API, and query local memory.

### Concurrent execution of read-only tools

`ToolDescriptor` carries an `is_read_only` flag, defaulting to `false` so any
new or forgotten tool is treated as having side effects. The dispatcher exposes
`execute_batch()`:

- If every call in the batch is read-only, they run concurrently via
  `futures::stream::iter(...).buffered(10)` (`MAX_CONCURRENT_READ_TOOLS = 10`),
  which protects file descriptors, the audit table, and CPU on constrained
  machines.
- If any call has side effects, the whole batch runs serially so effect ordering
  is always guaranteed.

Result order matches input order in both cases. Read-only executors include
`file_read`, `file_list`, `file_glob`, `file_grep`, `http_fetch`,
`memory_search`, `web_read`, `web_search`, `ask_user`, `notebook_read`, and
`permission_rules`; write, edit, shell, and MCP executors are not read-only.

### One converged dispatch path

Tools are invoked from three contexts: the native Rust ReAct chat loop, custom
Python agents (through the PyO3 bridge), and triggers. Rather than wiring each
new tool family into each context, all tools converge on a single dispatch
point. The native chat invoker delegates every tool call to a
`fallback_dispatcher` (`Arc<dyn ToolInvoker>`) backed by the real
`ToolDispatcher`, with no separate fast path for core native tools. A
`DispatcherToolInvoker` adapter bridges the JSON-typed dispatcher contract to
the string-typed invoker contract, preserving stable error codes
(`unknown_tool`, `invalid_input`). Every connector and tool descriptor is
registered once at supervisor boot from a single source, so all contexts see the
same catalog. The dispatcher carries the permission engine, audit trail, session
tool filter, and batch execution, so converging on it keeps those capabilities
rather than sacrificing them. Adding a new provider becomes registering its
descriptor and executor, not patching every context.

### Native web tools

Web access is two distinct tools rather than one combined fetch-and-extract,
keeping the LLM as the decision maker about which results to read.

- `web_search` is built on a `SearchBackend` trait with pluggable backends. The
  `DuckDuckGoBackend` is registered first and is the `auto` primary; it is
  always present and needs no configuration. The `BraveBackend` is appended as a
  secondary backend only when an API key is present at runtime, so a misconfigured
  Brave key cannot break the chain. Error codes are backend-agnostic and
  snake_case so the LLM reacts uniformly.
- `web_read` takes a URL and returns the readable extracted content (title,
  byline, text) using a maintained Rust port of Mozilla Readability. It applies
  a dedicated SSRF guard that rejects loopback, RFC 1918 private ranges,
  unique-local and link-local IPv6, multicast and broadcast, v4-mapped IPv6, and
  internal-looking domains before any I/O. The web tools manage their own
  network policy and do not share the `http_fetch` allowlist.

Both tools are compiled in by default but disabled at runtime: the session chat
tool picker presents them unchecked, and until the user enables them for a given
session any invocation returns `ToolNotAllowed`. No outgoing request happens
without explicit per-session consent.

## Alternatives considered

### Keep `file_io` and improve its description (rejected)
- Pros: zero structural change.
- Cons: the ambiguity is in the schema, not the prose; a single tool with three
  modes stays a friction point.

### N special fields per tool family in the chat invoker (rejected)
- Pros: smallest immediate change.
- Cons: every new provider needs synchronized patches in multiple places, with
  guaranteed divergence at the next iteration.

### Unify everything on the string-only `ToolInvoker`, drop `ToolDispatcher` (rejected)
- Pros: a single simpler contract.
- Cons: loses batch execution, the permission engine, the audit trail, and the
  session tool filter, a bad trade.

### `web_search` with a single hardcoded DDG scrape (rejected)
- Pros: minimal code.
- Cons: a single undocumented HTML endpoint can break with no fallback, and only
  returns snippets, capping synthesis quality.

### Brave through an external MCP subprocess (rejected)
- Pros: reuses an existing server.
- Cons: the local variant needs an external runtime, which violates principle #2.

### Chosen: atomic tools, read-only concurrency, converged dispatch, native web tools
- Pros: clear schemas, large speedups on observation batches, one wiring point
  for new tools, resilient web access, explicit security posture.
- Trade-offs: more tools to maintain, a mixed batch forces serial execution, and
  the web tools carry maintenance of their scraping and SSRF rules.

## Consequences

- Positive: fewer tool-call validation errors, observation batches that finish in
  a fraction of the serial time, a single attach point for new SaaS tools, and a
  consistent tool catalog across all execution contexts.
- Negative / trade-off: a single side-effecting tool in a batch forces serial
  execution, and the web tools add a small binary footprint and ongoing
  selector maintenance.
- Watch: agent adoption of the atomic tools over shell workarounds, the share of
  mixed batches the Reasoner produces, and the cadence of DDG selector breakage.

## Architectural principles

- Principle #1 (Local-first): web tools are opt-in per session with no outgoing
  request without explicit consent; `memory_search` exposes local memory.
- Principle #3 (Minimal contract): one tool, one semantic action, one schema.
- Principle #4 (Fail fast): unambiguous schemas surface input errors at resolve,
  and a mixed batch falls to serial immediately.
- Principle #5 (One actor, one responsibility): the dispatcher owns routing and
  concurrency, each executor owns one tool.

## Related

- [ADR-001](ADR-001-foundations-stack.md) the stack and crates the tools live in.
- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) the bridge through which Python
  agents reach the dispatcher.
- [ADR-005](ADR-005-oria-execution-model.md) the orchestrated loop that issues
  tool batches.
