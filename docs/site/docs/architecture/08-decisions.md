---
sidebar_position: 8
title: 8. Design decisions in force
---

# 8. Design decisions in force

This page states the structural decisions that hold today, and why. It is
written in the present tense: it describes the system as it is built, not the
route that led there. Where a decision has a cost, the cost is named.

## Stack and runtime {#stack-and-runtime}

The runtime is Rust on Tokio. Agents are Python, loaded in-process through a
PyO3 bridge rather than spawned as subprocesses: the bridge translates Rust
futures to Python coroutines directly, so an agent call costs no process
boundary and no serialization round-trip.

Every subsystem that owns mutable state is a Tokio actor with a bounded channel
and a clonable handle. No state is shared between actors behind a lock. The cost
is indirection, and it buys the property that matters at this size: a deadlock
cannot form between two subsystems that only exchange messages.

Persistence is SQLite in WAL mode, vendored into the binary. Nothing on the
default path requires a server, a daemon, or a network.

## The agent contract {#agent-contract}

An agent is a Python class carrying `@agent`, with at least one `@skill` or one
`@on_message` async method. That is the whole contract. The bridge refuses an
object that does not expose the dispatch entry point the decorators install;
there is no escape hatch that accepts an arbitrary callable.

The runtime hands each call a `ctx` object exposing the services an agent may
use: the LLM router, memory, tools, agent-to-agent messaging, notifications, and
a logger. `ctx` is the whole runtime surface. An agent reaching around it is
using something the runtime does not guarantee.

Agent payload schemas are `TypedDict`, read at registration time to build skill
schemas. This is why deferred annotation evaluation is refused in those modules:
it turns annotations into strings, and the schema comes out empty with no error.

The Python SDK has no third-party dependencies. Every one would become a
dependency of every agent, on every machine, forever.

## Execution model {#execution-model}

The autonomous engine observes, reasons, and acts in a loop. It runs in two
modes: direct, where a skill answers, and orchestrated, where a plan is built
and its steps executed.

On the orchestrated path, step arguments are filled at plan time under a
grammar, with just-in-time extraction as a fallback at execution. Filling them
at plan time is what lets a plan drive real tools with structured arguments
instead of re-parsing prose at each step.

A completed orchestrated run is verified by a critic from the `supervised` tier
upward; the `assisted` tier runs no verification pass at all. The verdict is
recorded as a runtime event, and a failing verdict triggers bounded re-planning
under the same budget that bounded the original run.

## Budget and safeguards {#budget-and-safeguards}

Every run carries a step budget with three dimensions: a maximum number of
steps, a maximum number of tool calls, and a wall-clock ceiling. The runtime
enforces it. Agent code cannot raise it, extend it, or opt out of it, because
the counter lives on the runtime side of the bridge and the agent never holds a
reference to it.

This is the safeguard the design rests on: an autonomous loop that can be
stopped is a product, and one that cannot is a liability.

## Supervision {#supervision}

<!-- claim:supervisor-has-no-restart-machinery -->
<!-- claim:supervisor-has-no-child-spec -->
<!-- claim:supervisor-has-no-restart-tracker -->
The supervisor is fail-fast then degrade. An actor that dies is not restarted.
The runtime reports the loss, degrades the capability that actor served, and
keeps serving everything else.

Restart-on-crash was considered and rejected: an actor that crashes has already
lost the state it owned, and restarting it produces a subsystem that answers
requests with a plausible but empty view. A capability that is honestly absent
is easier to operate than one that quietly lies.

## Tools and confinement {#tools-and-sandbox}

Tools are resolved at startup, not at first call, so a missing or misconfigured
tool is a startup error rather than a failure in the middle of a run.

The native tool set covers shell execution, path-confined file reads and writes,
Python execution in a per-agent virtual environment, HTTP fetch, web search, and
memory search. File tools are confined to a resolved root: the path is
canonicalized and re-checked against the root after resolution, so a symlink
cannot walk out of it.

Confinement is not uniform across platforms, and the difference is not
cosmetic. On Linux, shell execution runs under PID and mount namespaces. On
macOS the confinement is weaker. On Windows there is none. This is stated
plainly rather than smoothed over, because an operator choosing a platform is
choosing a threat model. See [the agent trust
model](/explanation/agent-trust-model) for what that means in practice.

Outbound HTTP is checked against private address ranges on every redirect hop,
not only on the first request, so a public URL cannot redirect into the local
network.

## Permission model {#permission-model}

