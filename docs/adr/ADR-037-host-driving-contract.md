# ADR-037 - Shared driving contract for host integration

- Status: Accepted
- Date: 2026-07-08

## Context

The chosen product positioning is: Apollia is the sovereign runtime, **embeddable and federable**, that IT product vendors integrate to add auditable autonomous agents, locally. The central subject is therefore integration: how a host application consumes an Apollia instance.

The certified cartography of 2026-07-08 (source of truth, verified against the code) establishes that this positioning has **no product integration contract**. This is a packaging gap, not a capability gap: the value building blocks are wired, but nothing lets a third party drive them cleanly.

Verified state of the code:
- The `/api/v1` API exists (axum, listening simultaneously on a Unix socket and TCP `127.0.0.1:7771`, ~25 route modules), but **with no OpenAPI schema, no typed client, no contract documentation**. The request/response types are private `serde` structs in each `routes_*.rs`.
- The reference client (`apollia-cli/src/client.rs`) is **Unix-socket-only on Unix** and **never sends an `Authorization` header**: it cannot even drive the authenticated TCP path. The API is architected for local same-host access, not third-party driving.
- **No host-side client SDK** in any language. The Python SDK `sdk/apollia` serves to *write* agents, not to *drive* the runtime.
- **Inconsistent** auth: the Unix socket is never authenticated (file-system permissions only); TCP is protected by a Bearer token by default, but the embedded path forces `api_token: None` (`embedded.rs:401`), so the TCP port is served without auth under Tauri.
- An agent executing MCP tools via `ctx.tools.call('mcp:...')` **resolves the tool but does not execute it** (the AIP ToolProxy path is not wired). The Yumni integration had to work around this with a hand-written REST worker.

Constraint: a host vendor can be written in any language. The realistic integration point is therefore the HTTP API, not the in-process Rust API. This is also the "machine API" side of principle 8. Why now: this contract is the keystone of the beachhead; without it, "integration is the product" has no product.

## Decision

We package the `/api/v1` API as a **shared driving contract**: a stable, typed and documented product, serving both federation (the Yumni pattern: Apollia as a sovereign peer that talks to the host) and direct driving. It comprises four components and one guarantee:

1. **OpenAPI spec generated from the code**, via `utoipa` (annotations on the handlers and on the `routes_*.rs` structs). The generated spec is the published contract artifact; it cannot diverge from the code since it derives from it.
2. **Host client SDKs generated from the OpenAPI**, TypeScript and Python first (consistent with Yumni: Node MCP server + Python director). Produced by tooling (for example `openapi-typescript` and `openapi-python-client`), not hand-written, to stay in sync with the spec.
3. **Consistent TCP auth**: the Bearer token is honored everywhere on TCP, including on the embedded path. By default, the embedded path does **not** bind a TCP port (Unix socket only); if it binds one, it honors the token. The Unix socket stays local-trust (FS permissions), documented as such.
4. **MCP execution wiring**: the AIP ToolProxy really executes tools prefixed `mcp:` through the MCP executor, so the federation pattern no longer requires a host-side workaround.
5. **Stability guarantee**: `/api/v1` becomes a versioned contract. Any breaking change goes through `/api/v2`, never through a silent mutation of `v1`.

The scope explicitly excludes **pure in-process embedding** (extracting `embedded` into a crate independent of Tauri, a reusable Rust API): deferred to phase 2, under a future ADR.

## Alternatives considered

