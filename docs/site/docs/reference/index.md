---
sidebar_position: 0
title: Reference
---

# Reference

Information-oriented, precise, generated where possible.

## Generated sections (source of truth, do not hand-edit)

- **[CLI reference](/reference/cli)** generated from the `apollia-os` clap command tree.
- **HTTP API reference** generated from the delivered OpenAPI spec (`clients/openapi.json`).
- **[SDK / ctx contract](/reference/sdk)** generated from `sdk/apollia/types.py` and the
  per-service `Ctx` protocols.

Refresh all three with `bash regen.sh`.

## Hand-maintained sections (to migrate)

- **[Configuration (apollia.toml)](/reference/configuration)** (migrate from the config docs).
- **[Native tool catalog](/reference/native-tools)** (migrate from the tools reference).
