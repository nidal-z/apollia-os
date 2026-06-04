# ADR-014: Operational config, triggers and notifications

- Status: Accepted
- Date: 2026-06-04

## Context

`apollia.toml` mixes two kinds of configuration with different lifecycles and audiences.
Structural config (ports, paths, feature flags, LLM backends) changes at deployment,
edited by a developer, on a slow cycle. Operational config (triggers, notification
channels) changes during routine operation, edited by an operator, on a fast cycle.
Mixing them forces operators to edit TOML, makes hot reload fragile, and blurs the
separation of concerns.

The runtime also exposes a `POST /webhooks/{id}` endpoint reachable from outside.
Without authentication, any process could fire agents arbitrarily. Schedules and secrets
change during operation, so a full restart to apply them would take down every running
agent.

Finally, runtime events (a task needing approval, a failure) must reach the operator
proactively. The dispatch path and the webhook payload format must be decided once, as
they shape the public configuration surface.

These choices are bound by local-first (no network call for config), fail-fast (any
detectable error caught before the first fire), and one-actor-one-responsibility (each
engine owns its own reload).

## Decision

We adopt structural config in read-only TOML and operational config in SQLite with hot
reload, HMAC-SHA256 webhook authentication, and a `NotificationChannel` trait with
per-target payload formatting.

### Structural config in TOML, operational config in SQLite

`apollia.toml` holds structural config only (runtime, memory, tools, budget, llm,
agents). It is read-only in the desktop app and requires a restart. SQLite holds
operational config (`triggers.db`, `notifications.db`), edited via CRUD from the REST
API and the desktop app, applied immediately through an actor reload.

The mutation pattern is API handler -> SQLite -> `Handle.reload()`:

```
POST /api/v1/triggers (axum handler)
  1. validate the payload (Rust types plus business rules)
  2. write to SQLite (TriggerDefinitionRepository)
  3. trigger_engine.reload() rereads all definitions from SQLite
  4. return 201/200
```

No watch, no polling, no cache invalidation. The API handler is the single entry point.
Repositories live in `AppState` behind `Arc<Mutex<Repository>>` because a rusqlite
`Connection` is not `Sync` and mutations are rare.

### HMAC-SHA256 webhook authentication

HMAC-SHA256 with constant-time comparison is the sole webhook authentication mechanism.
The inbound webhook secret is part of the trigger definition stored in SQLite
(`triggers.db`, `TriggerSourceConfig::Webhook { secret }`), read at request time by the
`POST /webhooks/{id}` handler. It is never logged and never appears in HTTP responses.
This is distinct from the outbound notification webhook `signing_secret`, which lives in
`apollia.toml` and signs the notifications Apollia sends out. The signature follows the
GitHub Webhooks format: `X-Apollia-Signature: sha256=<hex>`.
Verification order is strict: `503` (engine unavailable), `404` (unknown trigger), `401`
(missing or invalid signature), `200`. The `404` precedes the `401` so an unauthenticated
caller cannot confirm a trigger exists. HMAC binds the secret to the body cryptographi-
cally, so a replayed request with a different body is rejected.

### Hot reload by abort plus respawn

`TriggerEngineHandle::reload(new_definitions)` gives each active `JoinHandle` 2 seconds
to finish cleanly (`tokio::time::timeout(2s, handle)`) before a forced drop, swaps the
in-memory definitions without touching the SQLite counters (`fire_count`,
`last_fired_at`), respawns only the sources where `enabled = true`, and emits a reload
event. On reload error the current triggers keep running and the runtime answers
`422 Unprocessable Entity`.

### `NotificationChannel` trait with per-target payload formatting

A `NotificationChannel` trait (`Send + Sync`, `#[async_trait]`) backs every channel:

```rust
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    fn id(&self) -> &str;
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool;
    async fn send(&self, notif: &Notification) -> Result<(), NotifError>;
}
```

The notification engine subscribes to the event bus at startup, maps `RuntimeEvent` to
`Notification`, and iterates over `Vec<Box<dyn NotificationChannel>>`. Three channels
implement the trait: desktop (OS notification), terminal (OSC sequences for iTerm2,
GNOME/VTE, bell), and webhook (HTTP). A failed channel (webhook timeout, missing desktop
daemon) logs a warning and the runtime continues; the other channels are unaffected.