<!-- claim:prefix-rules-evaluated-per-invocation -->
On the chat path a tool call is decided in a fixed order. The operator's
persisted rules are evaluated first, per invocation, against this call's own
first argument: project rules, then rules scoped to the chat agent, then global
rules, longest argument prefix first. A matching deny refuses the call outright,
even one the turn had already authorized by name. Otherwise the call runs when
the tool's name is in the turn's authorization set or a rule allows it, and a
rule-based allow is not remembered, so the next call is evaluated again with its
own argument. Anything left raises an approval request and waits for a person; a
refusal, or five minutes without an answer, runs nothing.

A persisted rule carries one of three scopes: project, agent, or global. The
session scope is refused at write time and lives in memory only, and the chat
path passes no in-memory session rule.

Code executors are never blanket-authorized. A rule that would grant every tool
does not grant shell or Python execution; those require their own explicit
grant. A blanket grant is usually a convenience decision about reading files,
and it must not silently become permission to run arbitrary code.

<!-- claim:executor-guard-blocks-command-chaining -->
A rule only lets a shell command skip approval when that command is a single
simple command, with no chaining, pipe, redirection, substitution, or
backgrounding. That guard decides whether a call may skip approval; it is not a
filter placed in front of execution, and a code executor with no matching rule
asks a person on every invocation.

<!-- claim:permission-decision-is-not-recorded -->
The permission decision itself is not written anywhere. Nothing in a shipped
binary writes to the `permission_audit` table; it is read by
`apollia permissions audit` and by the desktop audit view, and by nothing else.
What is recorded is the invocation: `tool_invocations` holds what ran, not who
allowed it.

## Human in the loop {#human-in-the-loop}

Any tool can require human approval before it executes. When one does, the
runtime suspends the run, emits an event carrying what is being asked, and
resumes on the answer. Suspension is a first-class state, not a blocking wait:
the process is free, and a run can stay parked across a restart.

## Memory {#memory-model}

Memory has three layers: episodic events with an importance score, semantic
facts with a confidence score, and procedures with triggers. Each agent has its
own namespaced store, searchable full-text through SQLite FTS5 with BM25
ranking.

Memory is read at the agent's initiative. The runtime never injects memory
content into an agent's prompt. An agent that wants context asks for it, which
keeps the prompt something the agent author controls and can reason about.

Three exceptions exist, and none of them is reachable from an agent execution
path. Two are inside the built-in conversational assistant: a user-persona brief
at the longest autonomy tier, and past session summaries on the first message of
a free chat. The third is outside the assistant, in the desktop prompt-rewrite
command, which carries the Work section of the user profile into its own one-shot
prompt and returns text to the composer rather than to a run.

## Local inference {#local-inference}

Local models run through an embedded `llama-server`, the upstream llama.cpp
server, which the daemon spawns and supervises over its OpenAI-compatible HTTP
API, with native tool calling and continuous batching.

An in-tree binding was maintained previously and has been dropped. Keeping one
meant tracking a fast-moving upstream through a foreign-function layer, and
every capability that landed upstream landed here late, or not at all. Speaking
the upstream HTTP API instead costs one local process and buys the upstream's
release cadence.

A packaged build stages the engine binary. A source build expects
`llama-server` on the `PATH`. When a local backend is configured and no engine
is reachable, calls fail with an explicit unavailable reason rather than
silently falling back to a cloud provider.

Cloud backends are configured per backend with an API key. There is no OAuth
flow for a cloud model provider, because none of them offers one for this use.

## Speech to text {#speech-to-text}

Transcription runs out of process, in a sidecar built on whisper, so a model
crash cannot take the daemon with it. It is optional: a build without it loses
dictation and nothing else.

## Model Context Protocol {#mcp}

Apollia is an MCP client over stdio and HTTP transports, and also exposes its
own native tools as an MCP server over stdio.

Every MCP response is treated as untrusted input. Responses are capped per
server, tool names and descriptions are validated at ingestion against a
character set and a length limit, and the number of tools a single server may
contribute is bounded. A server that misbehaves degrades itself, not the
runtime.

Servers requiring OAuth go through a dedicated flow with its own, smaller
response cap.

## Connectors {#connectors}

Google and Microsoft connectors authenticate through OAuth2 with PKCE. No
aggregator sits in the path: a paid third-party relay between an operator and
their own mailbox contradicts the point of the product.

The binary embeds a Microsoft OAuth client and no Google one. Google's
restricted scopes require a verification process the project has not completed,
so a Google connection asks the operator to supply their own client credentials.
The asymmetry is deliberate, and it is stated where an operator meets it.

