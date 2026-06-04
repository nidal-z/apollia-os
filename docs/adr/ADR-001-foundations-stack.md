# ADR-001: Vision and stack foundations

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia is a sovereign runtime for autonomous AI agents. The product is a single
open-source binary, distributed under an MIT license, that hosts Python agents
(LangGraph, CrewAI, custom) locally with no cloud dependency. The differentiator
is architectural: local-first execution and zero external dependency. A clean
machine must run an agent within minutes by downloading one artifact, with no
prior service to install.

Three foundational constraints flow from that vision and must be decided
together:

- The runtime language must produce a single static binary, supervise async
  actors in parallel (sandbox management, Python bridge, EventBus), and host an
  in-process Python interpreter so a `RuntimeContext` can be injected directly
  into the agent rather than crossing a subprocess boundary.
- Persistence must be embedded. Requiring PostgreSQL, Redis, or a vector service
  would violate the zero-dependency principle. Agent memory still has to be
  durable, full-text searchable, and isolated per agent.
- The local API has to be consumable by two client classes at once: the
  `apollia-cli` process on the same machine, and Python SDKs or third-party
  integrations. It must be debuggable with `curl` and usable without a generated
  client.

## Decision

We adopt a Rust and Tokio runtime, SQLite with FTS5 as the only persistence
engine, and a REST/JSON local API served over both a Unix domain socket and a
loopback TCP port.

### Runtime: Rust and Tokio

The entire runtime (supervision, routing, memory, API) is written in Rust on
Tokio. Python is reserved for agents, reached through the PyO3 bridge (see
[ADR-002](ADR-002-pyo3-bridge-decoupling.md)). Rust compiles to a single static
binary, the borrow checker rules out segfaults and data races, and Tokio gives a
native actor model over `mpsc::channel` without `Arc<Mutex<T>>` shared across
actors. Core identifier types such as `AgentId` and `TaskId` are newtypes in
`apollia-core`, not string aliases, so the type system enforces that a task id is
never passed where an agent id is expected.

### Persistence: SQLite with FTS5

SQLite is compiled into the binary through the `bundled` feature of `rusqlite`,
in WAL mode. The FTS5 extension provides full-text search. Each agent memory
namespace maps to one `.db` file under `~/.apollia/memory/<namespace>.db`, which
gives strong per-agent data isolation. Vector search via `sqlite-vec` is
optional and not bundled, since the target workloads (thousands of episodes per
agent, not millions of rows) do not require it by default.

### Local API: REST/JSON over Unix socket and TCP

The API uses `axum`. It is served on two transports from the same router with
the same state: a Unix domain socket for the CLI (minimal latency, no TCP
overhead) and `localhost:7771` (bound to `127.0.0.1` only) for SDKs and external
integrations. SSE covers unidirectional streaming of long-running tasks. The
TCP router applies a token-authentication layer when an API token is configured,
while the Unix socket router is intentionally left unauthenticated and relies on
filesystem permissions for access control.

Because `axum::serve()` accepts only a `TcpListener`, the Unix socket listener is
driven explicitly through `hyper-util`, which is already a transitive dependency
of `axum` and therefore adds zero bytes to the binary. The types used are
`hyper_util::rt::TokioIo` to adapt a tokio `UnixStream` into a hyper stream,
`hyper_util::rt::TokioExecutor`, `hyper_util::server::conn::auto::Builder`, and
`hyper_util::service::TowerToHyperService` to bridge a `tower::Service` to a
`hyper` service. Graceful shutdown is uniform across both listeners through a
`watch::channel`. The `util` feature is enabled on `tower` so unit tests can call
`ServiceExt::oneshot()` against the router without starting a server.

## Alternatives considered

### Go for the runtime (rejected)
- Pros: native single binaries, good concurrency, simple builds.
- Cons: no PyO3 equivalent to embed the CPython interpreter in-process; agents
  would need a subprocess, which forbids injecting `RuntimeContext` directly.

### Python for the runtime (rejected)
- Pros: same ecosystem as agents, fast iteration.
- Cons: the GIL limits real concurrency, single-binary packaging is fragile, and
  it would violate the zero-dependency principle.

### PostgreSQL for persistence (rejected)
- Pros: robust concurrency, mature FTS, native JSON.
- Cons: requires a separate service, directly violating zero external
  dependency.

### gRPC and protobuf for the API (rejected)
- Pros: high performance, strong typing, native streaming.
- Cons: codegen in every client, not debuggable with `curl`, over-engineered for
  a local API.

### Unix socket only (rejected)
- Pros: maximum performance, full network isolation.
- Cons: non-Rust integrations cannot easily consume a Unix socket; Python SDKs
  would need wrappers.

### Chosen: Rust/Tokio, SQLite/FTS5, REST/JSON dual transport
- Pros: a static binary that runs on any clean machine, in-process Python through
  PyO3, embedded searchable persistence with per-namespace isolation, and an API
  that works out of the box with `curl localhost:7771/api/v1/health`.
- Trade-offs: a steeper Rust learning curve and longer compile times; SQLite
  write concurrency is bounded (WAL mitigates it); JSON serialization is slightly
  heavier than binary protobuf, which is negligible for local traffic.

## Consequences

- Positive: one downloadable binary with no system prerequisites; data never
  leaves the machine; the CLI gets socket-level latency while SDKs get plain HTTP.
- Negative / trade-off: bounded SQLite write concurrency; no static type contract
  between CLI and runtime (covered by tests); the Unix socket path is more verbose
  than the TCP path.
- Watch: FTS5 performance as agent memory grows; Linux user-namespace
  availability for the sandbox path that builds on this runtime.

## Architectural principles

- Principle #1 (Local-first): the reason for the open-source local runtime; data
  stays on the user machine, the Unix socket is local and TCP binds to loopback
  only.
- Principle #2 (Zero external dependency): Rust enables the static binary, SQLite
  is bundled, `axum` and `hyper-util` are pure Rust with no external service.
- Principle #5 (One actor, one responsibility): Tokio `mpsc::channel` is the
  standard inter-actor primitive.
- Principle #8 (Human CLI, machine API): REST/JSON is the machine surface; the
  CLI carries the human surface.

## Related

- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) hosts the Python agents this
  runtime supervises.
- [ADR-004](ADR-004-cli-design.md) consumes this API over the Unix socket.