The webhook channel formats its payload per target. It auto-detects the destination from
the URL hostname (`detect_webhook_kind`): a `discord.com` host gets a native Discord
embed payload, a `hooks.slack.com` host gets a native Slack `{ text, attachments }`
payload, and every other endpoint receives the Apollia JSON object:

```json
{
  "event":     "task.input_required",
  "timestamp": "2026-03-08T14:23:11Z",
  "runtime":   "apollia-os",
  "version":   "<CARGO_PKG_VERSION>",
  "task_id":   "t-0042",
  "agent":     "devis-agent",
  "message":   "...",
  "severity":  "warning",
  "metadata":  { "resume_url": "..." }
}
```

Here `severity` is a top-level field and `version` is filled from
`env!("CARGO_PKG_VERSION")`. An `X-Apollia-Event` header accompanies the Apollia payload.
Discord and Slack endpoints get a payload their platform accepts directly, so only custom
endpoints need integrator-side transformation.

## Alternatives considered

### TOML stays the source of truth with improved hot reload (rejected)
- Pros: no new storage.
- Cons: operators must still edit TOML, hot reload cannot validate interactively, and the
  app stays read-only for everything.

### EventBus notification of actors on config change (rejected)
- Pros: decoupled.
- Cons: extra event plus per-engine subscriber plus ordering; the handler cannot tell
  whether the reload succeeded. Direct `Handle.reload()` is simpler for rare operator
  mutations.

### Static bearer token for webhooks (rejected)
- Pros: trivial.
- Cons: does not authenticate the body; a replayed request with a different body passes.

### User-configurable template payload (rejected)
- Pros: arbitrary output format driven by an operator-written template.
- Cons: adds a templating dependency, a learning curve, and silent template bugs. The
  webhook channel instead ships hardcoded per-target formats (Discord, Slack, Apollia),
  which covers the common destinations natively without a templating engine.

### Hard-wired channels without a trait (rejected)
- Pros: no dynamic dispatch.
- Cons: a fourth channel forces editing the engine, and unit testing requires real
  concrete channels rather than a mock.

### Chosen: SQLite operational config plus HMAC plus the channel trait
- Pros: a non-developer configures triggers and notifications from the app; webhooks are
  immune to body tampering and timing attacks with no cloud dependency; new channels need
  only implement the trait; near-zero notification latency via direct event subscription.
- Trade-offs: trigger and notification CRUD moves to SQLite while `apollia.toml` keeps
  `[notifications]` and `[[notifications.channels]]` sections that load alongside
  `notifications.db`, so the two coexist; `Arc<Mutex<>>` repositories are not pure Tokio
  actors; business validation is duplicated client-side for live feedback.

## Consequences

- Positive: operators manage triggers and notifications from the desktop app with
  interactive validation; webhooks are authenticated and tamper-resistant; reload
  preserves history counters; the structural TOML stays read-only.
- Negative / trade-off: full-replace reload restarts even unchanged sources (minimal for
  cron and interval); the Apollia payload still needs integrator-side transformation for
  destinations beyond the natively formatted Discord and Slack targets; the notification
  log table grows without bound on very active runtimes.
- Watch: if a fourth operational subsystem appears, re-evaluate whether the pattern still
  holds; add automatic rotation (TTL) on the notification log; verify that `git`-less or
  headless environments degrade gracefully for the desktop channel.

## Architectural principles

- Principle #1 (local-first): operational config and CRUD are local SQLite; the desktop
  and terminal channels work offline; the webhook channel is optional.
- Principle #2 (zero external dependency): rusqlite, constant-time comparison, and the
  notification libraries are compiled into the binary; no external service is required.
- Principle #4 (fail fast): config is validated at write time with a `422` response, and
  channel config is validated at startup rather than at first notification.
- Principle #5 (one actor, one responsibility): each engine owns its reload, the
  notification engine only dispatches, and repositories stay passive.
- Principle #8 (human CLI, machine API): operator actions are explicit and the API returns
  structured results.

## Related

- [ADR-005](ADR-005-oria-execution-model.md) the execution model whose tasks emit the
  runtime events that triggers and notifications act on.
- [ADR-013](ADR-013-human-in-the-loop.md) the HITL flow whose `input_required` event is a
  primary notification driver.