### Option A - Federation only (rejected)
**For:** closest to the real code and the Yumni proof; minimal effort.
**Against:** too narrow. It does not serve direct driving, does not solve the missing schema/SDK/doc, and leaves each integration reinventing a one-off bridge (like Yumni's REST worker). It does not make integration a replicable product.

### Option B - Pure in-process embedding first (rejected)
**For:** the purest "embed the runtime" model; zero latency.
**Against:** Rust-only, so it excludes TS/Python hosts (including Yumni). Large refactor (extract `embedded` from Tauri, provide a PyO3 loader + backend). It does not meet the need for a multi-language host. Deferred to phase 2.

### Option C - Status quo, raw undocumented HTTP (rejected)
**For:** zero work.
**Against:** it is not a product. Every integrator has to reverse-engineer `routes_*.rs`, auth is inconsistent and exposed, nothing guarantees stability. This is exactly the current gap.

### Chosen: Shared driving contract
**For:** a single foundation serves both models (federation and driving); the generated OpenAPI stays in sync with the code; multi-language; it is already the real usage (Yumni drives Apollia over HTTP). It makes integration replicable, and therefore sellable.
**Trade-offs:** a stability commitment on `/api/v1`; added build dependencies (utoipa + generators).

## Consequences

**Positives:**
- The beachhead finally gets an integration product: a host vendor integrates Apollia in TS/Python without reverse-engineering.
- Consistent auth closes the "TCP without auth" exposure of the embedded path.
- MCP wiring unblocks the federation pattern without a workaround.
- The OpenAPI also becomes the API reference, and feeds the adopter documentation.

**Negatives / Trade-offs:**
- Stability commitment on `/api/v1`: a breaking change now costs a `/api/v2` + a migration.
- New build dependencies (utoipa, OpenAPI generators): a sovereignty surface to own. They are **build-time only**, not embedded at runtime, which makes them acceptable with respect to principle 2.
- Annotating all `routes_*.rs` with utoipa is broad mechanical work.
- Generating and maintaining two SDKs adds CI load.

**Neutral / Watch:**
- The Unix socket stays unauthenticated (local-trust): watch that remote hosts really go through TCP + token.
- SDK languages beyond TS/Python (Go, a Rust client) to be decided based on demand.
- This ADR does not address in-process embedding (phase 2) nor the budget safeguards (a separate workstream).

## Architectural principles

- **Principle #8 - Human CLI, machine API**: reinforces the "machine API" side by making it stable, typed and documented; this is its concrete realization.
- **Principle #2 - Zero external dependency**: the added dependencies are build-time and justified here; the served API stays local (Unix socket / localhost).
- **Principle #4 - Fail fast**: a typed contract + consistent auth make integration errors fail early.

## Related

- Related ADRs: ADR-016 (secrets, keyring and local API auth), ADR-017 (MCP client, transport, server mode), ADR-024 (SDK ctx runtime contract), ADR-020 (desktop / embedded architecture)

## Amendment (2026-07-08, post-implementation)

The host driving contract is delivered and proven on the `feat/driving-contract` branch (components 1, 2, 3, 5 conformant; end-to-end host demo green).

Rectification of **component 4 (MCP execution)**: the implementation established that an agent executing MCP tools via `ctx.tools.call('mcp:...')` **was already wired and functional** (fixed earlier in commit `4c7266d6`). The "broken" observation in the Context section came from a stale comment (`yumni_bridge.py:6-8`) taken at face value. Component 4 therefore reduces to: adding a non-regression test (`crates/apollia-mcp/tests/integration_agent_dispatch.rs`) and removing the REST workaround on the Yumni side. No other part of the decision is affected.

Known minor caveat: 3 raw-body endpoints (stt config/transcribe, webhook) stay documented in the spec but are not exposed as typed SDK methods.

## Amendment (2026-07-10, post-merge)

The host driving contract is now **merged into `main`** (the `feat/driving-contract` branch cited above has been integrated). The rectification of component 4 was **re-verified against the merged code**, and it holds:
- End-to-end proof of `ctx.tools.call('mcp:...')` present and executed in `crates/apollia-aip/src/context.rs` (the "Full end-to-end proof" test, real dispatch via `apollia_mcp::executor::build_agent_tool_executors`).
- Non-regression test `crates/apollia-mcp/tests/integration_agent_dispatch.rs` present.
- Earlier MCP execution fix confirmed (commit `4c7266d6`).
- REST workaround on the Yumni side removed (`yumni_bridge.py` absent from the repository).

The only remaining open caveat is the 3 raw-body endpoints not typed in the SDK (above). The initial "MCP execution broken" observation in the Context section is therefore invalidated and survives only as a trace of the original analysis (append-only): read it in the light of the two amendments.
