---
slug: /
sidebar_position: 0
title: Apollia OS documentation
---

# Apollia OS documentation

Apollia OS is a sovereign Rust runtime for autonomous AI agents. It runs any
Python agent in isolation, locally, with tools, and without a cloud dependency.

This site is organised with the [Diataxis](https://diataxis.fr) framework. Pick
the entry point that matches what you need right now.

## Where to go

- **[Tutorials](/tutorials)** learn by doing, start here if you are new.
- **[How-to guides](/how-to)** achieve a specific task.
- **[Reference](/reference)** precise, generated facts about the API, CLI, SDK,
  configuration, and tools.
- **[Explanation](/explanation)** understand the concepts and the reasoning.
- **[Architecture](/architecture)** the public system cartography (arc42 and C4).
- **[Operator help](/operator-help)** the desktop operator space (French).

## Source of truth

The three machine references are never hand-copied. They are generated from the
code:

- the HTTP API reference from the delivered OpenAPI spec,
- the CLI reference from the `apollia-os` command tree,
- the SDK contract from the typed `Ctx` services.

Run `bash regen.sh` to refresh them.

:::note Phase 1

This is the documentation skeleton. Most pages are placeholders that list the
content still to be migrated. The generated reference pages already carry real
data.

:::
