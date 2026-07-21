---
sidebar_position: 11
title: Package and distribute an agent
---

# Package and distribute an agent

An agent can ship as a single Python file or as a multi-agent bundle described by
an `agent.toml`, and it is distributed by installing from a local path or a Git
URL. This guide covers scaffolding, the bundle format, install-time validation, and
distribution over Git.

It assumes you have written an agent; if not, see
[Write a worker](/how-to/write-a-worker).

## Scaffold a starter

`agent create` generates a starter agent and a matching test file from a template:

```sh
apollia-os agent create my-agent --type react
```

`--type` is `react` (default), `conversational`, or `orchestrated`. This writes a
single agent module plus its test; it does not create a bundle. Author the
`agent.toml` yourself when you need more than one agent in a package.

## Single file versus bundle

Apollia recognizes two shapes at install time:

- **Single file.** A `.py` module that ends with a module-level `agent = ...`. It
  installs as one agent.
- **Bundle.** A directory containing an `agent.toml` at its root, describing one or
  more agents (plus optional shared configuration). It installs as a package.

A directory without an `agent.toml` is not a valid package and is rejected.

## The `agent.toml` bundle format

```toml
[package]
name = "sales-suite"
version = "0.1.0"
description = "A worker and a director for sales prep."
author = "you"

[[agents]]
name = "crm-lookup"
entry = "crm_lookup.py"
role = "worker"
packages = ["httpx>=0.27"]

[[agents]]
name = "sales-director"
entry = "director.py"
role = "director"
packages = []

[tools.web]
enabled = true
ssrf_guard = true

[pip]
packages = ["python-dateutil"]
```

- `[package]` carries `name`, `version`, `description`, and `author`.
- Each `[[agents]]` entry has a `name`, an `entry` module, a `role` (`worker`,
  `director`, or `assistant`), and its own `packages`.
- `[tools.web]` toggles the web tool surface and its SSRF guard.
- `[pip]` lists package-wide Python dependencies. Triggers may also be declared in
  the bundle.

## Install and its validation

Install from a local path:

```sh
# A single file
apollia-os agent install ./my_agent.py

# A bundle directory
apollia-os agent install ./sales-suite/
```

Install runs these checks, in order:

1. The source exists.
2. The agent loads and satisfies the contract (a `manifest()` and an async
   `run()`), validated through the Python bridge.
3. If a manifest declares `dangerous_tools_allowed`, the installer emits a
   warning and continues; it does not block or prompt for confirmation.
4. Declared Python packages are provisioned.
5. If the agent ships a `tests/` directory, its tests run under `pytest`. A
   failure blocks the install. Skip this step with `--skip-tests` (not
   recommended).

## Distribute over Git

Any Git repository whose root holds the agent file (or an `agent.toml` bundle) can
be installed directly. Point `agent install` at the clone URL, optionally pinning a
tag or branch with a `#` suffix:

```sh
apollia-os agent install https://github.com/you/my-agent.git
apollia-os agent install https://github.com/you/my-agent.git#v1.2.0
```

The runtime shells out to `git` to clone the repository (a shallow clone, on the
pinned ref when given), then validates and installs it exactly as a local source.
`git` must be present on the machine; there is no fallback when it is absent. There
is no built-in discovery index or search: you distribute the URL, the installer
takes it from there.

## Manage installed agents and packages

```sh
# Single agents
apollia-os agent list
apollia-os agent uninstall my-agent
apollia-os agent update my-agent ./my_agent.py      # replace with a new local module

# Packages (bundles)
apollia-os agent package list
apollia-os agent package show sales-suite
apollia-os agent package uninstall sales-suite      # removes its agents and triggers
```

`agent update` replaces an installed agent with a new local module path; it does
not re-clone a Git source. To update from Git, install again from the URL.

## Related

- [Write a worker](/how-to/write-a-worker) for the skill contract a distributed
  worker exposes.
- Every `agent` subcommand and flag is in the [CLI reference](/reference/cli).
