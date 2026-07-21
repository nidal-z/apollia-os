---
sidebar_position: 0
title: Reference
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
- **[Sampling defaults](/reference/sampling-defaults)** the default sampling
  parameters per model family and the precedence rule.
