# ADR-026: Agent install, bundle format and distribution

- Status: Accepted
- Date: 2026-06-04

## Context

Agents must persist across runtime restarts, ship in a self-describing format,
group several related agents into one installable unit, and be distributable
beyond the machine that created them. A few constraints shape the design. The
runtime must work offline: obtaining the agents that ship with Apollia must not
require a network endpoint (principle #2). Validation of a third-party agent must
happen at install time, not at execution (principle #4). Everything an agent
owns must live in the local home directory (principle #1). A self-describing
format must be readable by a human and indexable by future tooling without
executing Python. Distribution must be peer-to-peer, with no central server that
becomes a single point of failure.

## Decision

We adopt a folder-based agent bundle (a manifest plus an entry module,
mandatory; optional library and asset folders), install by copying into the home
agents directory with SQLite persistence and boot-time reload, support
multi-agent packages, separate bundled from community agents, and distribute
community agents through a Git-based registry.

### Bundle format

An agent bundle is a folder:

```
my-agent/
  manifest.toml        # static metadata (bundled / system agents)
  agent.py             # entry module, exposes the agent
  lib/                 # optional: local importable modules
  assets/              # optional: read-only resources
  requirements.txt     # optional: pip dependencies
  README.md            # optional
```

The manifest filename depends on the install context. System bundled agents use
a `manifest.toml` with an `[agent]` table. Multi-agent packages use an
`agent.toml`. Community agents installed from a Git or local source carry a
`manifest.json`. A manifest plus an entry module is the mandatory pair in every
case. For a community install the entry module is not required to be named
`agent.py`: the installer scans the bundle directory for the first
non-`__init__.py` Python file and treats it as the entry module. The entry
module exposes the agent through the decorator-first AgentKit
([ADR-023](ADR-023-sdk-agentkit-design.md)): the `@agent`-decorated class
produces the module-level `agent` attribute the bridge reads. Local modules live
exclusively under `lib/` (imported as `from lib import helpers`); root-level
imports are forbidden. Read-only resources live exclusively under `assets/`. The
manifest carries static metadata (name, version, description, license,
authors, tags, required and optional tools, memory namespace, supported Python
range, declared pip packages, and permission flags such as network and
filesystem scope). It is the packaging contract, readable at a glance and
indexable without running Python; the agent's own decorated definition remains
the source of truth for the runtime manifest.

On load, the PyO3 bridge prepends the install path to `sys.path`, loads the
entry module with the install path as the submodule search location, and cleans
up `sys.path` afterward.

### Install, persistence, and boot reload

Installation copies the bundle into `~/.apollia/agents/<name>/` and records
metadata in a SQLite database. The flow validates that the manifest is parseable
and an entry module is present, copies the bundle recursively, persists the
metadata as enabled, registers the agent if the runtime is active, and emits an
install event. At boot the supervisor reads the database,
filters enabled agents, and loads, validates, and registers each one; an agent
that fails to load is logged as a warning without blocking boot. The CLI provides
install, uninstall, enable/disable, update, and list. Update replaces the bundle
and re-validates; a downgrade is rejected.

### Multi-agent packages

A package is a self-contained folder described by a package manifest that lists
several agents (each with its name, entry file, and role), the tools they need,
and the triggers to inject. Installing a package validates fail-fast, copies the
folder, performs an idempotent upsert into the package and package-agent tables,
and injects the declared triggers into the database. Uninstalling a package
removes all of its agents and the triggers it injected. Standalone single-file
agents keep working unchanged.

### Bundled versus community

Agents that ship with the runtime are bundled: maintained by Apollia, covering
general-purpose use cases with stable dependencies, and installed automatically
at first boot. Agents that need infrastructure-specific configuration, or that
come from third parties, are community agents installed explicitly by the
operator. The two are physically separated, and the runtime refuses to install
into the bundled location; bundled agents are updated only through a runtime
update. At first boot (detected by an empty agents table), the supervisor
registers each bundled agent from a local index; pip venvs are installed at first
start, not at boot, to avoid slowing startup on modest hardware.

### Git-based community registry

Each community agent is its own public Git repository: the repository is the
registry, with no central HTTP server. Installation clones the repository into a
temporary directory, validates, and installs:

```
apollia agent install https://github.com/org/my-worker.git
```

An optional index repository holds a `registry.json` listing known agents (name,
description, git URL, declared skills, maintainer); its URL is configurable. If
the index is unset or unreachable, discovery is disabled but direct installation
by URL still works, so there is no central point of failure. A source argument is
resolved as a local path, then a Git URL, then a short identifier looked up in
the configured index. Installation validation runs sequential steps, any failure
aborting with an explicit message: validate the manifest against the schema, scan
for agents that request dangerous tools (emitting a `tracing::warn!` that signals
the operator that user approval is required), install the declared pip packages
with `pip install`, and run an optional smoke test if present. The CLI provides
install by URL, search in the index, list by source, and update (re-clone,
re-validate, replace). Community agents are not cryptographically signed at this
stage:
trust rests on the visible Git URL, install-time manifest validation, and the
dangerous-tool warning; GPG-signed commits are encouraged but not required,
and where Git is unavailable a native Rust Git implementation is the fallback.

## Alternatives considered

### Reference an agent by absolute path (rejected)
- Pros: no copy.
- Cons: the source file can move or vanish between sessions, with no integrity
  guarantee.

### Python wheel as the bundle format (rejected)
- Pros: standard Python packaging.
- Cons: imposes build tooling and complex metadata; a human must be able to read
  the manifest at a glance.

### Centralized HTTP registry hosted by Apollia (rejected)
- Pros: easy discovery.
- Cons: a single point of failure and an operational cost; if it is down, nothing
  installs. Violates local-first and zero external dependency.

### PyPI for the agents themselves (rejected)
- Pros: existing infrastructure.
- Cons: conflates an agent's pip dependencies with the agent itself, confusing
  for users.

### Embed every agent in the binary (rejected)
- Pros: one artifact.
- Cons: a heavy binary and no way to update agents independently of the runtime.

### Chosen: folder bundle, SQLite-backed install, bundled/community split, Git registry
- Pros: everything lives locally, the format is self-describing and indexable,
  packages are distributable as a folder or a repo, the runtime works offline,
  install-time validation catches malformed manifests and dangerous flags, and
  distribution is peer-to-peer with an optional index.
- Trade-offs: the source file is duplicated on install, there is no bundle
  signature or post-clone integrity check yet, and `git clone` requires Git (or
  the native fallback) on the target.

## Consequences

- Positive: a coherent persistence model with all artifacts under the home
  directory, install-once UX, a self-describing format usable by future tooling,
  clean versioning through the manifest version, full backward compatibility for
  single-file agents, and peer-to-peer distribution with no mandatory server.
- Negative / trade-off: minor source duplication, no cryptographic signature at
  this stage, brief `sys.path` pollution during load (bounded and cleaned), and
  limited discoverability without the optional index.
- Watch: cross-platform compatibility of bundled pip packages, first-boot time
  including bundled registration, and a moderation process for the index
  repository before a public beta.

## Architectural principles

- Principle #1 (Local-first): everything lives under `~/.apollia/`, and each
  community agent is cloned locally; the index never blocks direct installation.
- Principle #2 (Zero external dependency): the bundle is a manifest file plus a
  standard Python tree, bundled agents need no network, and Git is the ubiquitous
  transport with a native fallback.
- Principle #3 (Minimal contract): a manifest plus an entry module, and the agent
  definition stays the runtime source of truth.
- Principle #4 (Fail fast): an invalid bundle is rejected at install with an
  explicit message and no partial state.
- Principle #7 (Non-negotiable safeguards): the dangerous-tool scan at install
  emits an explicit warning that the operator must act on before trusting the
  agent.

## Related

- [ADR-023](ADR-023-sdk-agentkit-design.md) the AgentKit that defines how
  the entry module exposes the module-level agent.
- [ADR-025](ADR-025-worker-agents-a2a-routing.md) the worker agents most commonly
  packaged and distributed this way.