## Agent-to-agent messaging {#agent-messaging}

A director agent delegates to worker agents by skill identifier, over a durable
mailbox backed by SQLite. Delivery is leased: a consumer takes a lease, and
acknowledgement is fenced on the run holding it, so a stale consumer whose lease
was reassigned cannot delete or requeue a message the new owner is processing.

Dispatch propagates the full skill identifier. A shorter dispatch key was used
once and produced ambiguity as soon as two workers exposed similarly named
skills.

## Audit and evidence {#audit-and-evidence}

There are two registers, and the guarantee is not the same on each.

The tool-invocation trail holds what ran, a hash of its inputs, whether it
succeeded, and how long it took. A call that failed is persisted as failed. It is
written fire-and-forget, so recording never blocks execution, and a record is
dropped when the channel is saturated: the trail is a best-effort record, not a
complete one. Its rows are flat: the input hash each row carries is not chained
to the row before it, and no row is signed.

The hash-chained journal is the register that carries the evidence. Its entries
are chained and signed, and the chain is anchored globally so that truncation is
detectable: removing the tail of the journal breaks a verification a reader can
run themselves.

Both registers cover the agent path. A tool call made in a chat session reaches
neither.

Replaying a run and comparing it against its record is deliberately not built.
Re-execution proves that a second run behaved a certain way, not that the first
one did, and accountability already rests on the signed chain. It is named here
so its absence reads as a choice rather than a gap.

## Secrets and API authentication {#secrets-and-api-auth}

Secrets live in the OS keychain, or in an age-encrypted file where no keychain
exists. They are never written to configuration files.

<!-- claim:daemon-binds-tcp-by-default -->
The HTTP API listens on a Unix socket and on a TCP port; the daemon binds both on
every start, and the port number is the only thing `--port` decides. The Unix
socket is local-trust and relies on filesystem permissions. TCP requires a
bearer token on every path, and binding it to a non-loopback address without TLS
is a startup error rather than a warning: an insecure remote bind is the one
mistake that cannot be undone once traffic has flowed.

## Command line {#cli}

The CLI is noun-verb. Daily operations are bare verbs; everything else is a noun
carrying subcommands. `--json` is global and a terminal is auto-detected, so one
command serves a human and a script.

Exit codes are a contract: 0 success, 1 usage error, 2 runtime error, 3 task
failed, 4 timeout, 5 canceled. A caller branches on them without parsing output.

## Desktop application {#desktop}

The desktop app is Tauri with a Svelte frontend, sharing the same runtime and
the same embedded Python interpreter as the CLI. It is a second front end over
one runtime, not a second implementation.

All user-facing text goes through the internationalization layer with parallel
English and French entries. Color, spacing, and typography come from design
tokens; a hardcoded value duplicating a token is a value that will not follow a
theme change.

## Agent distribution {#agent-distribution}

An agent is installed from a local path or a Git URL. Publishing agents to a
package index was considered and rejected: it would make the index an
availability dependency of a product whose first promise is running without one.

Third-party Python dependencies declared by an agent are installed into that
agent's own virtual environment, after explicit consent, and the operator sees
the list before it happens.

## Host integration {#host-integration}

The versioned HTTP API is a driving contract, not an implementation detail. It
carries a generated OpenAPI schema and generated TypeScript and Python clients,
so a host product drives the runtime without reverse-engineering route modules.

Breaking changes go through a new version prefix, never through a silent
mutation of the current one.

## Platforms and release {#platforms-and-release}

Linux, macOS, and Windows are supported targets. Tool confinement differs
between them, as stated above.

Releases are published on GitHub Releases. The desktop app checks that feed only
when the operator asks, never in the background, and reports an empty feed as an
empty feed rather than as an error.

## Verification {#verification}

Beyond the test suite, two classes of property are checked by machine rather
than by review: concurrency interleavings of the actor algorithms, and the two
cardinal invariants, the non-bypassable step budget and the mailbox lease fence,
proven under bounded symbolic execution.

These run on a schedule rather than on every change, because they are slow. Each
has an in-tree counterpart running with the ordinary test suite, so a regression
surfaces at the usual moment and the scheduled job confirms it.

## Documentation {#documentation}

This site is the documentation, organized by what a reader is trying to do:
tutorials to learn, how-to guides to accomplish, reference to look up,
explanation to understand.

The command-line and SDK references are generated from the code and committed,
with a check that fails when the committed pages drift from what the code would
produce. A reference that can drift silently is worse than no reference.
