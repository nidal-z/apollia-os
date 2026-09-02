---
sidebar_position: 0
title: Reference
description: "The machine-checked facts about Apollia OS: HTTP API, CLI, SDK contract, configuration keys, native tools, events and evaluation suites."
---

# Reference

Information-oriented, precise, generated where possible.

## Generated sections (source of truth, do not hand-edit)

- **[CLI reference](/reference/cli)** generated from the `apollia-os` clap command tree.
- **[HTTP API reference](/reference/api/apollia-os-runtime-api)** generated from the
  delivered OpenAPI spec (`clients/openapi.json`).
- **[SDK / ctx contract](/reference/sdk)** generated from `sdk/apollia/types.py` and the
  per-service `Ctx` protocols.

Refresh all three with `bash regen.sh`.

## Additional references

- **[Configuration (apollia.toml)](/reference/configuration)** the configuration
  file sections and their fields.
- **[Native tool catalog](/reference/native-tools)** the tools the runtime exposes
  to agents out of the box.
- **[Environment variables](/reference/environment-variables)** what the runtime
  reads from its environment: the local engine, secret storage, connector OAuth
  clients, diagnostics.
- **[Sampling defaults](/reference/sampling-defaults)** which sampling parameter
  reaches a model, and what is written but not applied.
- **[Evaluation suite schema](/reference/eval-suites)** the TOML an
  `apollia-os eval run` suite accepts, field by field and assertion by assertion.
