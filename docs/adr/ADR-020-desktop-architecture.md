# ADR-020: Desktop application architecture

- Status: Accepted
- Date: 2026-06-04

## Context

The CLI is the primary interface for developers, but a non-technical user should
not have to open a terminal to run Apollia. `apollia-desktop` is the native
desktop application that starts Apollia by double-clicking an icon, packaged as a
`.dmg` (macOS) or `.AppImage` (Linux). Several architectural questions are
coupled and must be decided together: how the frontend communicates with the Rust
runtime, which frontend stack to build on, how real-time state reaches the UI,
and which parts of the configuration are editable in the application.

The constraints are the non-negotiable principles. Local-first (principle #1)
requires a single self-sufficient binary with no external server. Zero external
dependency (principle #2) means the user installs no prerequisite beyond the
binary itself, no Node.js, no separate Python runtime. Fail fast (principle #4)
requires an immediate error if the runtime does not start, not a blank window
that loads forever. Human CLI, machine API (principle #8) requires the existing
CLI to keep working in parallel through the existing Unix socket. The runtime
already exposes Tokio handles (`AgentRegistryHandle`, `TaskRouterHandle`, the
EventBus sender) that are `Clone + Send + Sync`, available for in-process passing
without serialization.

## Decision

We adopt a single-process Tauri v2 binary that starts the runtime in-process via
`init_embedded()`, with a Svelte 5 frontend, an EventBus-to-Tauri-events bridge
for real-time state, and a split configuration surface where `apollia.toml` is
read-only in the application while LLM backends and tool governance are editable
in-app through database-backed paths.

### Single process: Tauri plus embedded runtime

`apollia-desktop` is a Tauri v2 binary. Its `main()` calls `init_embedded()`,
which spawns the Tokio runtime thread, runs the existing supervisor startup
sequence (EventBus, then AgentRegistry, through to the API server), waits for the
all-ready signal on the EventBus, and returns a `RuntimeHandle`. Tauri then opens
the WebView. The supervisor is unchanged: `init_embedded()` is simply an
alternative to the CLI start loop.

Two complementary communication channels, with no duplication. Some mutations
go through `#[tauri::command]` handlers wrapping the Tokio handles directly,
in-process (for example `submit_task` calls `router_handle.submit`, and
`stop_agent` bypasses the REST API). Many commands, including reads and several
mutations, round-trip through the embedded axum API on the local port (for
example `resume_task` posts to `/api/v1/tasks/{id}/resume` and `start_agent`
posts to `/api/v1/agents`); thirteen command modules use this local HTTP path.
Real-time state flows through the events bridge described below. The CLI keeps
using the existing Unix socket in parallel, so the runtime stays reachable from
the command line while the desktop runs.

### Frontend: Svelte 5 plus Vite plus bits-ui

The frontend is Svelte 5 in runes mode, built with Vite, using bits-ui as the
headless component layer. Svelte 5 gives fine-grained reactivity and a small
bundle; bits-ui provides accessible, unstyled primitives that the Apollia design
system styles through tokens. State derived from runtime events lives in Svelte
stores. The assets are compiled and embedded in the Tauri binary, so there is no
CDN and no external fetch. The design system and internationalization layer that
dress these primitives are defined in [ADR-021](ADR-021-design-system-i18n.md).

### EventBus bridge to Tauri events (no IPC polling)

Real-time state reaches the frontend through an EventBus-to-Tauri-events bridge,
not through HTTP requests from the WebView and not through periodic IPC polling.
A `tokio::task` subscribes to the runtime broadcast channel of `RuntimeEvent`,
maps each event to a category, serializes it to JSON, and emits it through
`app_handle.emit("runtime-event", payload)`. The frontend listens with
`listen("runtime-event")` and dispatches by category (agent, task, approval, LLM,
trigger) into the relevant stores. Latency from a runtime event to its
appearance in the UI is under fifty milliseconds, against zero to three seconds
for any polling scheme.

A direct HTTP SSE connection from the WebView to the local API was rejected: in a
production build the WebView serves embedded assets from a `tauri://` origin, and
requests to the local HTTP port are blocked by the same-origin policy. The Tauri
events bridge avoids HTTP entirely. The REST SSE endpoint stays available for the
CLI and external integrations.

The bridge is fire-and-forget: a slow or lagging consumer can drop events. A
one-shot `refreshAll()` hydrates the stores at startup, and a watchdog triggers a
single `refreshAll()` if no event arrives within its window, flipping the
connection status to reconnecting after repeated misses. A five-second
`runtime:heartbeat` event keeps that watchdog fed during idle periods so the
reconnect banner does not appear when the bridge is alive but quiet.

### Settings: read-only file, editable backends and governance

`apollia.toml` is read-only inside the application. A structured `get_config()`
command returns a flat view for display, and an `open_config_in_editor()` command
opens the file in the native system editor. The reason is comment preservation: a
parse-then-reserialize round trip through the `toml` crate destroys user comments
and reorders sections, and pulling in `toml_edit` to avoid that adds complexity
for a marginal in-app editing case. Editing the file therefore happens in the
native editor, with a runtime restart to apply changes.

LLM backends and tool governance are not in `apollia.toml`; they live in
database-backed state and are fully editable in-app through dedicated views. The
split is deliberate: static bootstrap configuration stays in the commented TOML
file edited natively, while operational state that the user changes often is
edited in the application without touching the file.

## Alternatives considered

### Two separate processes (rejected)
- Pros: clean separation, the runtime can run without the frontend, no risk of a
  Tauri/PyO3 linker conflict.
- Cons: complex startup synchronization (the app must poll to learn the runtime
  is ready), two binaries to ship and coordinate, forced HTTP serialization for
  mutations when in-memory Tokio handles are available, and the user must manage
  two processes.

### Browser WebView without Tauri (rejected)
- Pros: no Tauri dependency, simpler build, reuses a browser-served dashboard.
- Cons: unacceptable friction for a non-technical user (open a browser, type a
  URL), no native packaging or double-click, no tray icon, no native
  notifications, no native file picker.

### IPC polling for real-time state (rejected)
- Pros: already worked as an early workaround, simple to reason about.
- Cons: zero-to-three-second latency, wasted IPC calls every interval even when
  idle, and it does not scale as each new view adds another polling call.

### In-app TOML editing with `toml_edit` (rejected)
- Pros: integrated editing UX for `apollia.toml`.
- Cons: `toml_edit` is more complex, comment preservation is fragile, and partial
  field editing creates an inconsistent surface where some fields are editable
  and others are not.

### Chosen: single-process Tauri with embedded runtime, Svelte plus bits-ui, events bridge, split settings
- Pros: a single distributed binary, in-process mutations with no serialization
  overhead, the CLI still working through the Unix socket, immediate failure if
  the runtime does not start, near-zero-latency event-driven UI, and safe
  comment-preserving configuration.
- Trade-offs: a larger binary that bundles the whole stack, the runtime stops
  when the window closes, and the events bridge can drop events under heavy load
  (mitigated by the watchdog).

## Consequences

- Positive: distribution is a single `.dmg` or `.AppImage`, the experience is
  native (double-click, window, agents visible), the existing Tokio handles are
  reused without architectural change, and the UI is event-driven rather than
  poll-driven.
- Negative / trade-off: the binary bundles Tauri, the WebView engine, the Rust
  runtime, and PyO3, so its size must be watched; closing the window stops the
  runtime; and editing `apollia.toml` requires a native editor plus a restart.
- Watch: binary size over time, the event rate under load (multiple agents plus
  active triggers) since a flood can drop events, and the WebView origin behavior
  across production builds.

## Architectural principles

- Principle #1 (Local-first): a single self-sufficient binary, no data leaves the
  machine, the WebView uses the native OS engine.
- Principle #2 (Zero external dependency): no Node.js or separate Python runtime
  for the user; assets are embedded.
- Principle #4 (Fail fast): `init_embedded()` returns an immediate error if the
  runtime does not start.
- Principle #8 (Human CLI, machine API): the desktop is a third interface on the
  same runtime, and the CLI keeps working through the Unix socket and the REST
  SSE endpoint.

## Related

- [ADR-001](ADR-001-foundations-stack.md) the stack foundations the desktop
  binary builds on.
- [ADR-021](ADR-021-design-system-i18n.md) the design system and i18n layer that
  dress the bits-ui primitives.
- [ADR-022](ADR-022-chat-subsystem.md) the chat subsystem that consumes the same
  runtime and events bridge.
