# ADR-019: Native connectors and integrations

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia must deliver a usable set of integrations for power users while keeping its
local-first promise: the user's data stays on the machine and the agent connects to
it directly. The split that drives the strategy is economic. Some SaaS vendors
maintain their own official MCP server (Notion, Slack, Atlassian, Linear, GitHub,
Stripe, Figma, Sentry, Cloudflare): usable for free with maintenance externalized.
Others (Google Workspace, Microsoft 365, Salesforce, HubSpot) have no vendor-maintained
MCP, so the only path is a native connector. The integration catalogue, the
configuration UI, and the file-access model must follow from that split without
relying on a paid cloud aggregator.

## Decision

We adopt a hybrid integration strategy: native Rust connectors for Google Workspace
and Microsoft 365, official MCP servers for every SaaS that publishes one, driven by
a catalogue and a generic wizard, with the Google Drive Picker for file access.

### Hybrid strategy

Critical workflows (mail, calendar, files) for Google and Microsoft are native, where
no official MCP exists. Everything a SaaS publishes as an official MCP is consumed as
such, externalizing its maintenance. Salesforce and HubSpot are deferred. No cloud
aggregator (Composio and similar) is used, since a proprietary paid relay is anti
local-first by construction.

### Connector trait

Native connectors live in a dedicated `apollia-connectors` crate, organized around a
`Connector` trait with four methods (`id`, `manifest`, `operations`, `check`)
returning concrete types (`ConnectorManifest`, `OperationSpec`, `HealthReport`, with
`check` taking an `AccountId` from `apollia-auth`). Each service implements the trait
and declares its operations; the runtime registers those operations in the tool
registry at startup through a single code path. Connectors are build-time only: dynamic plugins
(shared objects or WASM) are rejected for now because the security cost (sandboxing,
ABI stability, signing) is out of scope. Adding a native connector is a module plus a
trait implementation plus a build-time registration, at the cost of an Apollia rebuild.

### MCP catalogue

The catalogue is a curated, enriched static set of entries. Each entry carries
`cost_model`, `trust_level`, `auth`, and `transport`, and is overridable user-side via
`~/.apollia/mcp-overrides.json` (`add`, `disable`, `override`), so a power user can patch
or add an internal MCP without waiting for a release. The entry schema is versioned and
stable, so a future remote registry tier (a public registry repo synced at startup with
a local cache and checksum) and a later signed community marketplace can swap the
catalogue provider implementation without rewriting entries.

### Generic connector wizard

The wizard is a single Svelte component (`ConnectorWizard`) driven by catalogue
metadata: it reads the required fields (auth type, parameters) from the selected entry
and generates its steps dynamically, with no per-connector code in the frontend. It
tests the connection before confirming (fail fast). Collected secrets go to the OS
keychain and are referenced from configuration, never written in clear text.

### Google Drive Picker

Google's free OAuth scope `drive.file` only grants access to files Apollia itself
creates, which forces users to migrate files into a dedicated folder. Restricted scopes
(`drive.readonly`, `drive`) require a CASA Tier 2 audit, not viable for an open-source
project. We integrate the official Google Drive Picker as the first-choice mechanism:
the user designates folders through Google's widget, Google extends `drive.file` to each
picked folder and its descendants, and Apollia stores the folder ids and exposes
operations to list and read and write within them. An expert mode (the user supplies
their own Google Cloud client with restricted scopes) remains an escape hatch, never the
default. Bundled Google credentials (`client_id`, `client_secret`, `api_key`) are public
by Google's own documentation for native apps; the real gate is the per-user OAuth token
in the OS keychain plus explicit consent, with API restrictions in the Cloud Console to
limit blast radius.

## Alternatives considered

### All community MCP (rejected)
- Pros: zero Rust connector code.
- Cons: full dependence on variable community quality; no official MCP for Google or
  Microsoft, so mail and calendar are not tenable end to end.

### All native Rust (rejected)
- Pros: total quality control.
- Cons: prohibitive maintenance across 15-20 SaaS APIs; reinvents what vendors already
  publish for free.

### Cloud aggregator (rejected)
- Pros: hundreds of apps through one managed endpoint.
- Cons: a proprietary paid cloud relay, anti local-first by construction.

### Per-connector wizard, or no wizard (rejected)
- Pros: bespoke UX per service, or zero wizard code.
- Cons: a component per service does not scale to the catalogue; a raw TOML editor is
  inaccessible to non-technical operators and offers no secure secret handling.

### Restricted Drive scopes, or expert-mode-only access (rejected)
- Pros: full Drive access.
- Cons: restricted scopes need a costly CASA audit; expert mode is too technical for the
  bulk of the target audience.

### Chosen: hybrid native plus official MCP, build-time connector trait, enriched static catalogue, generic wizard, Drive Picker
- Pros: maximizes free usage, externalizes maintenance where the vendor takes it on,
  keeps control over critical workflows, no cloud relay, no file migration for the user.
- Trade-offs: native OAuth maintenance for Google and Microsoft; an official catalogue
  entry requires a release until the registry tier ships; a rebuild is needed to add a
  native connector.

## Consequences

- Positive: `apollia-connectors` stays minimal (two active providers); the catalogue
  grows freely; the local-first promise holds with no cloud relay; users can point agents
  at existing Drive folders in one click.
- Negative / trade-off: Google and Microsoft Graph APIs ship breaking changes, requiring
  recurring integration tests; the Picker loads JavaScript from Google's CDN, a runtime
  dependency that fails with a clear message offline.
- Watch: appearance of an official Google or Microsoft MCP would let us retire native
  code; maintainer load as official SaaS MCP entries accumulate.

## Architectural principles

- Principle #1 (Local-first): no cloud relay; tokens in the OS keychain; data stays
  local.
- Principle #2 (Zero external dependency): no aggregator; the catalogue works offline
  from its local cache.
- Principle #3 (Minimal contract): the `Connector` trait stays thin (four methods
  returning concrete types); the catalogue entry schema is minimal but extensible.
- Principle #5 (One actor, one responsibility): `apollia-connectors` is stateless I/O,
  tokens live in `apollia-auth`, registration lives in the tool registry.
- Principle #8 (Human CLI, machine API): the catalogue and connectors are exposed through
  the CLI, and the wizard delegates mutations to the existing API.

## Related

- [ADR-017](ADR-017-mcp-client-transport-server.md) provides the MCP client the catalogue and wizard configure.
- [ADR-018](ADR-018-mcp-oauth.md) provides the OAuth flow the wizard triggers for remote MCP servers.
